//! Native room/space creation and space membership management.
//! The parent link is authoritative; a failed advisory backlink is reported
//! separately so a successful creation is never retried as a second room.
use std::collections::HashSet;
use anyhow::{Result, anyhow, ensure};
use makepad_widgets::*;
use matrix_sdk::{Client, Room, RoomState};
use matrix_sdk::ruma::{
    OwnedRoomId, OwnedServerName, OwnedUserId, RoomId,
    api::client::room::{
        Visibility,
        create_room::v3::{CreationContent, Request, RoomPreset},
    },
    events::{
        InitialStateEvent, StateEventType,
        room::encryption::RoomEncryptionEventContent,
        space::{child::SpaceChildEventContent, parent::SpaceParentEventContent},
    },
    room::RoomType,
    serde::Raw,
};
use crate::{
    matrix_context,
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};

#[derive(Clone, Debug, Default)]
pub struct CreateRoomConfig {
    pub name: String,
    pub topic: String,
    pub public: bool,
    pub encrypted: bool,
    pub space: bool,
    pub parent: Option<OwnedRoomId>,
}

impl CreateRoomConfig {
    fn request(&self) -> Result<Request> {
        let name = self.name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 255,
            "Enter a name of 1–255 characters."
        );
        ensure!(self.topic.chars().count() <= 4096, "The topic is too long.");
        ensure!(
            !self.public || !self.encrypted || self.space,
            "Public rooms cannot use this encrypted-room preset."
        );
        let mut request = Request::new();
        request.name = Some(name.into());
        request.topic = Some(self.topic.trim().into());
        request.visibility = if self.public {
            Visibility::Public
        } else {
            Visibility::Private
        };
        request.preset = Some(if self.public {
            RoomPreset::PublicChat
        } else {
            RoomPreset::PrivateChat
        });
        if self.space {
            let mut creation = CreationContent::new();
            creation.room_type = Some(RoomType::Space);
            request.creation_content = Some(Raw::new(&creation)?);
        } else if self.encrypted {
            request.initial_state.push(
                InitialStateEvent::with_empty_state_key(
                    RoomEncryptionEventContent::with_recommended_defaults(),
                )
                .to_raw_any(),
            );
        }
        Ok(request)
    }
}

struct FreshState {
    space: bool,
    name: RoomNameId,
    topic: String,
    levels: matrix_sdk::ruma::events::room::power_levels::RoomPowerLevels,
    children: HashSet<OwnedRoomId>,
}

