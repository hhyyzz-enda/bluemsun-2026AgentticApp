//! Native instrument host for the production Palpo bundle and HTTP adapter.
//! Defaults to fixture credentials. PALPO_LIVE_SESSION_FILE explicitly borrows a
//! saved Matrix session for an operator-owned loopback/SSH validation sidecar.
//! It never opens or changes the user's Matrix database or logs credentials.
pub use makepad_widgets;
use makepad_widgets::*;
use makepad_widgets::splash_host::{take_splash_host_requests_for, splash_host_respond};
use rinx::miniapps::palpo::{PalpoHost, APP_ID};
use rinx::theme::{self, Selection, Appearance, Accent};
use rinx_miniapp_core::{InstanceId, Lease};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use serde_json::Value;
app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: #(if std::env::args().any(|a| a == "--narrow") {dvec2(430.,820.)} else {dvec2(1000.,800.)})
                body +: {flow: Down padding: 20 spacing: 12 show_bg: true draw_bg.color: RINX_PAGE
                    hint := RinxHint{text: "Local Palpo instrument test · fixture accounts"}
                    app := Splash {width: Fill height: Fill}
                }
            }
        }
    }
}
#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    runtime: Option<tokio::runtime::Runtime>,
    #[rust]
    host: Option<PalpoHost>,
    #[rust]
    lease: Option<Lease>,
    #[rust]
    pending: Vec<(usize, u64, mpsc::Receiver<Result<Value, String>>)>,
    #[rust]
    calls: usize,
    #[rust]
    matrix_token: String,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let profile = std::env::var_os("RINX_DATA_DIR").expect("isolated fixture profile");
        let manifest =
            octosense_app_contract::parse(include_str!("../apps/palpo/bundle/manifest.json"))
                .unwrap();
        let (account, token) = if let Some(path) = std::env::var_os("PALPO_LIVE_SESSION_FILE") {
            let raw = std::fs::read(path).expect("explicit live session file");
            let session: Value = serde_json::from_slice(&raw).expect("saved Matrix session");
            assert_eq!(session["client_session"]["homeserver"].as_str().unwrap().trim_end_matches('/'),
                "https://crew.ominix.io:19443", "this live check is bound to the authorized crew server");
            self.ui.label(cx, ids!(hint)).set_text(cx, "Live Matrix · isolated Palpo validation backend");
            (session["user_session"]["user_id"].as_str().or_else(|| session["user_session"]["meta"]["user_id"].as_str()).unwrap().to_owned(),
             session["user_session"]["access_token"].as_str().or_else(|| session["user_session"]["tokens"]["access_token"].as_str()).unwrap().to_owned())
        } else if std::env::args().any(|a| a == "--admin") {
            ("@admin:example.test".into(), "admin-secret".into())
        } else {
            ("@owner:example.test".into(), "owner-secret".into())
        };
        self.matrix_token = token;
        self.runtime = Some(tokio::runtime::Runtime::new().unwrap());
        self.host = Some(PalpoHost::new("a".repeat(64)).unwrap());
        self.lease = Some(Lease::new(
            InstanceId {
                app: APP_ID.into(),
                account,
                room: None,
                generation: 1,
            },
            manifest.capabilities.iter().cloned().collect(),
            Default::default(),
            Instant::now() + Duration::from_secs(3600),
        ));
        let splash = self.ui.splash(cx, ids!(app));
        splash.set_sandbox_dir(cx, Some(std::path::PathBuf::from(profile).join("app")));
        splash.set_storage_quota(cx, Some(1024 * 1024));
        splash.set_host_caps(cx, manifest.capabilities);
        splash.set_policy(cx, Some(vec![]), Some(50_000_000));
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/apps/palpo/bundle/main.splash"
        ))
        .unwrap();
        splash.set_text(cx, &source);
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        theme::init_standalone(vm);
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        desktop_style::apply_widgets(vm);
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Custom(command) = event {
            if command == "palpo:inspect" {
                let splash = self.ui.splash(cx, ids!(app));
                let data = serde_json::json!({"calls": self.calls, "heap": splash.isolate_heap_key(cx), "pending": self.pending.len(), "revision": theme::snapshot(cx).revision});
                std::fs::write(
                    rinx::app_data_dir().join("inspection.json"),
                    data.to_string(),
                )
                .unwrap();
            }
            if command == "palpo:revoke" {
                self.lease.as_ref().unwrap().revoke();
            }
            for (name, appearance, accent) in [
                ("palpo:dark", Appearance::Dark, Accent::Teal),
                ("palpo:light", Appearance::Light, Accent::Teal),
                ("palpo:violet", Appearance::Light, Accent::Violet),
            ] {
                if command == name {
                    theme::select(cx, Selection { appearance, accent }).unwrap();
                }
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if matches!(event, Event::Signal) {
            cx.redraw_all();
        }
        let owned: Vec<_> = self
            .ui
            .splash(cx, ids!(app))
            .isolate_heap_key(cx)
            .into_iter()
            .collect();
        for req in take_splash_host_requests_for(&owned) {
            self.calls += 1;
            let host = self.host.as_ref().unwrap().clone();
            let lease = self.lease.as_ref().unwrap().clone();
            let base = std::env::var("PALPO_FIXTURE_URL").expect("local fixture URL");
            let url: matrix_sdk::reqwest::Url = base.parse().unwrap();
            assert_eq!(
                url.host_str(),
                Some("127.0.0.1"),
                "fixture host must be loopback"
            );
            let account = lease.identity().account.clone();
            let token = self.matrix_token.clone();
            let (tx, rx) = mpsc::channel();
            self.pending.push((req.heap_key, req.req_id, rx));
            self.runtime.as_ref().unwrap().spawn(async move {
                let args = serde_json::from_str(&req.args_json).unwrap();
                let result = host
                    .execute(&lease, &account, url, &token, &req.service, args)
                    .await;
                let _ = tx.send(result);
                SignalToUI::set_ui_signal();
            });
        }
        let mut complete = Vec::new();
        for (i, (heap, id, rx)) in self.pending.iter().enumerate() {
            if let Ok(result) = rx.try_recv() {
                let result = result.map(|v| v.to_string());
                splash_host_respond(cx, *heap, *id, result.as_deref().map_err(String::as_str));
                complete.push(i);
            }
        }
        for i in complete.into_iter().rev() {
            self.pending.remove(i);
        }
    }
}
