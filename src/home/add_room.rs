//! A top-level view for adding (joining) or exploring new rooms and spaces.


use makepad_widgets::*;
use super::{navigation_tab_bar::{NavigationBarAction, SelectedTab}, room_directory::{self, DirectoryRoom, DirectoryPage}};
use crate::shared::navigation_bar_button::NavigationBarButtonWidgetRefExt;
use matrix_sdk::RoomState;
use ruma::{IdParseError, MatrixToUri, MatrixUri, OwnedRoomOrAliasId, OwnedServerName, matrix_uri::MatrixId, room::{JoinRuleSummary, RoomType}};

use crate::{app::AppStateAction, home::invite_screen::JoinRoomResultAction, room::{FetchedRoomAvatar, FetchedRoomPreview}, shared::{avatar::AvatarWidgetRefExt, popup_list::{PopupKind, enqueue_popup_notification}}, sliding_sync::{MatrixRequest, submit_async_request, current_user_id, get_client, spawn_async_task, fetch_room_preview_with_avatar}, utils};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*


    // The main view that allows the user to add (join) or explore new rooms/spaces.
    mod.widgets.AddRoomScreen = #(AddRoomScreen::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down
        draw_bg.color: mod.widgets.RINX_PAGE
        explore_header := DetailHeader { title +: {text: #(crate::i18n::tr("Explore Rooms")) i18n_text: "Explore Rooms"} }
        create_room_space := RobrixNeutralIconButton {
            width: Fill height: 40 margin: Inset{left: 16 right: 16}
            text: #(crate::i18n::tr("Create a room or space")) i18n_text: "Create a room or space"
            icon_walk: Walk{width: 0 height: 0}
        }
        help_info := DetailNote {
            padding: Inset{left: 16 right: 16 top: 10 bottom: 10}
            text: #(crate::i18n::tr("Find groups and Spaces by name on your server, or enter an alias, ID or Matrix link.")) i18n_text: "Find groups and Spaces by name on your server, or enter an alias, ID or Matrix link."
        }
        join_room_view := View {
            width: Fill,
            height: Fit,
            margin: Inset{ top: 3, bottom: 4, left: 10, right: 10 }
            align: Align{y: 0.5}
            spacing: 5
            flow: Right

            room_alias_id_input := RobrixTextInput {
                margin: Inset{top: 0, left: 5, right: 5, bottom: 0},
                padding: Inset{left: 12, right: 12, top: 0, bottom: 0}
                width: Fill
                height: 40
                empty_text: #(crate::i18n::tr("Room name, alias or link")) i18n_empty_text: "Room name, alias or link"
                autocapitalize: None,
                autocorrect: Disabled,
            }

            search_for_room_button := RobrixPositiveIconButton {
                draw_bg +: {color: mod.widgets.RINX_ACCENT color_hover: mod.widgets.RINX_ACCENT_HOVER color_down: mod.widgets.RINX_ACCENT_DOWN border_size: 0}
                draw_text +: {color: mod.widgets.RINX_ON_ACCENT color_hover: mod.widgets.RINX_ON_ACCENT color_down: mod.widgets.RINX_ON_ACCENT}
                draw_icon.color: mod.widgets.RINX_ON_ACCENT
                padding: Inset{top: 10, bottom: 10, left: 12, right: 14}
                height: 40
                draw_icon.svg: (ICON_SEARCH)
                icon_walk: Walk{width: 16, height: 16}
                text: #(crate::i18n::tr("Search")) i18n_text: "Search"
            }
        }

        loading_room_view := View {
            visible: false
            spacing: 5,
            padding: 10,
            width: Fill
            height: Fit
            align: Align{y: 0.5}
            flow: Right

            loading_spinner := LoadingSpinner {
                width: 25,
                height: 25,
                draw_bg +: {
                    color: (COLOR_ACTIVE_PRIMARY)
                    border_size: 3.0
                }
            }

            loading_text := Label {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true},
                margin: Inset { top: 4 }
                draw_text +: {
                    color: (MESSAGE_TEXT_COLOR),
                    text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                }
            }
        }

        error_view := View {
            padding: 10
            error_text := Label {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true},
                draw_text +: {
                    color: (COLOR_FG_DANGER_RED),
                    text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                }
            }
        }

        directory_results := View {
            visible: false width: Fill height: Fill flow: Down
            directory_status := DetailNote {}
            directory_list := PortalList {
                width: Fill height: Fill flow: Down
                auto_tail: false
                Result := SolidView {
                    width: Fill height: 74 flow: Down draw_bg.color: mod.widgets.RINX_SURFACE
                    row := NavigationBarButton {
                        width: Fill height: 73 flow: Down spacing: 7 align: Align{x: 0 y: 0.5}
                        padding: Inset{left: 20 right: 20 top: 14 bottom: 12}
                        draw_bg +: {color_hover: mod.widgets.RINX_BORDER color_active: mod.widgets.RINX_BORDER border_radius: 0}
                        name := DetailLabel {width: Fill padding: 0 max_lines: 1 text_overflow: Ellipsis}
                        description := DetailLabel {
                            width: Fill padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                        }
                    }
                    DetailDivider {}
                }
            }
            more_results := RobrixNeutralIconButton {text: #(crate::i18n::tr("More results")) i18n_text: "More results" visible: false}
        }
        preview_scroll := ScrollYView {
            width: Fill height: Fill flow: Down
            fetched_room_summary := RoundedView {
                visible: false
                padding: 15
                margin: Inset{top: 10, bottom: 5, left: 5, right: 5}
                flow: Down
                width: Fill, height: Fit

                show_bg: true
                draw_bg +: {
                    color: (COLOR_PRIMARY)
                    border_radius: 4.0
                    border_size: 1.0
                    border_color: (COLOR_BG_DISABLED)
                    // shadow_color: #0005
                    // shadow_radius: 15.0
                    // shadow_offset: vec2(1.0, 0.0), //5.0,5.0)
                }

                room_name_avatar_view := View {
                    width: Fill, height: Fit
                    spacing: 10
                    align: Align{y: 0.5}
                    flow: Right,

                    room_avatar := Avatar {
                        width: 45, height: 45,
                        cursor: MouseCursor.Default,
                        text_view +: {
                            text +: {
                                draw_text +: {
                                    text_style: TITLE_TEXT { font_size: (16.0 * mod.widgets.RINX_TEXT_SCALE) }
                                }
                            }
                        }
                    }

                    room_name := Label {
                        width: Fill, height: Fit,
                        margin: Inset{top: 3} // align it with the above room_avatar
                        flow: Flow.Right{wrap: true},
                        draw_text +: {
                            text_style: TITLE_TEXT { font_size: (16 * mod.widgets.RINX_TEXT_SCALE) }
                            color: (COLOR_TEXT)
                        }
                    }
                }

                // Something like "This is a [regular|direct] [room|space] with N members."
                room_summary := Label {
                    width: Fill, height: Fit
                    flow: Flow.Right{wrap: true},
                    margin: Inset{top: 10}
                    draw_text +: {
                        color: (MESSAGE_TEXT_COLOR),
                        text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                    }
                }

                subsection_alias_id := SubsectionLabel {
                    draw_text +: { text_style: theme.font_regular { font_size: (12 * mod.widgets.RINX_TEXT_SCALE) } }
                }

                room_alias_and_id_view := View {
                    padding: Inset{left: 15}
                    width: Fill, height: Fit
                    spacing: 8.4 // to line up the colons if the ID wraps to the next line
                    align: Align{y: 0.5}
                    flow: Flow.Right{wrap: true},

                    room_alias := Label {
                        width: Fit, height: Fit
                        flow: Flow.Right{wrap: true},
                        draw_text +: {
                            color: (MESSAGE_TEXT_COLOR),
                            text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                        }
                    }

                    room_id := Label {
                        width: Fit, height: Fit
                        flow: Flow.Right{wrap: true},
                        draw_text +: {
                            color: (SMALL_STATE_TEXT_COLOR),
                            text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                        }
                    }
                }

                subsection_topic := SubsectionLabel {
                    draw_text +: { text_style: theme.font_regular { font_size: (12 * mod.widgets.RINX_TEXT_SCALE) } }
                }

                room_topic := MessageHtml {
                    padding: Inset{left: 20, top: 5, right: 10, bottom: 10}
                    width: Fill,
                    height: Fit,
                    font_size: (11 * mod.widgets.RINX_TEXT_SCALE)
                    font_color: (MESSAGE_TEXT_COLOR)
                }

                buttons_view := View {
                    width: Fill
                    height: Fit,
                    flow: Flow.Right{wrap: true},
                    align: Align{y: 0.5}
                    spacing: 15
                    margin: Inset{top: 15}

                    // This button's text is based on the room state (e.g., joined, left, invited)
                    // the room's join rules (e.g., public, can knock, invite-only (in which we disable it)).
                    join_room_button := RobrixPositiveIconButton {
                        padding: 15,
                        draw_icon.svg: (ICON_JOIN_ROOM)
                        icon_walk: Walk{width: 17, height: 17, margin: Inset{left: -2, right: -1} }
                    }

                    cancel_button := RobrixNegativeIconButton {
                        align: Align{x: 0.5, y: 0.5}
                        padding: 15
                        draw_icon.svg: (ICON_FORBIDDEN)
                        icon_walk: Walk{width: 16, height: 16, margin: Inset{left: -2, right: -1} }
                        text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"
                    }
                }
            }

            View {
                width: Fill
                height: 20
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct AddRoomScreen {
    #[deref] view: View,
    #[rust] state: AddRoomState,
    #[rust] request: u64,
    #[rust] owner: Option<ruma::OwnedUserId>,
    #[rust] query: String,
    #[rust] results: Vec<DirectoryRoom>,
    #[rust] next_batch: Option<String>,
    #[rust] directory_status: String,
    #[rust] searching: bool,
    #[rust] showing_results: bool,
    /// The function to perform when the user clicks the `join_room_button`.
    #[rust(JoinButtonFunction::None)] join_function: JoinButtonFunction,
}

#[derive(Debug)]
struct ExploreResponse {
    owner: ruma::OwnedUserId,
    request: u64,
    result: ExploreResult,
}
#[derive(Debug)]
enum ExploreResult {
    Directory(DirectoryPage),
    Preview(Result<FetchedRoomPreview, String>),
}

impl AddRoomScreen {
    fn new_request(&mut self) -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.request = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.request
    }

    fn preview(&mut self, cx: &mut Cx, room_or_alias_id: OwnedRoomOrAliasId, via: Vec<OwnedServerName>) {
        let Some(client) = get_client() else { return; };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else { return; };
        self.owner = Some(owner.clone());
        let request = self.new_request();
        self.searching = false;
        self.showing_results = false;
        self.state = AddRoomState::Parsed {room_or_alias_id: room_or_alias_id.clone(), via: via.clone()};
        spawn_async_task(async move {
            let result = fetch_room_preview_with_avatar(&client, &room_or_alias_id, via).await.map_err(|e| e.to_string());
            Cx::post_action(ExploreResponse {owner, request, result: ExploreResult::Preview(result)});
        });
        self.redraw(cx);
    }

    fn search(&mut self, cx: &mut Cx, query: String, since: Option<String>) {
        let Some(client) = get_client() else { return; };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else { return; };
        self.owner = Some(owner.clone());
        let request = self.new_request();
        if since.is_none() {
            self.results.clear();
            self.view.portal_list(cx, ids!(directory_list)).set_first_id_and_scroll(0, 0.0);
        }
        self.query = query.clone();
        self.next_batch = None;
        self.searching = true;
        self.showing_results = true;
        self.directory_status = crate::i18n::format("Searching {0}…", &[("0", (owner.server_name()).to_string())]);
        self.state = AddRoomState::WaitingOnUserInput;
        spawn_async_task(async move {
            let result = room_directory::search(&client, &query, since).await;
            Cx::post_action(ExploreResponse {owner, request, result: ExploreResult::Directory(result)});
        });
        self.redraw(cx);
    }

    fn back(&mut self, cx: &mut Cx) {
        self.new_request(); // Ignore any result arriving after this navigation.
        self.searching = false;
        if !self.showing_results && !self.results.is_empty() {
            self.state = AddRoomState::WaitingOnUserInput;
            self.showing_results = true;
        } else {
            self.state = AddRoomState::WaitingOnUserInput;
            self.showing_results = false;
            cx.action(NavigationBarAction::CloseAddRoom);
        }
        self.redraw(cx);
    }
}

#[derive(Default)]
#[allow(clippy::large_enum_variant)]
enum AddRoomState {
    /// We're waiting for the user to input a room ID, alias, or matrix link.
    #[default]
    WaitingOnUserInput,
    /// We successfully parsed the user input and have sent a request.
    /// We're now waiting for the room preview to be fetched and returned.
    Parsed {
        room_or_alias_id: OwnedRoomOrAliasId,
        via: Vec<OwnedServerName>,
    },
    /// The user entered invalid input that we couldn't parse into a room address.
    ParseError(String),
    /// We successfully fetched the room preview and have displayed it,
    /// and are waiting for the user to join the room.
    FetchedRoomPreview {
        frp: FetchedRoomPreview,
        room_or_alias_id: OwnedRoomOrAliasId,
        via: Vec<OwnedServerName>,
    },
    /// We failed to fetch the room preview, likely because it couldn't be found
    /// or because of connectivity issues or something else.
    FetchError(String),
    /// We successfully knocked on the room or space, and are waiting for
    /// a member of that room/space to acknowledge our knock by inviting us.
    Knocked {
        frp: FetchedRoomPreview,
    },
    /// We successfully joined the room or space, and are waiting for it
    /// to be loaded from the homeserver.
    Joined {
        frp: FetchedRoomPreview,
    },
    /// The fetched room or space has been loaded from the homeserver,
    /// so we can allow the user to jump to it via the `join_room_button`.
    Loaded {
        frp: FetchedRoomPreview,
        is_invite: bool,
    }
}
impl AddRoomState {
    fn fetched_room_preview(&self) -> Option<&FetchedRoomPreview> {
        match self {
            Self::FetchedRoomPreview { frp, .. }
            | Self::Knocked { frp }
            | Self::Joined { frp }
            | Self::Loaded { frp, .. } => Some(frp),
            _ => None,
        }
    }

    fn transition_to_knocked(&mut self) {
        let prev = std::mem::take(self);
        if let Self::FetchedRoomPreview { frp, .. } = prev {
            *self = Self::Knocked { frp };
        } else {
            *self = prev;
        }
    }

    fn transition_to_joined(&mut self) {
        let prev = std::mem::take(self);
        if let Self::FetchedRoomPreview { frp, .. } = prev {
            *self = Self::Joined { frp };
        } else {
            *self = prev;
        }
    }

    fn transition_to_loaded(&mut self, is_invite: bool) {
        let prev = std::mem::take(self);
        match prev {
            Self::FetchedRoomPreview { frp, .. }
            | Self::Joined { frp }
            | Self::Knocked { frp } => {
                *self = Self::Loaded { frp, is_invite };
            }
            _ => {
                *self = prev;
            }
        }
    }
}

impl Widget for AddRoomScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let active = scope.data.get::<crate::app::AppState>().is_some_and(|app| app.selected_tab == SelectedTab::AddRoom);
        if active && (event.back_pressed() || matches!(event, Event::KeyUp(key) if key.key_code == KeyCode::Escape)) {
            self.back(cx);
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self.view.button(cx, ids!(create_room_space)).clicked(actions) {
                cx.action(super::space_management::SpaceManagementAction::Open { parent: None });
            }
            let room_alias_id_input = self.view.text_input(cx, ids!(room_alias_id_input));
            let search_for_room_button = self.view.button(cx, ids!(search_for_room_button));
            let cancel_button = self.view.button(cx, ids!(fetched_room_summary.buttons_view.cancel_button));
            let join_room_button = self.view.button(cx, ids!(fetched_room_summary.buttons_view.join_room_button));

            if active && self.view.button(cx, ids!(explore_header.controls.back)).clicked(actions) {
                self.back(cx);
            }
            if self.view.button(cx, ids!(more_results)).clicked(actions) && !self.searching {
                if let Some(since) = self.next_batch.clone() { self.search(cx, self.query.clone(), Some(since)); }
            }
            let list = self.view.portal_list(cx, ids!(directory_list));
            for (index, item) in list.items_with_actions(actions) {
                if !list.was_scrolling() && item.navigation_bar_button(cx, ids!(row)).clicked(actions) {
                    if let Some(room) = self.results.get(index) {
                        let via = self.owner.as_ref().map(|owner| vec![owner.server_name().to_owned()]).unwrap_or_default();
                        self.preview(cx, room.id.clone().into(), via);
                        break;
                    }
                }
            }

            // Enable or disable the button based on if the text input is empty.
            if let Some(text) = room_alias_id_input.changed(actions) {
                search_for_room_button.set_enabled(cx, !text.trim().is_empty());
            }

            // If the cancel button was clicked, hide the room preview and return to default state.
            if cancel_button.clicked(actions) {
                self.new_request();
                self.state = AddRoomState::WaitingOnUserInput;
                self.showing_results = !self.results.is_empty();
                room_alias_id_input.set_text(cx, if self.showing_results { &self.query } else { "" });
                room_alias_id_input.set_key_focus(cx);
                self.redraw(cx);
            }

            // If the join button was clicked, perform the appropriate action.
            if join_room_button.clicked(actions) {
                match (&self.join_function, &self.state) {
                    (
                        JoinButtonFunction::NavigateOrJoin,
                        AddRoomState::FetchedRoomPreview { frp, .. } | AddRoomState::Loaded { frp, .. }
                    ) => {
                        cx.action(AppStateAction::NavigateToRoom {
                            room_to_close: None,
                            destination_room: frp.clone().into(),
                        });
                    }
                    (
                        JoinButtonFunction::Knock,
                        AddRoomState::FetchedRoomPreview { frp, room_or_alias_id, via }
                    ) => {
                        submit_async_request(MatrixRequest::Knock {
                            room_or_alias_id: frp.canonical_alias.clone().map_or_else(
                                || room_or_alias_id.clone(),
                                Into::into
                            ),
                            reason: None,
                            server_names: via.clone(),
                        });
                    }
                    _ => { }
                }
            }

            // If the button was clicked or enter was pressed, try to parse the room address.
            let new_room_query = search_for_room_button.clicked(actions)
                .then(|| room_alias_id_input.text())
                .or_else(|| room_alias_id_input.returned(actions).map(|(t, _)| t));
            if let Some(t) = new_room_query {
                let t = t.trim();
                if !t.is_empty() {
                    self.results.clear();
                    self.next_batch = None;
                    match room_query(t) {
                        Ok(RoomQuery::Address(id, via)) => self.preview(cx, id, via),
                        Ok(RoomQuery::Name(name)) => self.search(cx, name, None),
                        Err(error) => {
                            self.new_request();
                            self.searching = false;
                            self.showing_results = false;
                            self.state = AddRoomState::ParseError(error);
                        }
                    }
                    self.redraw(cx);
                }
            }
            for action in actions {
                if let Some(response) = action.downcast_ref::<ExploreResponse>() {
                    if response.request != self.request || Some(&response.owner) != current_user_id().as_ref() { continue; }
                    match &response.result {
                        ExploreResult::Directory(page) => {
                            self.searching = false;
                            for room in &page.rooms {
                                if !self.results.iter().any(|old| old.id == room.id) { self.results.push(room.clone()); }
                            }
                            self.next_batch = page.next_batch.clone();
                            self.directory_status = page.warning.clone().unwrap_or_else(|| {
                                if self.results.is_empty() {
                                    "No rooms found. Try another name. Private or unlisted rooms need an alias, link or invitation.".into()
                                } else {
                                    format!("{} {} · {}", self.results.len(), if self.results.len() == 1 { "result" } else { "results" }, response.owner.server_name())
                                }
                            });
                        }
                        ExploreResult::Preview(result) => {
                            if let AddRoomState::Parsed {room_or_alias_id, via} = &self.state {
                                self.state = match result {
                                    Ok(frp) => AddRoomState::FetchedRoomPreview {frp: frp.clone(), room_or_alias_id: room_or_alias_id.clone(), via: via.clone()},
                                    Err(error) => AddRoomState::FetchError(crate::i18n::format("Could not load room: {error}", &[("error", (error).to_string())])),
                                };
                                join_room_button.reset_hover(cx);
                                cancel_button.reset_hover(cx);
                            }
                        }
                    }
                    self.redraw(cx);
                }
                if matches!(action.downcast_ref(), Some(crate::logout::logout_confirm_modal::LogoutAction::ClearAppState {..})) {
                    self.new_request();
                    self.owner = None;
                    self.results.clear();
                    self.query.clear();
                    self.next_batch = None;
                    self.showing_results = false;
                    self.searching = false;
                    self.state = AddRoomState::WaitingOnUserInput;
                    room_alias_id_input.set_text(cx, "");
                }
            }


            // If we've fetched and displayed the room preview, handle any responses to
            // the user clicking the join button (e.g., knocked on or joined the room/space).
            let mut transition_to_knocked = false;
            let mut transition_to_joined  = false;
            if let AddRoomState::FetchedRoomPreview { frp, room_or_alias_id, .. } = &self.state {
                for action in actions {
                    match action.downcast_ref() {
                        Some(KnockResultAction::Knocked { room, .. }) if room.room_id() == frp.room_name_id.room_id() => {
                            let room_type = match room.room_type() {
                                Some(RoomType::Space) => crate::i18n::tr("space"),
                                _ => crate::i18n::tr("room"),
                            };
                            enqueue_popup_notification(
                                crate::i18n::format("Successfully knocked on {room_type} {0}.", &[("room_type", (room_type).to_string()), ("0", (frp.room_name_id).to_string())]),
                                PopupKind::Success,
                                Some(4.0),
                            );
                            transition_to_knocked = true;
                            break;
                        }
                        Some(KnockResultAction::Failed { error, room_or_alias_id: roai }) if room_or_alias_id == roai => {
                            enqueue_popup_notification(
                                crate::i18n::format("Failed to knock on room.\n\nError: {error}.", &[("error", (error).to_string())]),
                                PopupKind::Error,
                                None,
                            );
                            break;
                        }
                        _ => { }
                    }

                    match action.downcast_ref() {
                        // Don't show success/failure popups here, they're shown in the backend task
                        // which is more consistent than doing it here, as the user may have left the AddRoom screen by then.
                        Some(JoinRoomResultAction::Joined { room_id }) if room_id == frp.room_name_id.room_id() => {
                            transition_to_joined = true;
                            break;
                        }
                        Some(JoinRoomResultAction::Failed { room_id, .. }) if room_id == frp.room_name_id.room_id() => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
            if transition_to_knocked {
                self.state.transition_to_knocked();
                self.redraw(cx);
            }
            if transition_to_joined {
                self.state.transition_to_joined();
                self.redraw(cx);
            }

            for action in actions {
                // If the room/space the user is searching for has been loaded from the homeserver
                // (e.g., by getting invited to it, or joining it in another client),
                // then update the state of 
                if let Some(AppStateAction::RoomLoadedSuccessfully { room_name_id, is_invite }) = action.downcast_ref() {
                    if self.state.fetched_room_preview().is_some_and(|frp| frp.room_name_id.room_id() == room_name_id.room_id()) {
                        self.state.transition_to_loaded(*is_invite);
                        self.redraw(cx);
                    }
                }
            }
        }
    }


    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let loading_room_view = self.view.view(cx, ids!(loading_room_view));
        let fetched_room_summary = self.view.view(cx, ids!(fetched_room_summary));
        let error_view = self.view.view(cx, ids!(error_view));

        self.view.view(cx, ids!(directory_results)).set_visible(cx, self.showing_results);
        self.view.view(cx, ids!(preview_scroll)).set_visible(cx, !self.showing_results);
        self.view.label(cx, ids!(directory_status)).set_text(cx, &self.directory_status);
        self.view.button(cx, ids!(more_results)).set_visible(cx, self.next_batch.is_some() && !self.searching);
        match &self.state {
            AddRoomState::WaitingOnUserInput => {
                loading_room_view.set_visible(cx, false);
                fetched_room_summary.set_visible(cx, false);
                error_view.set_visible(cx, false);
            }
            AddRoomState::ParseError(err_str) | AddRoomState::FetchError(err_str) => {
                loading_room_view.set_visible(cx, false);
                fetched_room_summary.set_visible(cx, false); 
                error_view.set_visible(cx, true);
                error_view.label(cx, ids!(error_text)).set_text(cx, err_str);
            }
            AddRoomState::Parsed { room_or_alias_id, .. } => {
                loading_room_view.set_visible(cx, true);
                loading_room_view.label(cx, ids!(loading_text)).set_text(
                    cx,
                    &crate::i18n::format("Fetching {room_or_alias_id}...", &[("room_or_alias_id", (room_or_alias_id).to_string())]),
                );
                fetched_room_summary.set_visible(cx, false); 
                error_view.set_visible(cx, false);
            }
            ars @ AddRoomState::FetchedRoomPreview { frp, .. } 
            | ars @ AddRoomState::Knocked { frp }
            | ars @ AddRoomState::Joined { frp } 
            | ars @ AddRoomState::Loaded { frp, .. } => {
                loading_room_view.set_visible(cx, false);
                fetched_room_summary.set_visible(cx, true);
                error_view.set_visible(cx, false);

                // Populate the content of the fetched room preview.
                let room_avatar = fetched_room_summary.avatar(cx, ids!(room_avatar));
                match &frp.room_avatar {
                    FetchedRoomAvatar::Members(_) => room_avatar.show_room_avatar(cx, &frp.room_avatar),
                    FetchedRoomAvatar::Text(text) => {
                        room_avatar.show_text(cx, None, None, text);
                    }
                    FetchedRoomAvatar::Image(avatar_image) => {
                        let res = room_avatar.show_image(
                            cx,
                            None,
                            |cx, img_ref| utils::load_avatar_image(&img_ref, cx, avatar_image),
                        );
                        if res.is_err() {
                            room_avatar.show_text(
                                cx,
                                None,
                                None,
                                frp.room_name_id.name_for_avatar().unwrap_or("?"),
                            );
                        }
                    }
                }

                let (room_or_space_lc, room_or_space_uc) = match &frp.room_type {
                    Some(RoomType::Space) => (crate::i18n::tr("space"), crate::i18n::tr("Space")),
                    _ => (crate::i18n::tr("room"), crate::i18n::tr("Room")),
                };
                let room_name = fetched_room_summary.label(cx, ids!(room_name));
                match frp.room_name_id.name_for_avatar() {
                    Some(n) => room_name.set_text(cx, n),
                    _ => room_name.set_text(cx, &crate::i18n::format("Unnamed {room_or_space_uc}, ID: {0}", &[("room_or_space_uc", (room_or_space_uc).to_string()), ("0", (frp.room_name_id.room_id()).to_string())])),
                }

                fetched_room_summary.label(cx, ids!(subsection_alias_id)).set_text(
                    cx,
                    &crate::i18n::format("Main {room_or_space_uc} Alias and ID", &[("room_or_space_uc", (room_or_space_uc).to_string())]),
                );
                fetched_room_summary.label(cx, ids!(room_alias)).set_text(
                    cx,
                    &crate::i18n::format("Alias: {0}", &[("0", (frp.canonical_alias.as_ref().map_or("not set", |a| a.as_str())).to_string())]),
                );
                fetched_room_summary.label(cx, ids!(room_id)).set_text(
                    cx,
                    &crate::i18n::format("ID: {0}", &[("0", (frp.room_name_id.room_id().as_str()).to_string())]),
                );
                fetched_room_summary.label(cx, ids!(subsection_topic)).set_text(
                    cx,
                    &crate::i18n::format("{room_or_space_uc} Topic", &[("room_or_space_uc", (room_or_space_uc).to_string())]),
                );
                fetched_room_summary.html(cx, ids!(room_topic)).set_text(
                    cx,
                    frp.topic.as_deref().unwrap_or("<i>No topic set</i>"),
                );

                let room_summary = fetched_room_summary.label(cx, ids!(room_summary));
                let join_room_button = fetched_room_summary.button(cx, ids!(join_room_button));
                let join_function = match (&frp.state, &frp.join_rule) {
                    (Some(RoomState::Joined), _) => {
                        room_summary.set_text(cx, &crate::i18n::format("You have already joined this {room_or_space_lc}.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, &crate::i18n::format("Go to {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        JoinButtonFunction::NavigateOrJoin
                    }
                    (Some(RoomState::Banned), _) => {
                        room_summary.set_text(cx, &crate::i18n::format("You have been banned from this {room_or_space_lc}.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, crate::i18n::tr("Cannot join until un-banned"));
                        JoinButtonFunction::None
                    }
                    (Some(RoomState::Invited), _) => {
                        room_summary.set_text(cx, &crate::i18n::format("You have already been invited to this {room_or_space_lc}.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, crate::i18n::tr("Go to invitation"));
                        JoinButtonFunction::NavigateOrJoin
                    }
                    (Some(RoomState::Knocked), _) => {
                        room_summary.set_text(cx, &crate::i18n::format("You have already knocked on this {room_or_space_lc}.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, crate::i18n::tr("Knock again (be nice!)"));
                        JoinButtonFunction::Knock
                    }
                    (Some(RoomState::Left), join_rule) => {
                        room_summary.set_text(cx, &crate::i18n::format("You previously left this {room_or_space_lc}.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        let (join_room_text, join_function) = match join_rule {
                            Some(JoinRuleSummary::Public) => (
                                crate::i18n::format("Re-join this {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::NavigateOrJoin,
                            ),
                            Some(JoinRuleSummary::Invite) => (
                                crate::i18n::format("Re-joining {room_or_space_lc} requires an invite", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                            Some(JoinRuleSummary::Knock | JoinRuleSummary::KnockRestricted(_)) => (
                                crate::i18n::format("Knock to re-join {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::Knock,
                            ),
                            // TODO: handle this after we update matrix-sdk to the new `JoinRule` enum.
                            Some(JoinRuleSummary::Restricted(_)) => (
                                crate::i18n::format("Re-joining {room_or_space_lc} requires an invite or other room membership", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                            _ => (
                                crate::i18n::format("Not allowed to re-join this {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                        };
                        join_room_button.set_text(cx, &join_room_text);
                        join_function
                    }
                    // This room is not yet known to the user.
                    (None, join_rule) => {
                        let direct = if frp.is_direct == Some(true) { crate::i18n::tr("direct") } else { crate::i18n::tr("regular") };
                        room_summary.set_text(cx, &crate::i18n::format("This is a {direct} {room_or_space_lc} with {0} {1}.", &[("direct", (direct).to_string()), ("room_or_space_lc", (room_or_space_lc).to_string()), ("0", (frp.num_joined_members).to_string()), ("1", (match frp.num_joined_members {
                                1 => crate::i18n::tr("member"),
                                _ => crate::i18n::tr("members"),
                            }).to_string())]));

                        let (join_room_text, join_function) = match join_rule {
                            Some(JoinRuleSummary::Public) => (
                                crate::i18n::format("Join this {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::NavigateOrJoin,
                            ),
                            Some(JoinRuleSummary::Invite) => (
                                crate::i18n::format("Joining {room_or_space_lc} requires an invite", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                            Some(JoinRuleSummary::Knock | JoinRuleSummary::KnockRestricted(_)) => (
                                crate::i18n::format("Knock to join {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::Knock,
                            ),
                            // TODO: handle this after we update matrix-sdk to the new `JoinRule` enum.
                            Some(JoinRuleSummary::Restricted(_)) => (
                                crate::i18n::format("Joining {room_or_space_lc} requires an invite or other room membership", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                            _ => ( 
                                crate::i18n::format("Not allowed to join this {room_or_space_lc}", &[("room_or_space_lc", (room_or_space_lc).to_string())]),
                                JoinButtonFunction::None,
                            ),
                        };
                        join_room_button.set_text(cx, &join_room_text);
                        join_function
                    }
                };

                match ars {
                    AddRoomState::FetchedRoomPreview { .. } => {
                        join_room_button.set_enabled(cx, !matches!(join_function, JoinButtonFunction::None));
                        self.join_function = join_function;
                    }
                    AddRoomState::Knocked { .. } => {
                        room_summary.set_text(cx, &crate::i18n::format("You have knocked on this {room_or_space_lc} and must now wait for someone to invite you in.", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, crate::i18n::tr("Successfully knocked!"));
                        join_room_button.set_enabled(cx, false);
                    }
                    AddRoomState::Joined { .. } => {
                        room_summary.set_text(cx, &crate::i18n::format("You have joined this {room_or_space_lc}. It is now being loaded from the homeserver; please wait...", &[("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_text(cx, crate::i18n::tr("Successfully joined!"));
                        join_room_button.set_enabled(cx, false);
                    }
                    AddRoomState::Loaded { is_invite, .. } => {
                        let verb = if *is_invite { crate::i18n::tr("been invited to") } else { crate::i18n::tr("fully joined") };
                        room_summary.set_text(cx, &crate::i18n::format("You have {verb} this {room_or_space_lc}.", &[("verb", (verb).to_string()), ("room_or_space_lc", (room_or_space_lc).to_string())]));
                        let adj = if *is_invite { crate::i18n::tr("invited") } else { crate::i18n::tr("joined") };
                        join_room_button.set_text(cx, &crate::i18n::format("Go to {adj} {room_or_space_lc}", &[("adj", (adj).to_string()), ("room_or_space_lc", (room_or_space_lc).to_string())]));
                        join_room_button.set_enabled(cx, true);
                        self.join_function = JoinButtonFunction::NavigateOrJoin;
                    }
                    _ => {}
                }
            }
        }

        while let Some(widget) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = widget.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.results.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(room) = self.results.get(index) else { continue; };
                    let item = list.item(cx, index, id!(Result));
                    item.label(cx, ids!(name)).set_text(cx, &room.name);
                    item.label(cx, ids!(description)).set_text(cx, &room.description);
                    item.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}


/// The function to perform when the user clicks the join button in the fetched room preview.
enum JoinButtonFunction {
    None,
    /// Navigate to an already-known room/space, or join it if possible.
    NavigateOrJoin,
    /// Knock on (request to join) a room/space.
    Knock,
}
 

/// Actions sent from the backend task as a result of a [`MatrixRequest::Knock`].
#[derive(Debug)]
pub enum KnockResultAction {
    /// The user successfully knocked on the room/space.
    Knocked {
        /// The room alias/ID that was originally sent with the knock request.
        room_or_alias_id: OwnedRoomOrAliasId,
        /// The room that was knocked on.
        room: matrix_sdk::Room,
    },
    /// There was an error attempting to knock on the room.
    Failed {
        /// The room alias/ID that was originally sent with the knock request.
        room_or_alias_id: OwnedRoomOrAliasId,
        error: matrix_sdk::Error,
    }
}


/// Tries to extract a room address (Alias or ID) from the given text.
///
/// This function is quite flexible and will attempt to parse `text` as:
/// * A Room ID (with a leading `!`).
/// * A Room Alias (with a leading `#`).
/// * A `https://matrix.to` URI, which includes either a room alias, or a room ID plus `via` servers.
/// * A `matrix:` scheme URI, which is similar to above.
fn parse_address(text: &str) -> Result<(OwnedRoomOrAliasId, Vec<OwnedServerName>), IdParseError> {
    match OwnedRoomOrAliasId::try_from(text) {
        Ok(room_or_alias_id) => Ok((room_or_alias_id, Vec::new())),
        Err(e) => {
            let uri_result = MatrixToUri::parse(text)
                .map(|uri| (uri.id().clone(), uri.via().to_owned()))
                .or_else(|_| MatrixUri::parse(text).map(|uri| (uri.id().clone(), uri.via().to_owned())));
            
            if let Ok((matrix_id, via)) = uri_result {
                if let Some(room_or_alias_id) = match matrix_id {
                    MatrixId::Room(room_id) => Some(room_id.into()),
                    MatrixId::RoomAlias(alias) => Some(alias.into()),
                    MatrixId::Event(room_or_alias_id, _) => Some(room_or_alias_id),
                    _ => None,
                } {
                    return Ok((room_or_alias_id, via));
                }
            }
            Err(e)
        }
    }    
}

#[derive(Debug)]
enum RoomQuery {
    Name(String),
    Address(OwnedRoomOrAliasId, Vec<OwnedServerName>),
}

fn room_query(text: &str) -> Result<RoomQuery, String> {
    let text = text.trim();
    if text.is_empty() { return Err("Enter a room name, alias or link.".into()); }
    if let Ok((id, via)) = parse_address(text) { return Ok(RoomQuery::Address(id, via)); }
    // Preserve helpful errors for malformed addresses; a bare #name is still a
    // name query, so users need not supply a homeserver just to search.
    if text.starts_with('!') || text.starts_with('@') || text.contains("://")
        || text.starts_with("matrix:") || (text.starts_with('#') && text.contains(':')) {
        return Err("This room address is invalid. Enter a room name, #alias:server, !id:server or Matrix room link.".into());
    }
    let name = text.strip_prefix('#').unwrap_or(text).trim();
    if name.is_empty() { return Err("Enter a room name after #.".into()); }
    Ok(RoomQuery::Name(name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_plain_partial_and_unicode_room_names() {
        for (input, expected) in [("  Rust  ", "Rust"), ("设计讨论", "设计讨论"), ("#rust", "rust"), ("Project: chat", "Project: chat")] {
            assert!(matches!(room_query(input), Ok(RoomQuery::Name(name)) if name == expected));
        }
    }
    #[test]
    fn keeps_matrix_addresses_and_routing_servers() {
        for input in ["#rust:matrix.org", "!abc:example.org", "!opaque", "https://matrix.to/#/#rust:matrix.org", "matrix:r/rust:matrix.org"] {
            assert!(matches!(room_query(input), Ok(RoomQuery::Address(..))), "{input}");
        }
        let RoomQuery::Address(_, via) = room_query("https://matrix.to/#/!abc:example.org?via=relay.example.org").unwrap() else { panic!() };
        assert_eq!(via[0].as_str(), "relay.example.org");
    }
    #[test]
    fn does_not_search_malformed_links_or_user_ids_as_room_names() {
        for input in ["", "   ", "#", "#  ", "#room:", "@user:matrix.org", "https://example.com", "https://matrix.to/#/@user:matrix.org", "matrix:u/user:matrix.org"] {
            assert!(room_query(input).is_err(), "{input}");
        }
    }
}
