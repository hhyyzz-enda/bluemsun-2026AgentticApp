//! Hosts the writing studio's review card in its own native window on
//! desktop, exactly as the article editor is hosted.
//!
//! The window holds a second `WritingPanel` face over the same shared
//! `model::Studio` store: whichever face edits the proposal, applies,
//! discards or undoes, the other face's sync poll rebinds within a fraction
//! of a second. Leaving and returning finds the same draft and the same
//! decision state, because neither face owns the state — the store does.
use makepad_widgets::*;
use super::ui::{WritingAction, WritingPanelWidgetRefExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.WritingWindow = Window {
        window.inner_size: vec2(760, 860)
        window.title: #(crate::i18n::tr("Writing studio"))
        pass.clear_color: #FFFFFF00
        caption_bar +: {
            draw_bg.color: #xffffff
            caption_label +: {
                label +: {
                    draw_text +: { color: #0 }
                    text: #(crate::i18n::tr("Writing studio"))
                }
            }
        }
        body +: {
            writing_card_panel := WritingPanel {
                // This window's own caption bar already sits above the panel.
                padding: Inset{top: 0 bottom: 0}
            }
        }
    }

    mod.widgets.WritingWindowHost = #(WritingWindowHost::register_widget(vm)) {}
}

#[derive(Script, WidgetRef, WidgetRegister)]
pub struct WritingWindowHost {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[rust] area: Area,
    /// The card window, while it is open.
    #[rust] window: Option<WidgetRef>,
    /// Set once we've asked the OS to close `window`, which is dropped on `WindowClosed`.
    #[rust] closing: bool,
}

impl ScriptHook for WritingWindowHost {}

impl WidgetNode for WritingWindowHost {
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
            visit(id!(writing_window), window.clone());
        }
    }
}

impl Widget for WritingWindowHost {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(window) = self.window.clone() else { return };
        let window_id = window.as_window().window_id();
        let closed = matches!(event, Event::WindowClosed(e) if Some(e.window_id) == window_id);
        // `Window` records its own geometry as the app-wide display context,
        // which drives the main window's desktop/mobile layout choice.
        // Resizing the card window must not flip the main window's layout.
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
            window.writing_panel(cx, ids!(writing_card_panel)).action(cx, ModalRef::default(), &WritingAction::Close);
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

impl WritingWindowHost {
    /// Routes a writing action to the window, creating the window if needed.
    pub fn action(&mut self, cx: &mut Cx, action: &WritingAction) {
        if matches!(action, WritingAction::Close) {
            self.close(cx);
            return;
        }
        if self.window.is_none() || self.closing {
            let window = cx.with_vm(|vm| {
                let template = vm.eval(script! { mod.widgets.WritingWindow });
                WidgetRef::script_from_value(vm, template)
            });
            self.window = Some(window);
            self.closing = false;
            cx.widget_tree_mark_dirty(self.uid);
        }
        let window = self.window.clone().unwrap();
        window.writing_panel(cx, ids!(writing_card_panel)).action(cx, ModalRef::default(), action);
        window.redraw(cx);
    }

    /// Closes the card window, if open.
    pub fn close(&mut self, cx: &mut Cx) {
        let Some(window) = self.window.as_ref() else { return };
        window.writing_panel(cx, ids!(writing_card_panel)).action(cx, ModalRef::default(), &WritingAction::Close);
        if !self.closing {
            if let Some(window_id) = window.as_window().window_id() {
                cx.push_unique_platform_op(CxOsOp::CloseWindow(window_id));
            }
            self.closing = true;
        }
    }
}

impl WritingWindowHostRef {
    pub fn action(&self, cx: &mut Cx, action: &WritingAction) {
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
