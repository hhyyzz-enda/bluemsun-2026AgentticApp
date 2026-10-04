//! What an installed app declares about itself.
//!
//! The manifest is the ONLY thing an app may say about its own limits, and
//! saying it is not the same as getting it: every field is a request that
//! [`crate::policy`] resolves against the host's ceilings. Unknown fields are
//! refused rather than ignored, so a manifest written for a newer host does
//! not silently run with less containment than it asked for.
//!
//! A manifest that needs something this contract added after `1.0` names it
//! in [`AppManifest::requires`]; a host that does not know a listed feature
//! refuses the app ("needs a newer host"). See the crate's Stability section.
use crate::research::ResearchScope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The manifest schema this build understands. A bundle declaring anything
/// else is refused: an older host must not guess at a newer grammar.
pub const SCHEMA: u32 = 1;

/// The newest `schema_minor` this build knows: which additions to schema 1
/// it understands. `0` is the grammar of `1.0.0`. A manifest may carry a
/// higher `schema_minor` (it records what the manifest uses, and asks for
/// nothing by itself); what a host must understand is listed in
/// [`AppManifest::requires`].
pub const SCHEMA_MINOR: u32 = 0;

/// The features a manifest may list in [`AppManifest::requires`] and this
/// build honours. Empty in `1.0.0`: every feature added in `1.x` that
/// restricts or changes what an app gets is added here, with the field
/// that carries it, in the same release.
pub const KNOWN_FEATURES: &[&str] = &["palpo-admin-v1"];

/// Parse a manifest: [`AppManifest::parse`].
pub fn parse(json: &str) -> Result<AppManifest, String> {
    AppManifest::parse(json)
}

