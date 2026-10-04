//! Privacy-related settings within the SettingsScreen.
//!
//! Currently this just includes the list of blocked/ignored users.

use makepad_widgets::*;
use matrix_sdk::ruma::OwnedUserId;

use crate::{
    block_user_modal::{BlockUserModalAction, BlockUserRequest},
    moments::dm_sharing::{SharingSettingChanged, change_sharing_setting, sharing_setting},
    sliding_sync::{BlockedUsersUpdated, get_blocked_users},
};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // A single blocked user in the blocked users list.
    mod.widgets.BlockedUserEntry = #(BlockedUserEntry::register_widget(vm)) {
        width: Fill, height: Fit
        flow: Down

        View {
            width: Fill, height: Fit
            flow: Right,
            padding: 10,
            spacing: 10,
            align: Align{y: 0.5}

            unblock_button := RobrixPositiveIconButton {
                height: mod.widgets.SETTINGS_BUTTON_HEIGHT,
                padding: Inset{left: 12, right: 15}
                icon_walk: Walk{width: 0, height: 0}
                spacing: 0
                text: #(crate::i18n::tr("Unblock")) i18n_text: "Unblock"
            }

            user_id := Label {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true},
                max_lines: 2,
                text_overflow: Ellipsis,
                draw_text +: {
                    color: (MESSAGE_TEXT_COLOR),
                    text_style: theme.font_bold { font_size: (12 * mod.widgets.RINX_TEXT_SCALE) },
                }
            }
        }

        LineH { padding: 10, margin: Inset{left: 5, right: 5} }
    }

    // One room the assistant may read, with its revoke button.
    mod.widgets.AssistantRoomGrantEntry = #(AssistantRoomGrantEntry::register_widget(vm)) {
        width: Fill, height: Fit
        flow: Down

        View {
            width: Fill, height: Fit
            flow: Right,
            padding: 10,
            spacing: 10,
            align: Align{y: 0.5}

            revoke_button := RobrixNegativeIconButton {
                height: mod.widgets.SETTINGS_BUTTON_HEIGHT,
                padding: Inset{left: 12, right: 15}
                icon_walk: Walk{width: 0, height: 0}
                spacing: 0
                text: #(crate::i18n::tr("Revoke")) i18n_text: "Revoke"
            }

            room_name := Label {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true},
                max_lines: 2,
                text_overflow: Ellipsis,
                draw_text +: {
                    color: (MESSAGE_TEXT_COLOR),
                    text_style: theme.font_bold { font_size: (12 * mod.widgets.RINX_TEXT_SCALE) },
                }
            }
        }

        LineH { padding: 10, margin: Inset{left: 5, right: 5} }
    }

    mod.widgets.PrivacySettings = #(PrivacySettings::register_widget(vm)) {
        width: Fill, height: Fit
        flow: Down

        LineH { width: 425, padding: 10, margin: Inset{top: 20, bottom: 5} }

        TitleLabel {
            text: #(crate::i18n::tr("Privacy Settings")) i18n_text: "Privacy Settings"
        }

        SubsectionLabel {
            text: #(crate::i18n::tr("Moments")) i18n_text: "Moments"
        }

        moments_sharing_toggle := ToggleFlat {
            margin: Inset{left: 6.5, top: 5, bottom: 4}
            padding: Inset { left: 15}
            // Set from the loaded setting by `sync_moments_sharing()`.
            draw_bg +: { size: 21 }
            text: #(crate::i18n::tr("Share Moments with DM contacts")) i18n_text: "Share Moments with DM contacts"
            draw_text +: {
                text_style: mod.widgets.SETTINGS_BOLD_TEXT_STYLE {},
            }
        }
        mod.widgets.SettingsSectionDescription {
            body: #(crate::i18n::tr("<ul><li>On by default. People you chat with 1-on-1 who also use Rinx see your Moments, and you see theirs. People on other apps are never invited.</li><li>Your Matrix profile shows that you share this way.</li></ul>")) i18n_body: "<ul><li>On by default. People you chat with 1-on-1 who also use Rinx see your Moments, and you see theirs. People on other apps are never invited.</li><li>Your Matrix profile shows that you share this way.</li></ul>"
        }

        SubsectionLabel {
            text: #(crate::i18n::tr("Assistant access")) i18n_text: "Assistant access"
        }
        mod.widgets.SettingsSectionDescription {
            body: #(crate::i18n::tr("<ul><li>Chats this account lets the assistant read. You chose \"Always allow\" for each; revoke one and the assistant asks again.</li></ul>")) i18n_body: "<ul><li>Chats this account lets the assistant read. You chose \"Always allow\" for each; revoke one and the assistant asks again.</li></ul>"
        }
        no_assistant_rooms_label := View {
            width: Fill, height: Fit
            Label {
                width: Fill, height: Fit
                margin: Inset{top: 10, bottom: 8, left: 13, right: 10},
                flow: Flow.Right{wrap: true},
                draw_text +: {
                    color: (COLOR_TEXT_WARNING_NOT_FOUND),
                    text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                }
                text: #(crate::i18n::tr("The assistant may not read any chat.")) i18n_text: "The assistant may not read any chat."
            }
        }
        RoundedView {
            width: Fill, height: Fit
            margin: 5,
            show_bg: true,
            draw_bg +: {
                color: mod.widgets.RINX_FIELD
                border_radius: 4.0
            }
            assistant_rooms_list := FlatList {
                width: Fill,
                height: Fit,
                spacing: 0.0
                flow: Down,
                grab_key_focus: true,
                drag_scrolling: true,
                scroll_bars: ScrollBars { show_scroll_x: false, show_scroll_y: false },
                assistant_room_entry := mod.widgets.AssistantRoomGrantEntry { }
            }
        }

        SubsectionLabel {
            text: #(crate::i18n::tr("Blocked Users")) i18n_text: "Blocked Users"
        }

        mod.widgets.SettingsSectionDescription {
            body: #(crate::i18n::tr("<ul><li>You won't see any messages or invites from a blocked user, in any room.</li></ul>")) i18n_body: "<ul><li>You won't see any messages or invites from a blocked user, in any room.</li></ul>"
        }

        no_blocked_users_label := View {
            width: Fill, height: Fit
            Label {
                width: Fill, height: Fit
                margin: Inset{top: 10, bottom: 8, left: 13, right: 10},
                flow: Flow.Right{wrap: true},
                draw_text +: {
                    color: (COLOR_TEXT_WARNING_NOT_FOUND),
                    text_style: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                }
                text: #(crate::i18n::tr("You haven't blocked anyone.")) i18n_text: "You haven't blocked anyone."
            }
        }

        RoundedView {
            width: Fill, height: Fit
            margin: 5,

            show_bg: true,
            draw_bg +: {
                color: mod.widgets.RINX_FIELD
                border_radius: 4.0
            }

            blocked_users_list := FlatList {
                width: Fill,
                height: Fit,
                spacing: 0.0
                flow: Down,

                grab_key_focus: true,
                drag_scrolling: true,
                scroll_bars: ScrollBars { show_scroll_x: false, show_scroll_y: false },

                blocked_user_entry := mod.widgets.BlockedUserEntry { }
            }
        }
    }
}


