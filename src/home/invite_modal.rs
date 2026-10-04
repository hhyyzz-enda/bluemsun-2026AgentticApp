//! A modal dialog for inviting a user to a room.

use makepad_widgets::*;
use ruma::OwnedUserId;

use crate::home::room_screen::InviteResultAction;
use crate::sliding_sync::{MatrixRequest, get_client, spawn_async_task, submit_async_request};
use crate::utils::RoomNameId;


script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*


    mod.widgets.InviteModal = set_type_default() do #(InviteModal::register_widget(vm)) {
        ..mod.widgets.SmallModal

        title := ModalTitle {}

        registered_agents := DropDown {width: Fill labels: ["People you know"]}
        user_id_input := RobrixTextInput {
            draw_text +: {
                text_style: REGULAR_TEXT {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)},
                color: mod.widgets.RINX_INK
            }
            empty_text: #(crate::i18n::tr("Name or @user:example.org")) i18n_empty_text: "Name or @user:example.org",
            autocapitalize: None,
            autocorrect: Disabled,
        }

        buttons_view := ModalButtonsRow {
            cancel_button := RobrixNeutralIconButton {
                width: Fit{min: FitBound.Abs(120.0)},
                align: Align{x: 0.5, y: 0.5}
                padding: 12,
                draw_icon.svg: (ICON_FORBIDDEN)
                icon_walk: Walk{width: 16, height: 16, margin: Inset{left: -2, right: -1} }
                text: #(crate::i18n::tr("Cancel")) i18n_text: "Cancel"
            }

            confirm_button := RobrixPositiveIconButton {
                width: Fit{min: FitBound.Abs(120.0)}
                align: Align{x: 0.5, y: 0.5}
                padding: 12,
                draw_icon.svg: (ICON_ADD_USER)
                icon_walk: Walk{width: 16, height: 16, margin: Inset{left: -2, right: -1} }
                text: #(crate::i18n::tr("Invite")) i18n_text: "Invite"
            }

            okay_button := RobrixIconButton {
                visible: false
                width: Fit{min: FitBound.Abs(120.0)}
                align: Align{x: 0.5, y: 0.5}
                padding: 12,
                draw_icon.svg: (ICON_CHECKMARK)
                icon_walk: Walk{width: 16, height: 16, margin: Inset{left: -2, right: -1} }
                text: #(crate::i18n::tr("Okay")) i18n_text: "Okay"
            }
        }

        status_label_view := View {
            visible: false
            width: Fill,
            height: Fit,
            align: Align{x: 0.5, y: 0.0}

            status_label := Label {
                width: Fill,
                height: Fit,
                flow: Flow.Right{wrap: true},
                align: Align{x: 0.5, y: 0.0}
                margin: Inset{top: 10}
                draw_text +: {
                    text_style: REGULAR_TEXT {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)},
                    color: mod.widgets.RINX_INK
                }
                text: ""
            }
        }
    }
}

/// Actions emitted by other widgets to show or hide the `InviteModal`.
#[derive(Clone, Debug)]
pub enum InviteModalAction {
    /// Open the modal to invite a user to the given room or space.
    Open(RoomNameId),
    /// Close the modal.
    Close,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum InviteModalState {
    /// Waiting for the user to enter a user ID.
    #[default]
    WaitingForUserInput,
    /// Waiting for the invite to be sent.
    WaitingForInvite(OwnedUserId),
    /// The invite was sent successfully.
    InviteSuccess,
    /// An error occurred while sending the invite.
    InviteError,
}


/// People the invite dialog can offer, by Matrix ID and display name.
type Person = (OwnedUserId, Option<String>);

/// Results of the dialog's background lookups, matched to the request that
/// asked for them so a late answer never overwrites a newer one.
#[derive(Debug)]
enum InvitePeopleAction {
    /// Everyone who shares a joined room with the user.
    Known { request: u64, people: Vec<Person> },
    /// The homeserver directory's answer for a typed name.
    Searched { request: u64, query: String, people: Vec<Person> },
}

#[derive(Script, ScriptHook, Widget)]
pub struct InviteModal {
    #[deref] view: View,
    #[rust] state: InviteModalState,
    #[rust] room_name_id: Option<RoomNameId>,
    /// The dropdown's entries after its "People you know" header.
    #[rust] agents: Vec<OwnedUserId>,
    /// Registered agents and people who share a room with the user.
    #[rust] known: Vec<Person>,
    #[rust] request: u64,
}

impl Widget for InviteModal {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        self.widget_match_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl WidgetMatchEvent for InviteModal {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions, _scope: &mut Scope) {
        for action in actions {
            match action.downcast_ref() {
                Some(InvitePeopleAction::Known { request, people }) if *request == self.request => {
                    for person in people {
                        if !self.known.iter().any(|(id, _)| id == &person.0) {
                            self.known.push(person.clone());
                        }
                    }
                    self.known.sort_by_key(|(id, name)| (name.as_deref().unwrap_or(id.as_str()).to_lowercase(), id.clone()));
                    let known = self.known.clone();
                    self.offer(cx, &known);
                }
                Some(InvitePeopleAction::Searched { request, query, people }) if *request == self.request => {
                    let mut matches = self.local_matches(query);
                    for person in people {
                        if !matches.iter().any(|(id, _)| id == &person.0) {
                            matches.push(person.clone());
                        }
                    }
                    self.resolved(cx, query, matches);
                }
                _ => {}
            }
        }
        if let Some(index) = self.view.drop_down(cx, ids!(registered_agents)).selected(actions) && let Some(user) = index.checked_sub(1).and_then(|i| self.agents.get(i)) {
            self.view.text_input(cx, ids!(user_id_input)).set_text(cx, user.as_str());
        }
        let cancel_button = self.view.button(cx, ids!(cancel_button));