/// Every capability an app may request. The list is closed on purpose — a
/// capability that is not here cannot be granted, so adding one is a change
/// to this file and to the service that enforces it, together.
pub const KNOWN_CAPABILITIES: &[&str] = &[
    // Read and write inside the app's own storage jail.
    "storage",
    // Make requests, but only to the hosts in `network.hosts`.
    "net",
    // Raise a prompt the person answers (a permission ask, a confirmation).
    "prompt",
    // Read the shared ledger. Writing is always the app's own rows.
    "ledger.read",
    // Location, camera and clipboard reach the person's world; each is a
    // separate consent, never implied by another.
    "location",
    "camera",
    "clipboard",
    // Show pictures from any public https host, not just `network.hosts`:
    // a feed reader's thumbnails come from wherever its stories link.
    "images",
    // Open any public https page in the system WebView, which gets no way
    // back into the app: a reader for the stories it lists.
    "web",
    // Record sound with a camera video.
    "microphone",
    // Offer what it captures to the system photo library, where other apps
    // can see it; without this, captures stay in the app's own storage.
    "library",
    // Read and send mail through the host's mail service, from accounts the
    // person signs in to on the host's own sheet. The app never holds the
    // password or the connection.
    "mail",
    // See and arrange the assistant's LLM providers through the host's llm
    // service. Keys are typed, shown as a QR and scanned only on the host's
    // own sheets; the app sees masked status, never a key.
    "llm",
    // Read the host's news service: feeds and topic feeds it collects on a
    // schedule into the app's store, and the items' text. The app never
    // fetches arbitrary sites itself through it.
    "news",
    // Publish cards to the glance screen through the host's glance service:
    // L0 cards the shell checks, caps, rate-limits and expires, keyed to the
    // app itself. The app sees only its own cards and a card opens only it.
    "glance",
    // Make bounded one-shot model calls through the host's model service
    // (`model.complete`): the app names a model class ("fast" or "strong")
    // and a JSON Schema; the host picks the model from the person's own
    // providers, validates the reply against the schema and keeps a per-app
    // daily budget. No tools, memory or history; the app never sees the
    // provider, model id or key. The app's inputs go to the AI provider the
    // person configured. Not `llm`, which manages providers for os.* apps.
    "model",
    // Search through the system toolbox (`search`, `deep_research`), within
    // the manifest's `research` scope: languages, regions, domains, recency,
    // categories and results per search. The host runs the search and
    // checks every call against the scope; the app never fetches the sites
    // itself. The scope's schema is octos's `Scope` (see [`crate::research`]).
    "research",
    // Crawl a site through the system toolbox (`deep_crawl`): follow links
    // up to the scope's `max_depth` and read up to its `max_pages` a crawl,
    // inside its domain lists. More reach than `research`, which only reads
    // search results; neither implies the other.
    "crawl",
    // Host services reached by exact name (App Hub's `services`). Each is
    // a separate consent: a host adapter checks the exact name, the person's
    // per-instance grant and its own ceilings on every request. A prefix is
    // never a grant: `octos.` or `matrix.` alone is an unknown capability,
    // and history access does not imply starting a turn.
    // Palpo operations are individual grants; they never imply server roles.
    "palpo.intent.new",
    "palpo.session.open",
    "palpo.session.disconnect",
    "palpo.catalog.list",
    "palpo.projects.list",
    "palpo.projects.create",
    "palpo.requests.list",
    "palpo.requests.create",
    "palpo.fleets.list",
    "palpo.fleets.register",
    "palpo.fleets.install",
    "palpo.fleets.set_state",
    "palpo.fleets.migrate",
    "palpo.fleets.queue",
    "palpo.fleets.export",
    "palpo.fleets.connect",
    "palpo.agents.list",
    "palpo.agents.register",
    "palpo.agents.rename",
    "palpo.agents.retire",
    "palpo.activity.list",
    "palpo.accounts.list",
    "palpo.inbox.list",
    "palpo.inbox.submit",
    "palpo.inbox.get",
    "palpo.inbox.decide",
    "palpo.inbox.activate",
    "palpo.inbox.seen",
    "palpo.inbox.snooze",
    "matrix.account_info",
    "matrix.device",
    "matrix.dm_find",
    "matrix.dm_open",
    "matrix.event",
    "matrix.favorite",
    "matrix.ignored_users",
    "matrix.invite",
    "matrix.invite_respond",
    "matrix.invites",
    "matrix.join",
    "matrix.low_priority",
    "matrix.mark_unread",
    "matrix.older_messages",
    "matrix.permalink",
    "matrix.pin",
    "matrix.pinned_events",
    "matrix.power_levels",
    "matrix.profile",
    "matrix.react",
    "matrix.read_messages",
    "matrix.read_receipt",
    "matrix.read_receipts",
    "matrix.reply",
    "matrix.room_info",
    "matrix.room_members",
    "matrix.room_preview",
    "matrix.room_threads",
    "matrix.rooms_info",
    "matrix.rooms_list",
    "matrix.rooms_messages",
    "matrix.rooms_search",
    "matrix.rooms_send",
    "matrix.search_room",
    "matrix.search_rooms",
    "matrix.send_message",
    "matrix.space_info",
    "matrix.space_rooms",
    "matrix.spaces",
    "matrix.successor",
    "matrix.thread_replies",
    "matrix.thread_reply",
    "matrix.typing",
    "matrix.unread",
    "matrix.user_profile",
    "octos.session.open",
    "octos.session.history",
    "octos.turn.start",
    "octos.turn.interrupt",
];

/// The permission profiles an app's agent session may ask for. Full access is
/// absent by construction: it is an operator setting for machines they own,
/// and no manifest may name it (ADR 0002, "the rule at the boundary").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ProfileMode {
    /// No writes at all; every write asks.
    ReadOnly,
    /// Read and write inside the workspace; anything else asks.
    WorkspaceWrite,
    /// Read and write inside the workspace; anything else is refused
    /// outright rather than asked. The right default for an unattended app.
    WorkspaceWriteNeverAsk,
}

