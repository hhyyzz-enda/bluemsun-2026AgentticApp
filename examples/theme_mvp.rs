//! Offline native theme reference. No Matrix account, grants or kernel.
//! MAKEPAD_REMOTE drives the real widget tree; --hosted injects host styles.
pub use makepad_widgets;
use makepad_widgets::*;
use makepad_widgets::splash_host::{take_splash_host_requests_for, splash_host_respond};
use rinx::home::link_preview::{LinkPreviewCache, LinkPreviewData, LinkPreviewWidgetRefExt};
use rinx::settings::theme_studio::{ThemeStudioAction, ThemeStudioWidgetRefExt};
use rinx::theme::{self, Accent, Appearance, Selection};
use rinx::miniapps::{MiniAppsAction, MiniAppsPanelWidgetRefExt};
use rinx::article_app::{ArticleAction, ArticlePanelWidgetRefExt};
use octoscript_ui_l0::InstanceStore;
use serde_json::{Value, json};
app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: #(if std::env::args().any(|a| a == "--narrow") {dvec2(430.,820.)} else {dvec2(1000.,800.)})
                body +: {flow: Overlay
                    gallery := SolidView {width: Fill height: Fill flow: Down padding: 20 spacing: 16 draw_bg.color: RINX_PAGE
                        RinxPageTitle {text: "One theme, three native surfaces"}
                        buttons := View {width: Fill height: Fit spacing: 8 flow: Flow.Right{wrap: true}
                            light := RinxButton {text: "Light"}
                            dark := RinxButton {text: "Dark"}
                            teal := RinxButton {text: "Teal"}
                            violet := RinxButton {text: "Violet"}
                            catalog := RinxButton {text: "Mini Apps"}
                            measure := RinxButton {text: "Inspect state"}
                        }
                        appearance := AppearanceSettings {}
                        scroll := ScrollYView {width: Fill height: Fill flow: Flow.Right{wrap: true} spacing: 16
                            native := RoundedView {width: 300 height: 300 padding: 16 flow: Down spacing: 12 draw_bg +: {color: RINX_SURFACE border_radius: theme.corner_radius}
                                RinxPageTitle {text: "Native article controls"}
                                RinxHint {width: Fill text: "The article editor uses these same input and action styles."}
                                draft := ArticleInput {width: Fill text: "Unsaved native draft"}
                                actions := View {width: Fill height: Fit spacing: 8 flow: Flow.Right{wrap: true}
                                    RinxPrimaryButton {text: "Save draft"}
                                    RinxButton {text: "Cancel"}
                                }
                            }
                            script_app := Splash {width: 300 height: 300}
                            l0_app := Splash {width: 300 height: 300}
                            web_card := View {width: 300 height: Fit flow: Down
                                body_reference := RinxLabel {text: "Shared UI body 中文"}
                                card := LinkPreview {}
                            }
                            RinxHint {text: "End of reference surfaces"}
                        }
                    }
                    theme_studio_modal := Modal {can_dismiss: false content := ThemeStudio {}}
                    miniapps := Modal {can_dismiss: false content := MiniAppsPanel {}}
                    article := Modal {can_dismiss: false content := ArticlePanel {}}
                    notifications := PopupList {}
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
    revision: u64,
    #[rust]
    state: InstanceStore,
    #[rust]
    selection: Selection,
    #[rust]
    requests: usize,
}

