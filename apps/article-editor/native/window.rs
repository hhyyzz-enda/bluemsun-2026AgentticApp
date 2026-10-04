//! Hosts the article editor in its own native window on desktop, as Moments is.
//!
//! Makepad creates a window's OS window as soon as its `Window` widget is built,
//! and has no way to re-show a window once hidden. So this host lives directly
//! under the app's `Root` (which forwards every event, including draws, to it),
//! builds the `ArticleWindow` widget when the editor is first opened,
//! and drops it once the user or the panel closes that window.
use makepad_widgets::*;
use super::ui::{ArticleAction, ArticlePanelWidgetRefExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.ArticleWindow = Window {
        window.inner_size: vec2(1200, 800)
        window.title: #(crate::i18n::tr("Article editor"))
        pass.clear_color: #FFFFFF00
        caption_bar +: {
            draw_bg.color: mod.widgets.RINX_PAGE
            caption_label +: {
                label +: {
                    draw_text +: { color: mod.widgets.RINX_INK }
                    text: #(crate::i18n::tr("Article editor"))
                }
            }
        }
        body +: {
            article_panel := ArticlePanel {
                // This window's own caption bar already sits above the panel.
                padding: Inset{top: 0 bottom: 0}
            }
        }
    }

    mod.widgets.ArticleWindowHost = #(ArticleWindowHost::register_widget(vm)) {}
}

#[derive(Script, WidgetRef, WidgetRegister)]
pub struct ArticleWindowHost {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[rust] area: Area,
    /// The editor window, while it is open.
    #[rust] window: Option<WidgetRef>,
    /// Set once we've asked the OS to close `window`, which is dropped on `WindowClosed`.
    #[rust] closing: bool,
}

impl ScriptHook for ArticleWindowHost {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _value: ScriptValue,
    ) {
        if apply.is_script_reapply() {
            if let Some(window) = &mut self.window {
                let value = script_eval!(vm,{mod.widgets.ArticleWindow {}});
                window.script_apply(vm, apply, scope, value);
            }
        }
    }
}

impl WidgetNode for ArticleWindowHost {
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
            visit(id!(article_window), window.clone());
        }
    }
}

impl Widget for ArticleWindowHost {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(window) = self.window.clone() else { return };
        let window_id = window.as_window().window_id();
        let closed = matches!(event, Event::WindowClosed(e) if Some(e.window_id) == window_id);
        // `Window` records its own geometry as the app-wide display context, which
        // drives the main window's desktop/mobile layout choice. Resizing the editor
        // window must not flip the main window's layout, so undo that.
        let saved_display = matches!(event, Event::WindowGeomChange(e) if Some(e.window_id) == window_id)
            .then(|| {
                let dc = &cx.display_context;
                (dc.screen_size, dc.safe_area_insets, dc.updated_on_event_id)
            });
        window.handle_event(cx, event, scope);
        if let Some((screen_size, safe_area_insets, updated_on_event_id)) = saved_display {
            cx.display_context.screen_size = screen_size;
            cx.display_context.safe_area_insets = safe_area_insets;
            cx.display_context.updated_on_event_id = updated_on_event_id;
        }
        if closed {
            // Saves the draft and revokes the app's permission, as closing the modal does.
            window.article_panel(cx, ids!(article_panel)).action(cx, ModalRef::default(), &ArticleAction::Close);
            self.window = None;
            self.closing = false;
            cx.widget_tree_mark_dirty(self.uid);
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

impl ArticleWindowHost {
    /// Routes an article action to the window, creating the window if needed.
    pub fn action(&mut self, cx: &mut Cx, action: &ArticleAction) {
        if matches!(action, ArticleAction::Close) {
            self.close(cx);
            return;
        }
        if self.window.is_none() || self.closing {
            let window = cx.with_vm(|vm| {
                let template = vm.eval(script! { mod.widgets.ArticleWindow });
                WidgetRef::script_from_value(vm, template)
            });
            self.window = Some(window);
            self.closing = false;
            cx.widget_tree_mark_dirty(self.uid);
        }
        let window = self.window.clone().unwrap();
        window.article_panel(cx, ids!(article_panel)).action(cx, ModalRef::default(), action);
        window.redraw(cx);
    }

    /// Closes the editor window, if open.
    pub fn close(&mut self, cx: &mut Cx) {
        let Some(window) = self.window.as_ref() else { return };
        window.article_panel(cx, ids!(article_panel)).action(cx, ModalRef::default(), &ArticleAction::Close);
        if !self.closing {
            if let Some(window_id) = window.as_window().window_id() {
                cx.push_unique_platform_op(CxOsOp::CloseWindow(window_id));
            }
            self.closing = true;
        }
    }
}

impl ArticleWindowHostRef {
    pub fn action(&self, cx: &mut Cx, action: &ArticleAction) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.action(cx, action);
        }
    }
    pub fn close(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.close(cx);
        }
    }
}
