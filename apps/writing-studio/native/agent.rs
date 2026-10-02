//! The rewrite agent. The service is a trait so a real LLM is admitted
//! without touching the panel, the tasks or the storage: `active_service()`
//! picks a configured `LlmRewriteService`, otherwise the offline mock.
//!
//! Key hygiene (hackathon rule): the app never collects or stores an AI key.
//! The LLM key lives outside the repo — env vars `WRITING_STUDIO_LLM_*` or
//! `<app_data_dir>/writing-studio-llm.json` — and is read at call time only.
//! Keep the mock's contract: failures are `Err(String)` shown on the card
//! with a retry, and citations must survive in the output.
use super::model::{citations, Constraints};

/// Turns a selected passage plus a natural-language request into a reviewable
/// proposal. The agent proposes; it never writes the document itself.
pub trait RewriteService: Send + Sync {
    fn rewrite(&self, selection: &str, request: &str, constraints: &Constraints) -> Result<String, String>;
    /// Long drafts prepare in the background so the studio stays responsive;
    /// the panel shows "Preparing" and the card returns when the proposal is
    /// ready.
    fn needs_background(&self, selection: &str) -> bool {
        selection.chars().count() > 500
    }
    /// Shown on the preparing card: "Mock", or the model name for the LLM.
    fn engine_name(&self) -> String {
        "Mock".to_owned()
    }
    /// Network services block for up to a minute: the panel runs them on a
    /// worker thread and the result is written back into the shared store.
    fn runs_off_thread(&self) -> bool {
        false
    }
    /// A fresh handle handed to the worker thread; the panel keeps its own.
    fn fork(&self) -> Box<dyn RewriteService>;
}

/// Citations are never sacrificed: re-anchor any marker the rules (or the
/// model) dropped at the end of the passage, in original order.
fn reanchor_citations(selection: &str, out: &mut String) {
    for marker in citations(selection) {
        if !out.contains(&marker) {
            if !out.ends_with(['。', '.', '！', '!', '？', '?']) && !out.is_empty() {
                out.push('。');
            }
            out.push_str(&marker);
        }
    }
}

/// Rule-based stand-in: deterministic, offline, and honest about its limits.
/// A request containing 「失败」 injects a failure, so the error path is
/// demoable end to end.
#[derive(Default)]
pub struct MockRewriteService;

/// Filler phrases dropped by the "more concise" rule.
const CONCISE_DROP: &[&str] = &[
    "基本上可以说", "其实可以说", "可以说", "基本上", "其实", "非常", "十分", "相当",
    "总的来说", "众所周知", "在一定程度上", "的话", "actually ", "basically ", "very ",
    "really ", "in general ", "it is known that ",
];
/// Colloquial → formal replacements for the "more formal" rule.
const FORMAL_MAP: &[(&str, &str)] = &[
    ("搞定", "完成"), ("弄好", "完成"), ("东西", "事项"), ("咱们", "我们"),
    ("挺好的", "良好"), ("很多", "大量"), ("挺", "较为"), ("但是", "然而"),
    ("所以", "因此"), ("a lot of", "numerous"), ("stuff", "material"),
    ("but ", "however "), ("so ", "therefore "), ("get done", "complete"),
];

