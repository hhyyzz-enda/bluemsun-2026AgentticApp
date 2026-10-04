//! ADR 0010 host adapter. Scripts choose an operation, never an origin or token.
use matrix_sdk::reqwest::{self, Url};
use rinx_miniapp_core::{Lease, bounded_reply, parse_arguments};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub const APP_ID: &str = "im.palpo.operations";
const PREFIX: &str = "/_palpo/miniapp/v1/";
const MAX_WIRE_BYTES: usize = 2 * 1024 * 1024;

struct Session {
    token: String,
    expires: Instant,
    account: String,
    origin: String,
    host_token_digest: blake3::Hash,
}
/// Owned by a single reviewed app instance. Dropping it discards credentials.
/// The lease revokes queued requests as well as replies after account changes.
#[derive(Clone)]
pub struct PalpoHost {
    digest: String,
    action: Option<String>,
    session: Arc<Mutex<Option<Session>>>,
    client: reqwest::Client,
}
impl PalpoHost {
    pub fn new(digest: String) -> Result<Self, String> {
        if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid verified bundle digest".into());
        }
        Ok(Self {
            digest,
            action: None,
            session: Default::default(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| "Cannot create Palpo connection")?,
        })
    }
    pub fn with_action(mut self, action: Option<String>) -> Self {
        self.action = action.filter(|id| valid_action(id));
        self
    }
    /// Production callers derive all four context values from the active SDK
    /// session and the reviewed lease. This public seam also serves the native
    /// instrument example; it is not exposed as a script API.
    pub async fn execute(
        &self,
        lease: &Lease,
        account: &str,
        homeserver: Url,
        matrix_token: &str,
        service: &str,
        args: Value,
    ) -> Result<Value, String> {
        lease.authorize(account, service, None)?;
        if lease.identity().app != APP_ID
            || !octosense_app_contract::palpo::SERVICES.contains(&service)
        {
            return Err("This Palpo app or operation is not supported".into());
        }
        parse_arguments(&args.to_string())?;
        // No package-supplied endpoint. HTTP is allowed only for local SDK test
        // servers, matching Rinx's local development homeserver support.
        let endpoint = endpoint(homeserver)?;
        let mut session = self.session.lock().await;
        lease.authorize(account, service, None)?;
        let origin = endpoint.as_str().to_owned();
        let host_token_digest = blake3::hash(matrix_token.as_bytes());
        if session.as_ref().is_some_and(|s| {
            Instant::now() >= s.expires
                || s.account != account
                || s.origin != origin
                || s.host_token_digest != host_token_digest
        }) {
            *session = None;
        }
        if service == "palpo.session.disconnect" {
            if let Some(previous) = session.take() {
                let _ = self
                    .post(&endpoint, "disconnect", &previous.token, &json!({}))
                    .await;
            }
            lease.check(account)?;
            return Ok(json!({"disconnected": true}));
        }
        if session.is_none() {
            let opened = self.post(&endpoint, "session", matrix_token, &json!({
                "appId": lease.identity().app, "bundleDigest": self.digest,
                "services": lease.services().iter().filter(|s| octosense_app_contract::palpo::SERVICES.contains(&s.as_str())).collect::<Vec<_>>()
            })).await?;
            lease.check(account)?;
            if opened["userId"].as_str() != Some(account) || opened["version"] != 1 {
                return Err("Palpo returned a different account or protocol version".into());
            }
            let token = opened["sessionToken"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or("Palpo did not issue an app session")?
                .to_owned();
            // Renew before the server's 15-minute limit; never persist the bearer.
            *session = Some(Session {
                token,
                expires: Instant::now() + Duration::from_secs(12 * 60),
                account: account.to_owned(),
                origin,
                host_token_digest,
            });
        }
        lease.authorize(account, service, None)?;
        let result = self
            .post(
                &endpoint,
                "call",
                &session.as_ref().unwrap().token,
                &json!({"service": service, "args": args}),
            )
            .await;
        // A failed mutation is never blindly replayed. The caller retries with
        // the same durable operation ID and can first fetch the latest result.
        if result.as_ref().is_err_and(|s| {
            s.starts_with("app_session_expired:") || s.starts_with("M_UNKNOWN_TOKEN:")
        }) {
            *session = None;
        }
        let mut result = result?;
        if service == "palpo.session.open" {
            result["openAction"] = json!(self.action.as_deref().unwrap_or(""));
        }
        lease.check(account)?;
        if service == "palpo.fleets.export" {
            let bytes =
                serde_json::to_vec_pretty(&result).map_err(|_| "Invalid fleet configuration")?;
            // Credentials go directly to a native save dialog, never into an
            // isolate result, clipboard, chat, app jail, or diagnostic log.
            let (tx, rx) = tokio::sync::oneshot::channel();
            robius_file_picker::FileDialog::new()
                .set_file_name("hagency-registration.json")
                .save_data(bytes, move |result| {
                    let _ = tx.send(
                        result
                            .map(|file| file.is_some())
                            .map_err(|_| "Could not save configuration".to_string()),
                    );
                })
                .map_err(|_| "Could not open the system save dialog")?;
            let saved = rx
                .await
                .map_err(|_| "Configuration save was interrupted")??;
            lease.check(account)?;
            return Ok(json!({"saved": saved}));
        }
        bounded_reply(result)
    }
    async fn post(
        &self,
        origin: &Url,
        operation: &str,
        token: &str,
        body: &Value,
    ) -> Result<Value, String> {
        let url = origin
            .join(&format!("{PREFIX}{operation}"))
            .map_err(|_| "Invalid Palpo endpoint")?;
        let mut response = self.client.post(url).bearer_auth(token).header("Content-Type", "application/json").body(serde_json::to_vec(body).map_err(|_| "Invalid Palpo request")?).send().await
            .map_err(|_| "Palpo could not be reached. Your request may still be pending; refresh before retrying.")?;
        let status = response.status();
        if status.is_redirection() {
            return Err(
                "Palpo redirects are refused; configure the same-origin mini-app route".into(),
            );
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_WIRE_BYTES as u64)
        {
            return Err("Palpo response is too large".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Palpo response was interrupted")?
        {
            if bytes.len() + chunk.len() > MAX_WIRE_BYTES {
                return Err("Palpo response is too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        decode_response(status, &bytes)
    }
}

fn decode_response(status: reqwest::StatusCode, bytes: &[u8]) -> Result<Value, String> {
    let value = serde_json::from_slice::<Value>(bytes);
    let code = value
        .as_ref()
        .ok()
        .and_then(|v| v["code"].as_str())
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 80
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
    // Existing adapters also return 404 for inaccessible objects and 501 for
    // unconfigured features. Preserve those application errors; only an
    // unrecognized route response means the adapter itself is missing.
    if matches!(status.as_u16(), 404 | 405 | 501) && code.is_none() {
        return Err("This server needs the Palpo mini-app adapter. Ask its operator to enable /_palpo/miniapp/v1 on this homeserver.".into());
    }
    if !status.is_success() {
        // Backend errors are public messages, but never echo arbitrary server
        // data (which could include a proxy's request headers/credentials).
        let code = code.unwrap_or("operation_failed");
        return Err(format!(
            "{code}: {}. Refresh to see the current result.",
            match status.as_u16() {
                401 => "Your app session expired; retry to reconnect",
                403 => "Your account is not authorized for this operation",
                404 => "This item is unavailable to your account",
                409 => "The action or its prerequisites changed",
                429 => "Too many requests; try again later",
                400 => "Review the form fields",
                501 => "This operation is not enabled on the server",
                _ => "Palpo could not complete this operation",
            }
        ));
    }
    value.map_err(|_| "Palpo returned an invalid response".into())
}

fn endpoint(mut url: Url) -> Result<Url, String> {
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))))
    {
        return Err("Palpo requires the trusted HTTPS homeserver origin".into());
    }
    url.set_path("/");
    Ok(url)
}

pub async fn request(
    host: PalpoHost,
    lease: Lease,
    service: String,
    args: Value,
) -> Result<Value, String> {
    let client = crate::sliding_sync::get_client().ok_or("Not logged in")?;
    let account = client.user_id().ok_or("Not logged in")?.to_string();
    let token = client.access_token().ok_or("Not logged in")?;
    host.execute(
        &lease,
        &account,
        client.homeserver(),
        &token,
        &service,
        args,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inaccessible_configuration_does_not_report_a_missing_adapter() {
        let error = decode_response(
            reqwest::StatusCode::NOT_FOUND,
            br#"{"code":"not_found","message":"private configuration details"}"#,
        )
        .unwrap_err();
        assert!(error.starts_with("not_found: This item is unavailable"));
        assert!(!error.contains("adapter"));
        assert!(!error.contains("private configuration"));
        let error = decode_response(
            reqwest::StatusCode::NOT_IMPLEMENTED,
            br#"{"code":"outbound_unconfigured"}"#,
        )
        .unwrap_err();
        assert!(error.starts_with("outbound_unconfigured: This operation is not enabled"));
    }
    #[test]
    fn absent_route_and_proxy_errors_have_safe_diagnostics() {
        for body in [
            b"<html>proxy debug headers</html>".as_slice(),
            br#"{"errcode":"M_UNRECOGNIZED"}"#,
        ] {
            let error = decode_response(reqwest::StatusCode::NOT_FOUND, body).unwrap_err();
            assert!(error.contains("enable /_palpo/miniapp/v1"));
            assert!(!error.contains("debug headers"));
        }
        let error = decode_response(
            reqwest::StatusCode::BAD_GATEWAY,
            b"Authorization: Bearer private-token",
        )
        .unwrap_err();
        assert!(!error.contains("private-token"));
        assert!(decode_response(reqwest::StatusCode::OK, b"not-json").is_err());
    }
    #[test]
    fn endpoint_is_the_authenticated_origin_only() {
        assert_eq!(
            endpoint(Url::parse("https://matrix.example/base").unwrap())
                .unwrap()
                .as_str(),
            "https://matrix.example/"
        );
        for url in [
            "http://matrix.example",
            "https://user:password@matrix.example",
            "https://matrix.example/?token=bad",
            "https://matrix.example/#other",
        ] {
            assert!(endpoint(Url::parse(url).unwrap()).is_err(), "{url}");
        }
    }
    #[test]
    fn notification_links_bind_to_the_current_homeserver() {
        let home = Url::parse("https://matrix.example:19443").unwrap();
        let id = format!("action_{}", "a".repeat(32));
        let path = format!("/_palpo/miniapp/action/{id}");
        assert_eq!(action_link(&home.join(&path).unwrap(), &home), Some(id));
        for link in [
            format!("https://other.example{path}"),
            format!("https://matrix.example{path}"),
            format!("https://matrix.example:19443{path}?account=other"),
            format!("https://matrix.example:19443{path}/approve"),
        ] {
            assert!(
                action_link(&Url::parse(&link).unwrap(), &home).is_none(),
                "{link}"
            );
        }
        assert!(!valid_action("action_../../secret"));
    }
}

/// Accept only a routing identifier, never data or executable card content.
pub fn valid_action(id: &str) -> bool {
    id.strip_prefix("action_").is_some_and(|hex| {
        hex.len() == 32
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn route(url: &str) -> Option<String> {
    if let Some(id) = url.strip_prefix("rinx://palpo/action/") {
        return valid_action(id).then(|| id.to_owned());
    }
    let link = Url::parse(url).ok()?;
    let home = crate::sliding_sync::get_client()?.homeserver();
    action_link(&link, &home)
}
fn action_link(link: &Url, home: &Url) -> Option<String> {
    if link.scheme() != "https"
        || link.origin() != home.origin()
        || !link.username().is_empty()
        || link.password().is_some()
        || link.query().is_some()
        || link.fragment().is_some()
    {
        return None;
    }
    let id = link.path().strip_prefix("/_palpo/miniapp/action/")?;
    valid_action(id).then(|| id.to_owned())
}
