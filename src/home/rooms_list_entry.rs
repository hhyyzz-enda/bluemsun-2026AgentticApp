use crate::theme::Snapshot as ThemeSnapshot;
use std::borrow::Cow;
use makepad_widgets::*;
use makepad_widgets::event::TouchState;
use matrix_sdk::ruma::{OwnedRoomId, RoomId};

use crate::{
    room::FetchedRoomAvatar,
    shared::{
        avatar::AvatarWidgetExt, context_menu::ContextMenuClosed,
        hover_highlight::handle_hover_hit, html_or_plaintext::HtmlOrPlaintextWidgetExt,
        unread_badge::UnreadBadgeWidgetExt as _,
    },
    utils::{self, relative_format}
};

use super::rooms_list::{InvitedRoomInfo, InviterInfo, JoinedRoomInfo};
use super::back_swipe::BackSwipe;
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*


    // A cancel icon to be displayed in the RoomsListEntry when the room is tombstoned.
    mod.widgets.TombstoneIcon = View {
        width: Fit, height: Fit,
        visible: false,

        Icon {
            width: 19, height: 19,
            align: Align{x: 0.5, y: 0.5}
            draw_icon +: {
                svg: (ICON_TOMBSTONE)
                color: (COLOR_FG_DANGER_RED)
            }
            icon_walk: Walk{ width: 15, height: 15 }
        }
    }

    mod.widgets.RoomName = Label {
        width: Fill, height: Fit
        flow: Flow.Right{wrap: false},
        padding: 0,
        max_lines: 1
        text_overflow: Ellipsis
        draw_text +: {
            color: (RBX_FG_PRIMARY),
            text_style: RBX_TEXT_BODY_STRONG {}
        }
        text: #(crate::i18n::tr("[Room name unknown]"))
    }

    mod.widgets.RoomsListEntryTimestamp = Label {
        padding: Inset{top: 1},
        width: Fit, height: Fit
        flow: Flow.Right{wrap: false},
        draw_text +: {
            color: (RBX_FG_TERTIARY)
            text_style: RBX_TEXT_META {}
        }
    }

    mod.widgets.MessagePreview = View {
        width: Fill, height: Fit
        latest_message := HtmlOrPlaintext {
            html_view +: {
                html +: {
                    font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE)
                    max_lines: 2
                    text_overflow: Ellipsis
                    text_style_normal +: { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 }
                    text_style_italic +: { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 }
                    text_style_bold +: { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 }
                    text_style_bold_italic +: { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 }
                    text_style_fixed +: { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 }
                    // Scale down the pill (title font, avatar size, avatar text) to fit.
                    a +: {
                        matrix_link_view +: {
                            matrix_link +: {
                                pill_bg +: {
                                    margin: Inset{top: 1}
                                    padding: Inset{ left: 4.5, right: 3.0, bottom: -3.5, top: -3.5 }
                                    draw_bg +: { border_radius: 4.5 }
                                    avatar +: {
                                        width: 13.0, height: 13.0,
                                        text_view +: {
                                            text +: {
                                                draw_text +: {
                                                    text_style +: { font_size: (6 * mod.widgets.RINX_TEXT_SCALE) }
                                                }
                                            }
                                        }
                                    }
                                    title +: {
                                        draw_text +: {
                                            text_style +: { font_size: (8.5 * mod.widgets.RINX_TEXT_SCALE) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            plaintext_view +: {
                pt_label +: {
                    max_lines: 2
                    text_overflow: Ellipsis
                    draw_text +: {
                        color: (RBX_FG_SECONDARY),
                        text_style: REGULAR_TEXT { font_size: (9.3 * mod.widgets.RINX_TEXT_SCALE), line_spacing: 1.32 },
                    }
                    text: #(crate::i18n::tr("[No recent messages]"))
                }
            }
        }
    }

    mod.widgets.RoomsListEntryContent = set_type_default() do #(RoomsListEntryContent::register_widget(vm)) {

        flow: Right,
        spacing: 10,
        padding: 10,
        width: Fill, height: Fit
        cursor: MouseCursor.Default,

        show_bg: true
        draw_bg +: {
            active: instance(0.0)
            hover: instance(0.0)
            // Idle rows sit flat on the surface; hover is a faint wash and the
            // selected room is a soft teal tint — no solid fill, so the text
            // keeps the same dark ink in every state.
            color: instance(#0000)
            color_hover: instance(RBX_BG_HOVER)
            color_selected: instance(RBX_BG_SELECTED)
            color_selected_hover: instance(RBX_BG_PRESSED)
            border_color: instance(#0000)
            border_size: uniform(0.0)
            border_radius: uniform(6.0)
            border_inset: uniform(vec4(0.0))

            get_color: fn() -> vec4 {
                return mix(
                    mix(self.color, self.color_hover, self.hover),
                    mix(self.color_selected, self.color_selected_hover, self.hover),
                    self.active
                )
            }

            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(
                    self.border_inset.x + self.border_size,
                    self.border_inset.y + self.border_size,
                    self.rect_size.x - (self.border_inset.x + self.border_inset.z + self.border_size * 2.0),
                    self.rect_size.y - (self.border_inset.y + self.border_inset.w + self.border_size * 2.0),
                    max(1.0, self.border_radius)
                )
                sdf.fill_keep(self.get_color())
                if self.border_size > 0.0 {
                    sdf.stroke(self.border_color, self.border_size)
                }
                return sdf.result;
            }
        }
        animator: Animator{
            selected: {
                default: @off
                off: AnimatorState{
                    from: {all: Snap}
                    apply: {
                        draw_bg: {active: 0.0}
                    }
                }
                on: AnimatorState{
                    from: {all: Snap}
                    apply: {
                        draw_bg: {active: 1.0}
                    }
                }
            }
            bg_hover: {
                default: @off
                off: AnimatorState{
                    from: {all: Snap}
                    apply: {
                        draw_bg: {hover: 0.0}
                    }
                }
                on: AnimatorState{
                    from: {all: Snap}
                    apply: {
                        draw_bg: {hover: 1.0}
                    }
                }
            }
        }
    }

    mod.widgets.MobileRoomsListEntry = mod.widgets.RoomsListEntryContent {
        mobile: true
        height: 72 padding: 0 spacing: 0 flow: Overlay clip_x: true
        draw_bg +: {
            border_radius: 0
            color: mod.widgets.RINX_SURFACE color_hover: mod.widgets.RINX_BORDER color_selected: mod.widgets.RINX_SURFACE
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.rect(0.0, 0.0, self.rect_size.x, self.rect_size.y)
                sdf.fill(self.get_color())
                sdf.rect(76.0, self.rect_size.y - 0.5, self.rect_size.x - 76.0, 0.5)
                sdf.fill(mod.widgets.RINX_BORDER)
                return sdf.result
            }
        }
        swipe_actions := View {
            visible: false width: Fill height: Fill flow: Right
            View {width: Fill height: Fill}
            swipe_unread := RobrixNeutralIconButton {
                width: 88 height: Fill padding: 4 spacing: 0 align: Align{x: 0.5 y: 0.5}
                text: #(crate::i18n::tr("Unread")) i18n_text: "Unread" icon_walk: Walk{width: 0 height: 0}
                draw_bg +: {pixel: fn() {return mod.widgets.RINX_ACCENT.mix(mod.widgets.RINX_ACCENT_DOWN, self.down)}}
                draw_text +: {color: mod.widgets.RINX_ON_ACCENT color_hover: mod.widgets.RINX_ON_ACCENT color_down: mod.widgets.RINX_ON_ACCENT text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}
            }
            swipe_hide := RobrixNeutralIconButton {
                width: 64 height: Fill padding: 4 spacing: 0 align: Align{x: 0.5 y: 0.5}
                text: #(crate::i18n::tr("Hide")) i18n_text: "Hide" icon_walk: Walk{width: 0 height: 0}
                draw_bg +: {pixel: fn() {return mod.widgets.RINX_WARNING_FG.mix(mod.widgets.RINX_WARNING_FG, self.down)}}
                draw_text +: {color: mod.widgets.RINX_WARNING_BG color_hover: mod.widgets.RINX_WARNING_BG color_down: mod.widgets.RINX_WARNING_BG text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}
            }
            swipe_delete := RobrixNeutralIconButton {
                width: 72 height: Fill padding: 4 spacing: 0 align: Align{x: 0.5 y: 0.5}
                text: #(crate::i18n::tr("Delete")) i18n_text: "Delete" icon_walk: Walk{width: 0 height: 0}
                draw_bg +: {pixel: fn() {return mod.widgets.RINX_DANGER_FG.mix(mod.widgets.RINX_DANGER_FG, self.down)}}
                draw_text +: {color: mod.widgets.RINX_DANGER_BG color_hover: mod.widgets.RINX_DANGER_BG color_down: mod.widgets.RINX_DANGER_BG text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}
            }
        }
        chat_row := SolidView {
            width: Fill height: Fill flow: Right spacing: 12
            padding: Inset{left: 16 right: 16 top: 12 bottom: 12}
            draw_bg +: {pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.rect(0.0, 0.0, self.rect_size.x, self.rect_size.y)
                sdf.fill(mod.widgets.RINX_SURFACE)
                sdf.rect(76.0, self.rect_size.y - 0.5, self.rect_size.x - 76.0, 0.5)
                sdf.fill(mod.widgets.RINX_BORDER)
                return sdf.result
            }}
        View {
            width: 48 height: 48 flow: Overlay clip_x: false clip_y: false
            avatar := MobileAvatar {}
            View {
                width: Fill height: Fill align: Align{x: 1.0 y: 0.0}
                margin: Inset{top: -5 right: -6}
                mobile_badge := RoundedView {
                    visible: false width: Fit height: 18 padding: Inset{left: 5 right: 5}
                    align: Align{x: 0.5 y: 0.5}
                    draw_bg +: {color: mod.widgets.RINX_DANGER_FG border_radius: 9}
                    count := Label {padding: 0 draw_text +: {color: mod.widgets.RINX_DANGER_BG text_style: theme.font_regular {font_size: (9 * mod.widgets.RINX_TEXT_SCALE)}}}
                }
            }
        }
        View {
            width: Fill height: Fill flow: Down spacing: 6 padding: Inset{top: 2}
            View {
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                room_name := mod.widgets.RoomName {draw_text.text_style: theme.font_regular {font_size: (12.5 * mod.widgets.RINX_TEXT_SCALE)}}
                timestamp := mod.widgets.RoomsListEntryTimestamp {draw_text.text_style.font_size: (8.5 * mod.widgets.RINX_TEXT_SCALE)}
            }
            View {
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                mod.widgets.MessagePreview {
                    latest_message +: {
                        html_view +: {html +: {max_lines: 1 font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                        plaintext_view +: {pt_label +: {max_lines: 1 draw_text +: {text_style +: {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}}
                    }
                }
                muted_indicator := View {
                    visible: false width: 16 height: 16
                    Icon {
                        icon_walk: Walk{width: 16 height: 16}
                        draw_icon +: {svg: crate_resource("self://resources/icons/bell_off.svg") color: mod.widgets.RINX_DISABLED}
                    }
                }
            }
        }        }

    }

    mod.widgets.RoomsListEntry = #(RoomsListEntry::register_widget(vm)) {
        flow: Down, height: Fit

        // Wrap the RoomsListEntryContent in an AdaptiveView to change the displayed content
        // (and its layout) based on the available space in the sidebar.
        adaptive_preview := AdaptiveView {
            height: Fit

            OnlyIcon := mod.widgets.RoomsListEntryContent {
                align: Align{x: 0.5, y: 0.5}
                padding: 5.
                View {
                    height: Fit
                    flow: Overlay
                    align: Align{ x: 1.0 }
                    // Don't clip (cut-off) the unread badge's glow
                    clip_x: false, clip_y: false
                    avatar := Avatar {}
                    unread_badge := UnreadBadge {}
                    tombstone_icon := mod.widgets.TombstoneIcon {}
                }
            }
            IconAndName := mod.widgets.RoomsListEntryContent {
                padding: 5.
                align: Align{x: 0.5, y: 0.5}
                avatar := Avatar {}
                room_name := mod.widgets.RoomName {}
                unread_badge := UnreadBadge {}
                tombstone_icon := mod.widgets.TombstoneIcon {}
            }
            FullPreview := mod.widgets.RoomsListEntryContent {
                padding: 10
                avatar := Avatar {}
                View {
                    flow: Down
                    width: Fill, height: 56
                    align: Align{ x: 0.0, y: 0.0 }
                    // Don't clip (cut-off) the unread badge's glow
                    clip_x: false, clip_y: false
                    top := View {
                        width: Fill, height: Fit,
                        spacing: 3,
                        flow: Right,
                        room_name := mod.widgets.RoomName {}
                        timestamp := mod.widgets.RoomsListEntryTimestamp { }
                    }
                    bottom := View {
                        width: Fill, height: Fill,
                        spacing: 2,
                        flow: Right,
                        // Don't clip (cut-off) the unread badge's glow
                        clip_x: false, clip_y: false
                        preview := mod.widgets.MessagePreview {
                            margin: Inset{ top: 2.5 }
                        }
                        View {
                            width: Fit, height: Fit
                            align: Align{ x: 1.0 }
                            // Don't clip the unread badge's glow off the top/bottom.
                            clip_x: false, clip_y: false
                            unread_badge := UnreadBadge {}
                            tombstone_icon := mod.widgets.TombstoneIcon {}
                        }
                    }
                }
            }
        }
    }
}

/// An entry in the rooms list.
#[derive(Script, Widget)]
pub struct RoomsListEntry {
    #[deref] view: View,
}

impl ScriptHook for RoomsListEntry {
    fn on_after_new(&mut self, vm: &mut ScriptVm) {
        vm.with_cx_mut(|cx| {
            self.set_adaptive_variant_selector(cx);
        })
    }
}

/// Widget actions that are emitted by a RoomsListEntry.
#[derive(Clone, Default, Debug)]
pub enum RoomsListEntryAction {
    /// This RoomsListEntry was primary-clicked or tapped.
    PrimaryClicked(OwnedRoomId),
    /// This RoomsListEntry was right-clicked or long-pressed.
    SecondaryClicked(OwnedRoomId, DVec2),
    #[default]
    None,
}

impl RoomsListEntry {
    fn set_adaptive_variant_selector(&self, cx: &mut Cx) {
        self.view
            .adaptive_view(cx, ids!(adaptive_preview))
            .set_variant_selector(|_cx, parent_size| match parent_size.x {
                width if width <= 70.0 => id!(OnlyIcon),
                width if width <= 200.0 => id!(IconAndName),
                _ => id!(FullPreview),
            });
    }
}

impl Widget for RoomsListEntry {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

#[derive(Script, Widget, Animator)]
pub struct RoomsListEntryContent {
    #[rust]
    appearance: ThemeSnapshot,
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[apply_default]
    animator: Animator,

    #[live] mobile: bool,

    #[rust] room_id: Option<OwnedRoomId>,

    /// `true` while a context menu that we opened is being shown.
    #[rust] is_context_menu_open: bool,
    /// Whether this entry currently shows an invited (not joined) room.
    #[rust] is_invited: bool,
    #[rust] swipe_open: bool,
    #[rust] swipe_dragged: bool,
    #[rust] left_swipe: BackSwipe,
    #[rust] latest_timestamp: u64,
    #[rust] has_unreads: bool,

    /// The preview colors that were last drawn for this entry.
    /// * Some(true): this entry was last drawn as selected.
    /// * Some(false): this entry was last drawn as not selected.
    /// * None: this entry hasn't been drawn yet.
    #[rust] last_selection_drawn: Option<bool>,

    /// The avatar content that was last drawn for this room.
    #[rust] last_avatar: Option<FetchedRoomAvatar>,
}

impl ScriptHook for RoomsListEntryContent {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, _: &Apply, _: &mut Scope, _: ScriptValue) {
        self.appearance = crate::theme::snapshot_for_vm(vm);
        self.last_selection_drawn = None;
    }
}

impl Widget for RoomsListEntryContent {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.animator_handle_event(cx, event).must_redraw() {
            self.redraw(cx);
        }
        if self.mobile && !self.is_invited {
            if let Event::Actions(actions) = event {
                if actions.iter().any(|a| a.downcast_ref::<super::chat_actions::ChatSwipeOpened>()
                    .is_some_and(|opened| self.room_id.as_ref() != Some(&opened.0))) {
                    self.set_swipe_open(cx, false);
                }
            }
            if let Event::Scroll(scroll) = event {
                if self.view.area().rect(cx).contains(scroll.abs) {
                    if self.left_swipe.update_left(scroll) { self.set_swipe_open(cx, true); }
                }
            }
            if self.swipe_open {
                let generated = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
                // Complete the capture that began on the row before it slid.
                // Otherwise the release never reaches that area and subsequent
                // taps cannot be captured by the revealed action buttons.
                if let Hit::FingerUp(_) = event.hits(cx, self.view.area()) {
                    self.swipe_dragged = false;
                }
                {
                    let actions = if let Event::Actions(actions) = event { actions } else { &generated };
                    if let Some(room_id) = self.room_id.clone() {
                        if self.button(cx, ids!(swipe_unread)).clicked(actions) {
                            use crate::sliding_sync::{submit_async_request, MatrixRequest};
                            if self.has_unreads {
                                submit_async_request(MatrixRequest::MarkRoomAsRead { room_id, receipt_type: crate::settings::app_preferences::preferred_receipt_type() });
                            } else {
                                submit_async_request(MatrixRequest::SetUnreadFlag { room_id, mark_as_unread: true });
                            }
                            self.set_swipe_open(cx, false);
                        } else if self.button(cx, ids!(swipe_hide)).clicked(actions) {
                            super::chat_actions::hide(cx, room_id, self.latest_timestamp, false);
                            self.set_swipe_open(cx, false);
                        } else if self.button(cx, ids!(swipe_delete)).clicked(actions) {
                            super::chat_actions::confirm_delete(cx, room_id, self.latest_timestamp);
                            self.set_swipe_open(cx, false);
                        }
                    }
                }
                let down = match event {
                    Event::MouseDown(e) => Some(e.abs),
                    Event::TouchUpdate(e) => e.touches.iter().find(|t| t.state == TouchState::Start).map(|t| t.abs),
                    _ => None,
                };
                if let Some(abs) = down {
                    let rect = self.view.area().rect(cx);
                    if !rect.contains(abs) || abs.x < rect.pos.x + rect.size.x - 224.0 {
                        self.set_swipe_open(cx, false);
                        self.swipe_dragged = true;
                        return;
                    }
                }
                return;
            }
        }

        if self.is_context_menu_open
            && let Event::Actions(actions) = event
            && actions.iter().any(|a| a.downcast_ref::<ContextMenuClosed>().is_some())
        {
            self.is_context_menu_open = false;
        }

        // We handle hits on this widget first to ensure that any clicks on it
        // will just select the room, rather than resulting in a click on any child view
        // within the RoomsListEntry content itself, such as links or avatars.
        if let Some(room_id) = self.room_id.clone() {
            let uid = self.widget_uid();
            let area = self.view.area();
            let claim_before = event.pointer_claimed_area();
            let hit = handle_hover_hit(self, cx, event, area, claim_before, self.is_context_menu_open);
            // TODO: Invited rooms have no context menu, so don't emit secondary clicks.
            //       We should add a context menu for invited rooms.
            match hit {
                Hit::FingerDown(fe) => {
                    self.swipe_dragged = false;
                    cx.set_key_focus(area);
                    if !self.is_invited && fe.device.mouse_button().is_some_and(|b| b.is_secondary()) {
                        self.is_context_menu_open = true;
                        cx.widget_action(
                            uid,
                            RoomsListEntryAction::SecondaryClicked(room_id, fe.abs),
                        );
                    }
                }
                Hit::FingerMove(fe) if self.mobile && !self.is_invited => {
                    let delta = fe.abs - fe.abs_start;
                    if delta.x < -48.0 && -delta.x > delta.y.abs() * 2.0 {
                        self.swipe_dragged = true;
                        self.set_swipe_open(cx, true);
                    }
                }
                Hit::FingerLongPress(fe) if !self.is_invited => {
                    self.is_context_menu_open = true;
                    cx.widget_action(
                        uid,
                        RoomsListEntryAction::SecondaryClicked(room_id, fe.abs),
                    );
                }
                Hit::FingerUp(fe) if !self.swipe_dragged && fe.is_over && fe.is_primary_hit() && fe.was_tap() => {
                    cx.widget_action(uid, RoomsListEntryAction::PrimaryClicked(room_id));
                }
                _ => { }
            }
        }

        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(joined_room_info) = scope.props.get::<JoinedRoomInfo>() {
            self.set_room(cx, joined_room_info.room_name_id.room_id(), false);
            self.draw_joined_room(cx, joined_room_info);
        } else if let Some(invited_room_info) = scope.props.get::<InvitedRoomInfo>() {
            self.set_room(cx, invited_room_info.room_name_id.room_id(), true);
            self.draw_invited_room(cx, invited_room_info);
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

impl RoomsListEntryContent {
    fn set_swipe_open(&mut self, cx: &mut Cx, open: bool) {
        if open && !self.swipe_open {
            if let Some(room) = &self.room_id { cx.action(super::chat_actions::ChatSwipeOpened(room.clone())); }
        }
        self.swipe_open = open;
        self.view.view(cx, ids!(swipe_actions)).set_visible(cx, open);
        let shift = if open {224.0} else {0.0};
        if let Some(mut row) = self.view.view(cx, ids!(chat_row)).borrow_mut() {
            row.walk.margin = Inset {left: -shift, right: shift, ..Default::default()};
        }
        self.redraw(cx);
    }
    fn set_room(&mut self, cx: &mut Cx, room_id: &RoomId, is_invited: bool) {
        // If the room ID changed, reset any UI state that belongs to the previous room.
        if self.room_id.as_deref() != Some(room_id) {
            self.set_swipe_open(cx, false);
            self.left_swipe = Default::default();
            self.is_context_menu_open = false;
            self.animator_cut(cx, ids!(bg_hover.off));
            self.room_id = Some(room_id.to_owned());
        }
        self.is_invited = is_invited;
    }

    /// Populates this RoomsListEntry with info about a joined room.
    pub fn draw_joined_room(
        &mut self,
        cx: &mut Cx,
        room_info: &JoinedRoomInfo,
    ) {
        self.latest_timestamp = room_info.latest.as_ref().map(|e| e.timestamp.0.into()).unwrap_or(0);
        self.has_unreads = room_info.is_marked_unread || room_info.num_unread_messages > 0 || room_info.num_unread_mentions > 0;
        self.button(cx, ids!(swipe_unread)).set_text(cx, if self.has_unreads {"Read"} else {crate::i18n::tr("Unread")});
        // Note: in general, we must always set all fields in case a rooms list entry widget
        // was re-used by the portal list, to avoid showing any old content for a different entry.
        self.view.label(cx, ids!(room_name)).set_text(cx, &room_info.room_name_id.display());
        let timestamp = self.view.label(cx, ids!(timestamp));
        let latest_message = self.view.html_or_plaintext(cx, ids!(latest_message));
        if let Some(latest) = room_info.latest.as_ref().filter(|e| super::chat_actions::cleared_through(room_info.room_name_id.room_id()).is_none_or(|t| u64::from(e.timestamp.0) > t)) {
            let time = if self.mobile {
                utils::unix_time_millis_to_datetime(latest.timestamp).map(|dt| {
                    let today = chrono::Local::now().date_naive();
                    let date = dt.date_naive();
                    if date == today { dt.format("%H:%M").to_string() }
                    else if Some(date) == today.pred_opt() { crate::i18n::tr("Yesterday").to_owned() }
                    else if (0..7).contains(&today.signed_duration_since(date).num_days()) { crate::i18n::tr(&dt.format("%a").to_string()).to_owned() }
                    else { dt.format("%m/%d/%y").to_string() }
                }).map(Cow::Owned)
            } else { relative_format(latest.timestamp) };
            timestamp.set_text(cx, time.as_deref().unwrap_or(""));
            let preview = if self.mobile && room_info.is_direct {
                latest.direct_text.as_deref().unwrap_or(&latest.text)
            } else { &latest.text };
            latest_message.show_html(cx, preview);
        } else {
            timestamp.set_text(cx, "");
            latest_message.show_plaintext(cx, crate::i18n::tr("[No recent messages]"));
        }

        self.view.unread_badge(cx, ids!(unread_badge)).update_counts(
            room_info.is_marked_unread,
            room_info.num_unread_mentions,
            room_info.num_unread_messages,
        );
        if self.mobile {
            self.view.view(cx, ids!(muted_indicator)).set_visible(cx, room_info.notification_mode == Some(matrix_sdk::notification_settings::RoomNotificationMode::Mute));
            let count = room_info.num_unread_messages.max(room_info.num_unread_mentions);
            self.view.view(cx, ids!(mobile_badge)).set_visible(cx, room_info.is_marked_unread || count > 0);
            let text = if count > 99 { "99+".into() } else if count == 0 { "•".into() } else { count.to_string() };
            self.view.label(cx, ids!(count)).set_text(cx, &text);
        }
        self.draw_common(cx, &room_info.room_avatar, room_info.is_selected);
        // Show tombstone icon if the room is tombstoned
        self.view.view(cx, ids!(tombstone_icon)).set_visible(cx, room_info.is_tombstoned);
    }

    /// Populates this RoomsListEntry with info about an invited room.
    pub fn draw_invited_room(
        &mut self,
        cx: &mut Cx,
        room_info: &InvitedRoomInfo,
    ) {
        self.view.view(cx, ids!(muted_indicator)).set_visible(cx, false);
        let name = room_info.room_name_id.display();
        let name = match room_info.is_space {
            true => Cow::Owned(crate::i18n::format("[Space] {name}", &[("name", (name).to_string())])),
            false => name,
        };
        self.view.label(cx, ids!(room_name)).set_text(cx, &name);
        // Hide the timestamp field, and use the latest message field to show the inviter.
        self.view.label(cx, ids!(timestamp)).set_text(cx, "");
        let inviter_string = match &room_info.inviter_info {
            Some(InviterInfo { user_id, display_name: Some(dn), .. }) => crate::i18n::format("Invited by <b>{0}</b> ({1})", &[("0", (htmlize::escape_text(dn)).to_string()), ("1", (htmlize::escape_text(user_id.as_str())).to_string())]),
            Some(InviterInfo { user_id, .. }) => crate::i18n::format("Invited by {0}", &[("0", (htmlize::escape_text(user_id.as_str())).to_string())]),
            None => String::from("You were invited"),
        };
        self.view.html_or_plaintext(cx, ids!(latest_message)).show_html(cx, &inviter_string);
        // an invite cannot ever be tombstoned
        self.view.view(cx, ids!(tombstone_icon)).set_visible(cx, false);
        self.view
            .unread_badge(cx, ids!(unread_badge))
            .update_counts(false, 1, 0);
        if self.mobile {
            self.view.view(cx, ids!(mobile_badge)).set_visible(cx, true);
            self.view.label(cx, ids!(count)).set_text(cx, "!");
        }

        self.draw_common(cx, &room_info.room_avatar, room_info.is_selected);
    }

    /// Populates the widgets common to both invited and joined rooms list entries.
    pub fn draw_common(
        &mut self,
        cx: &mut Cx,
        room_avatar: &FetchedRoomAvatar,
        is_selected: bool,
    ) {
        // Only redraw the avatar if it changed
        if self.last_avatar.as_ref() != Some(room_avatar) {
            match room_avatar {
                FetchedRoomAvatar::Text(text) => {
                    self.view.avatar(cx, ids!(avatar)).show_text(cx, None, None, text);
                }
                FetchedRoomAvatar::Members(_) => {
                    self.view.avatar(cx, ids!(avatar)).show_room_avatar(cx, room_avatar);
                }
                FetchedRoomAvatar::Image(avatar_image) => {
                    let _ = self.view.avatar(cx, ids!(avatar)).show_image(
                        cx,
                        None, // Avatars in a RoomsListEntry shouldn't be clickable.
                        |cx, img| utils::load_avatar_image(&img, cx, avatar_image),
                    );
                }
            }
            self.last_avatar = Some(room_avatar.clone());
        }

        self.update_latest_event_colors(cx, is_selected);
    }

    /// Updates styling of the latest event preview based on whether the room is selected or not.
    pub fn update_latest_event_colors(&mut self, cx: &mut Cx, is_selected: bool) {
        // Link colors must be re-applied on every draw because the HTML's link widgets
        // get created dynamically during the draw walk. Both states sit on a light
        // surface (transparent / soft-teal wash), so the token link colour is used
        // in both cases.
        self.view
            .html_or_plaintext(cx, ids!(latest_message))
            .set_link_color(cx, Some(self.appearance.accent));

        // Skip redrawing if nothing changed.
        if self.last_selection_drawn == Some(is_selected) {
            return;
        }
        self.last_selection_drawn = Some(is_selected);

        // The selected row signals the active room with the soft teal wash alone
        // (see the draw_bg shader), so the text keeps identical dark ink in both
        // states and stays fully legible.
        let message_text_color = self.appearance.muted;
        let room_name_color = self.appearance.ink;
        let timestamp_color = self.appearance.muted;
        let code_bg_color = self.appearance.role("color.code.background");

        // Toggle the background color via the animator (handles selected/deselected bg).
        self.animator_toggle(cx, is_selected, Animate::No, ids!(selected.on), ids!(selected.off));

        // Update the text colors for the room name and timestamp.
        self.view.label(cx, ids!(room_name)).set_text_color(cx, room_name_color);
        self.view.label(cx, ids!(timestamp)).set_text_color(cx, timestamp_color);

        // Update text colors for the latest message preview (both HTML and plaintext variants).
        if let Some(mut html) = self.view.html(cx, ids!(latest_message.html_view.html)).borrow_mut() {
            html.set_font_color(cx, message_text_color);
            html.set_code_color(cx, code_bg_color);
            html.set_quote_bg_color(cx, code_bg_color);
        }
        self.view.label(cx, ids!(latest_message.plaintext_view.pt_label))
            .set_text_color(cx, message_text_color);
    }
}