impl ProfileMode {
    /// The string the kernel's permission profile uses.
    pub fn as_kernel_mode(self) -> &'static str {
        match self {
            ProfileMode::ReadOnly => "read-only",
            ProfileMode::WorkspaceWrite => "workspace-write",
            ProfileMode::WorkspaceWriteNeverAsk => "workspace-write-never",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AppManifest {
    /// Must equal [`SCHEMA`].
    pub schema: u32,
    /// Stable identity. Also the name of the app's storage jail, so it is
    /// constrained to the characters a path component may hold.
    pub id: String,
    /// Opaque to the host, but pinned: a different version is a different
    /// bundle and must be admitted again.
    pub version: String,
    /// What a person calls it.
    pub name: String,
    pub integrity: Integrity,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub storage: Storage,
    #[serde(default)]
    pub compute: Compute,
    /// Absent means the app gets no agent at all, which is the default.
    #[serde(default)]
    pub agent: Option<AgentSpec>,
    /// The scope of the `research` and `crawl` capabilities: octos's
    /// `Scope`, exactly (see [`crate::research`]). Required with either
    /// capability and refused without both. Skipped when absent, so a
    /// manifest written before it existed signs exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<ResearchScope>,
    /// Features added in `1.x` that this manifest needs a host to honour,
    /// because they restrict or change what the app gets. A host that does
    /// not know one ([`KNOWN_FEATURES`]) refuses the app rather than run it
    /// under weaker rules than its author wrote. Skipped when empty, so a
    /// manifest written before it existed signs exactly as it did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Which additions to schema 1 the manifest uses ([`SCHEMA_MINOR`]).
    /// `0`, the default, is the `1.0.0` grammar. Skipped when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub schema_minor: u32,
    /// Fields of a newer-minor manifest this build did not know and
    /// ignored, with their values ([`AppManifest::ignored_fields`]). Never
    /// part of a manifest this build parses strictly.
    #[serde(skip)]
    ignored: Vec<crate::lenient::Ignored>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// What the bundle must hash to. The digest covers the bundle bytes as they
/// were signed; a signature, when we have one, signs this manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Integrity {
    /// Lowercase hex blake3 of the bundle.
    pub bundle_blake3: String,
    /// Detached signature over the canonical manifest bytes, if the host
    /// requires signing. Verified by a [`crate::verify::SignatureVerifier`].
    #[serde(default)]
    pub signature: Option<Signature>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Signature {
    /// Which key signed it, as the host knows the key.
    pub key_id: String,
    /// Lowercase hex signature bytes.
    pub value: String,
}

impl Signature {
    /// A signature by `key_id` with hex `value`, as a signing tool attaches
    /// it to `integrity.signature`.
    pub fn new(key_id: impl Into<String>, value: impl Into<String>) -> Self {
        Signature { key_id: key_id.into(), value: value.into() }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Network {
    /// The hosts this app may reach, exactly. No wildcards, no schemes, no
    /// paths: a host and nothing else, and the request is HTTPS by the time
    /// the service makes it.
    #[serde(default)]
    pub hosts: Vec<String>,
}

/// The `storage` block (OctoSense ADR 0004 §11). The host lays out the
/// app's folders from it (OctoSense's `app_storage::spec`, which reads the
/// same fields); App Hub admits it and clamps `max_bytes`. `external`
/// (a path outside the jail) is for reviewed native apps only, so a script
/// manifest that names it is refused like any unknown field. The fields
/// added after `max_bytes` are skipped when unset, so a manifest signed
/// before they existed signs exactly as it did.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Storage {
    /// Whole-jail ceiling the app asks for. Clamped to the host's maximum.
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// Data per account, one agent per account; `false` (the default) is one
    /// `device` folder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accounts: bool,
    /// What the app's agent may read from disk: its account's folder (the
    /// default) or nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_workspace: Option<AgentWorkspace>,
    /// Ceiling for the jail's `cache/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_max_bytes: Option<u64>,
}

/// `storage.agent_workspace`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AgentWorkspace {
    /// The account's folder.
    #[default]
    Account,
    /// No files: tools only.
    None,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Compute {
    /// Script instructions the app may run per session, cumulative — not the
    /// per-evaluation cap, which only stops one runaway expression.
    #[serde(default)]
    pub instruction_budget: Option<u64>,
    /// Ceiling for the isolate's heap.
    #[serde(default)]
    pub memory_bytes: Option<u64>,
}

/// The agent session an app asks for. Everything here is bounded by the same
/// manifest: its workspace is the app's jail, its network is the app's hosts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AgentSpec {
    pub profile: ProfileMode,
    /// The tools the session may call. Closed list, resolved against the
    /// host's own allowlist; shell and arbitrary file tools are never in it.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Model turns per request before the session stops and reports.
    #[serde(default)]
    pub max_iterations: Option<u32>,
    /// Tokens the session may spend per request.
    #[serde(default)]
    pub token_budget: Option<u64>,
    /// What the agent needs from a model, never which model (ADR 0002 §3).
    /// The host picks one from the person's providers that meets it.
    ///
    /// Every field added after the first release of schema 1 is skipped when
    /// it holds its default, so a manifest written before it existed
    /// serialises, and therefore signs, exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSpec>,
    /// The agent may run while the app is closed, woken by `triggers`. A
    /// request: the person grants or refuses it per app, and it requires at
    /// least one trigger.
    #[serde(default, skip_serializing_if = "is_false")]
    pub background: bool,
    /// What wakes the agent besides the person (ADR 0002 §2).
    #[serde(default, skip_serializing_if = "Triggers::is_empty")]
    pub triggers: Triggers,
    /// The bundle-relative path of the agent's instructions, conventionally
    /// `AGENT.md`. Named here so the file is declared, not
    /// merely present; the digest pins its bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// The skills the bundle ships under `skills/<name>/`. Installed into
    /// this app's peer workspace only, and data-only for a contained app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What a task needs from a model. Closed: a need that is not here cannot be
/// matched by any host, so adding one is a change here and in the host's
/// model selection, together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelNeed {
    /// Structured tool calls. An agent with any tools needs this.
    ToolCalling,
    /// Image input, for example to critique a rendered card.
    Vision,
    /// A long context window (the host decides what "long" means today).
    LongContext,
    /// A reasoning (thinking) model.
    Reasoning,
    /// Reliable JSON output against a schema.
    StructuredOutput,
    /// Reads and writes more than one language well.
    Multilingual,
}

/// Every [`ModelNeed`], as the manifest spells it.
pub const KNOWN_MODEL_NEEDS: &[&str] =
    &["tool_calling", "vision", "long_context", "reasoning", "structured_output", "multilingual"];

/// How capable (and costly) a model the task deserves. The host maps a tier
/// to the person's providers; policy may lower it (budget, battery).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ModelTier {
    Fast,
    #[default]
    Standard,
    Strong,
}

/// The model an app's agent needs. Never a provider or a model name: the
/// app cannot know which ones the person configured, and never sees keys.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ModelSpec {
    #[serde(default)]
    pub needs: Vec<ModelNeed>,
    #[serde(default)]
    pub tier: ModelTier,
    /// Only a model that runs on the device (or the person's own machine)
    /// may see this app's data. App-wide: a task cannot relax it.
    #[serde(default)]
    pub local_only: bool,
    /// Different requirements for named tasks, for example a fast model for
    /// `triage` and a strong one for `synthesis`. Task names are the app's
    /// own (`[a-z_]{1,32}`); `AGENT.md` says which task a step is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_task: BTreeMap<String, TaskModel>,
}

/// One task's model requirements. `local_only` is deliberately absent: it
/// holds for the whole app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TaskModel {
    #[serde(default)]
    pub needs: Vec<ModelNeed>,
    #[serde(default)]
    pub tier: ModelTier,
}

