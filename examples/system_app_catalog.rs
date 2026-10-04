//! Offline native catalog harness. --hosted exercises an injected-host deployment.
//! No Matrix login, account restore, or kernel is started by this example.
pub use makepad_widgets;
use makepad_widgets::*;
use rinx::miniapps::{MiniAppsAction, MiniAppsPanelWidgetRefExt};
use rinx::article_app::{ArticleAction, ArticlePanelWidgetRefExt};
app_main!(App);
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let app = startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(430, 820)
                body +: {flow: Overlay
                    miniapps := MiniAppsPanel {}
                    article := Modal {can_dismiss: false content := ArticlePanel {}}
                }
            }
        }
    }
    app
}
#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        if std::env::args().any(|a| a == "--hosted") {
            rinx::octos_service::install_hosted(None);
        }
        self.ui.mini_apps_panel(cx, ids!(miniapps)).action(
            cx,
            ModalRef::default(),
            &MiniAppsAction::Open,
        );
    }
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for action in actions {
            if let Some(MiniAppsAction::Close) = action.downcast_ref::<MiniAppsAction>() {
                self.ui.mini_apps_panel(cx, ids!(miniapps)).action(
                    cx,
                    ModalRef::default(),
                    &MiniAppsAction::Close,
                );
            }
            if let Some(action) = action.downcast_ref::<ArticleAction>() {
                let modal = self.ui.modal(cx, ids!(article));
                self.ui
                    .article_panel(cx, ids!(article.content))
                    .action(cx, modal, action);
                self.ui
                    .view(cx, ids!(miniapps))
                    .set_visible(cx, matches!(action, ArticleAction::Close));
                if matches!(action, ArticleAction::Close) {
                    self.ui.mini_apps_panel(cx, ids!(miniapps)).action(
                        cx,
                        ModalRef::default(),
                        &MiniAppsAction::Open,
                    );
                }
            }
        }
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        if std::env::args().any(|a| a == "--hosted") {
            rinx::theme::init_hosted(vm);
        } else {
            rinx::theme::init_standalone(vm);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        makepad_widgets::desktop_style::apply_widgets(vm);
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