        // Handle canceling/closing the modal.
        let cancel_clicked = cancel_button.clicked(actions);
        if cancel_clicked ||
            actions.iter().any(|a| matches!(a.downcast_ref(), Some(ModalAction::Dismissed)))
        {
            // If the modal was dismissed by clicking outside of it, we MUST NOT emit
            // a `InviteModalAction::Close` action, as that would cause
            // an infinite action feedback loop.
            if cancel_clicked {
                cx.action(InviteModalAction::Close);
            }
            return;
        }

        // Handle the okay button (shown after invite success).
        let okay_button = self.view.button(cx, ids!(okay_button));
        if okay_button.clicked(actions) {
            cx.action(InviteModalAction::Close);
            return;
        }

        let confirm_button = self.view.button(cx, ids!(confirm_button));
        let user_id_input = self.view.text_input(cx, ids!(user_id_input));
        let status_view = self.view.view(cx, ids!(status_label_view));
        let mut status_label = self.view.label(cx, ids!(status_label_view.status_label));

        // Handle return key or invite button click.
        if let Some(user_id_str) = confirm_button.clicked(actions)
            .then(|| user_id_input.text())
            .or_else(|| user_id_input.returned(actions).map(|(t, _)| t))
        {
            // Validate the user ID
            if user_id_str.is_empty() {
                script_apply_eval!(cx, status_label, {
                    text: crate::i18n::tr("Please enter a user ID."),
                    draw_text +: {
                        color: mod.widgets.COLOR_FG_DANGER_RED,
                    },
                });
                status_view.set_visible(cx, true);
                self.view.redraw(cx);
                return;
            }

            // A full Matrix ID is invited as typed; anything else is a name.
            let parsed = if user_id_str.trim_start().starts_with('@') {
                ruma::UserId::parse(user_id_str.trim())
            } else {
                Err(ruma::IdParseError::MissingLeadingSigil)
            };
            if parsed.is_err() && !user_id_str.trim_start().starts_with('@') {
                self.search(cx, user_id_str.trim().to_owned());
                return;
            }
            match parsed {
                Ok(user_id) => {
                    if let Some(room_name_id) = &self.room_name_id {
                        submit_async_request(MatrixRequest::InviteUser {
                            room_id: room_name_id.room_id().clone(),
                            user_id: user_id.clone(),
                        });
                        self.state = InviteModalState::WaitingForInvite(user_id);
                        script_apply_eval!(cx, status_label, {
                            text: crate::i18n::tr("Sending invite..."),
                            draw_text +: {
                                color: mod.widgets.COLOR_ACTIVE_PRIMARY_DARKER,
                            },
                        });
                        status_view.set_visible(cx, true);
                        confirm_button.set_enabled(cx, false);
                        user_id_input.set_is_read_only(cx, true);
                    }
                }
                Err(_) => {
                    script_apply_eval!(cx, status_label, {
                        text: crate::i18n::tr("Invalid User ID. Expected format: @user:server.xyz"),
                        draw_text +: {
                            color: mod.widgets.COLOR_FG_DANGER_RED,
                        },
                    });
                    status_view.set_visible(cx, true);
                    user_id_input.set_key_focus(cx);
                }
            }
            self.view.redraw(cx);
        }