/// What wakes an app's agent without the person asking.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Triggers {
    /// Five-field cron expressions (minute hour day-of-month month
    /// day-of-week), in the device's local time: `"0 7 * * *"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedule: Vec<String>,
    /// Events from the app's own host service, in the app's namespace:
    /// `"news.items.new"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

impl Triggers {
    pub fn is_empty(&self) -> bool {
        self.schedule.is_empty() && self.events.is_empty()
    }
}

/// The namespace an app's tools and events live in: the last segment of its
/// id (`os.news` and `dev.example.news` are both `news`). Peers are per app,
/// so two apps with the same short id never share a tool registry.
pub fn short_id(app_id: &str) -> &str {
    app_id.rsplit('.').next().unwrap_or(app_id)
}

/// Names no script app may take, as its id or as its namespace
/// ([`short_id`]). A host keys an app's jail, storage folders, tool
/// declarations, executor and consent by its id, and its tools by its
/// namespace, so a store app named `terminal` (or `com.example.terminal`,
/// whose tools would be `terminal.*`) would stand in for the Terminal.
///
/// - The native apps OctoSense ships (its `native-apps.json`; OctoSense
///   checks this list against that file): `apphub`, `appcard`, `reference`,
///   `rinx`, `sheets`, `terminal`, and the Makepad apps it has made native
///   apps: `calculator`, `clock`, `notes`, `reminders`, `weather`, and the
///   desktop-only `browser` and `task`. Their tools (`notes.search`,
///   `calculator.eval`, `browser.tabs`, …) are named by OctoSense's grants,
///   so no store app may declare them. A Makepad app is reserved here when
///   OctoSense makes it native, not before.
/// - What the shell itself acts as, or owns tools under: `system` (the
///   system agent as a caller), `toolbox` and `workflow` (the system
///   toolbox), `dev` (developer mode's `dev.run`), `agents`
///   (`agents.list`), `octos`, `shell`, and `card` and `os`, the peer and
///   system-app prefixes.
///
/// System apps' own namespaces (`os.news` is `news`) are not in it: a store
/// app may share one, and a host resolves another app's tool by its owner,
/// not by its name.
pub const RESERVED_NAMES: &[&str] = &[
    "agents", "apphub", "appcard", "browser", "calculator", "card", "clock", "dev", "notes", "octos", "os", "reference",
    "reminders", "rinx", "sheets", "shell", "system", "task", "terminal", "toolbox", "weather", "workflow",
];