impl RewriteService for MockRewriteService {
    fn rewrite(&self, selection: &str, request: &str, constraints: &Constraints) -> Result<String, String> {
        if request.contains("失败") {
            return Err("Mock 注入失败：请求文本包含「失败」。".into());
        }
        if selection.trim().is_empty() {
            return Err("选区为空，无法改写。".into());
        }
        let mut out = selection.to_owned();
        let want_concise = constraints.concise || request.contains("简洁") || request.contains("concise");
        let want_formal = constraints.formal || request.contains("正式") || request.contains("formal");
        if want_concise {
            for phrase in CONCISE_DROP {
                out = out.replace(phrase, "");
            }
            while out.contains("  ") {
                out = out.replace("  ", " ");
            }
            out = out.trim().to_owned();
        }
        if want_formal {
            for (from, to) in FORMAL_MAP {
                out = out.replace(from, to);
            }
        }
        if !want_concise && !want_formal {
            // No recognizable constraint: a light tidy, so the proposal still
            // differs from the original only where it helps.
            out = out.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        // Citations are never sacrificed to brevity: re-anchor any marker the
        // rules dropped at the end of the passage, in original order.
        if constraints.keep_citations || !citations(selection).is_empty() {
            reanchor_citations(selection, &mut out);
        }
        if out == selection {
            out = format!("{}（改写：更凝练的表达）", out.trim_end_matches(['。', '.']));
        }
        Ok(out)
    }

    fn fork(&self) -> Box<dyn RewriteService> {
        Box::new(MockRewriteService)
    }
}

// ---------------------------------------------------------------------------
// The real LLM: OpenAI-compatible chat completions, configured from outside
// the repo, called through the system `curl` so no Cargo dependency is added.
// ---------------------------------------------------------------------------

/// Environment variables checked before the config file.
pub const ENV_BASE_URL: &str = "WRITING_STUDIO_LLM_BASE_URL";
pub const ENV_KEY: &str = "WRITING_STUDIO_LLM_KEY";
pub const ENV_MODEL: &str = "WRITING_STUDIO_LLM_MODEL";
pub const ENV_FAMILY: &str = "WRITING_STUDIO_LLM_FAMILY";
/// Config file name, directly under the app data dir (NOT the per-account
/// directory): `crate::app_data_dir().join(CONFIG_FILE)`.
pub const CONFIG_FILE: &str = "writing-studio-llm.json";

/// OpenAI-compatible endpoint configuration. base_url/key/model are required
/// for the config to count as present; `family` (the Octos kernel's provider
/// family id) defaults to "openai" — the generic OpenAI-compatible route.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub family: String,
}
impl LlmConfig {
    /// Resolves the effective config: each env var wins over the matching
    /// file field; missing pieces fall back to the file. `None` when any of
    /// the three required fields is still empty.
    pub fn resolve(env: impl Fn(&str) -> Option<String>, file: Option<LlmConfig>) -> Option<LlmConfig> {
        let file = file.unwrap_or_default();
        let pick = |name: &str, fallback: &str| {
            env(name).filter(|v| !v.trim().is_empty()).unwrap_or_else(|| fallback.to_owned())
        };
        let mut config = LlmConfig {
            base_url: pick(ENV_BASE_URL, &file.base_url),
            api_key: pick(ENV_KEY, &file.api_key),
            model: pick(ENV_MODEL, &file.model),
            family: pick(ENV_FAMILY, &file.family),
        };
        if config.family.trim().is_empty() {
            config.family = "openai".to_owned();
        }
        if config.base_url.trim().is_empty() || config.api_key.trim().is_empty() || config.model.trim().is_empty() {
            None
        } else {
            Some(config)
        }
    }
    /// Loads the config: env vars first, then `<app_data_dir>/writing-studio-llm.json`.
    pub fn load() -> Option<LlmConfig> {
        let path = crate::app_data_dir().join(CONFIG_FILE);
        let file = std::fs::read_to_string(path).ok().and_then(|text| parse_config(&text));
        Self::resolve(|name| std::env::var(name).ok(), file)
    }
}
/// Parses the flat config file with the same minimal extractor used for
/// responses — prototype-grade, no parser dependency.
fn parse_config(text: &str) -> Option<LlmConfig> {
    Some(LlmConfig {
        base_url: json_string_field(text, "base_url")?,
        api_key: json_string_field(text, "api_key")?,
        model: json_string_field(text, "model")?,
        family: json_string_field(text, "family").unwrap_or_default(),
    })
}

