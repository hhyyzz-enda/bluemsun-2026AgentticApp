use makepad_widgets::*;
use matrix_sdk::{
    RoomState,
    ruma::{
        OwnedUserId, OwnedRoomId, UserId,
        profile::{DisplayName, AvatarUrl},
    },
};
use crate::{
    app::AppState,
    matrix_context,
    utils::RoomNameId,
    sliding_sync::{get_client, current_user_id, spawn_async_task},
};
use super::model::*;

#[derive(Clone, Debug)]
pub enum AgentAccessAction {
    Open,
    Close,
    Changed,
}
#[derive(Debug)]
struct Completed {
    owner: OwnedUserId,
    request: u64,
    result: Result<Outcome, String>,
}
#[derive(Debug)]
enum Outcome {
    Profile(OwnedUserId, AgentEntry),
    Health(String),
    Command(RoomNameId),
}
#[derive(Clone, Debug)]
struct Unbind {
    owner: OwnedUserId,
    user: OwnedUserId,
}
#[derive(Clone, Debug)]
struct DeleteBot {
    owner: OwnedUserId,
    target: OwnedUserId,
    botfather: OwnedUserId,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let Text = Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
    let Action = RobrixNeutralIconButton {height: 40 icon_walk: Walk{width: 0 height: 0} spacing: 0}
    let Input = TextInput {width: Fill height: 40}
    mod.widgets.AgentAccessPanel = #(AgentAccessPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down padding: Inset{top: 32 left: 16 right: 16 bottom: 12} spacing: 8 draw_bg.color: mod.widgets.RINX_PAGE
        View {width: Fill height: 40 spacing: 8 align: Align{y: 0.5}
            close := Action {text: #(crate::i18n::tr("Back")) i18n_text: "Back"}
            Text {text: #(crate::i18n::tr("Agent Access")) i18n_text: "Agent Access"}
        }
        status := Text {}
        ScrollYView {width: Fill height: Fill flow: Down spacing: 10
            Text {text: #(crate::i18n::tr("Registered agents")) i18n_text: "Registered agents"}
            agents := DropDown {width: Fill labels: ["Select an agent"]}
            agent_id := Input {empty_text: "@agent:example.org"}
            framework := DropDown {width: Fill labels: ["Agent" "Octos AppService" "Octos Direct" "Hermes" "OpenClaw"]}
            Text {text: #(crate::i18n::tr("New chats with registered agents are unencrypted. Existing chats keep their encryption.")) i18n_text: "New chats with registered agents are unencrypted. Existing chats keep their encryption."}
            register := Action {width: Fill text: #(crate::i18n::tr("Look up and register / re-check")) i18n_text: "Look up and register / re-check"}
            View {width: Fill height: 40 spacing: 8
                open := Action {width: Fill text: #(crate::i18n::tr("Open chat")) i18n_text: "Open chat"}
                unbind := Action {width: Fill text: #(crate::i18n::tr("Unbind agent")) i18n_text: "Unbind agent"}
            }
            Hr {}
            Text {text: #(crate::i18n::tr("Matrix AppService")) i18n_text: "Matrix AppService"}
            enabled := CheckBox {text: #(crate::i18n::tr("Enable AppService controls")) i18n_text: "Enable AppService controls"}
            botfather := Input {empty_text: #(crate::i18n::tr("BotFather Matrix ID")) i18n_empty_text: "BotFather Matrix ID"}
            endpoint := Input {empty_text: "http://127.0.0.1:8010"}
            View {width: Fill height: 40 spacing: 8
                save := Action {width: Fill text: #(crate::i18n::tr("Save")) i18n_text: "Save"}
                health := Action {width: Fill text: #(crate::i18n::tr("Check service")) i18n_text: "Check service"}
            }
            rooms := DropDown {width: Fill labels: ["Select a room"]}
            binding_id := Input {empty_text: #(crate::i18n::tr("Bot Matrix ID to bind")) i18n_empty_text: "Bot Matrix ID to bind"}
            remark := Input {empty_text: #(crate::i18n::tr("Binding remark")) i18n_empty_text: "Binding remark"}
            bindings := Text {}
            View {width: Fill height: 40 spacing: 8
                bind_room := Action {width: Fill text: #(crate::i18n::tr("Bind room")) i18n_text: "Bind room"}
                unbind_room := Action {width: Fill text: #(crate::i18n::tr("Unbind room")) i18n_text: "Unbind room"}
            }
            Text {text: #(crate::i18n::tr("Bot management commands go to your saved BotFather. Replies appear in its chat.")) i18n_text: "Bot management commands go to your saved BotFather. Replies appear in its chat."}
            View {width: Fill height: 40 spacing: 8
                list := Action {width: Fill text: #(crate::i18n::tr("List bots")) i18n_text: "List bots"}
                help := Action {width: Fill text: #(crate::i18n::tr("Help")) i18n_text: "Help"}
            }
            bot_username := Input {empty_text: #(crate::i18n::tr("Bot username")) i18n_empty_text: "Bot username"}
            bot_name := Input {empty_text: #(crate::i18n::tr("Bot display name")) i18n_empty_text: "Bot display name"}
            bot_prompt := Input {empty_text: #(crate::i18n::tr("System prompt (optional)")) i18n_empty_text: "System prompt (optional)"}
            create := Action {width: Fill text: #(crate::i18n::tr("Create bot")) i18n_text: "Create bot"}
            delete_id := Input {empty_text: #(crate::i18n::tr("Bot Matrix ID to delete")) i18n_empty_text: "Bot Matrix ID to delete"}
            delete := Action {width: Fill text: #(crate::i18n::tr("Delete bot…")) i18n_text: "Delete bot…"}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct AgentAccessPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    active: bool,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    request: u64,
    #[rust]
    busy: bool,
    #[rust]
    agents: Vec<OwnedUserId>,
    #[rust]
    rooms: Vec<RoomNameId>,
}
fn nonce() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
impl AgentAccessPanel {
    fn status(&self, cx: &mut Cx, text: &str) {
        self.view
            .label(cx, ids!(status))
            .set_text(cx, crate::i18n::tr(text));
    }
    fn refresh(&mut self, cx: &mut Cx, settings: &AgentAccessSettings) {
        self.agents = settings.agent_registry.agent_user_ids();
        self.view.drop_down(cx, ids!(agents)).set_labels(
            cx,
            std::iter::once(crate::i18n::tr("Select an agent").into())
                .chain(settings.agent_registry.agents().map(|(id, e)| {
                    format!(
                        "{} · {} ({})",
                        e.display_name.as_deref().unwrap_or(id.as_str()),
                        e.framework.label(),
                        id
                    )
                }))
                .collect(),
        );
        self.view
            .drop_down(cx, ids!(agents))
            .set_selected_item(cx, 0);
        self.view.check_box(cx, ids!(enabled)).set_active(
            cx,
            settings.bot_settings.enabled,
            Animate::No,
        );
        self.view
            .text_input(cx, ids!(botfather))
            .set_text(cx, &settings.bot_settings.botfather_user_id);
        self.view
            .text_input(cx, ids!(endpoint))
            .set_text(cx, settings.bot_settings.resolved_octos_service_url());
        self.update_bindings(cx, settings);
        self.view.redraw(cx);
    }
    fn room(&self, cx: &mut Cx) -> Option<OwnedRoomId> {
        self.view
            .drop_down(cx, ids!(rooms))
            .selected_item()
            .checked_sub(1)
            .and_then(|i| self.rooms.get(i))
            .map(|r| r.room_id().clone())
    }
    fn update_bindings(&self, cx: &mut Cx, settings: &AgentAccessSettings) {
        let text = self
            .room(cx)
            .map(|id| {
                settings
                    .bot_settings
                    .room_bindings_for(&id)
                    .into_iter()
                    .map(|b| format!("{} {}", b.bot_user_id, b.remark))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        self.view.label(cx, ids!(bindings)).set_text(cx, &text);
    }
    fn changed(&mut self, cx: &mut Cx, settings: &AgentAccessSettings) {
        super::publish(self.owner.clone(), settings);
        cx.action(AgentAccessAction::Changed);
        self.refresh(cx, settings);
    }
    fn run(
        &mut self,
        cx: &mut Cx,
        operation: impl std::future::Future<Output = anyhow::Result<Outcome>> + Send + 'static,
    ) {
        if self.busy {
            return;
        }
        let Some(owner) = self.owner.clone() else {
            return;
        };
        self.request = nonce();
        let request = self.request;
        self.busy = true;
        self.status(cx, "Working…");
        spawn_async_task(async move {
            let result = operation.await.map_err(|e| e.to_string());
            Cx::post_action(Completed {
                owner,
                request,
                result,
            });
        });
    }
    fn send_command(&mut self, cx: &mut Cx, botfather: OwnedUserId, command: String) {
        let Some(client) = get_client() else { return };
        self.run(cx, async move {
            matrix_context::ensure_current(&client)?;
            let room = if let Some(room) = super::find_dm(&client, &botfather).await {
                room
            } else {
                super::create_bot_dm(&client, &botfather).await?
            };
            matrix_context::ensure_current(&client)?;
            anyhow::ensure!(
                room.state() == RoomState::Joined,
                "Join the BotFather chat before sending a command."
            );
            room.send(
                matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(
                    command,
                ),
            )
            .await?;
            matrix_context::ensure_current(&client)?;
            Ok(Outcome::Command(RoomNameId::from_room(&room).await))
        });
    }
}
impl Widget for AgentAccessPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if !self.active {
            return;
        }
        if self.owner != current_user_id() {
            cx.action(AgentAccessAction::Close);
            return;
        }
        if matches!(event, Event::KeyDown(k) if k.key_code == KeyCode::Escape) {
            cx.action(AgentAccessAction::Close);
            return;
        }
        let Event::Actions(actions) = event else {
            return;
        };
        let Some(app) = scope.data.get_mut::<AppState>() else {
            return;
        };
        let settings = &mut app.agent_access;
        for action in actions {
            if let Some(done) = action.downcast_ref::<Completed>()
                && self.owner.as_ref() == Some(&done.owner)
                && self.request == done.request
            {
                self.busy = false;
                match &done.result {
                    Ok(Outcome::Profile(user, entry)) => {
                        settings.agent_registry.unregister(user);
                        settings
                            .agent_registry
                            .register(user.clone(), entry.clone());
                        self.changed(cx, settings);
                        self.status(cx, "Agent profile checked and saved.");
                    }
                    Ok(Outcome::Health(message)) => self.status(cx, message),
                    Ok(Outcome::Command(room)) => {
                        let botfather = settings
                            .bot_settings
                            .resolved_bot_user_id(self.owner.as_deref())
                            .ok();
                        settings.bot_settings.set_room_bound(
                            room.room_id().clone(),
                            botfather,
                            true,
                        );
                        self.changed(cx, settings);
                        cx.action(AgentAccessAction::Close);
                        cx.action(crate::app::AppStateAction::NavigateToRoom {
                            room_to_close: None,
                            destination_room: crate::room::BasicRoomDetails::RoomId(room.clone()),
                        });
                    }
                    Err(error) => self.status(cx, error),
                }
            }
            if let Some(unbind) = action.downcast_ref::<Unbind>()
                && self.owner.as_ref() == Some(&unbind.owner)
            {
                settings
                    .unregister_agent_and_clear_bot_identity(&unbind.user, self.owner.as_deref());
                self.changed(cx, settings);
                self.status(cx, "Agent unbound.");
            }
            if let Some(delete) = action.downcast_ref::<DeleteBot>()
                && self.owner.as_ref() == Some(&delete.owner)
                && !self.busy
            {
                self.send_command(
                    cx,
                    delete.botfather.clone(),
                    format!("/deletebot {}", delete.target),
                );
            }
        }
        if self.view.button(cx, ids!(close)).clicked(actions) {
            cx.action(AgentAccessAction::Close);
        }
        if self.busy {
            return;
        }
        if let Some(index) = self.view.drop_down(cx, ids!(agents)).selected(actions)
            && let Some(user) = index.checked_sub(1).and_then(|i| self.agents.get(i))
        {
            self.view
                .text_input(cx, ids!(agent_id))
                .set_text(cx, user.as_str());
            self.view
                .text_input(cx, ids!(binding_id))
                .set_text(cx, user.as_str());
            if let Some(entry) = settings.agent_registry.get(user) {
                self.view.drop_down(cx, ids!(framework)).set_selected_item(
                    cx,
                    AgentFramework::ALL
                        .iter()
                        .position(|f| *f == entry.framework)
                        .unwrap_or(0),
                );
            }
        }
        if self.view.button(cx, ids!(register)).clicked(actions) {
            let user = match UserId::parse(self.view.text_input(cx, ids!(agent_id)).text().trim()) {
                Ok(user) => user,
                Err(_) => {
                    self.status(cx, "Enter a full Matrix user ID.");
                    return;
                }
            };
            if self.owner.as_ref() == Some(&user) {
                self.status(cx, "Choose an agent other than your own account.");
                return;
            }
            let framework = AgentFramework::ALL
                .get(self.view.drop_down(cx, ids!(framework)).selected_item())
                .copied()
                .unwrap_or_default();
            let Some(client) = get_client() else { return };
            let previous = settings
                .agent_registry
                .get(&user)
                .cloned()
                .unwrap_or_default();
            self.run(cx, async move {
                matrix_context::ensure_current(&client)?;
                let profile = client.account().fetch_user_profile_of(&user).await?;
                matrix_context::ensure_current(&client)?;
                Ok(Outcome::Profile(
                    user,
                    AgentEntry {
                        display_name: profile.get_static::<DisplayName>()?,
                        avatar: profile.get_static::<AvatarUrl>()?,
                        framework,
                        ..previous
                    },
                ))
            });
            return;
        }
        if self.view.button(cx, ids!(open)).clicked(actions) {
            if let Ok(user) = UserId::parse(self.view.text_input(cx, ids!(agent_id)).text().trim())
                && let Some(entry) = settings.agent_registry.get(&user)
            {
                crate::sliding_sync::submit_async_request(
                    crate::sliding_sync::MatrixRequest::OpenOrCreateDirectMessage {
                        user_profile: crate::profile::user_profile::UserProfile {
                            user_id: user,
                            username: entry.display_name.clone(),
                            avatar_state: crate::shared::avatar::AvatarState::Known(
                                entry.avatar.clone(),
                            ),
                        },
                        allow_create: true,
                    },
                );
                cx.action(AgentAccessAction::Close);
            } else {
                self.status(cx, "Register the agent before opening its chat.");
            }
        }
        if self.view.button(cx, ids!(unbind)).clicked(actions)
            && let Ok(user) = UserId::parse(self.view.text_input(cx, ids!(agent_id)).text().trim())
            && let Some(owner) = self.owner.clone()
        {
            crate::shared::confirmation_modal::confirm_removal(
                cx,
                crate::shared::confirmation_modal::ConfirmationModalContent {
                    title_text: crate::i18n::tr("Unbind agent?").into(),
                    body_text: format!(
                        "{}\n{}",
                        user,
                        crate::i18n::tr(
                            "Remove the saved agent identity and room bindings. Chats are kept."
                        )
                    )
                    .into(),
                    on_accept_clicked: Some(Box::new(move |cx| {
                        cx.action(Unbind {
                            owner: owner.clone(),
                            user: user.clone(),
                        })
                    })),
                    ..Default::default()
                },
            );
        }
        if self.view.button(cx, ids!(save)).clicked(actions) {
            let enabled = self.view.check_box(cx, ids!(enabled)).active(cx);
            let botfather = self
                .view
                .text_input(cx, ids!(botfather))
                .text()
                .trim()
                .to_owned();
            let endpoint = self
                .view
                .text_input(cx, ids!(endpoint))
                .text()
                .trim()
                .to_owned();
            if let Err(error) =
                BotSettingsState::validate_octos_service_url(&endpoint).and_then(|_| {
                    BotSettingsState::validate_botfather_user_id(&botfather, self.owner.as_deref())
                })
            {
                self.status(cx, &error);
                return;
            }
            settings.bot_settings.enabled = enabled;
            settings.bot_settings.botfather_user_id = botfather;
            settings.bot_settings.octos_service_url = endpoint;
            self.changed(cx, settings);
            self.status(cx, "AppService settings saved.");
        }
        if self.view.button(cx, ids!(health)).clicked(actions) {
            let endpoint = self
                .view
                .text_input(cx, ids!(endpoint))
                .text()
                .trim()
                .trim_end_matches('/')
                .to_owned();
            if let Err(error) = BotSettingsState::validate_octos_service_url(&endpoint) {
                self.status(cx, &error);
                return;
            }
            self.run(cx, async move {
                let client = matrix_sdk::reqwest::Client::builder()
                    .no_proxy()
                    .redirect(matrix_sdk::reqwest::redirect::Policy::none())
                    .timeout(std::time::Duration::from_secs(8))
                    .build()?;
                let mut results = Vec::new();
                for path in ["/health", "/api/status"] {
                    match client.get(format!("{endpoint}{path}")).send().await {
                        Ok(response) if response.status().is_success() => {
                            return Ok(Outcome::Health(crate::i18n::format(
                                "Service reachable: {path} ({status})",
                                &[
                                    ("path", path.into()),
                                    ("status", response.status().to_string()),
                                ],
                            )));
                        }
                        Ok(response) => results.push(format!("{path}: {}", response.status())),
                        Err(error) => results.push(format!("{path}: {error}")),
                    }
                }
                anyhow::bail!("{}", results.join("\n"))
            });
            return;
        }
        if self
            .view
            .drop_down(cx, ids!(rooms))
            .selected(actions)
            .is_some()
        {
            self.update_bindings(cx, settings);
        }
        let bind = self.view.button(cx, ids!(bind_room)).clicked(actions);
        if bind || self.view.button(cx, ids!(unbind_room)).clicked(actions) {
            let Some(room) = self.room(cx) else {
                self.status(cx, "Select a room.");
                return;
            };
            let user = match UserId::parse(self.view.text_input(cx, ids!(binding_id)).text().trim())
            {
                Ok(user) => user,
                Err(_) => {
                    self.status(cx, "Enter a full Matrix user ID.");
                    return;
                }
            };
            settings
                .bot_settings
                .set_room_bound(room.clone(), Some(user.clone()), bind);
            if bind {
                settings.bot_settings.set_room_bot_remark(
                    &room,
                    &user,
                    self.view.text_input(cx, ids!(remark)).text(),
                );
            }
            self.changed(cx, settings);
            self.status(cx, "Room binding saved.");
        }
        let command = if self.view.button(cx, ids!(list)).clicked(actions) {
            Some(Ok("/listbots".into()))
        } else if self.view.button(cx, ids!(help)).clicked(actions) {
            Some(Ok("/bothelp".into()))
        } else if self.view.button(cx, ids!(create)).clicked(actions) {
            Some(super::create_command(
                &self.view.text_input(cx, ids!(bot_username)).text(),
                &self.view.text_input(cx, ids!(bot_name)).text(),
                &self.view.text_input(cx, ids!(bot_prompt)).text(),
            ))
        } else {
            None
        };
        let delete = self.view.button(cx, ids!(delete)).clicked(actions);
        if command.is_some() || delete {
            if !settings.bot_settings.enabled {
                self.status(cx, "Save and enable AppService controls first.");
                return;
            }
            let botfather = match settings
                .bot_settings
                .resolved_bot_user_id(self.owner.as_deref())
            {
                Ok(id) => id,
                Err(error) => {
                    self.status(cx, &error);
                    return;
                }
            };
            if let Some(command) = command {
                match command {
                    Ok(command) => self.send_command(cx, botfather, command),
                    Err(error) => self.status(cx, &error),
                }
            } else if let Ok(target) =
                UserId::parse(self.view.text_input(cx, ids!(delete_id)).text().trim())
                && let Some(owner) = self.owner.clone()
            {
                if target == botfather || target == owner {
                    self.status(cx, "Choose a managed bot to delete.");
                    return;
                }
                crate::shared::confirmation_modal::confirm_removal(cx, crate::shared::confirmation_modal::ConfirmationModalContent {title_text: crate::i18n::tr("Delete bot?").into(), body_text: format!("{}\n{}", target, crate::i18n::tr("This sends a deletion request to BotFather. Check its reply for the result.")).into(), on_accept_clicked: Some(Box::new(move |cx| cx.action(DeleteBot {owner: owner.clone(), target: target.clone(), botfather: botfather.clone()}))), ..Default::default()});
            } else {
                self.status(cx, "Enter a full Matrix user ID.");
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
impl AgentAccessPanelRef {
    pub fn action(
        &self,
        cx: &mut Cx,
        modal: ModalRef,
        action: &AgentAccessAction,
        settings: &AgentAccessSettings,
    ) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        match action {
            AgentAccessAction::Open => {
                inner.request = nonce();
                inner.busy = false;
                inner.owner = current_user_id();
                inner.active = inner.owner.is_some();
                inner.rooms = get_client()
                    .map(|c| {
                        c.rooms()
                            .into_iter()
                            .filter(|r| r.state() == RoomState::Joined && !r.is_space())
                            .map(|r| {
                                RoomNameId::new(
                                    matrix_sdk::RoomDisplayName::Named(
                                        r.name().unwrap_or_else(|| r.room_id().to_string()),
                                    ),
                                    r.room_id().to_owned(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                inner
                    .rooms
                    .sort_by_cached_key(|r| r.display().to_lowercase());
                inner.view.drop_down(cx, ids!(rooms)).set_labels(
                    cx,
                    std::iter::once(crate::i18n::tr("Select a room").into())
                        .chain(
                            inner
                                .rooms
                                .iter()
                                .map(|r| format!("{} ({})", r.display(), r.room_id())),
                        )
                        .collect(),
                );
                inner
                    .view
                    .drop_down(cx, ids!(rooms))
                    .set_selected_item(cx, 0);
                inner.refresh(cx, settings);
                inner.status(cx, "");
                modal.open(cx);
            }
            AgentAccessAction::Close => {
                inner.active = false;
                inner.request = nonce();
                modal.close(cx);
            }
            AgentAccessAction::Changed => {}
        }
    }
}