impl App {
    fn data() -> Value {
        serde_json::from_str(include_str!("miniapps/theme-reference/data.json")).unwrap()
    }
    fn render_l0(&mut self, cx: &mut Cx) {
        let card = rinx::miniapps::presentation::prepare(
            include_str!("miniapps/theme-reference/page.card"),
            &Self::data(),
            &self.state,
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples/miniapps/theme-reference/kit"),
            &theme::snapshot(cx),
        )
        .expect("reference L0 card");
        let ui = rinx::miniapps::presentation::embedded_ui(card).unwrap();
        let source = format!("let NAV = fn(t, v=\"\") {{host.request(\"rinx.event\", {{route:t,value:v}}, fn(r){{}})}}\nwidth:Fill height:Fill\n{ui}");
        self.ui.splash(cx, ids!(l0_app)).reapply_text(cx, &source);
    }
    fn switch(&mut self, cx: &mut Cx) {
        if std::env::args().any(|a| a == "--hosted") {
            cx.with_vm(|vm| {
                desktop_style::install(
                    vm,
                    self.selection
                        .stylesheet(desktop_style::DesktopStyle::Macos),
                )
            });
            cx.request_style_reload();
        } else {
            theme::select(cx, self.selection).unwrap();
        }
    }
    fn inspect(&mut self, cx: &mut Cx) {
        let script = self.ui.splash(cx, ids!(script_app)).isolate_heap_key(cx);
        let l0 = self.ui.splash(cx, ids!(l0_app)).isolate_heap_key(cx);
        let fields: Vec<_> = [
            ids!(native.draft),
            ids!(script_app.draft),
            ids!(l0_app.beauty_0_2_0),
        ]
        .into_iter()
        .map(|path| {
            let input = self.ui.text_input(cx, path);
            let selection = input.selection();
            json!({"uid":format!("{:?}",input.widget_uid()), "text":input.text(),
                    "anchor": selection.anchor.index, "cursor":selection.cursor.index,
                    "focus":cx.has_key_focus(input.area())})
        })
        .collect();
        let timers = cx
            .script_data
            .timers
            .timers
            .iter()
            .filter(|t| Some(t.callback.heap_key()) == script)
            .count();
        let palette = theme::snapshot(cx);
        let l0_ink = self
            .ui
            .label(cx, ids!(l0_app.beauty_0_0))
            .borrow()
            .map(|label| theme::argb(label.draw_text.color));
        let font_sizes: Vec<_> = [
            ids!(web_card.body_reference),
            ids!(card.title_label),
            ids!(card.description_label),
        ]
        .into_iter()
        .map(|path| {
            self.ui
                .label(cx, path)
                .borrow()
                .map(|label| label.draw_text.text_style.font_size)
        })
        .collect();
        let report = json!({"revision":theme::snapshot(cx).revision, "script_heap":script, "l0_heap":l0,
            "requests":self.requests, "timers":timers, "fields":fields, "selection":theme::selection(cx),
            "l0_draft":self.state.get(octoscript_ui_l0::CARD_STATE_KEY, "draft"),
            "ink":theme::argb(palette.ink), "l0_ink":l0_ink,
            "accent":theme::argb(palette.accent), "scale":palette.text_scale, "radius":palette.radius,
            "font_sizes":font_sizes, "preview":theme::packages::is_preview(cx), "preferences":theme::packages::current(cx).ok()});
        std::fs::write(
            rinx::app_data_dir().join("theme-inspection.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        self.selection = theme::selection(cx).unwrap_or_default();
        let url = "https://example.org/theme-fixture".parse().unwrap();
        let mut cache = LinkPreviewCache::new(None);
        cache.insert(
            &url,
            LinkPreviewData {
                title: Some("Shared card title 中文".into()),
                description: Some("A website summary uses the same body text size as Rinx.".into()),
                site_name: Some("Example site".into()),
                ..Default::default()
            },
        );
        self.ui.link_preview(cx, ids!(card)).populate_below_message(
            cx,
            &[url],
            &mut rinx::media_cache::MediaCache::new(None),
            &mut cache,
            &|_, _, _, _, _, _| true,
        );
        let script = self.ui.splash(cx, ids!(script_app));
        script.set_host_tag(cx, Some("theme-validation".into()));
        script.set_text(cx, include_str!("miniapps/theme-reference/main.splash"));
        for (field, value) in
            octoscript_ui_l0::state_initials(include_str!("miniapps/theme-reference/page.card"))
        {
            self.state
                .set_cell(octoscript_ui_l0::CARD_STATE_KEY, &field, value);
        }
        self.render_l0(cx);
        self.revision = theme::snapshot(cx).revision;
    }
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mut changed = false;
        for (path, appearance) in [
            (ids!(light), Appearance::Light),
            (ids!(dark), Appearance::Dark),
        ] {
            if self.ui.button(cx, path).clicked(actions) {
                self.selection.appearance = appearance;
                changed = true;
            }
        }
        for (path, accent) in [(ids!(teal), Accent::Teal), (ids!(violet), Accent::Violet)] {
            if self.ui.button(cx, path).clicked(actions) {
                self.selection.accent = accent;
                changed = true;
            }
        }
        if changed {
            self.switch(cx);
        }
        if self.ui.button(cx, ids!(measure)).clicked(actions) {
            self.inspect(cx);
        }
        if self.ui.button(cx, ids!(catalog)).clicked(actions) {
            self.ui.mini_apps_panel(cx, ids!(miniapps.content)).action(
                cx,
                self.ui.modal(cx, ids!(miniapps)),
                &MiniAppsAction::Open,
            );
        }
        for action in actions {
            if let Some(action) = action.downcast_ref::<ThemeStudioAction>() {
                let modal = self.ui.modal(cx, ids!(theme_studio_modal));
                self.ui
                    .theme_studio(cx, ids!(theme_studio_modal.content))
                    .action(cx, modal, action);
            }
            if let Some(action) = action.downcast_ref::<MiniAppsAction>() {
                self.ui.mini_apps_panel(cx, ids!(miniapps.content)).action(
                    cx,
                    self.ui.modal(cx, ids!(miniapps)),
                    action,
                );
            }
            if let Some(action) = action.downcast_ref::<ArticleAction>() {
                self.ui.article_panel(cx, ids!(article.content)).action(
                    cx,
                    self.ui.modal(cx, ids!(article)),
                    action,
                );
            }
        }
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        if std::env::args().any(|a| a == "--hosted") {
            if desktop_style::current(vm).is_none() {
                desktop_style::install(
                    vm,
                    Selection::default().stylesheet(desktop_style::DesktopStyle::Macos),
                );
            }
            theme::init_hosted(vm);
        } else {
            theme::init_standalone(vm);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        desktop_style::apply_widgets(vm);
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        rinx::theme::system::handle_event(cx, event);
        if let Event::Custom(command) = event {
            match command.as_str() {
                "theme:light" => {
                    self.selection.appearance = Appearance::Light;
                    self.switch(cx);
                }
                "theme:dark" => {
                    self.selection.appearance = Appearance::Dark;
                    self.switch(cx);
                }
                "theme:teal" => {
                    self.selection.accent = Accent::Teal;
                    self.switch(cx);
                }
                "theme:violet" => {
                    self.selection.accent = Accent::Violet;
                    self.switch(cx);
                }
                "theme:import" => {
                    let result = octosense_theme_contract::ThemePackage::parse(include_bytes!(
                        "themes/ocean-violet.octotheme"
                    ));
                    cx.action(ThemeStudioAction::Imported {
                        owner: None,
                        result,
                    });
                }
                "theme:host-package" => {
                    let p = octosense_theme_contract::ThemePackage::parse(include_bytes!(
                        "themes/ocean-violet.octotheme"
                    ))
                    .unwrap();
                    let tokens = theme::packages::resolve(
                        cx,
                        &p,
                        Selection::default(),
                        desktop_style::DesktopStyle::Macos,
                    )
                    .unwrap();
                    let snapshot =
                        octosense_theme_contract::ResolvedTheme::new(p.id, false, tokens);
                    theme::host::receive(cx, &snapshot.bytes().unwrap()).unwrap();
                }
                "theme:inspect" => self.inspect(cx),
                "theme:popup" => rinx::shared::popup_list::enqueue_popup_notification(
                    "Retained theme notification",
                    rinx::shared::popup_list::PopupKind::Info,
                    Some(180.),
                ),
                _ => {}
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        rinx::theme::packages::after_event(cx, event);
        let revision = theme::snapshot(cx).revision;
        if self.revision != 0 && self.revision != revision {
            self.render_l0(cx);
            self.revision = revision;
        }
        let heaps: Vec<_> = [ids!(script_app), ids!(l0_app)]
            .into_iter()
            .filter_map(|p| self.ui.splash(cx, p).isolate_heap_key(cx))
            .collect();
        for request in take_splash_host_requests_for(&heaps) {
            if request.service == "rinx.event" {
                let args: Value = serde_json::from_str(&request.args_json).unwrap();
                let mut event: Value = serde_json::from_str(
                    args["route"].as_str().unwrap().strip_prefix("l0:").unwrap(),
                )
                .unwrap();
                if event["v"] == "$$" {
                    event["v"] = args["value"].clone();
                }
                if octoscript_ui_l0::dispatch_with_data(
                    include_str!("miniapps/theme-reference/page.card"),
                    &mut self.state,
                    event["k"].as_str().unwrap(),
                    event["e"].as_str().unwrap(),
                    Some(&event["v"]),
                    &Self::data(),
                ) {
                    self.render_l0(cx);
                }
            } else {
                self.requests += 1;
            }
            splash_host_respond(cx, request.heap_key, request.req_id, Ok("{}".into()));
        }
    }
}