/// Escapes a string for embedding into a hand-built JSON body.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
/// Extracts the string value of the FIRST occurrence of `"key"` — a minimal,
/// prototype-grade stand-in for a JSON parser (adding serde_json here is
/// disallowed). Handles the standard string escapes. For OpenAI-compatible
/// responses the first `"content"` is the assistant message (choices come
/// before usage); for our flat config file the keys are unique anyway.
fn json_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start().strip_prefix(':')?.trim_start();
    let body = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut iter = body.chars();
    while let Some(c) = iter.next() {
        match c {
            '"' => return Some(out),
            '\\' => match iter.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let hex: String = iter.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// The constraints are encoded as Chinese instructions; citation markers are
/// a hard rule, always stated regardless of the toggles.
fn system_prompt(constraints: &Constraints) -> String {
    let mut prompt = String::from(
        "你是一名严谨的中文写作助手。用户会给你一段选中的文字和一条改写请求。\
         只输出改写后的文字本身：不要解释、不要加引号、不要复述请求。",
    );
    if constraints.concise {
        prompt.push_str("改写必须更简洁：删去冗余词语与套话，原意不变。");
    }
    if constraints.formal {
        prompt.push_str("改写必须更正式：把口语化表达替换为规范的书面语。");
    }
    prompt.push_str("硬性要求：保留所有 [^n] 形式的引用标记，不得增加、删除、改写或移动。");
    prompt
}
/// Builds the chat-completions request body by hand (no serializer
/// dependency): `{model, messages: [system, user], temperature: 0.3}`.
fn build_request_body(config: &LlmConfig, selection: &str, request: &str, constraints: &Constraints) -> String {
    let system = system_prompt(constraints);
    let user = format!("【选中文字】\n{selection}\n\n【改写请求】\n{request}");
    format!(
        "{{\"model\":\"{}\",\"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},\
         {{\"role\":\"user\",\"content\":\"{}\"}}],\"temperature\":0.3}}",
        json_escape(&config.model),
        json_escape(&system),
        json_escape(&user),
    )
}
/// POSTs the body through the system `curl` (bundled since Windows 10, and
/// on macOS/Linux), with the body in a temp file to avoid command-line
/// escaping pitfalls; the temp file is removed right after the call.
/// Blocking — the panel runs this on a worker thread.
fn chat_completion(config: &LlmConfig, body: &str) -> Result<String, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("writing-studio-llm-{}-{nanos}.json", std::process::id()));
    std::fs::write(&path, body).map_err(|e| format!("无法写入请求临时文件：{e}"))?;
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let output = std::process::Command::new("curl")
        .args([
            "-sS", "-m", "60", "-X", "POST",
            "-H", "Content-Type: application/json",
            "-H", &format!("Authorization: Bearer {}", config.api_key),
            "-d", &format!("@{}", path.display()),
            &url,
        ])
        .output();
    let _ = std::fs::remove_file(&path);
    let output = output.map_err(|e| format!("无法启动 curl：{e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!("LLM 请求失败（curl 退出码 {:?}）：{stderr}", output.status.code()));
    }
    if let Some(content) = json_string_field(&stdout, "content").filter(|c| !c.trim().is_empty()) {
        return Ok(content);
    }
    // An error body usually carries "message"; surface it for the retry card.
    Err(json_string_field(&stdout, "message")
        .map(|m| format!("LLM 返回错误：{m}"))
        .unwrap_or_else(|| "LLM 响应中没有可用的改写内容。".into()))
}

/// The networked service: same contract as the mock, plus citation re-anchor
/// insurance when the model drops a marker despite the hard rule.
#[derive(Clone)]
pub struct LlmRewriteService {
    pub config: LlmConfig,
}
impl RewriteService for LlmRewriteService {
    fn rewrite(&self, selection: &str, request: &str, constraints: &Constraints) -> Result<String, String> {
        if selection.trim().is_empty() {
            return Err("选区为空，无法改写。".into());
        }
        let body = build_request_body(&self.config, selection, request, constraints);
        let mut out = chat_completion(&self.config, &body)?;
        // Citations are never sacrificed: re-anchor any marker the model
        // dropped at the end of the passage, in original order.
        reanchor_citations(selection, &mut out);
        Ok(out)
    }
    /// Network calls always prepare in the background.
    fn needs_background(&self, _selection: &str) -> bool {
        true
    }
    fn engine_name(&self) -> String {
        self.config.model.clone()
    }
    fn runs_off_thread(&self) -> bool {
        true
    }
    fn fork(&self) -> Box<dyn RewriteService> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// The official path: the rewrite turn runs through Rinx's Octos service
// layer (app → host → model). The host holds the provider key, every request
// is auditable, and the app never sees a credential. `OctosAppService` has
// no raw-completion API; a single turn on a request context
// (`open_context` + `ContextOp::TurnFrom`) is the sanctioned "ask the
// assistant" channel — same one the mini-app host adapts in `host/octos.rs`.
// ---------------------------------------------------------------------------

/// One combined prompt: a turn carries a single text, so the constraint
/// instructions and the system rule lead the selection and the request.
/// The selection is capped at 24 KiB, comfortably under the 32 KiB turn cap.
fn octos_prompt(selection: &str, request: &str, constraints: &Constraints) -> String {
    const MAX_SELECTION: usize = 24 * 1024;
    let mut end = selection.len().min(MAX_SELECTION);
    while !selection.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n【选中文字】\n{}\n\n【改写请求】\n{}",
        system_prompt(constraints),
        &selection[..end],
        request,
    )
}
/// A completed turn's text: the public result's `text` field, else the last
/// streamed chunk (Data events carry the cumulative "so far").
fn reply_text(value: &serde_json::Value, streamed: &str) -> Option<String> {
    value
        .get("text")
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| (!streamed.trim().is_empty()).then(|| streamed.to_owned()))
}

