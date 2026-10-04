//! Mobile room information backed by the current Matrix room and push rules.

use makepad_widgets::*;
use matrix_sdk::{notification_settings::RoomNotificationMode, ruma::OwnedUserId, RoomMemberships};

use crate::{
    home::{invite_modal::InviteModalAction, rooms_list::RoomNotificationModeUpdated},
    logout::logout_confirm_modal::LogoutAction,
    profile::user_profile::{ShowUserProfileAction, UserProfile, UserProfileAndRoomId},
    shared::{avatar::{AvatarState, AvatarWidgetRefExt}, navigation_bar_button::NavigationBarButtonWidgetRefExt},
    sliding_sync::{current_user_id, get_client, spawn_async_task, submit_async_request, MatrixRequest},
    utils::{self, RoomNameId},
};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.MobileChatInfo = #(MobileChatInfo::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down
        draw_bg.color: mod.widgets.MOBILE_BG
        status := Label {
            width: Fill height: Fit margin: 16
            flow: Flow.Right{wrap: true}
            draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
        }
        retry := RobrixNeutralIconButton {visible: false text: #(crate::i18n::tr("Retry")) i18n_text: "Retry" height: 44 margin: 12}
        list := PortalList {
            width: Fill height: Fill flow: Down
            Header := mod.widgets.MobileSection {
                padding: 20 spacing: 8 margin: Inset{bottom: 8}
                name := Label {
                    width: Fill flow: Flow.Right{wrap: true}
                    draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_bold {font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}
                }
                topic := Label {
                    width: Fill flow: Flow.Right{wrap: true}
                    draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                }
                member_count := Label {
                    draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                }
            }
            Member := mod.widgets.MobileSection {
                // Clicking a member opens their profile (DM, block, ...).
                row := NavigationBarButton {
                    width: Fill height: 62 flow: Right spacing: 12 padding: Inset{left: 20 right: 20}
                    align: Align{y: 0.5}
                    draw_bg +: {color_hover: mod.widgets.RINX_HOVER color_active: mod.widgets.RINX_HOVER border_radius: 0}
                    avatar := mod.widgets.MobileAvatar {width: 40 height: 40}
                    View {
                        width: Fill height: Fit flow: Down spacing: 3
                        name := Label {
                            width: Fill max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}
                        }
                        user_id := Label {
                            width: Fill max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (9 * mod.widgets.RINX_TEXT_SCALE)}}
                        }
                    }
                }
            }
            Action := mod.widgets.MobileSection {
                row := NavigationBarButton {
                    width: Fill height: 56 flow: Right spacing: 10
                    padding: Inset{left: 20 right: 20} align: Align{y: 0.5}
                    draw_bg +: {color_hover: mod.widgets.RINX_HOVER color_active: mod.widgets.RINX_HOVER border_radius: 0}
                    title := Label {
                        width: Fill max_lines: 1 text_overflow: Ellipsis
                        draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12.5 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                    value := Label {
                        max_lines: 1
                        draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                    Icon {
                        icon_walk: Walk{width: 8 height: 13}
                        draw_icon +: {color: mod.widgets.RINX_DISABLED svg: ICON_CHEVRON_RIGHT}
                    }
                }
                mod.widgets.MobileDivider {margin: Inset{left: 20}}
            }
            Filler := View {height: 100 width: Fill}
        }
    }
}

#[derive(Clone, Debug)]
struct ChatInfoData {
    room: RoomNameId,
    topic: String,
    members: Vec<UserProfile>,
    mode: Option<RoomNotificationMode>,
}