        // Handle the result of a previously-sent invite.
        if let InviteModalState::WaitingForInvite(invited_user_id) = &self.state {
            for action in actions {
                let new_state = match action.downcast_ref() {
                    Some(InviteResultAction::Sent { room_id, user_id })
                        if self.room_name_id.as_ref().is_some_and(|rni| rni.room_id() == room_id)
                            && invited_user_id == user_id
                    => {
                        let status = format!("Successfully invited {user_id}!");
                        script_apply_eval!(cx, status_label, {
                            text: #(status),
                            draw_text +: {
                                color: mod.widgets.COLOR_FG_ACCEPT_GREEN
                            }
                        });
                        status_view.set_visible(cx, true);
                        confirm_button.set_visible(cx, false);
                        cancel_button.set_visible(cx, false);
                        okay_button.set_visible(cx, true);
                        Some(InviteModalState::InviteSuccess)
                    }
                    Some(InviteResultAction::Failed { room_id, user_id, error })
                        if self.room_name_id.as_ref().is_some_and(|rni| rni.room_id() == room_id)
                            && invited_user_id == user_id
                    => {
                        let status = format!("Failed to send invite: {error}");
                        script_apply_eval!(cx, status_label, {
                            text: #(status),
                            draw_text +: {
                                color: mod.widgets.COLOR_FG_DANGER_RED,
                            }
                        });
                        status_view.set_visible(cx, true);
                        confirm_button.set_enabled(cx, true);
                        user_id_input.set_is_read_only(cx, false);
                        user_id_input.set_key_focus(cx);
                        Some(InviteModalState::InviteError)
                    }
                    _ => None,
                };
                if let Some(new_state) = new_state {
                    self.state = new_state;
                    self.view.redraw(cx);
                    break;
                }
            }
        }
    }
}

impl InviteModal {
    /// Put `people` in the dropdown, after its header.
    fn offer(&mut self, cx: &mut Cx, people: &[Person]) {
        self.agents = people.iter().map(|(id, _)| id.clone()).collect();
        let labels = std::iter::once(crate::i18n::tr("People you know").into())
            .chain(people.iter().map(|(id, name)| match name {
                Some(name) if !name.is_empty() => format!("{name} ({id})"),
                _ => id.to_string(),
            }))
            .collect();
        let drop_down = self.view.drop_down(cx, ids!(registered_agents));
        drop_down.set_labels(cx, labels);
        drop_down.set_selected_item(cx, 0);
        drop_down.set_visible(cx, !people.is_empty());
        self.view.redraw(cx);
    }