/// Rewrite over Rinx's Octos peer. Blocking (the turn streams until
/// Complete) — the panel runs it on a worker thread like the curl service.
pub struct OctosRewriteService {
    context: std::sync::Arc<dyn octosense_app_peers::OctosContext>,
}
impl OctosRewriteService {
    /// Opens the app's request context on the current service. `Err` when
    /// the host grants no assistant — the engine chain then falls back.
    pub fn open() -> Result<Self, String> {
        let account = crate::sliding_sync::current_user_id()
            .map(|id| id.to_string())
            .ok_or("未登录，无法使用 Octos 服务。")?;
        let service = crate::octos_service::service()?;
        let context = service.open_context(octosense_app_peers::ContextSpec {
            account,
            instance: format!("{}-g0", super::APP_ID),
            services: ["octos.turn.start".to_owned()].into_iter().collect(),
        })?;
        Ok(Self { context })
    }
}
impl Drop for OctosRewriteService {
    fn drop(&mut self) {
        self.context.close();
    }
}
impl RewriteService for OctosRewriteService {
    fn rewrite(&self, selection: &str, request: &str, constraints: &Constraints) -> Result<String, String> {
        use octosense_app_peers::{ContextEvent, ContextOp, TurnTrigger};
        if selection.trim().is_empty() {
            return Err("选区为空，无法改写。".into());
        }
        let (tx, rx) = std::sync::mpsc::channel::<ContextEvent>();
        let sink: octosense_app_peers::EventSink = std::sync::Arc::new(move |event| {
            let _ = tx.send(event);
        });
        self.context.call(
            ContextOp::TurnFrom { text: octos_prompt(selection, request, constraints), trigger: TurnTrigger::Person },
            sink,
        )?;
        let mut streamed = String::new();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(120)) {
                Ok(ContextEvent::Data(data)) => {
                    if let Some(text) = data.get("text").and_then(|t| t.as_str()) {
                        streamed = text.to_owned();
                    }
                }
                Ok(ContextEvent::Complete(result)) => {
                    return result.and_then(|value| {
                        reply_text(&value, &streamed).ok_or("Octos 回复中没有可用文本。".to_owned())
                    }).map(|mut out| {
                        reanchor_citations(selection, &mut out);
                        out
                    });
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err("Octos 响应超时（120 秒）。".into());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Octos 连接中断。".into());
                }
            }
        }
    }
    /// A turn can stream for many seconds: always the background path.
    fn needs_background(&self, _selection: &str) -> bool {
        true
    }
    fn engine_name(&self) -> String {
        "Octos".to_owned()
    }
    fn runs_off_thread(&self) -> bool {
        true
    }
    fn fork(&self) -> Box<dyn RewriteService> {
        Box::new(Self { context: self.context.clone() })
    }
}

/// The engine picked, from probe results — kept pure so the fallback chain
/// is unit-testable with injected outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineChoice {
    Octos,
    DirectLlm,
    Mock,
}
/// ① Octos host service → ② direct curl LLM from local config → ③ mock.
fn choose_engine(octos_ready: bool, llm: Option<&LlmConfig>) -> EngineChoice {
    if octos_ready {
        EngineChoice::Octos
    } else if llm.is_some() {
        EngineChoice::DirectLlm
    } else {
        EngineChoice::Mock
    }
}

/// Hands the locally configured provider key to the host exactly once: the
/// host writes it into its own kernel profile (mode 0600) and the app holds
/// no credential afterwards. Standalone-local builds only.
#[cfg(feature = "octos-local")]
fn host_key_once(llm: Option<&LlmConfig>) -> Option<()> {
    static HOSTED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    if crate::octos_service::is_hosted() {
        return None;
    }
    let config = llm?;
    HOSTED
        .get_or_init(|| {
            crate::octos_service::configure_local_provider(
                &config.family,
                &config.model,
                &config.base_url,
                &config.api_key,
            )
        })
        .as_ref()
        .ok()
        .copied()
}
#[cfg(not(feature = "octos-local"))]
fn host_key_once(_llm: Option<&LlmConfig>) -> Option<()> {
    None
}