async fn fresh_state(client: &Client, room: &Room) -> Result<FreshState> {
    use matrix_sdk::ruma::events::room::{
        create::RoomCreateEventContent,
        power_levels::{RoomPowerLevels, RoomPowerLevelsEventContent, RoomPowerLevelsSource},
    };
    matrix_context::ensure_current(client)?;
    let response = client
        .send(
            matrix_sdk::ruma::api::client::state::get_state_events::v3::Request::new(
                room.room_id().to_owned(),
            ),
        )
        .await?;
    matrix_context::ensure_current(client)?;
    let events = response
        .room_state
        .iter()
        .map(|raw| serde_json::from_str::<serde_json::Value>(raw.json().get()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let find = |kind: &str| {
        events
            .iter()
            .find(|e| e["type"] == kind && e["state_key"] == "")
    };
    let create =
        find("m.room.create").ok_or_else(|| anyhow!("The room creation event is unavailable."))?;
    let creation: RoomCreateEventContent = serde_json::from_value(create["content"].clone())?;
    let rules = creation
        .room_version
        .rules()
        .ok_or_else(|| anyhow!("Unsupported room version."))?;
    #[allow(deprecated)]
    let creator = if rules.authorization.use_room_create_sender {
        create["sender"]
            .as_str()
            .and_then(|id| OwnedUserId::try_from(id).ok())
    } else {
        creation.creator.clone()
    };
    let mut creators: Vec<_> = creator.into_iter().collect();
    if rules.authorization.additional_room_creators {
        creators.extend(creation.additional_creators);
    }
    let powers = find("m.room.power_levels")
        .map(|e| serde_json::from_value::<RoomPowerLevelsEventContent>(e["content"].clone()))
        .transpose()?;
    let levels = RoomPowerLevels::new(
        RoomPowerLevelsSource::from(powers),
        &rules.authorization,
        creators,
    );
    let name = find("m.room.name")
        .and_then(|e| e["content"]["name"].as_str())
        .unwrap_or(room.room_id().as_str());
    let topic = find("m.room.topic")
        .and_then(|e| e["content"]["topic"].as_str())
        .unwrap_or_default()
        .to_owned();
    let children = events
        .iter()
        .filter(|e| {
            e["type"] == "m.space.child"
                && e["content"]["via"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
        })
        .filter_map(|e| {
            e["state_key"]
                .as_str()
                .and_then(|id| OwnedRoomId::try_from(id).ok())
        })
        .collect();
    Ok(FreshState {
        space: creation.room_type == Some(RoomType::Space),
        name: RoomNameId::new(
            matrix_sdk::RoomDisplayName::Named(name.into()),
            room.room_id().to_owned(),
        ),
        topic,
        levels,
        children,
    })
}

async fn editable_space(client: &Client, id: &RoomId, event: StateEventType) -> Result<Room> {
    matrix_context::ensure_current(client)?;
    let room = client
        .get_room(id)
        .ok_or_else(|| anyhow!("The space is no longer available."))?;
    ensure!(room.state() == RoomState::Joined, "Join the space first.");
    // Newly created rooms can have incomplete SDK state until sliding sync
    // catches up. Read authoritative state for management and permission checks.
    let state = fresh_state(client, &room).await?;
    ensure!(state.space, "This room is not a space.");
    let user = client.user_id().ok_or_else(|| anyhow!("Sign in first."))?;
    ensure!(
        state.levels.user_can_send_state(user, event),
        "You do not have permission to change this space."
    );
    matrix_context::ensure_current(client)?;
    Ok(room)
}

async fn route(room: &Room) -> Result<Vec<OwnedServerName>> {
    let mut route = room.route().await.unwrap_or_default();
    if route.is_empty() {
        route.extend(room.room_id().server_name().map(ToOwned::to_owned));
    }
    ensure!(!route.is_empty(), "The room has no routing server.");
    Ok(route)
}

async fn link(client: &Client, parent: &RoomId, child: &RoomId, add: bool) -> Result<String> {
    ensure!(parent != child, "A space cannot contain itself.");
    let space = editable_space(client, parent, StateEventType::SpaceChild).await?;
    let child_room = client.get_room(child);
    if add {
        let room = child_room
            .as_ref()
            .ok_or_else(|| anyhow!("Join the room before adding it."))?;
        ensure!(
            room.state() == RoomState::Joined,
            "Join the room before adding it."
        );
        let via = route(room).await?;
        matrix_context::ensure_current(client)?;
        space
            .send_state_event_for_key(child, SpaceChildEventContent::new(via))
            .await?;
    } else {
        space
            .send_state_event_raw("m.space.child", child.as_str(), serde_json::json!({}))
            .await?;
    }
    // Linking succeeds even if the user cannot edit state in the child room.
    if let Some(room) = child_room
        && room.state() == RoomState::Joined
        && let Some(user) = client.user_id()
        && room
            .power_levels()
            .await
            .is_ok_and(|levels| levels.user_can_send_state(user, StateEventType::SpaceParent))
    {
        let result: Result<()> = async {
            matrix_context::ensure_current(client)?;
            if add {
                let content = SpaceParentEventContent::new(route(&space).await?);
                room.send_state_event_for_key(parent, content).await?;
            } else {
                room.send_state_event_raw("m.space.parent", parent.as_str(), serde_json::json!({}))
                    .await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            return Ok(format!(
                "Space updated. The room's parent link could not be updated: {error}"
            ));
        }
    }
    Ok(if add {
        "Room added to space."
    } else {
        "Room removed from space. Membership is unchanged."
    }
    .into())
}

#[derive(Clone, Debug)]
struct SpaceChoice {
    name: RoomNameId,
    can_link: bool,
    can_edit: bool,
    topic: String,
}
#[derive(Clone, Debug, Default)]
struct Snapshot {
    spaces: Vec<SpaceChoice>,
    rooms: Vec<RoomNameId>,
    children: HashSet<OwnedRoomId>,
}

async fn snapshot(client: &Client, parent: Option<&OwnedRoomId>) -> Result<Snapshot> {
    matrix_context::ensure_current(client)?;
    let user = client.user_id().ok_or_else(|| anyhow!("Sign in first."))?;
    let mut data = Snapshot::default();
    for room in client.joined_rooms() {
        let mut name = RoomNameId::from_room(&room).await;
        let has_creation = room.get_state_event_static::<matrix_sdk::ruma::events::room::create::RoomCreateEventContent>().await?.is_some();
        if room.is_space() || !has_creation || parent.is_some_and(|id| id == room.room_id()) {
            let state = fresh_state(client, &room).await?;
            name = state.name.clone();
            if state.space {
                data.spaces.push(SpaceChoice {
                    name: state.name,
                    can_link: state
                        .levels
                        .user_can_send_state(user, StateEventType::SpaceChild),
                    can_edit: state
                        .levels
                        .user_can_send_state(user, StateEventType::RoomName)
                        && state
                            .levels
                            .user_can_send_state(user, StateEventType::RoomTopic),
                    topic: state.topic,
                });
                if parent.is_some_and(|id| id == room.room_id()) {
                    data.children = state.children;
                }
            }
        }
        data.rooms.push(name);
    }
    for id in &data.children {
        if !data.rooms.iter().any(|room| room.room_id() == id) {
            data.rooms.push(RoomNameId::new(
                matrix_sdk::RoomDisplayName::Named(id.to_string()),
                id.clone(),
            ));
        }
    }
    data.spaces
        .sort_by_cached_key(|space| space.name.display().to_lowercase());
    data.rooms
        .sort_by_cached_key(|room| room.display().to_lowercase());
    matrix_context::ensure_current(client)?;
    Ok(data)
}

#[derive(Clone, Debug)]
pub enum SpaceManagementAction {
    Open { parent: Option<RoomNameId> },
    Close,
}
#[derive(Clone, Debug)]
pub struct SpaceManagementChanged {
    pub owner: OwnedUserId,
    pub parent: Option<OwnedRoomId>,
}
#[derive(Debug)]
struct Loaded {
    owner: OwnedUserId,
    request: u64,
    result: Result<Snapshot, String>,
}
#[derive(Debug)]
struct Finished {
    owner: OwnedUserId,
    request: u64,
    parent: Option<OwnedRoomId>,
    result: Result<(String, Option<RoomNameId>), String>,
}
#[derive(Debug)]
struct LinkFinished {
    owner: OwnedUserId,
    parent: OwnedRoomId,
    message: String,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let Text = Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
    let Action = RobrixNeutralIconButton {height: 40 icon_walk: Walk{width: 0 height: 0} spacing: 0}
    let Input = TextInput {width: Fill height: 40}
    mod.widgets.SpaceManagementPanel = #(SpaceManagementPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down padding: Inset{top: 32 left: 16 right: 16 bottom: 12} spacing: 8
        draw_bg.color: mod.widgets.RINX_PAGE
        View {width: Fill height: 40 spacing: 8 align: Align{y: 0.5}
            close := Action {text: #(crate::i18n::tr("Back")) i18n_text: "Back"}
            refresh := Action {text: #(crate::i18n::tr("Refresh")) i18n_text: "Refresh"}
            Text {text: #(crate::i18n::tr("Rooms and spaces")) i18n_text: "Rooms and spaces"}
        }
        status := Text {}
        View {width: Fill height: 40 spacing: 8
            create_tab := Action {width: Fill text: #(crate::i18n::tr("Create")) i18n_text: "Create"}
            manage_tab := Action {width: Fill text: #(crate::i18n::tr("Manage space")) i18n_text: "Manage space"}
        }
        Text {text: #(crate::i18n::tr("Parent space")) i18n_text: "Parent space"}
        parent := DropDown {width: Fill labels: ["No parent space"]}
        create_form := ScrollYView {width: Fill height: Fill flow: Down spacing: 8
            kind := DropDown {width: Fill labels: [#(crate::i18n::tr("Room")) #(crate::i18n::tr("Space"))] i18n_labels: ["Room" "Space"]}
            name := Input {empty_text: #(crate::i18n::tr("Name")) i18n_empty_text: "Name"}
            topic := Input {empty_text: #(crate::i18n::tr("Topic")) i18n_empty_text: "Topic"}
            public := CheckBox {text: #(crate::i18n::tr("Public")) i18n_text: "Public"}
            encrypted := CheckBox {text: #(crate::i18n::tr("Encrypted room")) i18n_text: "Encrypted room"}
            create := Action {width: Fill text: #(crate::i18n::tr("Create")) i18n_text: "Create"}
            created := Action {visible: false width: Fill text: #(crate::i18n::tr("Open")) i18n_text: "Open"}
        }
        manage_form := View {visible: false width: Fill height: Fill flow: Down spacing: 8
            space_name := Input {empty_text: #(crate::i18n::tr("Space name")) i18n_empty_text: "Space name"}
            space_topic := Input {empty_text: #(crate::i18n::tr("Topic")) i18n_empty_text: "Topic"}
            save := Action {width: Fill text: #(crate::i18n::tr("Save space details")) i18n_text: "Save space details"}
            search := Input {empty_text: #(crate::i18n::tr("Filter rooms")) i18n_empty_text: "Filter rooms"}
            rooms := PortalList {width: Fill height: Fill
                Row := View {width: Fill height: Fit flow: Down padding: 8 spacing: 4
                    name := Text {}
                    id := Text {draw_text.color: mod.widgets.RINX_MUTED}
                    toggle := Action {width: Fill}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct SpaceManagementPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    request: u64,
    #[rust]
    active: bool,
    #[rust]
    busy: bool,
    #[rust]
    manage: bool,
    #[rust]
    parent: Option<OwnedRoomId>,
    #[rust]
    data: Snapshot,
    #[rust]
    filtered: Vec<RoomNameId>,
    #[rust]
    query: String,
    #[rust]
    created: Option<RoomNameId>,
    #[rust]
    created_space: bool,
}

fn next_request() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl SpaceManagementPanel {
    fn current(&self, owner: &OwnedUserId, request: u64) -> bool {
        self.active
            && self.owner.as_ref() == Some(owner)
            && current_user_id().as_ref() == Some(owner)
            && self.request == request
    }
    fn status(&self, cx: &mut Cx, status: &str) {
        self.view
            .label(cx, ids!(status))
            .set_text(cx, crate::i18n::tr(status));
    }
    fn choice(&self) -> Option<&SpaceChoice> {
        self.data
            .spaces
            .iter()
            .find(|space| Some(space.name.room_id()) == self.parent.as_ref())
    }
    fn sync(&mut self, cx: &mut Cx) {
        self.view
            .view(cx, ids!(create_form))
            .set_visible(cx, !self.manage);
        self.view
            .view(cx, ids!(manage_form))
            .set_visible(cx, self.manage);
        self.view.button(cx, ids!(create)).set_enabled(
            cx,
            !self.busy
                && (self.parent.is_none() || self.choice().is_some_and(|c| c.can_link))
                && self.created.is_none(),
        );
        self.view
            .button(cx, ids!(save))
            .set_enabled(cx, !self.busy && self.choice().is_some_and(|c| c.can_edit));
        self.view
            .button(cx, ids!(created))
            .set_visible(cx, self.created.is_some());
        self.filtered = self
            .data
            .rooms
            .iter()
            .filter(|room| {
                Some(room.room_id()) != self.parent.as_ref()
                    && (room.display().to_lowercase().contains(&self.query)
                        || room.room_id().as_str().to_lowercase().contains(&self.query))
            })
            .cloned()
            .collect();
        self.view.redraw(cx);
    }
    fn load(&mut self, cx: &mut Cx) {
        let Some(client) = get_client() else { return };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else {
            return;
        };
        self.owner = Some(owner.clone());
        self.request = next_request();
        let request = self.request;
        let parent = self.parent.clone();
        self.busy = true;
        self.sync(cx);
        spawn_async_task(async move {
            let result = snapshot(&client, parent.as_ref())
                .await
                .map_err(|e| e.to_string());
            Cx::post_action(Loaded {
                owner,
                request,
                result,
            });
        });
    }
    fn run(
        &mut self,
        cx: &mut Cx,
        operation: impl std::future::Future<Output = Result<(String, Option<RoomNameId>)>>
        + Send
        + 'static,
    ) {
        let Some(owner) = self.owner.clone() else {
            return;
        };
        if self.busy || current_user_id().as_ref() != Some(&owner) {
            return;
        }
        self.request = next_request();
        let request = self.request;
        let parent = self.parent.clone();
        self.busy = true;
        self.status(cx, "Saving…");
        self.sync(cx);
        spawn_async_task(async move {
            let result = operation.await.map_err(|e| e.to_string());
            Cx::post_action(Finished {
                owner,
                request,
                parent,
                result,
            });
        });
    }
}

impl Widget for SpaceManagementPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if !self.active {
            return;
        }
        if self.owner != current_user_id() {
            self.active = false;
            cx.action(SpaceManagementAction::Close);
            return;
        }
        let Event::Actions(actions) = event else {
            return;
        };
        for action in actions {
            if let Some(done) = action.downcast_ref::<LinkFinished>()
                && self.owner.as_ref() == Some(&done.owner)
                && self.parent.as_ref() == Some(&done.parent)
            {
                self.status(cx, &done.message);
            }
            if let Some(loaded) = action.downcast_ref::<Loaded>()
                && self.current(&loaded.owner, loaded.request)
            {
                self.busy = false;
                match &loaded.result {
                    Ok(data) => {
                        self.data = data.clone();
                        let labels = std::iter::once(crate::i18n::tr("No parent space").to_owned())
                            .chain(
                                self.data
                                    .spaces
                                    .iter()
                                    .map(|space| space.name.display().to_string()),
                            )
                            .collect::<Vec<_>>();
                        self.view.drop_down(cx, ids!(parent)).set_labels(cx, labels);
                        let index = self
                            .data
                            .spaces
                            .iter()
                            .position(|s| Some(s.name.room_id()) == self.parent.as_ref())
                            .map_or(0, |i| i + 1);
                        self.view
                            .drop_down(cx, ids!(parent))
                            .set_selected_item(cx, index);
                        if index == 0 {
                            self.parent = None;
                        }
                        if let Some(space) = self.choice() {
                            self.view
                                .text_input(cx, ids!(space_name))
                                .set_text(cx, &space.name.display());
                            self.view
                                .text_input(cx, ids!(space_topic))
                                .set_text(cx, &space.topic);
                        }
                    }
                    Err(error) => self.status(cx, error),
                }
                self.sync(cx);
            }
            if let Some(done) = action.downcast_ref::<Finished>()
                && self.current(&done.owner, done.request)
            {
                self.busy = false;
                match &done.result {
                    Ok((message, created)) => {
                        self.created = created.clone();
                        self.status(cx, message);
                        cx.action(SpaceManagementChanged {
                            owner: done.owner.clone(),
                            parent: done.parent.clone(),
                        });
                        self.load(cx);
                    }
                    Err(error) => self.status(cx, error),
                }
                self.sync(cx);
            }
        }
        if self.view.button(cx, ids!(close)).clicked(actions) {
            cx.action(SpaceManagementAction::Close);
        }
        if self.busy {
            return;
        }
        if self.view.button(cx, ids!(refresh)).clicked(actions) {
            self.load(cx);
            return;
        }
        if self.view.button(cx, ids!(create_tab)).clicked(actions) {
            self.manage = false;
            self.created = None;
            self.sync(cx);
        }
        if self.view.button(cx, ids!(manage_tab)).clicked(actions) {
            self.manage = true;
            self.sync(cx);
        }
        if let Some(index) = self.view.drop_down(cx, ids!(parent)).selected(actions) {
            self.parent = index
                .checked_sub(1)
                .and_then(|i| self.data.spaces.get(i))
                .map(|s| s.name.room_id().clone());
            self.data.children.clear();
            self.load(cx);
        }
        if let Some(query) = self.view.text_input(cx, ids!(search)).changed(actions) {
            self.query = query.to_lowercase();
            self.sync(cx);
        }
        if self
            .view
            .drop_down(cx, ids!(kind))
            .selected(actions)
            .is_some()
        {
            let is_space = self.view.drop_down(cx, ids!(kind)).selected_item() == 1;
            self.view
                .check_box(cx, ids!(encrypted))
                .set_visible(cx, !is_space);
        }
        if self.view.button(cx, ids!(created)).clicked(actions)
            && let Some(room) = self.created.clone()
        {
            cx.action(SpaceManagementAction::Close);
            if self.created_space {
                cx.action(crate::home::navigation_tab_bar::NavigationBarAction::GoToHome);
                cx.widget_action(
                    self.widget_uid(),
                    crate::home::rooms_list::RoomsListAction::Selected(
                        crate::app::SelectedRoom::Space {
                            space_name_id: room,
                        },
                    ),
                );
            } else {
                cx.action(crate::app::AppStateAction::NavigateToRoom {
                    room_to_close: None,
                    destination_room: crate::room::BasicRoomDetails::RoomId(room),
                });
            }
        }
        let Some(client) = get_client() else { return };
        if self.view.button(cx, ids!(create)).clicked(actions) && self.created.is_none() {
            let config = CreateRoomConfig {
                name: self.view.text_input(cx, ids!(name)).text(),
                topic: self.view.text_input(cx, ids!(topic)).text(),
                public: self.view.check_box(cx, ids!(public)).active(cx),
                encrypted: self.view.check_box(cx, ids!(encrypted)).active(cx),
                space: self.view.drop_down(cx, ids!(kind)).selected_item() == 1,
                parent: self.parent.clone(),
            };
            self.created_space = config.space;
            self.run(cx, async move {
                let request = config.request()?;
                if let Some(parent) = &config.parent { editable_space(&client, parent, StateEventType::SpaceChild).await?; }
                matrix_context::ensure_current(&client)?;
                let room = client.create_room(request).await?;
                let name = RoomNameId::new(matrix_sdk::RoomDisplayName::Named(config.name.trim().into()), room.room_id().to_owned());
                let message = if let Some(parent) = &config.parent {
                    match link(&client, parent, room.room_id(), true).await {
                        Ok(message) => format!("Created {}. {message}", config.name.trim()),
                        Err(error) => format!("Created {} ({}) but could not add it to the space: {error}. Use Manage space to retry the link.", config.name.trim(), room.room_id()),
                    }
                } else { format!("Created {}.", config.name.trim()) };
                Ok((message, Some(name)))
            });
            return;
        }
        if self.view.button(cx, ids!(save)).clicked(actions)
            && let Some(parent) = self.parent.clone()
        {
            let name = self
                .view
                .text_input(cx, ids!(space_name))
                .text()
                .trim()
                .to_owned();
            let topic = self.view.text_input(cx, ids!(space_topic)).text();
            self.run(cx, async move {
                CreateRoomConfig {
                    name: name.clone(),
                    topic: topic.clone(),
                    ..Default::default()
                }
                .request()?;
                let room = editable_space(&client, &parent, StateEventType::RoomName).await?;
                editable_space(&client, &parent, StateEventType::RoomTopic).await?;
                room.set_name(name).await?;
                matrix_context::ensure_current(&client)?;
                match room.set_room_topic(&topic).await {
                    Ok(_) => Ok(("Space details saved.".into(), None)),
                    Err(error) => Ok((
                        format!("Space name saved; the topic could not be saved: {error}"),
                        None,
                    )),
                }
            });
            return;
        }
        if self.manage
            && let Some(parent) = self.parent.clone()
        {
            let list = self.view.portal_list(cx, ids!(rooms));
            for (index, item) in list.items_with_actions(actions) {
                if item.button(cx, ids!(toggle)).clicked(actions)
                    && let Some(room) = self.filtered.get(index).cloned()
                {
                    let add = !self.data.children.contains(room.room_id());
                    let child = room.room_id().clone();
                    if add {
                        self.run(cx, async move {
                            Ok((link(&client, &parent, &child, true).await?, None))
                        });
                    } else {
                        // The confirmation callback carries the exact account/space/room,
                        // so a later selection change cannot unlink another room.
                        let owner = self.owner.clone();
                        crate::shared::confirmation_modal::confirm_removal(
                            cx,
                            crate::shared::confirmation_modal::ConfirmationModalContent {
                                title_text: crate::i18n::tr("Remove from space?").into(),
                                body_text: format!(
                                    "{}\n{}\n{}",
                                    room.display(),
                                    child,
                                    crate::i18n::tr(
                                        "This removes the space link only. Nobody leaves the room."
                                    )
                                )
                                .into(),
                                on_accept_clicked: Some(Box::new(move |cx| {
                                    if owner != current_user_id() {
                                        return;
                                    }
                                    let Some(owner) = owner.clone() else { return };
                                    let client = client.clone();
                                    let parent = parent.clone();
                                    let child = child.clone();
                                    spawn_async_task(async move {
                                        let result = link(&client, &parent, &child, false).await;
                                        if !matrix_context::is_current(&client) {
                                            return;
                                        }
                                        let message = result.unwrap_or_else(|e| e.to_string());
                                        Cx::post_action(LinkFinished {
                                            owner: owner.clone(),
                                            parent: parent.clone(),
                                            message: message.clone(),
                                        });
                                        crate::shared::popup_list::enqueue_popup_notification(
                                            crate::i18n::tr(&message).to_owned(),
                                            crate::shared::popup_list::PopupKind::Info,
                                            Some(6.0),
                                        );
                                        Cx::post_action(SpaceManagementChanged {
                                            owner,
                                            parent: Some(parent),
                                        });
                                    });
                                    cx.redraw_all();
                                })),
                                ..Default::default()
                            },
                        );
                    }
                    break;
                }
            }
        }
        for action in actions {
            if let Some(changed) = action.downcast_ref::<SpaceManagementChanged>()
                && self.owner.as_ref() == Some(&changed.owner)
                && self.parent == changed.parent
                && !self.busy
            {
                self.load(cx);
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.filtered.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(room) = self.filtered.get(index) else {
                        continue;
                    };
                    let item = list.item(cx, index, id!(Row));
                    item.label(cx, ids!(name)).set_text(cx, &room.display());
                    item.label(cx, ids!(id))
                        .set_text(cx, room.room_id().as_str());
                    item.button(cx, ids!(toggle)).set_text(
                        cx,
                        crate::i18n::tr(if self.data.children.contains(room.room_id()) {
                            "Remove from space"
                        } else {
                            "Add to space"
                        }),
                    );
                    item.button(cx, ids!(toggle))
                        .set_enabled(cx, !self.busy && self.choice().is_some_and(|c| c.can_link));
                    item.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}

impl SpaceManagementPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &SpaceManagementAction) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.request = next_request();
        match action {
            SpaceManagementAction::Open { parent } => {
                inner.active = true;
                inner.parent = parent.as_ref().map(|p| p.room_id().clone());
                inner.manage = parent.is_some();
                inner.created = None;
                inner.data = Snapshot::default();
                inner.query.clear();
                inner
                    .view
                    .drop_down(cx, ids!(kind))
                    .set_selected_item(cx, 0);
                inner
                    .view
                    .check_box(cx, ids!(public))
                    .set_active(cx, false, Animate::No);
                inner
                    .view
                    .check_box(cx, ids!(encrypted))
                    .set_visible(cx, true);
                inner
                    .view
                    .check_box(cx, ids!(encrypted))
                    .set_active(cx, true, Animate::No);
                inner.view.text_input(cx, ids!(name)).set_text(cx, "");
                inner.view.text_input(cx, ids!(topic)).set_text(cx, "");
                inner.view.text_input(cx, ids!(search)).set_text(cx, "");
                inner.status(cx, "Create a room or space, or manage a joined space.");
                inner.load(cx);
                modal.open(cx);
            }
            SpaceManagementAction::Close => {
                inner.active = false;
                inner.data = Snapshot::default();
                modal.close(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn space_creation_sets_room_type_and_never_encrypts_spaces() {
        let request = CreateRoomConfig {
            name: "Project".into(),
            space: true,
            encrypted: true,
            ..Default::default()
        }
        .request()
        .unwrap();
        assert_eq!(
            request
                .creation_content
                .unwrap()
                .deserialize()
                .unwrap()
                .room_type,
            Some(RoomType::Space)
        );
        assert!(request.initial_state.is_empty());
        assert_eq!(request.visibility, Visibility::Private);
    }
    #[test]
    fn private_room_encryption_and_invalid_names() {
        let mut config = CreateRoomConfig {
            name: "Team".into(),
            encrypted: true,
            ..Default::default()
        };
        assert_eq!(config.request().unwrap().initial_state.len(), 1);
        config.public = true;
        assert!(config.request().is_err());
        config.public = false;
        config.name = " ".into();
        assert!(config.request().is_err());
        config.name = "界".repeat(256);
        assert!(config.request().is_err());
    }
}