    /// Known people whose name or ID contains `query`, ignoring case.
    fn local_matches(&self, query: &str) -> Vec<Person> {
        let needle = query.to_lowercase();
        self.known
            .iter()
            .filter(|(id, name)| {
                id.as_str().to_lowercase().contains(&needle)
                    || name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect()
    }

    /// A typed name: ask the homeserver directory too, then resolve.
    fn search(&mut self, cx: &mut Cx, query: String) {
        self.request += 1;
        let request = self.request;
        self.status(cx, &crate::i18n::format("Searching for {0}…", &[("0", query.clone())]), false);
        let Some(client) = get_client() else {
            let matches = self.local_matches(&query);
            self.resolved(cx, &query, matches);
            return;
        };
        spawn_async_task(async move {
            let people = match client.search_users(&query, 20).await {
                Ok(found) => found
                    .results
                    .into_iter()
                    .filter(|u| Some(u.user_id.as_ref()) != client.user_id())
                    .map(|u| (u.user_id, u.display_name))
                    .collect(),
                Err(_) => Vec::new(),
            };
            Cx::post_action(InvitePeopleAction::Searched { request, query, people });
            SignalToUI::set_ui_signal();
        });
    }

    /// One match fills the field, ready to invite; several are offered in the
    /// dropdown; none asks for the full ID.
    fn resolved(&mut self, cx: &mut Cx, query: &str, matches: Vec<Person>) {
        match matches.as_slice() {
            [(id, _)] => {
                self.view.text_input(cx, ids!(user_id_input)).set_text(cx, id.as_str());
                self.status(cx, &crate::i18n::format("Found {0}. Press Invite to send.", &[("0", id.to_string())]), false);
            }
            [] => self.status(
                cx,
                &crate::i18n::format("No one found for \"{0}\". Enter their full ID, like @name:server.", &[("0", query.to_owned())]),
                true,
            ),
            _ => {
                self.offer(cx, &matches);
                self.status(cx, &crate::i18n::format("{0} people match. Pick one above.", &[("0", matches.len().to_string())]), false);
            }
        }
    }

    fn status(&mut self, cx: &mut Cx, text: &str, error: bool) {
        let mut label = self.view.label(cx, ids!(status_label_view.status_label));
        let text = text.to_owned();
        if error {
            script_apply_eval!(cx, label, { text: #(text), draw_text +: { color: mod.widgets.COLOR_FG_DANGER_RED } });
        } else {
            script_apply_eval!(cx, label, { text: #(text), draw_text +: { color: mod.widgets.COLOR_ACTIVE_PRIMARY_DARKER } });
        }
        self.view.view(cx, ids!(status_label_view)).set_visible(cx, true);
        self.view.redraw(cx);
    }

    /// Everyone who shares a joined room with the user, except the user and
    /// whoever is already in the target room.
    fn load_known(&mut self, room: ruma::OwnedRoomId) {
        let Some(client) = get_client() else { return };
        let request = self.request;
        spawn_async_task(async move {
            let me = client.user_id().map(ToOwned::to_owned);
            let present: Vec<OwnedUserId> = match client.get_room(&room) {
                Some(target) => target
                    .members_no_sync(matrix_sdk::RoomMemberships::JOIN | matrix_sdk::RoomMemberships::INVITE)
                    .await
                    .map(|members| members.iter().map(|m| m.user_id().to_owned()).collect())
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            let mut people: Vec<Person> = Vec::new();
            for joined in client.joined_rooms() {
                if crate::moments::is_moments(&joined) {
                    continue;
                }
                let Ok(members) = joined.members_no_sync(matrix_sdk::RoomMemberships::JOIN).await else {
                    continue;
                };
                for member in members {
                    let id = member.user_id().to_owned();
                    if Some(&id) == me.as_ref() || present.contains(&id) || people.iter().any(|(p, _)| p == &id) {
                        continue;
                    }
                    people.push((id, member.display_name().map(str::to_owned)));
                    if people.len() >= 500 {
                        break;
                    }
                }
            }
            Cx::post_action(InvitePeopleAction::Known { request, people });
            SignalToUI::set_ui_signal();
        });
    }

    pub fn show(&mut self, cx: &mut Cx, room_name_id: RoomNameId) {
        self.view.label(cx, ids!(title)).set_text(
            cx,
            &format!("Invite to {room_name_id}"),
        );
        let settings = crate::agent_access::current();
        self.request += 1;
        self.known = settings
            .agent_registry
            .agents()
            .map(|(id, entry)| (id.clone(), entry.display_name.clone()))
            .collect();
        let known = self.known.clone();
        self.offer(cx, &known);
        self.load_known(room_name_id.room_id().clone());
        self.state = InviteModalState::WaitingForUserInput;
        self.room_name_id = Some(room_name_id);

        // Reset the UI state
        let confirm_button = self.view.button(cx, ids!(confirm_button));
        let cancel_button = self.view.button(cx, ids!(cancel_button));
        let okay_button = self.view.button(cx, ids!(okay_button));
        let user_id_input = self.view.text_input(cx, ids!(user_id_input));
        confirm_button.set_visible(cx, true);
        confirm_button.set_enabled(cx, true);
        confirm_button.reset_hover(cx);
        cancel_button.set_visible(cx, true);
        cancel_button.set_enabled(cx, true);
        cancel_button.reset_hover(cx);
        okay_button.set_visible(cx, false);
        okay_button.reset_hover(cx);
        user_id_input.set_is_read_only(cx, false);
        user_id_input.set_text(cx, "");
        self.view.view(cx, ids!(status_label_view)).set_visible(cx, false);
        self.view.label(cx, ids!(status_label_view.status_label)).set_text(cx, "");
        self.view.redraw(cx);
        user_id_input.set_key_focus(cx);
    }
}

impl InviteModalRef {
    pub fn show(&self, cx: &mut Cx, room_name_id: RoomNameId) {
        let Some(mut inner) = self.borrow_mut() else { return };
        inner.show(cx, room_name_id);
    }
}