#[derive(Debug)]
struct ChatInfoLoaded {
    owner: OwnedUserId,
    request: u64,
    result: Result<ChatInfoData, String>,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MobileChatInfo {
    #[source] source: ScriptObjectRef,
    #[deref] view: View,
    #[rust] owner: Option<OwnedUserId>,
    #[rust] room: Option<RoomNameId>,
    #[rust] request: u64,
    #[rust] data: Option<ChatInfoData>,
    #[rust] status: String,
    #[rust] failed: bool,
    #[rust] choosing_notifications: bool,
}

fn modes() -> [(Option<RoomNotificationMode>, &'static str); 4] { [
    (None, crate::i18n::tr("Use Account Defaults")),
    (Some(RoomNotificationMode::AllMessages), crate::i18n::tr("All Messages")),
    (Some(RoomNotificationMode::MentionsAndKeywordsOnly), crate::i18n::tr("Mentions and Keywords")),
    (Some(RoomNotificationMode::Mute), crate::i18n::tr("Mute Notifications")),
] }

impl MobileChatInfo {
    fn load(&mut self, cx: &mut Cx, room: RoomNameId) {
        self.clear();
        self.room = Some(room.clone());
        let Some(client) = get_client() else { return };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else { return };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.request = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.owner = Some(owner.clone());
        let request = self.request;
        self.status = crate::i18n::tr("Loading chat information…").into();
        self.view.portal_list(cx, ids!(list)).set_first_id_and_scroll(0, 0.0);
        self.view.redraw(cx);
        spawn_async_task(async move {
            let result = async {
                let room = client.get_room(room.room_id()).ok_or(crate::i18n::tr("This chat is no longer available."))?;
                let members = room.members(RoomMemberships::JOIN).await.map_err(|e| e.to_string())?;
                let mut members: Vec<_> = members.into_iter().map(|member| UserProfile {
                    user_id: member.user_id().to_owned(),
                    username: member.display_name().map(ToOwned::to_owned),
                    avatar_state: AvatarState::Known(member.avatar_url().map(ToOwned::to_owned)),
                }).collect();
                members.sort_by_key(|p| (p.displayable_name().to_lowercase(), p.user_id.clone()));
                Ok(ChatInfoData {
                    room: RoomNameId::from_room(&room).await,
                    topic: room.topic().unwrap_or_default(),
                    members,
                    mode: room.user_defined_notification_mode().await,
                })
            }.await;
            Cx::post_action(ChatInfoLoaded {owner, request, result});
        });
    }

