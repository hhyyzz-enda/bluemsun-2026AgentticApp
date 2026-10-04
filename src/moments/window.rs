//! Hosts Moments in its own native window on desktop.
//!
//! Makepad creates a window's OS window as soon as its `Window` widget is built,
//! and has no way to re-show a window once hidden. So this host lives directly
//! under the app's `Root` (which forwards every event, including draws, to it),
//! builds the `MomentsWindow` widget when Moments is first opened,
//! and drops it once the user or the panel closes that window.
use makepad_widgets::*;
use super::ui::{MomentsAction, MomentsPanelWidgetRefExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.MomentsWindow = Window {
        window.inner_size: vec2(480, 760)
        window.title: "Moments"
        pass.clear_color: #FFFFFF00
        caption_bar +: {
            draw_bg.color: mod.widgets.RINX_PAGE
            caption_label +: {
                label +: {
                    draw_text +: { color: mod.widgets.RINX_INK }
                    text: "Moments"
                }
            }
        }
        body +: {
            moments_panel := MomentsPanel {
                // This window's own caption bar already sits above the panel.
                padding: Inset{top: 0 bottom: 0}
            }
        }
    }

    mod.widgets.MomentsWindowHost = #(MomentsWindowHost::register_widget(vm)) {}
}

#[derive(Script, WidgetRef, WidgetRegister)]
pub struct MomentsWindowHost {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[rust] area: Area,
    /// The Moments window, while it is open.
    #[rust] window: Option<WidgetRef>,
    /// Set once we've asked the OS to close `window`, which is dropped on `WindowClosed`.
    #[rust] closing: bool,
}

impl ScriptHook for MomentsWindowHost {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _value: ScriptValue,
    ) {
        if apply.is_script_reapply() {
            if let Some(window) = &mut self.window {
                let value = script_eval!(vm, { mod.widgets.MomentsWindow {} });
                window.script_apply(vm, apply, scope, value);
            }
        }
    }
}

impl WidgetNode for MomentsWindowHost {
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
            visit(id!(moments_window), window.clone());
        }
    }
}

impl Widget for MomentsWindowHost {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(window) = self.window.clone() else { return };
        let window_id = window.as_window().window_id();
        let closed = matches!(event, Event::WindowClosed(e) if Some(e.window_id) == window_id);
        // `Window` records its own geometry as the app-wide display context, which
        // drives the main window's desktop/mobile layout choice. This small window
        // must not flip the main window into its mobile layout, so undo that.
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
            window.moments_panel(cx, ids!(moments_panel)).action(cx, None, &MomentsAction::Close);
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

impl MomentsWindowHost {
    /// Routes a Moments action to the window, creating the window if needed.
    pub fn action(&mut self, cx: &mut Cx, action: &MomentsAction) {
        if matches!(action, MomentsAction::Close) {
            self.close(cx);
            return;
        }
        if self.window.is_none() || self.closing {
            let window = cx.with_vm(|vm| {
                let template = vm.eval(script! { mod.widgets.MomentsWindow });
                WidgetRef::script_from_value(vm, template)
            });
            self.window = Some(window);
            self.closing = false;
            cx.widget_tree_mark_dirty(self.uid);
        }
        let window = self.window.clone().unwrap();
        window.moments_panel(cx, ids!(moments_panel)).action(cx, None, action);
        window.redraw(cx);
    }

    /// Closes the Moments window, if open.
    pub fn close(&mut self, cx: &mut Cx) {
        let Some(window) = self.window.as_ref() else { return };
        window.moments_panel(cx, ids!(moments_panel)).action(cx, None, &MomentsAction::Close);
        if !self.closing {
            if let Some(window_id) = window.as_window().window_id() {
                cx.push_unique_platform_op(CxOsOp::CloseWindow(window_id));
            }
            self.closing = true;
        }
    }
}

impl MomentsWindowHostRef {
    pub fn action(&self, cx: &mut Cx, action: &MomentsAction) {
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
