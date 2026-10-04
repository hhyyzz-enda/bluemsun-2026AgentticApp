//! Offline native fixture for chat body selection. It never logs in or sends messages.
use makepad_widgets::*;
use rinx::shared::html_or_plaintext::HtmlOrPlaintextWidgetRefExt;
use rinx::home::{new_message_context_menu::{MessageAbilities, MessageDetails, NewMessageContextMenuWidgetRefExt}, room_screen::MessageAction};
use matrix_sdk_ui::timeline::TimelineEventItemId;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(510, 660)
                window.title: "Rinx · chat text selection test"
                body +: {
                    flow: Overlay
                    content := View {
                    flow: Down spacing: 16 padding: 20
                    Label {text: "Native chat selection · offline fixture"}
                    plain := HtmlOrPlaintext {selectable: true}
                    rich := HtmlOrPlaintext {selectable: true}
                    wrapped := HtmlOrPlaintext {selectable: true width: 160}
                    preview := HtmlOrPlaintext {}
                    controls := View {
                        width: Fill height: Fit flow: Right spacing: 8
                        inspect := Button {text: "Inspect selection" grab_key_focus: false}
                        reset := Button {text: "Replace text" grab_key_focus: false}
                        menu_button := Button {text: "Menu" grab_key_focus: false}
                    }
                    result := Label {width: Fill height: Fit text: "Ready"}
                    }
                    menu := NewMessageContextMenu {}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live] ui: WidgetRef,
    #[rust] link_clicks: usize,
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.ui.html_or_plaintext(cx, ids!(plain)).show_plaintext(cx,
            "Alpha 中文 👩‍💻 bravo & <literal>\nSecond line has selectable words.\nThird line ends here.");
        self.ui.html_or_plaintext(cx, ids!(rich)).show_html(cx,
            "<p>Rich <b>bold 中文</b> and <i>italic words</i>.</p><p><a href='https://example.org/'>Selectable link text</a> after the link.</p>");
        self.ui.html_or_plaintext(cx, ids!(wrapped)).show_plaintext(cx,
            "Wrapped 中文 text crosses several visual lines without losing spaces or emoji 👩‍💻.");
        self.ui.html_or_plaintext(cx, ids!(preview)).show_plaintext(cx, "Preview remains a non-selectable Label.");
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for action in actions {
            if matches!(action.as_widget_action().cast::<HtmlLinkAction>(), HtmlLinkAction::Clicked { .. }) {
                self.link_clicks += 1;
            }
            if let MessageAction::CopySelectedText(text) = action.as_widget_action().cast() {
                self.ui.label(cx, ids!(result)).set_text(cx, &serde_json::json!({"menu_copy": text}).to_string());
            }
        }
        if self.ui.button(cx, ids!(menu_button)).clicked(actions) {
            let plain = self.ui.html_or_plaintext(cx, ids!(plain)).selected_text(cx);
            let selected = if plain.is_empty() { self.ui.html_or_plaintext(cx, ids!(rich)).selected_text(cx) } else { plain };
            self.ui.new_message_context_menu(cx, ids!(menu)).show(cx, MessageDetails {
                item_id: 0,
                timeline_event_id: TimelineEventItemId::EventId(matrix_sdk::ruma::event_id!("$fixture:example.org").to_owned()),
                related_event_id: None, thread_root_event_id: None,
                room_screen_widget_uid: self.ui.widget_uid(),
                should_be_highlighted: false,
                abilities: MessageAbilities::empty(),
                selected_text: (!selected.is_empty()).then_some(selected),
            });
            self.ui.redraw(cx);
        }
        if self.ui.button(cx, ids!(reset)).clicked(actions) {
            self.ui.html_or_plaintext(cx, ids!(plain)).show_plaintext(cx, "Replacement 中文");
        }
        if self.ui.button(cx, ids!(inspect)).clicked(actions) {
            let response = std::rc::Rc::new(std::cell::RefCell::new(None));
            self.ui.handle_event(cx, &Event::TextCopy(TextClipboardEvent {response: response.clone()}), &mut Scope::empty());
            let state = serde_json::json!({
                "plain": self.ui.html_or_plaintext(cx, ids!(plain)).selected_text(cx),
                "rich": self.ui.html_or_plaintext(cx, ids!(rich)).selected_text(cx),
                "wrapped": self.ui.html_or_plaintext(cx, ids!(wrapped)).selected_text(cx),
                "copy": *response.borrow(),
                "link_clicks": self.link_clicks,
            });
            self.ui.label(cx, ids!(result)).set_text(cx, &state.to_string());
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        rinx::i18n::install(vm);
        makepad_code_editor::script_mod(vm);
        rinx::shared::script_mod(vm);
        rinx::home::new_message_context_menu::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        let menu = self.ui.new_message_context_menu(cx, ids!(menu));
        if menu.is_currently_shown(cx) && rinx::utils::is_interactive_hit_event(event) {
            menu.handle_event(cx, event, &mut Scope::empty());
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
    }
}