    fn clear(&mut self) {
        self.owner = None;
        self.room = None;
        self.request = 0;
        self.data = None;
        self.status.clear();
        self.failed = false;
        self.choosing_notifications = false;
    }
}

impl Widget for MobileChatInfo {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.owner.is_some() && self.owner != current_user_id() { self.clear(); }
        if matches!(event, Event::Signal) {
            crate::avatar_cache::process_avatar_updates(cx);
            self.view.redraw(cx);
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            for action in actions {
                if matches!(action.downcast_ref::<LogoutAction>(), Some(LogoutAction::ClearAppState {..})) { self.clear(); }
                if let Some(loaded) = action.downcast_ref::<ChatInfoLoaded>() {
                    if self.owner.as_ref() == Some(&loaded.owner) && self.request == loaded.request {
                        match &loaded.result {
                            Ok(data) => { self.data = Some(data.clone()); self.status.clear(); }
                            Err(error) => { self.status = crate::i18n::format("Could not load chat information: {error}", &[("error", (error).to_string())]); self.failed = true; }
                        }
                        self.view.redraw(cx);
                    }
                }
                if let Some(update) = action.downcast_ref::<RoomNotificationModeUpdated>() {
                    if let Some(data) = self.data.as_mut() {
                        if data.room.room_id() == &update.room_id {
                            data.mode = update.mode;
                            self.view.redraw(cx);
                        }
                    }
                }
            }
            if self.view.button(cx, ids!(retry)).clicked(actions) {
                if let Some(room) = self.room.clone() { self.load(cx, room); }
                return;
            }
            let list = self.view.portal_list(cx, ids!(list));
            for (index, widget) in list.items_with_actions(actions) {
                if list.was_scrolling() || !widget.navigation_bar_button(cx, ids!(row)).clicked(actions) { continue; }
                let Some(data) = self.data.as_ref() else { continue };
                if let Some(profile) = index.checked_sub(3).and_then(|i| data.members.get(i)) {
                    // The enclosing RoomScreen shows this in its user profile pane.
                    cx.widget_action(
                        self.widget_uid(),
                        ShowUserProfileAction::ShowUserProfile(UserProfileAndRoomId {
                            user_profile: profile.clone(),
                            room_id: data.room.room_id().clone(),
                        }),
                    );
                    break;
                }
                if index == 1 || index == 2 {
                    cx.action(super::room_history::RoomHistoryAction::Open {
                        room: data.room.clone(),
                        filter: if index == 1 {super::room_history::HistoryFilter::Messages} else {super::room_history::HistoryFilter::Media},
                    });
                    break;
                }
                let Some(index) = index.checked_sub(data.members.len() + 3) else { continue };
                if self.choosing_notifications {
                    if let Some((mode, _)) = modes().get(index) {
                        submit_async_request(MatrixRequest::SetRoomNotificationMode {room_id: data.room.room_id().clone(), mode: *mode});
                    } else { self.choosing_notifications = false; }
                } else {
                    match index {
                        0 => self.choosing_notifications = true,
                        1 => cx.action(InviteModalAction::Open(data.room.clone())),
                        2 => cx.action(crate::agent_access::AgentAccessAction::Open),
                        #[cfg(feature = "agent_chat")]
                        3 => cx.action(crate::agent_chat::approval_inbox::ApprovalInboxAction::Open {project: Some(data.room.room_id().clone())}),
                        _ => (),
                    }
                }
                self.view.redraw(cx);
                // PortalList returns an item once per grouped child action.
                // A single pointer event can include hover/focus and click;
                // handle its click only once, especially for Matrix writes.
                break;
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.owner.is_some() && self.owner != current_user_id() { self.clear(); }
        self.view.label(cx, ids!(status)).set_text(cx, &self.status);
        self.view.label(cx, ids!(status)).set_visible(cx, !self.status.is_empty());
        self.view.button(cx, ids!(retry)).set_visible(cx, self.failed);
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let count = self.data.as_ref().map_or(0, |d| d.members.len() + 3 + if self.choosing_notifications {5} else {3 + usize::from(cfg!(feature = "agent_chat"))});
                list.set_item_range(cx, 0, count);
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(data) = self.data.as_mut().filter(|_| index < count) else {
                        list.item(cx, index, id!(Filler)).draw_all(cx, scope);
                        continue;
                    };
                    let widget = if index == 0 {
                        let widget = list.item(cx, index, id!(Header));
                        widget.label(cx, ids!(name)).set_text(cx, &data.room.display());
                        widget.label(cx, ids!(topic)).set_text(cx, &data.topic);
                        widget.label(cx, ids!(topic)).set_visible(cx, !data.topic.is_empty());
                        widget.label(cx, ids!(member_count)).set_text(cx, &crate::i18n::format("{0} members", &[("0", (data.members.len()).to_string())]));
                        widget
                    } else if index <= 2 {
                        let widget = list.item(cx, index, id!(Action));
                        widget.label(cx, ids!(title)).set_text(cx, if index == 1 {crate::i18n::tr("Search Chat History")} else {crate::i18n::tr("Shared Attachments")});
                        widget.label(cx, ids!(value)).set_text(cx, "");
                        widget
                    } else if let Some(profile) = data.members.get_mut(index - 3) {
                        let widget = list.item(cx, index, id!(Member));
                        widget.label(cx, ids!(name)).set_text(cx, profile.displayable_name());
                        let detail = crate::agent_access::framework_label(&profile.user_id).map_or_else(||profile.user_id.to_string(), |framework|format!("{framework} · {}", profile.user_id));
                        widget.label(cx, ids!(user_id)).set_text(cx, &detail);
                        let avatar = widget.avatar(cx, ids!(avatar));
                        let loaded = profile.avatar_state.update_from_cache(cx).is_some_and(|image| {
                            avatar.show_image(cx, None, |cx, img| utils::load_avatar_image(&img, cx, image)).is_ok()
                        });
                        if !loaded { avatar.show_user_text(cx, &profile.user_id, profile.displayable_name()); }
                        widget
                    } else {
                        let index_in_actions = index - data.members.len() - 3;
                        let widget = list.item(cx, index, id!(Action));
                        let (title, value) = if self.choosing_notifications {
                            match modes().get(index_in_actions) {
                                Some((mode, title)) => (*title, if *mode == data.mode {"✓"} else {""}),
                                None => (crate::i18n::tr("Back to Chat Info"), ""),
                            }
                        } else if index_in_actions == 0 {
                            (crate::i18n::tr("Notifications"), match data.mode {
                                Some(RoomNotificationMode::Mute) => crate::i18n::tr("Muted"),
                                Some(RoomNotificationMode::AllMessages) => crate::i18n::tr("All"),
                                Some(RoomNotificationMode::MentionsAndKeywordsOnly) => crate::i18n::tr("Mentions"),
                                None => crate::i18n::tr("Default"),
                            })
                        } else {match index_in_actions {
                            1 => (crate::i18n::tr("Invite to Chat"), ""),
                            2 => (crate::i18n::tr("Agent Access"), ""),
                            _ => (crate::i18n::tr("Approval rooms"), ""),
                        }};
                        widget.label(cx, ids!(title)).set_text(cx, title);
                        widget.label(cx, ids!(value)).set_text(cx, value);
                        widget
                    };
                    widget.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}

impl MobileChatInfoRef {
    pub fn show(&self, cx: &mut Cx, room: RoomNameId) {
        if let Some(mut inner) = self.borrow_mut() { inner.load(cx, room); }
    }

    pub fn clear(&self) {
        if let Some(mut inner) = self.borrow_mut() { inner.clear(); }
    }
}
