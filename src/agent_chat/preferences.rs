//! The agent-chat section of the App Settings screen.
//!
//! The toggle is shared by desktop and mobile settings.
//! Builds without the feature get an empty placeholder from
//! `crate::agent_chat_dummy` instead, so the settings screen is unchanged.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.AgentChatPreferences = #(AgentChatPreferences::register_widget(vm)) {
        ..mod.widgets.View
        width: Fill, height: Fit
        flow: Down

        SubsectionLabel {
            text: #(crate::i18n::tr("Hagency")) i18n_text: "Hagency"
        }

        View {
            width: Fill, height: Fit
            flow: Down,
            margin: Inset{left: 6},

            agent_chat_toggle := ToggleFlat {
                margin: Inset{left: 0.5, top: 5, bottom: 10}
                padding: Inset { left: 15}
                active: false,
                draw_bg +: { size: 21 }
                text: #(crate::i18n::tr("Enable agent workflow commands")) i18n_text: "Enable agent workflow commands"
                draw_text +: {
                    // `SETTINGS_BOLD_TEXT_STYLE` is registered after this module; inline its definition.
                    text_style: theme.font_bold { font_size: (mod.widgets.SETTINGS_REGULAR_FONT_SIZE) },
                }
            }
            agent_ops := ButtonFlat {width: Fill height: 40 text: #(crate::i18n::tr("Agent Operations")) i18n_text: "Agent Operations"}
            Html {
                width: Fill, height: Fit
                flow: Flow.Right{wrap: true}
                margin: Inset{left: 14, top: 0, bottom: 0, right: 5}
                padding: 0,
                font_size: (11 * mod.widgets.RINX_TEXT_SCALE),
                font_color: mod.widgets.RINX_MUTED,
                text_style_normal: MESSAGE_TEXT_STYLE { font_size: (11 * mod.widgets.RINX_TEXT_SCALE) },
                body: #(crate::i18n::tr("<p>Show workflow commands in coordinator rooms, and /task and /thread in agent rooms. Approval cards remain available when this is off.</p>"))
                i18n_body: "<p>Show workflow commands in coordinator rooms, and /task and /thread in agent rooms. Approval cards remain available when this is off.</p>"
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct AgentChatPreferences {
    #[deref] view: View,
}

impl Widget for AgentChatPreferences {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self.view.button(cx,ids!(agent_ops)).clicked(actions) {cx.action(super::ops::ui::AgentOpsAction::Open);}
            if let Some(enabled) = self.view.check_box(cx, ids!(agent_chat_toggle)).changed(actions) {
                if let Some(app) = scope.data.get_mut::<crate::app::AppState>() {
                    app.app_prefs.agent_chat_enabled = enabled;
                    app.app_prefs.on_agent_chat_enabled_changed(cx);
                }
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let enabled = cx.global::<crate::settings::app_preferences::AppPreferencesGlobal>().0.agent_chat_enabled;
        self.view.check_box(cx, ids!(agent_chat_toggle)).set_active(cx, enabled, Animate::No);
        self.view.draw_walk(cx, scope, walk)
    }
}