/// Probes the Octos service layer: an already installed/running service
/// first; otherwise host the local llm config's key (once) and try again.
fn probe_octos(llm: Option<&LlmConfig>) -> Option<OctosRewriteService> {
    if crate::sliding_sync::current_user_id().is_none() {
        return None;
    }
    if crate::octos_service::service().is_err() {
        host_key_once(llm)?;
    }
    OctosRewriteService::open().ok()
}

/// Picks the rewrite engine: Octos host service → configured direct LLM →
/// offline mock. Every step falls back silently so a demo never breaks.
pub fn active_service() -> Box<dyn RewriteService> {
    let llm = LlmConfig::load();
    let octos = probe_octos(llm.as_ref());
    match choose_engine(octos.is_some(), llm.as_ref()) {
        EngineChoice::Octos => Box::new(octos.expect("probed")),
        EngineChoice::DirectLlm => Box::new(LlmRewriteService { config: llm.expect("probed") }),
        EngineChoice::Mock => Box::new(MockRewriteService),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_failure_is_an_error() {
        let service = MockRewriteService;
        assert!(service.rewrite("任何文本", "让它失败", &Constraints::default()).is_err());
    }

    #[test]
    fn concise_drops_fillers() {
        let service = MockRewriteService;
        let constraints = Constraints { concise: true, ..Default::default() };
        let out = service.rewrite("这其实基本上可以说是一个非常冗长的方案。", "写得更简洁", &constraints).unwrap();
        assert!(!out.contains("其实"));
        assert!(!out.contains("非常"));
        assert!(out.contains("冗长"));
    }

    #[test]
    fn formal_replaces_colloquialisms() {
        let service = MockRewriteService;
        let constraints = Constraints { formal: true, ..Default::default() };
        let out = service.rewrite("咱们搞定了很多东西，挺好的。", "更正式一些", &constraints).unwrap();
        assert!(out.contains("我们"));
        assert!(out.contains("完成"));
        assert!(out.contains("大量"));
        assert!(out.contains("良好"));
    }

    #[test]
    fn citations_survive_every_rule() {
        let service = MockRewriteService;
        let constraints = Constraints { concise: true, formal: true, keep_citations: true };
        let out = service.rewrite("其实非常多的证据[^1]支持这点[^2]。", "简洁且正式", &constraints).unwrap();
        assert!(out.contains("[^1]"));
        assert!(out.contains("[^2]"));
    }

    #[test]
    fn long_selections_prepare_in_background() {
        let service = MockRewriteService;
        let long: String = "长".repeat(501);
        assert!(service.needs_background(&long));
        assert!(!service.needs_background("短"));
    }

    // -- LLM config and wire format (no network) ----------------------------

    fn file_config() -> LlmConfig {
        LlmConfig {
            base_url: "https://file.example/v1".into(),
            api_key: "file-key".into(),
            model: "file-model".into(),
            family: "file-family".into(),
        }
    }

    #[test]
    fn env_wins_over_file_field_by_field() {
        let env = |name: &str| (name == ENV_KEY).then(|| "env-key".to_owned());
        let config = LlmConfig::resolve(env, Some(file_config())).unwrap();
        assert_eq!(config.api_key, "env-key");
        assert_eq!(config.base_url, "https://file.example/v1");
        assert_eq!(config.model, "file-model");
    }

    #[test]
    fn file_is_used_when_env_is_silent() {
        let config = LlmConfig::resolve(|_| None, Some(file_config())).unwrap();
        assert_eq!(config, file_config());
    }

    #[test]
    fn missing_or_partial_config_is_none() {
        assert!(LlmConfig::resolve(|_| None, None).is_none());
        // A key without a base url or model is not usable.
        let env = |name: &str| (name == ENV_KEY).then(|| "k".to_owned());
        assert!(LlmConfig::resolve(env, None).is_none());
        let mut file = file_config();
        file.model = "  ".into();
        assert!(LlmConfig::resolve(|_| None, Some(file)).is_none());
    }

    #[test]
    fn config_file_parses_flat_json() {
        let text = r#"{"base_url": "https://api.example.com/v1", "api_key": "k", "model": "gpt-x"}"#;
        let config = parse_config(text).unwrap();
        assert_eq!(config.base_url, "https://api.example.com/v1");
        assert_eq!(config.model, "gpt-x");
    }

    #[test]
    fn request_body_carries_constraints_and_escapes() {
        let config = LlmConfig { base_url: "https://x/v1".into(), api_key: "k".into(), model: "m\"odel".into(), family: "openai".into() };
        let constraints = Constraints { concise: true, formal: false, keep_citations: true };
        let body = build_request_body(&config, "证据[^1]支持。\n引用\"文献\"", "更简洁", &constraints);
        // Citation preservation is a hard rule in the system prompt.
        assert!(body.contains("保留所有 [^n]"));
        assert!(body.contains("更简洁"));
        // Newlines and quotes are escaped, not literal.
        assert!(body.contains("\\n"));
        assert!(body.contains("\\\"文献\\\""));
        assert!(body.contains("[^1]"));
        // The escaped model name round-trips through the extractor.
        assert_eq!(json_string_field(&body, "model").as_deref(), Some("m\"odel"));
        assert!(body.contains("\"temperature\":0.3"));
    }

    #[test]
    fn content_is_extracted_from_a_chat_response() {
        let response = r#"{"id":"chatcmpl-1","choices":[{"index":0,"message":{"role":"assistant","content":"改写后的「文字」\n保留[^1]。"},"finish_reason":"stop"}],"usage":{"total_tokens":12}}"#;
        let content = json_string_field(response, "content").unwrap();
        assert_eq!(content, "改写后的「文字」\n保留[^1]。");
    }

    #[test]
    fn escaped_content_round_trips() {
        let response = "{\"choices\":[{\"message\":{\"content\":\"a\\\"b\\\\c\\u4e2d\"}}]}";
        assert_eq!(json_string_field(response, "content").as_deref(), Some("a\"b\\c中"));
    }

    #[test]
    fn llm_always_runs_off_thread_in_the_background() {
        let service = LlmRewriteService {
            config: LlmConfig { base_url: "https://x/v1".into(), api_key: "k".into(), model: "gpt-x".into(), family: "openai".into() },
        };
        assert!(service.needs_background("短"));
        assert!(service.runs_off_thread());
        assert_eq!(service.engine_name(), "gpt-x");
    }

    // -- Engine fallback chain and the Octos turn channel (no service) -------

    #[test]
    fn engine_chain_prefers_octos_then_direct_llm_then_mock() {
        let config = file_config();
        assert_eq!(choose_engine(true, Some(&config)), EngineChoice::Octos);
        assert_eq!(choose_engine(true, None), EngineChoice::Octos);
        assert_eq!(choose_engine(false, Some(&config)), EngineChoice::DirectLlm);
        assert_eq!(choose_engine(false, None), EngineChoice::Mock);
    }

    #[test]
    fn family_defaults_to_openai_and_env_wins() {
        let mut file = file_config();
        file.family = String::new();
        assert_eq!(LlmConfig::resolve(|_| None, Some(file)).unwrap().family, "openai");
        let env = |name: &str| (name == ENV_FAMILY).then(|| "zhipu".to_owned());
        assert_eq!(LlmConfig::resolve(env, Some(file_config())).unwrap().family, "zhipu");
    }

    #[test]
    fn octos_prompt_leads_with_rules_and_caps_the_selection() {
        let constraints = Constraints { concise: true, formal: false, keep_citations: true };
        let prompt = octos_prompt("证据[^1]支持这一点。", "更简洁", &constraints);
        assert!(prompt.starts_with("你是一名严谨的中文写作助手"));
        assert!(prompt.contains("保留所有 [^n]"));
        assert!(prompt.contains("【选中文字】\n证据[^1]支持这一点。"));
        assert!(prompt.contains("【改写请求】\n更简洁"));
        // A huge selection is capped on a char boundary.
        let huge = "长".repeat(40 * 1024);
        let capped = octos_prompt(&huge, "请求", &Constraints::default());
        assert!(capped.len() < 40 * 1024);
    }

    #[test]
    fn reply_text_prefers_the_result_then_the_stream() {
        let result = serde_json::json!({"turn_id": "t1", "text": "改写后的文字。"});
        assert_eq!(reply_text(&result, "流式片段").as_deref(), Some("改写后的文字。"));
        let empty = serde_json::json!({"turn_id": "t1", "text": "  "});
        assert_eq!(reply_text(&empty, "流式片段").as_deref(), Some("流式片段"));
        assert_eq!(reply_text(&empty, ""), None);
    }
}
