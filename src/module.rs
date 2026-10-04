//! Rinx as an OctoSense app module: the same client, seated in the host's pane.
//!
//! OctoSense links trusted native apps as `AppModule`s. The host owns the
//! window, safe area and theme; Rinx supplies its window-less `RinxContent`.
//! Matrix state is process-wide, so one instance runs at a time.
use makepad_app_module::{
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, ModuleWindows, OpenSchema, ReplySink, ServiceExecutor,
    ValidatedOpen,
    makepad_ai_services::wire::{Risk, ServiceCall, ServiceManifest, ToolDef, ToolResult},
};
use makepad_widgets::*;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTANCE_ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The running instance's extra host windows (UI thread only).
    static WINDOWS: std::cell::RefCell<Option<ModuleWindows>> = const { std::cell::RefCell::new(None) };
}

/// Whether the host shows extra windows (a desktop, not a phone).
pub fn windows_supported() -> bool {
    WINDOWS.with(|w| w.borrow().as_ref().is_some_and(|w| w.is_supported()))
}

/// Shows `root` in the host window `key` (opening or focusing it).
pub fn open_window(key: LiveId, title: &str, root: WidgetRef) {
    WINDOWS.with(|w| if let Some(w) = w.borrow().as_ref() { w.open(key, title, root, None) });
    // The host takes requests on its UI signal.
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// Closes the host window `key`.
pub fn close_window(key: LiveId) {
    WINDOWS.with(|w| if let Some(w) = w.borrow().as_ref() { w.close(key) });
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// Host windows the person closed since the last call.
pub fn take_closed_windows() -> Vec<LiveId> {
    WINDOWS.with(|w| w.borrow().as_ref().map(|w| w.take_closed()).unwrap_or_default())
}

/// Whether Rinx is running inside an OctoSense host.
pub fn is_hosted() -> bool { INSTANCE_ACTIVE.load(Ordering::Acquire) }

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.RinxModuleView = set_type_default() do #(RinxModuleView::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay
    }
}

#[derive(Script, Widget)]
pub struct RinxModuleView {
    #[deref] view: View,
    #[rust] app: Option<crate::app::App>,
}

impl ScriptHook for RinxModuleView {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        if apply.is_script_reapply() {
            if let Some(app) = self.app.as_mut() { app.reapply_embedded(vm); }
        }
    }
}

impl RinxModuleView {
    fn close(&mut self, cx: &mut Cx) {
        if let Some(mut app) = self.app.take() {
            crate::assistant::uninstall(cx);
            app.close_embedded(cx);
            WINDOWS.with(|w| w.borrow_mut().take());
            self.view.children.clear();
            INSTANCE_ACTIVE.store(false, Ordering::Release);
        }
    }
}

impl Widget for RinxModuleView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if matches!(event, Event::Shutdown) { self.close(cx); return; }
        if let Some(app) = self.app.as_mut() {
            AppMain::handle_event(app, cx, event);
        } else {
            self.view.handle_event(cx, event, scope);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        match self.app.as_mut() {
            Some(app) => app.draw_embedded(cx, &mut self.view, walk),
            None => self.view.draw_walk(cx, scope, walk),
        }
    }
}

pub struct RinxModule;
pub static RINX_MODULE: RinxModule = RinxModule;

impl AppModule for RinxModule {
    fn id(&self) -> &'static str { "rinx" }
    fn label(&self) -> &'static str { "Rinx" }
    /// The host reads Rinx's assistant needs from here (ADR 0007): the exact
    /// App Hub `octos.*` service names its mini-app host serves. Declaring
    /// them grants nothing; the host intersects them with its policy.
    fn capabilities(&self) -> &'static [&'static str] {
        &[
            "net", "storage", "audio.output", "clipboard",
            "octos.session.open", "octos.session.history", "octos.turn.start", "octos.turn.interrupt",
        ]
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }

    fn register(&self, vm: &mut ScriptVm) {
        crate::theme::init_hosted(vm);
        crate::app::register_widgets(vm);
        script_mod(vm);
    }

    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let owns_runtime = INSTANCE_ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok();
        // Hosted mode comes from the shell creating this module: take the
        // scoped assistant service it offered to THIS instance, if any. No
        // fallback kernel, no credentials, no AppCard.
        let assistant = octosense_app_peers::injection::claim(self.id(), &handles.scope.to_string());
        if owns_runtime {
            crate::octos_service::install_hosted(assistant);
        } else if let Some(service) = assistant {
            service.release();
        }
        let value = script_eval!(vm, { mod.widgets.RinxModuleView {} });
        let root = WidgetRef::script_from_value(vm, value);
        if owns_runtime {
            // Later answers (a sheet the person answers, a Matrix read) go up
            // this instance's reply sink.
            crate::assistant::install(reply_sink(handles.replies.clone()));
        }
        if let Some(mut view) = root.borrow_mut::<RinxModuleView>() {
            if owns_runtime {
                WINDOWS.with(|w| *w.borrow_mut() = Some(handles.windows.clone()));
                let app = crate::app::App::create_embedded(vm);
                let content = app.content();
                view.view.children.push((live_id!(content), content.clone()));
                vm.cx_mut().widget_tree_insert_child_deep(view.widget_uid(), live_id!(content), content);
                vm.cx_mut().widget_tree_mark_dirty(view.widget_uid());
                view.app = Some(app);
            } else {
                let message = script_eval!(vm, {
                    use mod.prelude.widgets.*
                    Label { width: Fill draw_text.wrap: Words text: "Rinx is already open." }
                });
                view.view.children.push((live_id!(already_open), WidgetRef::script_from_value(vm, message)));
            }
        }
        let cleanup = root.clone();
        InstanceParts {
            root,
            executor: Box::new(RinxExecutor { live: owns_runtime }),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = cleanup.borrow_mut::<RinxModuleView>() { view.close(vm.cx_mut()); }
            }),
        }
    }
}