/// Refuse an app id that is, or whose namespace is, a [`RESERVED_NAMES`]
/// entry. Every script app passes through it: [`crate::policy::resolve`],
/// App Hub's agent review (`AgentBundle::load`) and the store's gate.
pub fn check_reserved_id(app_id: &str) -> Result<(), String> {
    let namespace = short_id(app_id);
    if RESERVED_NAMES.contains(&app_id) {
        return Err(format!("app id {app_id:?} is reserved: it names a native app or the host itself"));
    }
    if RESERVED_NAMES.contains(&namespace) {
        return Err(format!(
            "app id {app_id:?} ends in {namespace:?}, which is reserved: its tools would be {namespace}.*, a native app's or the host's"
        ));
    }
    Ok(())
}

impl AppManifest {
    /// Parse a manifest.
    ///
    /// - `schema` must be [`SCHEMA`], and every `requires` entry must be in
    ///   [`KNOWN_FEATURES`], at every `schema_minor`.
    /// - A manifest whose `schema_minor` is at most [`SCHEMA_MINOR`] is read
    ///   strictly: an unknown field, at any level, refuses it.
    /// - A manifest written for a newer `1.x` (`schema_minor` above
    ///   [`SCHEMA_MINOR`]) is read with its unknown fields ignored, at any
    ///   level: by the growth rule they are optional, because a field that
    ///   restricts or changes what the app gets must be named in `requires`.
    ///   The ignored fields are reported by [`AppManifest::ignored_fields`].
    pub fn parse(json: &str) -> Result<Self, String> {
        let manifest = match serde_json::from_str::<AppManifest>(json) {
            Ok(manifest) => manifest,
            Err(strict) => Self::parse_newer(json).unwrap_or_else(|| {
                // A research scope in the toolbox's old shape gets the fields
                // to rename, not only serde's "unknown field".
                Err(serde_json::from_str::<serde_json::Value>(json)
                    .ok()
                    .and_then(|v| v.get("research").and_then(crate::research::old_shape_advice))
                    .unwrap_or_else(|| format!("manifest is not valid: {strict}")))
            })?,
        };
        if manifest.schema != SCHEMA {
            return Err(format!("manifest schema {} is not {}", manifest.schema, SCHEMA));
        }
        manifest.check_requires()?;
        Ok(manifest)
    }

