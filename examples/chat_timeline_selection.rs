//! Offline selection fixture using the production chat timeline and message bodies.
use makepad_widgets::*;
use rinx::shared::html_or_plaintext::HtmlOrPlaintextWidgetRefExt;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(720, 640)
                window.title: "Rinx · timeline selection test"
                body +: {
                    flow: Down
                    controls := View {
                        width: Fill height: Fit flow: Right
                        inspect := Button {text: "Inspect" grab_key_focus: false}
                        reset := Button {text: "Reset position" grab_key_focus: false}
                    }
                    result := Label {width: Fill height: 65 text: "Ready"}
                    timeline := Timeline {
                        list +: {
                            Fixture := View {
                                width: Fill height: 160 flow: Down
                                padding: Inset{left: 70 right: 70 top: 12 bottom: 12}
                                row_id := Label {text: "Row"}
                                body := HtmlOrPlaintext {selectable: true}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live] ui: WidgetRef,
    #[rust] link_clicks: usize,
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let list = self.ui.portal_list(cx, ids!(timeline.list));
        list.set_tail_range(false);
        list.set_first_id_and_scroll(5, 0.0);
    }

    fn handle_draw(&mut self, cx: &mut Cx, event: &DrawEvent) {
        let mut draw = CxDraw::new(cx, event);
        let mut cx = Cx2d::new(&mut draw);
        while let Some(widget) = self.ui.draw(&mut cx, &mut Scope::empty()).step() {
            let mut list = widget.borrow_mut::<PortalList>().unwrap();
            list.set_item_range(&mut cx, 0, 30);
            while let Some(index) = list.next_visible_item(&mut cx) {
                let row = list.item(&mut cx, index, id!(Fixture));
                row.label(&mut cx, ids!(row_id)).set_text(&mut cx, &format!("Row {index}"));
                let body = row.html_or_plaintext(&mut cx, ids!(body));
                if index % 2 == 0 {
                    body.show_plaintext(&mut cx, "Alpha 中文 👩‍💻 selectable text.\nSecond line continues selection.\nThird line ends here.");
                } else {
                    body.show_html(&mut cx, "<p><a href='https://example.org/'>Selectable link text</a> and <b>bold 中文</b>.</p><p>Second line continues selection.</p><p>Third line ends here.</p>");
                }
                row.draw_all(&mut cx, &mut Scope::empty());
            }
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for action in actions {
            if matches!(action.as_widget_action().cast::<HtmlLinkAction>(), HtmlLinkAction::Clicked {..}) {
                self.link_clicks += 1;
            }
        }
        let list = self.ui.portal_list(cx, ids!(timeline.list));
        if self.ui.button(cx, ids!(reset)).clicked(actions) {
            list.set_tail_range(false);
            list.set_first_id_and_scroll(5, 0.0);
            list.redraw(cx);
        }
        if self.ui.button(cx, ids!(inspect)).clicked(actions) {
            let mut selections = Vec::new();
            for index in 4..10 {
                if let Some((_, row)) = list.get_item(index) {
                    let body = row.html_or_plaintext(cx, ids!(body));
                    selections.push(serde_json::json!({"index": index, "text": body.selected_text(cx)}));
                }
            }
            let state = serde_json::json!({
                "first_id": list.first_id(), "scroll": list.scroll_position(),
                "selections": selections, "link_clicks": self.link_clicks,
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
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        if !matches!(event, Event::Draw(_)) {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
    }
}