/// The assistant's tools (`crate::assistant`) on the host's AI bus. Only the
/// instance that owns the Matrix runtime serves them; a second instance
/// ("Rinx is already open") answers `Unavailable`.
struct RinxExecutor {
    live: bool,
}

impl ServiceExecutor for RinxExecutor {
    fn manifest(&self) -> ServiceManifest {
        manifest()
    }
    fn execute(&mut self, cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        if !self.live {
            return ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Rinx is already open in another window."));
        }
        match crate::assistant::execute(cx, &call.call_id, &call.tool, &call.args) {
            crate::assistant::Exec::Done(reply) => ExecOutcome::Done(tool_result(reply)),
            crate::assistant::Exec::Pending => ExecOutcome::Pending,
        }
    }
    fn cancel(&mut self, cx: &mut Cx, call_id: &str) {
        if self.live {
            crate::assistant::cancel(cx, call_id);
        }
    }
}

/// The manifest the host registers: `crate::assistant::TOOLS`, with the send
/// tool confirmed by Rinx's own sheet so the chat pane does not ask again.
pub fn manifest() -> ServiceManifest {
    let mut manifest = ServiceManifest::new("rinx", "Rinx", crate::assistant::BRIEF);
    for tool in crate::assistant::TOOLS {
        let risk = match tool.risk {
            crate::assistant::Risk::Read => Risk::Read,
            crate::assistant::Risk::Act => Risk::Act,
            crate::assistant::Risk::Destructive => Risk::Destructive,
        };
        let mut def = ToolDef::new(tool.name, tool.description, tool.parameters, risk);
        if tool.confirms_itself {
            def = def.confirmed_by_app();
        }
        manifest = manifest.with_tool(def);
    }
    manifest
}

fn tool_result(reply: crate::assistant::Reply) -> ToolResult {
    use crate::assistant::Outcome;
    let note: String = reply.text.chars().take(120).collect();
    let mut result = match reply.outcome {
        Outcome::Ok => ToolResult::ok(&reply.call_id, reply.text, note),
        Outcome::Failed => ToolResult::failed(&reply.call_id, reply.text),
        Outcome::Refused => ToolResult::refused(&reply.call_id, reply.text),
        Outcome::Denied => ToolResult::denied(&reply.call_id, reply.text),
        Outcome::Unavailable => ToolResult::unavailable(&reply.call_id, reply.text),
    };
    if !reply.data.is_empty() {
        result = result.with_data(reply.data);
    }
    result.bound();
    result
}

fn reply_sink(replies: ReplySink) -> crate::assistant::Sink {
    let replies = std::sync::Mutex::new(replies);
    std::sync::Arc::new(move |reply| replies.lock().unwrap().reply(tool_result(reply)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manifest_validates_and_only_send_confirms_itself() {
        let manifest = super::manifest();
        manifest.validate().expect("a valid manifest");
        assert_eq!(manifest.tools.len(), crate::assistant::TOOLS.len());
        for tool in &manifest.tools {
            assert_eq!(tool.confirms_itself(), tool.name == "send_message", "{}", tool.name);
        }
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    use crate::theme::{self, Accent, Appearance, Selection};

    #[test]
    fn theme_reload_reaches_dynamically_owned_rinx_content() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut wrapper = cx.with_vm(|vm| {
            theme::tests::install(vm, Selection::default());
            RinxModule.register(vm);
            let value = script_eval!(vm, {mod.widgets.RinxModuleView {}});
            let mut wrapper = RinxModuleView::script_from_value(vm,value);
            // Construct actual Rinx content without Startup: no account restore,
            // networking, kernel or production profile access in this test.
            let component = crate::app::App::script_component(vm);
            let value = script_eval!(vm, {#(component) {ui: mod.widgets.RinxContent {}}});
            let app = crate::app::App::script_from_value(vm,value);
            wrapper.view.children.push((id!(content),app.content()));
            wrapper.app = Some(app);
            wrapper
        });
        let content = wrapper.app.as_ref().unwrap().content();
        let uid = content.widget_uid();
        let title = content.label(&cx, ids!(octoscript_apps_modal.content.catalog_title));
        assert!(!title.is_empty());
        let original_ink = title.borrow().unwrap().draw_text.color;
        cx.with_vm(|vm| vm.with_reload(|vm| {
            theme::tests::install(vm,Selection {appearance: Appearance::Dark,accent: Accent::Violet});
            RinxModule.register(vm);
            let value = script_eval!(vm, {mod.widgets.RinxModuleView {}});
            wrapper.script_apply(vm,&Apply::ScriptReapply,&mut Scope::empty(),value);
        }));
        assert_eq!(wrapper.app.as_ref().unwrap().content().widget_uid(),uid);
        assert_ne!(title.borrow().unwrap().draw_text.color,original_ink);
        assert_eq!(title.borrow().unwrap().draw_text.color,theme::snapshot(&mut cx).ink);
        assert!(theme::selection(&mut cx).is_none());
    }
}
