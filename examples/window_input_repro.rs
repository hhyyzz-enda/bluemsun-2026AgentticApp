//! Repro v2: dynamically-created second window (ArticleWindowHost pattern).
//!
//! Main window has a button that spawns the second OS window at runtime via a
//! host widget under Root — exactly how apps/article-editor/native/window.rs
//! does it. The second window holds a counter button and a TextInput.
//! If the button counts but the input never takes focus/text, the bug is in
//! dynamic-window key routing; if both work, the framework is fine.
use makepad_widgets::*;
app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.ReproWindow = Window {
        window.inner_size: vec2(420, 260)
        window.position: vec2(500, 120)
        body +: {
            flow: Down padding: 20 spacing: 12
            Label {text: "Second window (dynamic)" draw_text.text_style: theme.font_bold{font_size: 14}}
            counter_btn := Button {text: "Count: 0"}
            second_input := TextInput {width: Fill height: 44 empty_text: "Click here, then type"}
            second_status := Label {width: Fill height: Fit text: "second: (no focus event yet)"}
        }
    }

    mod.widgets.ReproWindowHost = #(ReproWindowHost::register_widget(vm)) {}

    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(420, 260)
                body +: {
                    flow: Down padding: 20 spacing: 12
                    Label {text: "Main window" draw_text.text_style: theme.font_bold{font_size: 14}}
                    open_btn := Button {text: "Open second window"}
                    main_input := TextInput {width: Fill height: 44 empty_text: "Type in main window"}
                    main_status := Label {width: Fill height: Fit text: "main: (no focus event yet)"}
                }
            }
            host := mod.widgets.ReproWindowHost {}
        }
    }
}

#[derive(Script, WidgetRef, WidgetRegister)]
pub struct ReproWindowHost {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[rust] area: Area,
    #[rust] window: Option<WidgetRef>,
}

impl ScriptHook for ReproWindowHost {}

impl WidgetNode for ReproWindowHost {
    fn widget_uid(&self) -> WidgetUid { self.uid }
    fn area(&self) -> Area { self.area }
    fn walk(&mut self, _cx: &mut Cx) -> Walk { Walk::default() }
    fn redraw(&mut self, cx: &mut Cx) {
        if let Some(window) = self.window.as_ref() {
            window.redraw(cx);
        }
    }
    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        if let Some(window) = self.window.as_ref() {
            visit(id!(repro_window), window.clone());
        }
    }
}

impl Widget for ReproWindowHost {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(window) = self.window.clone() {
            window.handle_event(cx, event, scope);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, _walk: Walk) -> DrawStep {
        if let Some(window) = self.window.as_ref() {
            let walk = window.walk(cx);
            window.draw_walk(cx, scope, walk)?;
        }
        DrawStep::done()
    }
}

impl ReproWindowHost {
    fn open(&mut self, cx: &mut Cx) {
        if self.window.is_none() {
            let window = cx.with_vm(|vm| {
                let template = vm.eval(script! { mod.widgets.ReproWindow });
                WidgetRef::script_from_value(vm, template)
            });
            self.window = Some(window);
            cx.widget_tree_mark_dirty(self.uid);
        }
        self.window.as_ref().unwrap().redraw(cx);
    }
}

impl ReproWindowHostRef {
    fn open(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.open(cx);
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    count: u32,
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.ui.button(cx, ids!(open_btn)).clicked(actions) {
            self.ui.repro_window_host(cx, ids!(host)).open(cx);
        }
        let host = self.ui.repro_window_host(cx, ids!(host));
        if let Some(inner) = host.borrow() {
            if let Some(window) = &inner.window {
                if window.button(cx, ids!(counter_btn)).clicked(actions) {
                    self.count += 1;
                    window
                        .button(cx, ids!(counter_btn))
                        .set_text(cx, &format!("Count: {}", self.count));
                }
                let input = window.text_input(cx, ids!(second_input));
                if let Some(text) = input.changed(actions) {
                    window
                        .label(cx, ids!(second_status))
                        .set_text(cx, &format!("second: changed → \"{text}\""));
                }
            }
        }
        let input = self.ui.text_input(cx, ids!(main_input));
        if let Some(text) = input.changed(actions) {
            self.ui
                .label(cx, ids!(main_status))
                .set_text(cx, &format!("main: changed → \"{text}\""));
        }
        for action in actions {
            if let TextInputAction::KeyFocus = action.as_widget_action().cast() {
                log!("repro: some TextInput got KeyFocus");
            }
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
