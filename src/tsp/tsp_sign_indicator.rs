//! An indicator badge shown next to a message that has a TSP signature.

use crate::theme::Snapshot as ThemeSnapshot;
use makepad_widgets::*;


script_mod! {
    link tsp_enabled

    use mod.prelude.widgets.*
    use mod.widgets.*


    mod.widgets.TspSignIndicator = #(TspSignIndicator::register_widget(vm)) {
        visible: false, // default to hidden
        width: Fit, height: Fit
        flow: Right,
        padding: 0,
        margin: Inset{ top: 5 }

        // TODO: re-enable this once we have implemented the ability
        // to click on the indicator to show the user's profile and TSP info.
        // cursor: MouseCursor.Hand,

        tsp_html := Html {
            width: Fit, height: Fit
            flow: Flow.Right { wrap: false },
            padding: 0,
            margin: 0,

            font_size: (9.5 * mod.widgets.RINX_TEXT_SCALE),
            font_color: (TIMESTAMP_TEXT_COLOR),
            body: "TSP ❔"
        }
    }
}

/// The state of a TSP signature.
#[derive(Debug, Default)]
pub enum TspSignState {
    /// The sender is unknown to the current user.
    #[default]
    Unknown,
    /// The sender is verified by the current user.
    Verified,
    /// The sender has a different signature than the one previously verified.
    WrongSignature,
}


/// An indicator that is shown nearby a message that has a TSP signature.
///
/// This widget is basically just a clickable icon group that shows
/// the TSP logo plus an emoji indicating whether the sender has been verified
/// by the current user.
/// * If the sender is verified, a green checkmark '✅' is shown.
/// * If the sender is unknown, a gray question mark '❔' is shown.
/// * If the sender is using a different signature than the one previously verified,
///   a red exclamation mark '❗' is shown (could also use a red X '❌').
/// * If the message doesn't contain a TSP signature, nothing at all is shown.
///
#[derive(Script, Widget)]
pub struct TspSignIndicator {
    #[rust]
    appearance: ThemeSnapshot,
    #[deref]
    view: View,
    #[rust]
    state: TspSignState,
}

impl ScriptHook for TspSignIndicator {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, _: &Apply, _: &mut Scope, _: ScriptValue) {
        self.appearance = crate::theme::snapshot_for_vm(vm);
    }
}
impl Widget for TspSignIndicator {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        let area = self.view.area();
        let should_hover_in = match event.hits(cx, area) {
            Hit::FingerLongPress(_)
            | Hit::FingerHoverIn(..) => true,
            // TODO: show user profile and TSP info on click
            // Hit::FingerUp(fue) if fue.is_over && fue.is_primary_hit() => {
            //     log!("todo: show user profile and TSP info.");
            //     false
            // },
            Hit::FingerHoverOut(_) => {
                cx.widget_action(self.widget_uid(),  TooltipAction::HoverOut);
                false
            }
            _ => false,
        };
        if should_hover_in {
            let (text, text_color) = match self.state {
                TspSignState::Unknown => (
                    "The sender's TSP signature is unknown.\n\nClick on their avatar to verify their TSP identity.",
                    self.appearance.muted,
                ),
                TspSignState::Verified => (
                    "This message was signed with the user's verified TSP identity.",
                    self.appearance.role("color.status.success.foreground"),
                ),
                TspSignState::WrongSignature => (
                    "Warning: this message's TSP signature does NOT match the expected sender signature.",
                    self.appearance.role("color.status.danger.foreground"),
                ),
            };
            cx.widget_action(
                self.widget_uid(), 
                TooltipAction::HoverIn {
                    text: text.to_string(),
                    widget_rect: area.rect(cx),
                    options: CalloutTooltipOptions {
                        text_color,
                        bg_color: self.appearance.surface,
                        ..Default::default()
                    },
                },
            );
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl TspSignIndicator {
    /// Sets this indicator to show given state of this message's TSP signature.
    pub fn show_with_state(&mut self, cx: &mut Cx, state: TspSignState) {
        let tsp_html_ref = self.view.html(cx, ids!(tsp_html));
        if let Some(mut tsp_html) = tsp_html_ref.borrow_mut() {
            let (text, font_color) = match state {
                TspSignState::Unknown => ("TSP ❔", self.appearance.muted),
                TspSignState::Verified => (
                    "TSP ✅",
                    self.appearance.role("color.status.success.foreground"),
                ),
                TspSignState::WrongSignature => (
                    "❗TSP❗",
                    self.appearance.role("color.status.danger.foreground"),
                ),
            };
            tsp_html.set_text(cx, text);
            tsp_html.font_color = font_color;
        }
        self.state = state;
        self.visible = true;
        self.redraw(cx);
    }

    /// Hides this indicator, e.g., for a message that has no TSP signature.
    pub fn hide(&mut self, cx: &mut Cx) {
        self.visible = false;
        self.redraw(cx);
    }
}

impl TspSignIndicatorRef {
    /// See [`TspSignIndicator::set_state()`].
    pub fn show_with_state(&self, cx: &mut Cx, state: TspSignState) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show_with_state(cx, state);
        }
    }

    /// See [`TspSignIndicator::hide()`].
    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.hide(cx);
        }
    }
}


/// Actions emitted by an `TspSignIndicator` widget.
#[derive(Clone, Debug, Default)]
pub enum TspSignIndicatorAction {
    /// The indicator was clicked, and thus we should open
    /// a modal/dialog showing the message's full edit history.
    ShowEditHistory,
    #[default]
    None,
}
