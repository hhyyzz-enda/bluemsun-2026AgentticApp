//! Scoped sessions live only for the lifetime of this panel. No capabilities are persisted.
use makepad_widgets::*;
use serde_json::Value;
use futures_util::future::{AbortHandle, Abortable};
use tokio::sync::mpsc;
use super::{
    backend,
    protocol::{self, Config},
};

#[derive(Clone, Debug)]
pub enum AgentOpsAction {
    Open,
    Close,
}
#[derive(Clone)]
struct Update {
    run: String,
    owner: ruma::OwnedUserId,
    result: Result<Projection, String>,
}
// Projections contain capabilities. Never let instrumentation/debug output print them.
impl std::fmt::Debug for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AgentOperationsUpdate [redacted]")
    }
}
#[derive(Clone)]
enum Projection {
    Snapshot(Value),
    Inspection(Value),
}
enum Command {
    Refresh,
    Apply {
        action: Value,
        inspection: Option<Value>,
        resolution: Option<String>,
        note: String,
        recovery: String,
        request_id: String,
    },
}
#[derive(Clone, Default)]
struct Row {
    text: String,
    action: Option<Value>,
}
fn action_label(action: &Value) -> &'static str {
    match action["kind"].as_str() {
        Some("cancel_dispatch") => "Cancel dispatch",
        Some("mark_resource_inspected") => "Mark workspace inspected",
        Some("begin_outcome_inspection") => "Inspect outcome",
        Some("resolve_outcome") => "Resolve outcome",
        _ => "Unavailable",
    }
}
fn rows(snapshot: &Value) -> Vec<Row> {
    let mut rows = Vec::new();
    for (section, title) in [
        ("attention", "Needs attention"),
        ("tasks", "Tasks"),
        ("queue", "Queue"),
        ("worktrees", "Workspaces"),
    ] {
        rows.push(Row {
            text: crate::i18n::tr(title).into(),
            action: None,
        });
        for item in snapshot[section].as_array().into_iter().flatten() {
            // Render explicit public fields, never the raw projection/capability JSON.
            let text = [
                "title",
                "summary",
                "label",
                "task_id",
                "dispatch_id",
                "resource_id",
                "agent",
                "state",
                "dispatch_state",
                "waiting_on",
                "branch",
            ]
            .iter()
            .filter_map(|k| item[*k].as_str())
            .map(|s| s.chars().take(600).collect::<String>())
            .collect::<Vec<_>>()
            .join(" · ");
            rows.push(Row { text, action: None });
            for action in item["available_actions"].as_array().into_iter().flatten() {
                if action["kind"] != "resolve_outcome" {
                    rows.push(Row {
                        text: String::new(),
                        action: Some(action.clone()),
                    });
                }
            }
        }
    }
    rows
}
async fn worker(
    config: Config,
    mut commands: mpsc::UnboundedReceiver<Command>,
    run: String,
    owner: ruma::OwnedUserId,
) {
    let emit = |result| {
        Cx::post_action(Update {
            run: run.clone(),
            owner: owner.clone(),
            result,
        })
    };
    let result: anyhow::Result<()> = async {
        let client =
            crate::sliding_sync::get_client().ok_or_else(|| anyhow::anyhow!("Sign in first"))?;
        let (transport, mut session) = backend::connect(&client, config).await?;
        backend::guard(&client, &owner, &session.config).await?;
        emit(Ok(Projection::Snapshot(session.snapshot.clone().unwrap())));
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            let command = tokio::select! {
                command = commands.recv() => match command { Some(c) => Some(c), None => break },
                _ = interval.tick() => None,
            };
            backend::guard(&client, &owner, &session.config).await?;
            match command {
                Some(Command::Apply {
                    action,
                    inspection,
                    resolution,
                    note,
                    recovery,
                    request_id,
                }) => {
                    let (path, body) = session.command(
                        &action,
                        inspection.as_ref(),
                        resolution.as_deref(),
                        &note,
                        &recovery,
                        &request_id,
                    )?;
                    let response = transport
                        .request(&session, &path, Some(&body))
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("Missing command response"))?;
                    backend::guard(&client, &owner, &session.config).await?;
                    if action["kind"] == "begin_outcome_inspection" {
                        session.validate_inspection(&response, &action)?;
                        // Beginning inspection advances the backend entity version and sequence.
                        transport.refresh(&mut session).await?;
                        session.validate_inspection(&response, &action)?;
                        backend::guard(&client, &owner, &session.config).await?;
                        emit(Ok(Projection::Snapshot(session.snapshot.clone().unwrap())));
                        emit(Ok(Projection::Inspection(response)));
                    } else {
                        anyhow::ensure!(
                            response["schema"] == super::SCHEMA,
                            "Unsupported command response"
                        );
                        transport.refresh(&mut session).await?;
                        backend::guard(&client, &owner, &session.config).await?;
                        emit(Ok(Projection::Snapshot(session.snapshot.clone().unwrap())));
                    }
                }
                command => {
                    if command.is_some() || transport.invalidated(&session).await? {
                        transport.refresh(&mut session).await?;
                        backend::guard(&client, &owner, &session.config).await?;
                        emit(Ok(Projection::Snapshot(session.snapshot.clone().unwrap())));
                    }
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        emit(Err(error.to_string()));
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let Text = Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
    let Button = RobrixNeutralIconButton {width: Fill height: 40 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
    let Input = TextInput {width: Fill height: 38 draw_text +: {text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
    mod.widgets.AgentOpsPanel = #(AgentOpsPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down padding: Inset{top: 32 left: 16 right: 16 bottom: 12} spacing: 8
        draw_bg.color: mod.widgets.RINX_PAGE
        View {width: Fill height: 40 align: Align{y: 0.5}
            close := Button {width: 65 text: #(crate::i18n::tr("Back")) i18n_text: "Back"}
            Text {text: #(crate::i18n::tr("Agent Operations")) i18n_text: "Agent Operations"}
        }
        status := Text {}
        setup := ScrollYView {width: Fill height: Fill flow: Down spacing: 6
            Text {text: #(crate::i18n::tr("Connect an enrolled Matrix device to your local Hagency backend. Enter the server fingerprint supplied by its operator.")) i18n_text: "Connect an enrolled Matrix device to your local Hagency backend. Enter the server fingerprint supplied by its operator."}
            device := Text {}
            agent := Input {empty_text: #(crate::i18n::tr("Agent name")) i18n_empty_text: "Agent name"}
            project := Input {empty_text: #(crate::i18n::tr("Project room ID")) i18n_empty_text: "Project room ID"}
            owner_room := Input {empty_text: #(crate::i18n::tr("Owner approval room ID")) i18n_empty_text: "Owner approval room ID"}
            bridge := Input {empty_text: #(crate::i18n::tr("Bridge Matrix user ID")) i18n_empty_text: "Bridge Matrix user ID"}
            endpoint := Input {text: "http://127.0.0.1:8090"}
            fingerprint := Input {empty_text: #(crate::i18n::tr("Pinned server fingerprint (sha256:...)")) i18n_empty_text: "Pinned server fingerprint (sha256:...)"}
            connect := Button {text: #(crate::i18n::tr("Connect")) i18n_text: "Connect"}
        }
        connected := View {width: Fill height: Fill flow: Down spacing: 8 visible: false
            View {width: Fill height: 40
                refresh := Button {text: #(crate::i18n::tr("Refresh")) i18n_text: "Refresh"}
                disconnect := Button {text: #(crate::i18n::tr("Disconnect")) i18n_text: "Disconnect"}
            }
            projection := PortalList {width: Fill height: Fill
                Row := View {width: Fill height: Fit flow: Down padding: 5
                    summary := Text {}
                    operation := Button {}
                }
            }
        }
        confirmation := ScrollYView {width: Fill height: Fill flow: Down spacing: 8 visible: false
            action_title := Text {}
            Text {text: #(crate::i18n::tr("Review this action before sending it to Hagency.")) i18n_text: "Review this action before sending it to Hagency."}
            inspection_details := Text {}
            resolutions := View {width: Fill height: Fit flow: Down visible: false spacing: 6
                continue_work := Button {text: #(crate::i18n::tr("Continue with instructions")) i18n_text: "Continue with instructions"}
                accept := Button {text: #(crate::i18n::tr("Accept as completed")) i18n_text: "Accept as completed"}
                blocked := Button {text: #(crate::i18n::tr("Keep blocked")) i18n_text: "Keep blocked"}
                chosen := Text {}
                note := Input {empty_text: #(crate::i18n::tr("Inspection note (required)")) i18n_empty_text: "Inspection note (required)"}
                recovery := Input {empty_text: #(crate::i18n::tr("Recovery instructions")) i18n_empty_text: "Recovery instructions"}
            }
            confirm := Button {text: #(crate::i18n::tr("Confirm action")) i18n_text: "Confirm action"}
            cancel := Button {text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"}
        }
    }
}
#[derive(Script, ScriptHook, Widget)]
pub struct AgentOpsPanel {
    #[deref]
    view: View,
    #[rust]
    abort: Option<AbortHandle>,
    #[rust]
    commands: Option<mpsc::UnboundedSender<Command>>,
    #[rust]
    owner: Option<ruma::OwnedUserId>,
    #[rust]
    run: String,
    #[rust]
    rows: Vec<Row>,
    #[rust]
    selected: Option<Value>,
    #[rust]
    inspection: Option<Value>,
    #[rust]
    resolution: Option<String>,
    #[rust]
    busy: bool,
    #[rust]
    connected: bool,
    #[rust]
    status: String,
    #[rust]
    error: String,
}
impl Drop for AgentOpsPanel {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}
impl AgentOpsPanel {
    fn reset(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
        self.commands = None;
        self.run.clear();
        self.owner = None;
        self.rows.clear();
        self.selected = None;
        self.inspection = None;
        self.resolution = None;
        self.busy = false;
        self.connected = false;
    }
    fn clear_selection(&mut self, cx: &mut Cx) {
        self.selected = None;
        self.inspection = None;
        self.resolution = None;
        self.view.text_input(cx, ids!(note)).set_text(cx, "");
        self.view.text_input(cx, ids!(recovery)).set_text(cx, "");
    }
    fn config(&self, cx: &mut Cx) -> anyhow::Result<Config> {
        let text = |id| self.view.text_input(cx, id).text().trim().to_owned();
        let config = Config {
            agent: text(ids!(agent)),
            project_room: text(ids!(project)).try_into()?,
            owner_room: text(ids!(owner_room)).try_into()?,
            bridge: text(ids!(bridge)).try_into()?,
            owner_agent: None,
            origin: protocol::origin(&text(ids!(endpoint)))?,
            fingerprint: text(ids!(fingerprint)),
        };
        config.validate()?;
        Ok(config)
    }
    fn start(&mut self, cx: &mut Cx) -> anyhow::Result<()> {
        anyhow::ensure!(
            super::available(),
            "Agent Operations awaits a released backend contract"
        );
        let config = self.config(cx)?;
        let owner = crate::sliding_sync::current_user_id()
            .ok_or_else(|| anyhow::anyhow!("Sign in first"))?;
        self.reset();
        self.owner = Some(owner.clone());
        self.run = protocol::nonce();
        self.busy = true;
        self.status = "Waiting for an encrypted session grant…".into();
        self.error.clear();
        let (sender, receiver) = mpsc::unbounded_channel();
        self.commands = Some(sender);
        let (abort, registration) = AbortHandle::new_pair();
        self.abort = Some(abort);
        let run = self.run.clone();
        crate::sliding_sync::spawn_async_task(async move {
            let _ = Abortable::new(worker(config, receiver, run, owner), registration).await;
        });
        Ok(())
    }
    fn send(&mut self, command: Command) {
        if self.busy {
            return;
        }
        if self
            .commands
            .as_ref()
            .is_some_and(|sender| sender.send(command).is_ok())
        {
            self.busy = true;
            self.status = "Waiting for Hagency…".into();
            self.error.clear();
        }
    }
    fn sync(&mut self, cx: &mut Cx) {
        let available = super::available();
        let device = crate::sliding_sync::get_client()
            .and_then(|client| client.device_id().map(ToString::to_string))
            .unwrap_or_default();
        self.view.label(cx, ids!(device)).set_text(
            cx,
            &crate::i18n::format("Matrix device: {0}", &[("0", device)]),
        );
        self.view.label(cx, ids!(status)).set_text(
            cx,
            &if self.error.is_empty() {
                crate::i18n::tr(&self.status).to_string()
            } else {
                format!(
                    "{}\n{}",
                    crate::i18n::tr(&self.status),
                    crate::i18n::tr(&self.error)
                )
            },
        );
        self.view
            .view(cx, ids!(setup))
            .set_visible(cx, available && !self.connected && !self.busy);
        self.view
            .view(cx, ids!(connected))
            .set_visible(cx, self.connected && self.selected.is_none());
        self.view
            .view(cx, ids!(confirmation))
            .set_visible(cx, self.connected && self.selected.is_some());
        self.view
            .view(cx, ids!(resolutions))
            .set_visible(cx, self.inspection.is_some());
        let title = self
            .selected
            .as_ref()
            .map(|a| {
                format!(
                    "{}\n{}",
                    crate::i18n::tr(action_label(a)),
                    a["target"]["entity_id"].as_str().unwrap_or_default()
                )
            })
            .unwrap_or_default();
        self.view.label(cx, ids!(action_title)).set_text(cx, &title);
        let detail = self
            .inspection
            .as_ref()
            .map(|i| {
                [
                    i["terminal_reason"].as_str(),
                    i["resource"]["label"].as_str(),
                    i["resource"]["branch"].as_str(),
                    i["resource"]["dirty_reason"].as_str(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n")
            })
            .unwrap_or_default();
        self.view
            .label(cx, ids!(inspection_details))
            .set_text(cx, &detail);
        for (id, kind) in [
            (ids!(continue_work), "continue"),
            (ids!(accept), "accept_completed"),
            (ids!(blocked), "keep_blocked"),
        ] {
            let allowed = self.selected.as_ref().is_some_and(|a| {
                a["allowed_resolutions"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == kind))
            });
            self.view
                .button(cx, id)
                .set_enabled(cx, allowed && !self.busy);
        }
        let chosen = match self.resolution.as_deref() {
            Some("continue") => "Continue with instructions",
            Some("accept_completed") => "Accept as completed",
            Some("keep_blocked") => "Keep blocked",
            _ => "Choose a resolution",
        };
        self.view
            .label(cx, ids!(chosen))
            .set_text(cx, crate::i18n::tr(chosen));
        self.view
            .text_input(cx, ids!(recovery))
            .set_visible(cx, self.resolution.as_deref() == Some("continue"));
        for id in [ids!(confirm), ids!(cancel), ids!(refresh)] {
            self.view.button(cx, id).set_enabled(cx, !self.busy);
        }
    }
}
impl Widget for AgentOpsPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.owner.is_some() && self.owner != crate::sliding_sync::current_user_id() {
            self.reset();
            self.status = "Account changed; reconnect".into();
            self.error.clear();
        }
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else {
            return;
        };
        for action in actions {
            if let Some(update) = action.downcast_ref::<Update>() {
                if update.run != self.run || Some(&update.owner) != self.owner.as_ref() {
                    continue;
                }
                self.busy = false;
                self.clear_selection(cx);
                match &update.result {
                    Ok(Projection::Snapshot(value)) => {
                        self.rows = rows(value);
                        self.connected = true;
                        self.status = "Connected to scoped Agent Operations".into();
                        self.error.clear();
                    }
                    Ok(Projection::Inspection(value)) => {
                        self.selected = Some(value["resolution_action"].clone());
                        self.inspection = Some(value.clone());
                        self.status = "Inspect the workspace before resolving the outcome".into();
                        self.error.clear();
                    }
                    Err(error) => {
                        self.reset();
                        self.status = "Session closed. Reconnect to check the latest state.".into();
                        self.error = error.clone();
                    }
                }
            }
        }
        if self.view.button(cx, ids!(close)).clicked(actions) {
            cx.action(AgentOpsAction::Close);
        }
        if self.view.button(cx, ids!(disconnect)).clicked(actions) {
            self.reset();
            self.clear_selection(cx);
            self.status = "Disconnected".into();
            self.error.clear();
        }
        if self.view.button(cx, ids!(connect)).clicked(actions) {
            if let Err(error) = self.start(cx) {
                self.status = "Unable to connect".into();
                self.error = error.to_string();
            }
        }
        if self.view.button(cx, ids!(refresh)).clicked(actions) {
            self.send(Command::Refresh);
        }
        if !self.busy {
            if self.view.button(cx, ids!(cancel)).clicked(actions) {
                self.clear_selection(cx);
            }
            for (index, row) in self
                .view
                .portal_list(cx, ids!(projection))
                .items_with_actions(actions)
            {
                if row.button(cx, ids!(operation)).clicked(actions) {
                    self.clear_selection(cx);
                    self.selected = self.rows.get(index).and_then(|r| r.action.clone());
                }
            }
            for (id, kind) in [
                (ids!(continue_work), "continue"),
                (ids!(accept), "accept_completed"),
                (ids!(blocked), "keep_blocked"),
            ] {
                if self.view.button(cx, id).clicked(actions) {
                    self.resolution = Some(kind.into());
                    self.view.text_input(cx, ids!(recovery)).set_text(cx, "");
                }
            }
            if self.view.button(cx, ids!(confirm)).clicked(actions) {
                if let Some(action) = self.selected.clone() {
                    if self.inspection.is_some()
                        && (self.resolution.is_none()
                            || self
                                .view
                                .text_input(cx, ids!(note))
                                .text()
                                .trim()
                                .is_empty()
                            || (self.resolution.as_deref() == Some("continue")
                                && self
                                    .view
                                    .text_input(cx, ids!(recovery))
                                    .text()
                                    .trim()
                                    .is_empty()))
                    {
                        self.status = "Choose a resolution, add an inspection note, and provide instructions for Continue.".into();
                    } else {
                        self.send(Command::Apply {
                            action,
                            inspection: self.inspection.clone(),
                            resolution: self.resolution.clone(),
                            note: self.view.text_input(cx, ids!(note)).text(),
                            recovery: self.view.text_input(cx, ids!(recovery)).text(),
                            request_id: protocol::nonce(),
                        });
                    }
                }
            }
        }
        self.sync(cx);
        self.view.redraw(cx);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.sync(cx);
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.rows.len());
                while let Some(index) = list.next_visible_item(cx) {
                    if let Some(value) = self.rows.get(index) {
                        let row = list.item(cx, index, id!(Row));
                        row.label(cx, ids!(summary)).set_text(cx, &value.text);
                        row.label(cx, ids!(summary))
                            .set_visible(cx, !value.text.is_empty());
                        row.button(cx, ids!(operation))
                            .set_visible(cx, value.action.is_some());
                        if let Some(action) = &value.action {
                            row.button(cx, ids!(operation))
                                .set_text(cx, crate::i18n::tr(action_label(action)));
                            row.button(cx, ids!(operation)).set_enabled(
                                cx,
                                !self.busy
                                    && action["expires_at_unix_ms"].as_u64().unwrap_or(0)
                                        > protocol::now(),
                            );
                        }
                        row.draw_all(cx, scope);
                    }
                }
            }
        }
        DrawStep::done()
    }
}
impl AgentOpsPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &AgentOpsAction) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.reset();
        inner.clear_selection(cx);
        inner.error.clear();
        match action {
            AgentOpsAction::Open => {
                inner.status = if super::available() {"Development Agent Operations: enrolled devices and a pinned local backend are required."} else {
                    "Agent Operations awaits a released backend contract. Agent chat, approvals and workflow commands are available."
                }.into();
                modal.open(cx);
            }
            AgentOpsAction::Close => modal.close(cx),
        }
        inner.sync(cx);
    }
}