/// A single blocked user shown in the list of blocked users.
///
/// Note that this intentionally excludes the blocked users' display names
/// and avatars, and they may be offensive and our current user won't want to see them.
#[derive(Script, ScriptHook, Widget)]
pub struct BlockedUserEntry {
    #[deref] view: View,
    #[rust] user_id: Option<OwnedUserId>,
}

impl Widget for BlockedUserEntry {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        let Some(user_id) = self.user_id.as_ref() else { return };
        let Event::Actions(actions) = event else { return };
        if self.view.button(cx, ids!(unblock_button)).clicked(actions) {
            cx.action(BlockUserModalAction::Open(BlockUserRequest {
                user_id: user_id.clone(),
                display_name: None,
                block: false,
                reject_invite_to: None,
            }));
        }
        if actions.iter().any(|a| matches!(a.downcast_ref(), Some(BlockUserModalAction::Close))) {
            self.view.button(cx, ids!(unblock_button)).reset_hover(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let user_id = scope.props.get::<OwnedUserId>().unwrap();
        self.view.label(cx, ids!(user_id)).set_text(cx, user_id.as_str());
        if self.user_id.as_ref() != Some(user_id) {
            self.user_id = Some(user_id.clone());
        }

        self.view.draw_walk(cx, scope, walk)
    }
}


/// One room the assistant may read, shown with its name when Rinx knows it.
#[derive(Script, ScriptHook, Widget)]
pub struct AssistantRoomGrantEntry {
    #[deref] view: View,
    #[rust] room_id: Option<String>,
}

impl Widget for AssistantRoomGrantEntry {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else { return };
        if let Some(room_id) = self.room_id.clone()
            && self.view.button(cx, ids!(revoke_button)).clicked(actions)
            && let Err(e) = crate::assistant::revoke_room(cx, &room_id)
        {
            error!("Could not revoke the assistant's access to {room_id}: {e}");
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let room_id = scope.props.get::<String>().unwrap().clone();
        let name = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str())
            .ok()
            .filter(|_| cx.has_global::<crate::home::rooms_list::RoomsListRef>())
            .and_then(|id| cx.get_global::<crate::home::rooms_list::RoomsListRef>().get_room_name(&id))
            .map(|name| format!("{} ({room_id})", name.display()))
            .unwrap_or_else(|| room_id.clone());
        self.view.label(cx, ids!(room_name)).set_text(cx, &name);
        self.room_id = Some(room_id);
        self.view.draw_walk(cx, scope, walk)
    }
}