    /// The lenient read of a manifest written for a newer `1.x`, or `None`
    /// when the manifest is not one (it is then refused as the strict read
    /// refused it).
    fn parse_newer(json: &str) -> Option<Result<Self, String>> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let minor = value.get("schema_minor")?.as_u64()?;
        if value.get("schema")?.as_u64()? != u64::from(SCHEMA) || minor <= u64::from(SCHEMA_MINOR) {
            return None;
        }
        // Required features first: a newer field this build would ignore
        // must never be one the author marked as required.
        if let Some(requires) = value.get("requires").and_then(|r| r.as_array()) {
            let id = value.get("id").and_then(|i| i.as_str()).unwrap_or("?");
            let unknown: Vec<&str> =
                requires.iter().filter_map(|f| f.as_str()).filter(|f| !KNOWN_FEATURES.contains(f)).collect();
            if !unknown.is_empty() {
                return Some(Err(format!("app {id} needs a newer host: {}", unknown.join(", "))));
            }
        }
        // A scope in the toolbox's old shape is a mistake, not a newer
        // field: ignoring `languages` would widen the grant.
        if let Some(advice) = value.get("research").and_then(crate::research::old_shape_advice) {
            return Some(Err(advice));
        }
        Some(
            crate::lenient::from_value::<AppManifest>(value)
                .map(|(mut manifest, ignored)| {
                    manifest.ignored = ignored;
                    manifest
                })
                .map_err(|e| format!("manifest is not valid: {e}")),
        )
    }

    /// The fields of a manifest written for a newer `1.x` that this build
    /// does not know and ignored, as dotted paths (`network.proxy`,
    /// `agent.model.per_task.triage.budget`), for a host to log. Always
    /// empty for a manifest at or below [`SCHEMA_MINOR`], which is refused
    /// instead. Sorted.
    pub fn ignored_fields(&self) -> Vec<String> {
        let mut fields: Vec<String> = self.ignored.iter().map(|(path, _)| path.join(".")).collect();
        fields.sort();
        fields
    }

    /// Refuse a manifest that requires a feature this build does not know:
    /// running it would give the app less containment, or something other,
    /// than its author declared.
    pub fn check_requires(&self) -> Result<(), String> {
        let unknown: Vec<&str> =
            self.requires.iter().map(String::as_str).filter(|feature| !KNOWN_FEATURES.contains(feature)).collect();
        if unknown.is_empty() {
            return Ok(());
        }
        Err(format!("app {} needs a newer host: {}", self.id, unknown.join(", ")))
    }

    /// The research scope as the host grants it (normalised), for the words
    /// a store shows. A scope the host would refuse is shown as written; an
    /// absent one as `{}`.
    pub fn shown_research_scope(&self) -> ResearchScope {
        let scope = self.research.clone().unwrap_or_default();
        scope.validated().unwrap_or(scope)
    }

    /// The bytes a signature covers: the manifest without its own signature,
    /// serialised canonically, so the same manifest always signs the same way.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bare = self.clone();
        bare.integrity.signature = None;
        let mut value = serde_json::to_value(&bare).map_err(|e| e.to_string())?;
        // A newer manifest was signed with the fields this build ignored.
        crate::lenient::restore(&mut value, &self.ignored);
        Ok(canonical(&value).into_bytes())
    }
}

/// Canonical JSON: object keys sorted, no insignificant whitespace. Enough
/// for a stable signing input; it is not a general JCS implementation.
fn canonical(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", serde_json::Value::String((*k).clone()), canonical(&map[*k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        serde_json::Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}
