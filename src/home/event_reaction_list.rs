use crate::theme::Snapshot as ThemeSnapshot;
use crate::home::room_screen::RoomScreenTooltipActions;
use crate::profile::user_profile_cache;
use crate::sliding_sync::{current_user_id, submit_async_request, MatrixRequest, TimelineKind};
use indexmap::IndexMap;
use makepad_widgets::*;
use crate::{LivePtr, widget_ref_from_live_ptr};
use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId};
use matrix_sdk_ui::timeline::{ReactionInfo, ReactionsByKeyBySender, TimelineEventItemId};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*


    mod.widgets.COLOR_BUTTON_GREY = mod.widgets.RINX_FIELD
    mod.widgets.REACTION_LIST_PADDING_RIGHT = 30.0;

    mod.widgets.ReactionList = #(ReactionList::register_widget(vm)) {
        width: Fill,
        height: Fit,
        flow: Flow.Right{wrap: true},
        margin: Inset{top: 5.0}
        padding: Inset{
            right: (mod.widgets.REACTION_LIST_PADDING_RIGHT)
        }
        item: Button {
            width: Fit,
            height: Fit,
            padding: 6,
            // Use a zero margin on the left because we want the first reaction
            // to be flush with the left edge of the message text.
            margin: Inset{ top: 3, bottom: 3, left: 0, right: 6 },

            draw_bg +: {
                // Anything that we apply over must be an `instance`,
                // and their names must be distinct from the base Button type.
                reaction_bg_color: instance(mod.widgets.COLOR_BUTTON_GREY)
                reaction_border_color: instance(mod.widgets.RINX_BORDER)
                // Override values from the base Button type.
                color_hover: mod.widgets.RINX_HOVER
                hover: 0.0
                border_size: 1.5
                border_radius: 3.0

                get_color: fn() -> vec4 {
                    return mix(self.reaction_bg_color, mix(self.reaction_bg_color, self.color_hover, 0.2), self.hover)
                }

                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(
                        self.border_size,
                        self.border_size,
                        self.rect_size.x - self.border_size * 2.0,
                        self.rect_size.y - self.border_size * 2.0,
                        max(1.0, self.border_radius)
                    )
                    sdf.fill_keep(self.get_color())
                    if self.border_size > 0.0 {
                        sdf.stroke(self.reaction_border_color, self.border_size)
                    }
                    return sdf.result;
                }
            }
            draw_text +: {
                text_style: REGULAR_TEXT {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)},
                color: mod.widgets.RINX_INK
                get_color: fn() -> vec4 {
                    return self.color;
                }
            }
        }
    }

}

#[derive(Clone, Debug)]
pub struct ReactionData {
    /// Original reaction string from the backend before emoji shortcode conversion.
    pub reaction: String,
    /// Boolean indicating if the current user is also a sender of this reaction.
    pub includes_user: bool,
    /// List of all users who have reacted to the emoji.
    pub reaction_senders: IndexMap<OwnedUserId, ReactionInfo>,
    /// The ID of the room that the reaction is for
    pub room_id: OwnedRoomId,
}

#[derive(Script, Widget)]
pub struct ReactionList {
    #[rust]
    appearance: ThemeSnapshot,
    #[uid]
    uid: WidgetUid,
    #[redraw]
    #[rust]
    area: Area,
    #[live]
    item: Option<LivePtr>,
    #[rust]
    children: Vec<(ButtonRef, ReactionData)>,
    #[layout]
    layout: Layout,
    #[walk]
    walk: Walk,

    #[rust] timeline_kind: Option<TimelineKind>,
    #[rust] timeline_event_id: Option<TimelineEventItemId>,