/// The privacy settings section, which lists the users blocked by this account.
#[derive(Script, ScriptHook, Widget)]
pub struct PrivacySettings {
    #[deref] view: View,

    /// The blocked users known to this widget, or `None` if they haven't been loaded yet.
    #[rust] blocked_users: Option<Vec<OwnedUserId>>,
    /// The rooms the signed-in account lets the assistant read.
    #[rust] assistant_rooms: Option<Vec<String>>,
}

impl Widget for PrivacySettings {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(BlockedUsersUpdated(blocked_users)) = action.downcast_ref() {
                    self.blocked_users = Some(blocked_users.clone());
                    self.view.redraw(cx);
                }
                if matches!(action.downcast_ref(), Some(crate::assistant::AssistantAction::GrantsChanged)) {
                    self.assistant_rooms = Some(crate::assistant::granted_rooms());
                    self.view.redraw(cx);
                }
                if action.downcast_ref::<SharingSettingChanged>().is_some() {
                    self.sync_moments_sharing(cx);
                }
            }
            if let Some(share) = self.view.check_box(cx, ids!(moments_sharing_toggle)).changed(actions) {
                change_sharing_setting(share);
            }
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let blocked_users = &*self.blocked_users.get_or_insert_with(get_blocked_users);
        self.view.view(cx, ids!(no_blocked_users_label)).set_visible(cx, blocked_users.is_empty());
        let assistant_rooms = &*self.assistant_rooms.get_or_insert_with(crate::assistant::granted_rooms);
        self.view.view(cx, ids!(no_assistant_rooms_label)).set_visible(cx, assistant_rooms.is_empty());
        let assistant_list = self.view.flat_list(cx, ids!(assistant_rooms_list)).widget_uid();

        while let Some(subview) = self.view.draw_walk(cx, scope, walk).step() {
            if subview.widget_uid() == assistant_list {
                let list_ref = subview.as_flat_list();
                let Some(mut list) = list_ref.borrow_mut() else { continue };
                for room_id in assistant_rooms {
                    if let Some(item) = list.item(cx, LiveId::from_str(room_id), id!(assistant_room_entry)) {
                        item.draw_all(cx, &mut Scope::with_props(room_id));
                    }
                }
                list.items.retain_visible();
                continue;
            }
            // Otherwise, the blocked users list.
            let flat_list_ref = subview.as_flat_list();
            let Some(mut list) = flat_list_ref.borrow_mut() else {
                error!("!!! PrivacySettings::draw_walk(): BUG: expected a FlatList widget, but got something else");
                continue;
            };
            for user_id in blocked_users {
                if let Some(item) = list.item(cx, LiveId::from_str(user_id.as_str()), id!(blocked_user_entry)) {
                    item.draw_all(cx, &mut Scope::with_props(user_id));
                }
            }
            // Drop the entries for any users who were unblocked and removed from this list.
            list.items.retain_visible();
        }
        DrawStep::done()
    }
}

impl PrivacySettings {
    /// Shows the current Moments sharing setting on its switch.
    ///
    /// Called from event handling rather than drawing: `set_active` moves the switch
    /// through its animator, which doesn't take effect when called mid-draw.
    fn sync_moments_sharing(&mut self, cx: &mut Cx) {
        if let Some(share) = sharing_setting() {
            self.view
                .check_box(cx, ids!(moments_sharing_toggle))
                .set_active(cx, share, Animate::No);
        }
        self.view.redraw(cx);
    }
}

impl PrivacySettingsRef {
    /// Reloads the list of blocked users and the Moments sharing setting shown by this section.
    pub fn populate(&self, cx: &mut Cx) {
        let Some(mut inner) = self.borrow_mut() else { return };
        inner.blocked_users = Some(get_blocked_users());
        inner.assistant_rooms = Some(crate::assistant::granted_rooms());
        inner.sync_moments_sharing(cx);
        inner.redraw(cx);
    }
}
