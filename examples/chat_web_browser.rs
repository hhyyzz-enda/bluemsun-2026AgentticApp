//! Native WebKit fixture, served only by the loopback regression harness.
use makepad_widgets::*;
use rinx::shared::web_browser::{
    open_chat_link, WebBrowserAction, WebBrowserRef, WebBrowserWidgetRefExt,
};
use rinx::shared::web_browser_window::WebBrowserWindowHostWidgetRefExt;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.WebBrowserWindow = mod.widgets.WebBrowserWindow {
        body +: {web_browser +: {toolbar +: {
            fixture_next := Button {text: "Next"}
            fixture_inspect := Button {text: "Inspect"}
            fixture_scroll := Button {text: "Scroll"}
            fixture_new_tab := Button {text: "New tab"}
            fixture_markdown := Button {text: "Markdown"}
        }}}
    }
    startup() do #(App::script_component(vm)) {
        ui: Root {
            browser_window := WebBrowserWindowHost {}
            main_window := Window {
                window.inner_size: vec2(920, 760)
                window.title: "Rinx · in-app browser fixture"
                body +: {
                    flow: Down padding: 24 spacing: 12
                    open_link := Button {text: "Open chat link"}
                    chat_action := Button {text: "Chat remains interactive"}
                    chat_status := Label {text: "Chat idle"}
                    status := Label {text: "Ready"}
                    browser_modal := Modal {
                        can_dismiss: false
                        content := WebBrowser {
                            toolbar +: {
                                fixture_next := Button {text: "Next"}
                                fixture_inspect := Button {text: "Inspect"}
                                fixture_scroll := Button {text: "Scroll"}
                                fixture_new_tab := Button {text: "New tab"}
                                fixture_markdown := Button {text: "Markdown"}
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
    #[live]
    ui: WidgetRef,
    #[rust]
    timer: Option<Timer>,
    #[rust]
    markdown_count: usize,
}

impl App {
    fn desktop(&self) -> bool {
        std::env::var("RINX_BROWSER_FIXTURE_MODE").is_ok_and(|mode| mode == "desktop")
    }

    fn browser(&self, cx: &mut Cx) -> WebBrowserRef {
        if self.desktop() {
            self.ui
                .web_browser_window_host(cx, ids!(browser_window))
                .browser(cx)
        } else {
            self.ui.web_browser(cx, ids!(browser_modal.content))
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.timer = Some(cx.start_interval(0.25));
        if let Ok(path) = std::env::var("RINX_BROWSER_FIXTURE_SESSION") {
            let session = rinx::shared::web_browser_session::ReaderSession::load_from(
                std::path::Path::new(&path),
            )
            .unwrap();
            if !session.tabs.is_empty() {
                cx.action(WebBrowserAction::Restore {
                    session,
                    account: None,
                });
            }
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let browser = self.browser(cx);
        let base = std::env::var("RINX_BROWSER_FIXTURE_URL").unwrap();
        if self.ui.button(cx, ids!(open_link)).clicked(actions) {
            assert!(open_chat_link(cx, &format!("{base}/redirect")));
        }
        if self.ui.button(cx, ids!(chat_action)).clicked(actions) {
            self.ui
                .label(cx, ids!(chat_status))
                .set_text(cx, "Chat clicked");
        }
        if browser.button(cx, ids!(fixture_markdown)).clicked(actions) {
            self.markdown_count += 1;
            let mut source = format!(
                "# Shared Markdown\n\n**Bold text** and *emphasis*.\n\n| Feature | Status |\n| --- | --- |\n| Tables | Ready |\n| Tabs | Ready |\n\n```rust\nlet answer = 42;\n```\n\n[Open webpage]({base}/first)\n\n"
            );
            for i in 1..=30 {
                source.push_str(&format!(
                    "## Section {i}\n\nDocument {} paragraph {i}. 中文阅读测试.\n\n",
                    self.markdown_count
                ));
            }
            cx.action(WebBrowserAction::OpenMarkdown {
                title: format!("Shared notes {}.md", self.markdown_count),
                source: source.into(),
                account: None,
                attachment: None,
            });
        }
        if browser.button(cx, ids!(fixture_new_tab)).clicked(actions) {
            assert!(open_chat_link(cx, &format!("{base}/first")));
        }
        if browser.button(cx, ids!(fixture_next)).clicked(actions) {
            if let Some(id) = browser.browser_id() {
                cx.system_browser(id)
                    .set_url(&format!("{base}/second"), false);
            } else {
                browser
                    .portal_list(cx, ids!(article_reader))
                    .set_first_id_and_scroll(16, 0.0);
                self.ui.redraw(cx);
            }
        }
        #[cfg(target_os = "macos")]
        if browser.button(cx, ids!(fixture_inspect)).clicked(actions)
            || browser.button(cx, ids!(fixture_scroll)).clicked(actions)
        {
            let scroll_y = browser
                .button(cx, ids!(fixture_scroll))
                .clicked(actions)
                .then_some(320.0);
            if let Some(id) = browser.browser_id() {
                let dir = std::env::var("RINX_BROWSER_FIXTURE_OUTPUT").unwrap();
                cx.system_browser(id).inspect(
                    format!("{dir}/webkit.json"),
                    Some(format!("{dir}/webkit.png")),
                    scroll_y,
                );
            }
        }
        for action in actions {
            if let Some(action) = action.downcast_ref::<WebBrowserAction>() {
                if self.desktop() {
                    self.ui
                        .web_browser_window_host(cx, ids!(browser_window))
                        .action(cx, action);
                } else {
                    browser.action(cx, self.ui.modal(cx, ids!(browser_modal)), action);
                }
            }
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        rinx::theme::init_standalone(vm);
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        let closing_main = matches!(event, Event::WindowCloseRequested(e) if Some(e.window_id) == self.ui.window(cx, ids!(main_window)).window_id());
        if closing_main {
            let browser = self.browser(cx);
            if let Ok(path) = std::env::var("RINX_BROWSER_FIXTURE_SESSION") {
                browser
                    .session()
                    .save_to(std::path::Path::new(&path))
                    .unwrap();
            }
            browser.prepare_shutdown();
        }
        if let Event::Custom(command) = event {
            if command == "theme:inspect" {
                let appearance = rinx::theme::snapshot(cx);
                let label = self.browser(cx).label(cx, ids!(web_title));
                let toolbar_ink = label
                    .borrow()
                    .map(|label| rinx::theme::argb(label.draw_text.color));
                let data = serde_json::json!({"revision":appearance.revision,"ink":rinx::theme::argb(appearance.ink),"code_bg":rinx::theme::argb(appearance.role("color.code.background")),"code_fg":rinx::theme::argb(appearance.role("color.code.foreground")),"toolbar_ink":toolbar_ink,"toolbar_uid":format!("{:?}",label.widget_uid())});
                let root = std::env::var("RINX_BROWSER_FIXTURE_OUTPUT").unwrap();
                std::fs::write(
                    std::path::Path::new(&root).join("appearance.json"),
                    data.to_string(),
                )
                .unwrap();
            }
            if command == "theme:dark" || command == "theme:light" {
                let mut selection = rinx::theme::selection(cx).unwrap();
                selection.appearance = if command == "theme:dark" {
                    rinx::theme::Appearance::Dark
                } else {
                    rinx::theme::Appearance::Light
                };
                rinx::theme::select(cx, selection).unwrap();
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        rinx::theme::packages::after_event(cx, event);
        if matches!(event, Event::WindowCloseRequested(e) if Some(e.window_id) == self.ui.window(cx, ids!(main_window)).window_id())
        {
            self.ui
                .web_browser_window_host(cx, ids!(browser_window))
                .close(cx);
        }
        #[cfg(target_os = "macos")]
        if self
            .timer
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            let count = cx.system_browser_count();
            self.ui
                .label(cx, ids!(status))
                .set_text(cx, &format!("Browsers: {count}"));
        }
    }
}
