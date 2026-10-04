//! Rinx's own sheet for the assistant's requests: reading a room (once,
//! always, deny) and sending a message (the exact text; send or cancel).
//! It is the only place the person answers them.
use super::{Choice, Prompt, PromptKind};
use makepad_widgets::*;
use std::time::Instant;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.AssistantSheet = set_type_default() do #(AssistantSheet::register_widget(vm)) {
        ..mod.widgets.SmallModal

        title := ModalTitle {}
        body := ModalBody {}
        message_view := RoundedView {
            visible: false
            width: Fill, height: Fit
            margin: Inset{top: 12}
            padding: 12
            show_bg: true
            draw_bg +: { color: mod.widgets.RINX_FIELD border_radius: 4.0 }
            message := Label {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true}
                draw_text +: {
                    text_style: REGULAR_TEXT {font_size: (11.5 * mod.widgets.RINX_TEXT_SCALE)},
                    color: mod.widgets.RINX_INK
                }
            }
        }

        buttons_view := ModalButtonsRow {
            spacing: 10
            deny_button := RobrixNeutralIconButton {
                width: Fit{min: FitBound.Abs(88.0)},
                align: Align{x: 0.5, y: 0.5}
                padding: Inset{left: 12, right: 12, top: 14, bottom: 14},
                icon_walk: Walk{width: 0, height: 0, margin: 0}
                text: #(crate::i18n::tr("Deny")) i18n_text: "Deny"
            }
            always_button := RobrixPositiveIconButton {
                width: Fit{min: FitBound.Abs(88.0)},
                align: Align{x: 0.5, y: 0.5}
                padding: Inset{left: 12, right: 12, top: 14, bottom: 14},
                icon_walk: Walk{width: 0, height: 0, margin: 0}
                text: #(crate::i18n::tr("Always allow")) i18n_text: "Always allow"
            }
            once_button := RobrixPositiveIconButton {
                width: Fit{min: FitBound.Abs(88.0)},
                align: Align{x: 0.5, y: 0.5}
                padding: Inset{left: 12, right: 12, top: 14, bottom: 14},
                icon_walk: Walk{width: 0, height: 0, margin: 0}
                text: #(crate::i18n::tr("Allow once")) i18n_text: "Allow once"
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct AssistantSheet {
    #[deref] view: View,
    /// Fires at the shown prompt's deadline: no answer is a denial.
    #[rust] timer: Timer,
}

impl Widget for AssistantSheet {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.timer.is_event(event).is_some() {
            self.timer = Timer::default();
            super::expire(cx);
        }
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else { return };
        let choice = if self.view.button(cx, ids!(once_button)).clicked(actions) {
            Some(Choice::Once)
        } else if self.view.button(cx, ids!(always_button)).clicked(actions) {
            Some(Choice::Always)
        } else if self.view.button(cx, ids!(deny_button)).clicked(actions) {
            Some(Choice::Deny)
        } else {
            None
        };
        if let Some(choice) = choice {
            cx.stop_timer(self.timer);
            self.timer = Timer::default();
            super::answer(cx, choice);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl AssistantSheet {
    fn show(&mut self, cx: &mut Cx, prompt: &Prompt) {
        let room = &prompt.room.name;
        let (title, body, message) = match &prompt.kind {
            PromptKind::ReadGrant { .. } => (
                crate::i18n::tr("Let the assistant read this chat?"),
                crate::i18n::format(
                    "The assistant asks to read the latest messages in {room}. \"Always allow\" lets it read this chat on this account until you revoke it in Settings > Privacy.",
                    &[("room", room.to_string())],
                ),
                None,
            ),
            PromptKind::Send { text } => (
                crate::i18n::tr("Send this message?"),
                crate::i18n::format("The assistant wants to send this message to {room}:", &[("room", room.to_string())]),
                Some(text.as_str()),
            ),
        };
        let is_read = message.is_none();
        self.view.label(cx, ids!(title)).set_text(cx, title);
        self.view.label(cx, ids!(body)).set_text(cx, &body);
        self.view.view(cx, ids!(message_view)).set_visible(cx, !is_read);
        self.view.label(cx, ids!(message)).set_text(cx, message.unwrap_or_default());
        self.view.button(cx, ids!(always_button)).set_visible(cx, is_read);
        self.view
            .button(cx, ids!(once_button))
            .set_text(cx, if is_read { crate::i18n::tr("Allow once") } else { crate::i18n::tr("Send") });
        self.view
            .button(cx, ids!(deny_button))
            .set_text(cx, if is_read { crate::i18n::tr("Deny") } else { crate::i18n::tr("Cancel") });
        for button in [ids!(once_button), ids!(always_button), ids!(deny_button)] {
            self.view.button(cx, button).reset_hover(cx);
        }
        cx.stop_timer(self.timer);
        let left = prompt.deadline.saturating_duration_since(Instant::now()).as_secs_f64();
        self.timer = cx.start_timeout(left.max(0.05));
        self.view.redraw(cx);
    }

    fn hide(&mut self, cx: &mut Cx) {
        cx.stop_timer(self.timer);
        self.timer = Timer::default();
    }
}

impl AssistantSheetRef {
    pub fn show(&self, cx: &mut Cx, prompt: &Prompt) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show(cx, prompt);
        }
    }

    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.hide(cx);
        }
    }
}
