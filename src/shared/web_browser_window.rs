//! Hosts the web reader in its own native window on desktop.
//!
//! Makepad creates a window's OS window as soon as its `Window` widget is built,
//! and has no way to re-show a window once hidden. So this host lives directly
//! under the app's `Root` (which forwards every event, including draws, to it),
//! builds the `WebBrowserWindow` widget when the web reader is first opened,
//! and drops it once the user or the panel closes that window.
use makepad_widgets::*;
use super::web_browser::{WebBrowserAction, WebBrowserWidgetRefExt, WebBrowserRef};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.WebBrowserWindow = Window {
        window.inner_size: vec2(1040, 800)
        window.title: "Rinx · Web"
        pass.clear_color: #FFFFFF00
        caption_bar +: {
            draw_bg.color: mod.widgets.RINX_PAGE
            caption_label +: {
                label +: {
                    draw_text +: { color: mod.widgets.RINX_INK }
                    text: "Rinx · Web"
                }
            }
        }
        body +: {
            web_browser := WebBrowser {
                // This window's own caption bar already sits above the panel.
                padding: Inset{top: 0 bottom: 0}
            }
        }
    }

    mod.widgets.WebBrowserWindowHost = #(WebBrowserWindowHost::register_widget(vm)) {}
}

#[derive(Script, WidgetRef, WidgetRegister)]
pub struct WebBrowserWindowHost {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[rust]
    area: Area,
    /// The web reader window, while it is open.
    #[rust]
    window: Option<WidgetRef>,
    /// Set once we've asked the OS to close `window`, which is dropped on `WindowClosed`.
    #[rust]
    closing: bool,
    #[rust]
    focused: bool,
}

impl ScriptHook for WebBrowserWindowHost {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _value: ScriptValue,
    ) {
        if apply.is_script_reapply() {
            if let Some(window) = &mut self.window {
                let value = script_eval!(vm, { mod.widgets.WebBrowserWindow {} });
                window.script_apply(vm, apply, scope, value);
            }
        }
    }
}

impl WidgetNode for WebBrowserWindowHost {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }
    fn area(&self) -> Area {
        self.area
    }
    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        Walk::default()
    }
    fn redraw(&mut self, cx: &mut Cx) {
        if let Some(window) = self.window.as_ref() {
            window.redraw(cx);
        }
    }
    fn children(&self, visit: &mut dyn FnMut(LiveId, WidgetRef)) {
        if let Some(window) = self.window.as_ref() {
            visit(id!(web_browser_window), window.clone());
        }
    }
}

impl Widget for WebBrowserWindowHost {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let window_id = window.as_window().window_id();
        match event {
            Event::WindowGotFocus(id) if Some(*id) == window_id => self.focused = true,
            Event::WindowLostFocus(id) if Some(*id) == window_id => self.focused = false,
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::TextInput(_)
            | Event::BackPressed { .. }
                if !self.focused =>
            {
                return;
            }
            _ => {}
        }
        let closed = matches!(event, Event::WindowClosed(e) if Some(e.window_id) == window_id);
        // `Window` records its own geometry as the app-wide display context, which
        // drives the main window's desktop/mobile layout choice. This small window
        // must not flip the main window into its mobile layout, so undo that.
        let saved_display =
            matches!(event, Event::WindowGeomChange(e) if Some(e.window_id) == window_id).then(
                || {
                    let dc = &cx.display_context;
                    (dc.screen_size, dc.safe_area_insets, dc.updated_on_event_id)
                },
            );
        window.handle_event(cx, event, scope);
        if let Some((screen_size, safe_area_insets, updated_on_event_id)) = saved_display {
            cx.display_context.screen_size = screen_size;
            cx.display_context.safe_area_insets = safe_area_insets;
            cx.display_context.updated_on_event_id = updated_on_event_id;
        }
        if closed {
            window.web_browser(cx, ids!(web_browser)).action(
                cx,
                ModalRef::default(),
                &WebBrowserAction::Close,
            );
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

impl WebBrowserWindowHost {
    /// Routes a web reader action to the window, creating the window if needed.
    pub fn action(&mut self, cx: &mut Cx, action: &WebBrowserAction) {
        if matches!(action, WebBrowserAction::Close) {
            self.close(cx);
            return;
        }
        if self.window.is_none() || self.closing {
            let window = cx.with_vm(|vm| {
                let template = vm.eval(script! { mod.widgets.WebBrowserWindow });
                WidgetRef::script_from_value(vm, template)
            });
            self.window = Some(window);
            self.closing = false;
            self.focused = false;
            cx.widget_tree_mark_dirty(self.uid);
        } else {
            // Reusing the reader should bring it back in front of the chat.
            #[cfg(target_os = "macos")]
            if let Some(window_id) = self
                .window
                .as_ref()
                .and_then(|window| window.as_window().window_id())
            {
                // AppKit can synchronously re-enter Makepad's event callback.
                // Activate after this widget and Cx have released their borrows.
                dispatch2::DispatchQueue::main().exec_async(move || {
                    use makepad_widgets::makepad_platform::os::apple::macos::macos_app::{
                        activate_cocoa_window_on_pointer_down, try_with_macos_app,
                    };
                    let native =
                        try_with_macos_app(|app| app.cocoa_window_for_id(window_id)).flatten();
                    if let Some(native) = native {
                        activate_cocoa_window_on_pointer_down(native);
                    }
                });
            }
        }
        let window = self.window.clone().unwrap();
        window
            .web_browser(cx, ids!(web_browser))
            .action(cx, ModalRef::default(), action);
        window.redraw(cx);
    }

    /// Closes the web reader window, if open.
    pub fn close(&mut self, cx: &mut Cx) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        window.web_browser(cx, ids!(web_browser)).action(
            cx,
            ModalRef::default(),
            &WebBrowserAction::Close,
        );
        if !self.closing {
            if let Some(window_id) = window.as_window().window_id() {
                cx.push_unique_platform_op(CxOsOp::CloseWindow(window_id));
            }
            self.closing = true;
        }
    }
}

impl WebBrowserWindowHostRef {
    pub fn browser(&self, cx: &mut Cx) -> WebBrowserRef {
        self.borrow()
            .and_then(|inner| {
                inner
                    .window
                    .as_ref()
                    .map(|window| window.web_browser(cx, ids!(web_browser)))
            })
            .unwrap_or_default()
    }

    pub fn window_id(&self) -> Option<WindowId> {
        self.borrow().and_then(|inner| {
            inner
                .window
                .as_ref()
                .and_then(|window| window.as_window().window_id())
        })
    }

    pub fn action(&self, cx: &mut Cx, action: &WebBrowserAction) {
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