    /// A cheap hash of the last reaction list populuated in this widget.
    /// Consists of: `(reaction count, total senders, num sent by me)`.
    #[rust] last_reaction_counts: Option<(usize, usize, usize)>,
}
impl ScriptHook for ReactionList {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _: ScriptValue,
    ) {
        self.appearance = crate::theme::snapshot_for_vm(vm);
        if apply.is_script_reapply() {
            if let Some(template) = self.item {
                for (button, data) in &mut self.children {
                    let text = button.text();
                    button.script_apply(vm, apply, scope, template);
                    button.set_text(vm.cx_mut(), &text);
                    let (bg, border) = if data.includes_user {
                        (self.appearance.selected, self.appearance.accent)
                    } else {
                        (self.appearance.field, self.appearance.border)
                    };
                    let style = script! {
                        __script_source__ {draw_bg +: {reaction_bg_color: #(bg) reaction_border_color: #(border)}}
                    };
                    button.script_apply_eval(vm, style);
                }
            }
        }
    }
}
impl Widget for ReactionList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        for (button, _) in self.children.iter_mut() {
            let _ = button.draw(cx, scope);
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        for (button_ref, reaction_data) in self.children.iter() {
            let button_area = button_ref.area();
            // Note: the `break` statements are used to break out of the loop over
            // all reaction buttons, since a hit event can only occur on one button.
            match event.hits(cx, button_area) {
                Hit::FingerDown(..) => {
                    cx.set_key_focus(button_area);
                    break;
                }
                Hit::FingerHoverIn(..) => {
                    self.do_hover_in(cx, scope, button_ref, reaction_data.clone());
                    break;
                }
                Hit::FingerHoverOut(_) => {
                    self.do_hover_out(cx, scope, button_ref);
                    break;
                }
                // A long press is treated as a hover-in.
                Hit::FingerLongPress(_) => {
                    self.do_hover_in(cx, scope, button_ref, reaction_data.clone());
                    break;
                }
                Hit::FingerUp(fue) => {
                    // If the finger is not over the button, treat it as a hover-out.
                    if !fue.is_over {
                        self.do_hover_out(cx, scope, button_ref);
                    }
                    // Otherwise, a primary click/press over the button should toggle the reaction.
                    else if fue.is_primary_hit() && fue.was_tap() {
                        let Some(kind) = &self.timeline_kind else { return };
                        let Some(timeline_event_id) = &self.timeline_event_id else {
                            return;
                        };
                        submit_async_request(MatrixRequest::ToggleReaction {
                            timeline_kind: kind.clone(),
                            timeline_event_id: timeline_event_id.clone(),
                            reaction: reaction_data.reaction.clone(),
                        });
                        // update the reaction button before the timeline is updated
                        let (bg_color, border_color) = if !reaction_data.includes_user {
                            (self.appearance.selected, self.appearance.accent)
                        } else {
                            (self.appearance.field, self.appearance.border)
                        };
                        let mut reaction_button = button_ref.clone();
                        script_apply_eval!(cx, reaction_button, {
                            draw_bg +: { reaction_bg_color: #(bg_color), reaction_border_color: #(border_color) }
                        });
                        self.do_hover_out(cx, scope, button_ref);
                    }
                    break;
                }
                Hit::FingerScroll(_) => {
                    self.do_hover_out(cx, scope, button_ref);
                    break;
                }
                _ => {}
            }
        }
    }
}

impl ReactionList {
    /// Deals with to any event/hit that triggers a hover-in action.
    fn do_hover_in(
        &self,
        cx: &mut Cx,
        _scope: &mut Scope,
        button_ref: &ButtonRef,
        reaction_data: ReactionData,
    ) {
        cx.widget_action(
            self.widget_uid(), 
            RoomScreenTooltipActions::HoverInReactionButton {
                widget_rect: button_ref.area().rect(cx),
                reaction_data,
            },
        );
        let mut button_ref = button_ref.clone();
        script_apply_eval!(cx, button_ref, { draw_bg +: { hover: 1.0 } });
        cx.set_cursor(MouseCursor::Hand);
    }

    /// Deals with to any event/hit that triggers a hover-out action.
    fn do_hover_out(
        &self,
        cx: &mut Cx,
        _scope: &mut Scope,
        button_ref: &ButtonRef,
    ) {
        cx.widget_action(self.widget_uid(),  RoomScreenTooltipActions::HoverOut);
        let mut button_ref = button_ref.clone();
        script_apply_eval!(cx, button_ref, { draw_bg +: { hover: 0.0 } });
        cx.set_cursor(MouseCursor::Default);
    }
}


impl ReactionListRef {
    /// Set the list of reactions and their counts to display in the ReactionList widget,
    /// along with the room ID and event ID that these reactions are for.
    ///
    /// This will clear any existing list of reactions and replace it with the given one.
    ///
    /// The given `event_tl_item_reactions` is a map from each reaction's raw string (including any variant selectors)
    /// to the list of users who have reacted with that reaction.
    ///
    /// The given `room_id` is the ID of the room that these reactions are for.
    ///
    /// The given `timeline_event_item_id` is the ID of the event that these reactions are for.
    /// Required by Matrix API
    pub fn set_list(
        &mut self,
        cx: &mut Cx,
        event_tl_item_reactions: Option<&ReactionsByKeyBySender>,
        timeline_kind: &TimelineKind,
        timeline_event_item_id: &TimelineEventItemId,
        _id: usize,
    ) {
        let Some(mut inner) = self.borrow_mut() else { return };
        let Some(curr_user_id) = current_user_id() else { return };
        let Some(event_tl_item_reactions) = event_tl_item_reactions else {
            inner.children.clear();
            inner.last_reaction_counts = None;
            return;
        };

        // try to see if we can skip populating/redrawing if nothing's changed.
        let mut total_senders = 0;
        let mut num_sent_by_me = 0;
        for (_, reaction_senders) in event_tl_item_reactions.iter() {
            total_senders += reaction_senders.len();
            num_sent_by_me += reaction_senders.contains_key(&curr_user_id) as usize;
        }
        let new_counts = (event_tl_item_reactions.len(), total_senders, num_sent_by_me);
        if inner.last_reaction_counts == Some(new_counts)
            && inner.timeline_event_id.as_ref() == Some(timeline_event_item_id)
        {
            return;
        }
        // here, things have changed, so we need to repopulate the list of reactions and redraw it.
        inner.last_reaction_counts = Some(new_counts);
        inner.children.clear();
        for (reaction_text, reaction_senders) in event_tl_item_reactions.iter() {
            let mut includes_user: bool = false;
            for (sender, _) in reaction_senders.iter() {
                if sender == &curr_user_id {
                    includes_user = true;
                }
                // Pre-fetch the profile of each reaction sender into the cache for future use in the tooltip.
                // TODO: we shouldn't do this, it's wasteful. Instead, fetch it on demand
                //       when showing the tooltip; we just need to add a way to update a live tooltip.
                //       And now that we have room member lists, we can just use that (especially once
                //       we convert them to a hashmap in the async backend, in the future)
                let _ = user_profile_cache::with_user_profile(
                    cx,
                    sender.clone(),
                    Some(timeline_kind.room_id()),
                    true, |_, _| { },
                );
            }

            let reaction_data = ReactionData {
                reaction: reaction_text.to_string(),
                includes_user,
                reaction_senders: reaction_senders.clone(),
                room_id: timeline_kind.room_id().clone(),
            };
            let mut button = widget_ref_from_live_ptr(cx, inner.item).as_button();
            button.set_text(cx, &format!("{}  {}",
                reaction_data.reaction,
                reaction_senders.len()
            ));
            let (bg_color, border_color) = if reaction_data.includes_user {
                (inner.appearance.selected, inner.appearance.accent)
            } else {
                (inner.appearance.field, inner.appearance.border)
            };
            script_apply_eval!(cx, button, {
                draw_bg +: { reaction_bg_color: #(bg_color), reaction_border_color: #(border_color) }
            });
            inner.children.push((button, reaction_data));
        }
        inner.timeline_kind = Some(timeline_kind.clone());
        inner.timeline_event_id = Some(timeline_event_item_id.clone());
    }

    /// Returns `true` if the `abs` coord is within one of the reaction buttons.
    pub fn contains_button(&self, cx: &Cx, abs: DVec2) -> bool {
        let Some(inner) = self.borrow() else { return false };
        inner.children.iter().any(|(button, _)|
            button.area().clipped_rect(cx).contains(abs)
        )
    }

    /// Returns any `RoomScreenTooltipActions` that occurred in the given list of `actions`.
    ///
    /// This function checks if there is a widget action associated with the current
    /// widget's unique identifier in the provided `actions`.
    /// If an action exists, it is cast to `RoomScreenTooltipActions` and returned.
    /// Otherwise, it returns `RoomScreenTooltipActions::None`.
    pub fn hovered_in(&self, actions: &Actions) -> RoomScreenTooltipActions {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            item.cast()
        } else {
            RoomScreenTooltipActions::None
        }
    }
    /// Returns whether the given `actions` contained a `RoomScreenTooltipActions::HoverOut` action.
    pub fn hovered_out(&self, actions: &Actions) -> bool {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            matches!(item.cast(), RoomScreenTooltipActions::HoverOut)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    use crate::theme::{self, Accent, Appearance, Selection};

    #[test]
    fn retained_reaction_reapplies_without_reentering_the_vm() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut reactions = cx.with_vm(|vm| {
            theme::tests::install(vm, Selection::default());
            crate::shared::styles::script_mod(vm);
            self::super::script_mod(vm);
            let value = script_eval!(vm, {mod.widgets.ReactionList {}});
            ReactionList::script_from_value(vm, value)
        });
        let button = widget_ref_from_live_ptr(&mut cx, reactions.item).as_button();
        button.set_text(&mut cx, "👍 2");
        let uid = button.widget_uid();
        reactions.children.push((button, ReactionData {
            reaction: "👍".into(), includes_user: true,
            reaction_senders: IndexMap::new(),
            room_id: matrix_sdk::ruma::room_id!("!theme:example.org").to_owned(),
        }));
        let old_revision = reactions.appearance.revision;
        cx.with_vm(|vm| vm.with_reload(|vm| {
            theme::tests::install(vm, Selection {appearance: Appearance::Dark, accent: Accent::Violet});
            crate::shared::styles::script_mod(vm);
            self::super::script_mod(vm);
            let value = script_eval!(vm, {mod.widgets.ReactionList {}});
            reactions.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), value);
            assert!(vm.take_errors().is_empty());
        }));
        assert_eq!(reactions.children[0].0.widget_uid(), uid);
        assert_eq!(reactions.children[0].0.text(), "👍 2");
        assert!(reactions.children[0].1.includes_user);
        assert_ne!(reactions.appearance.revision, old_revision);
    }
}
