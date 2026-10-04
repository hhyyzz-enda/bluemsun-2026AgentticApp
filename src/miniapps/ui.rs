//! One native mini-app screen shared by standalone and embedded Rinx.
use super::library::{LibraryAction, MiniAppLibraryWidgetExt};
use super::{
    ContextProvider, InstanceId, Lease, OctosProvider, ServiceEvent,
    package::{Call, Package},
};
use makepad_widgets::splash_host::{splash_host_respond, take_splash_host_requests_for};
use makepad_widgets::*;
use octoscript_ui_l0::InstanceStore;
use octosense_app_contract::AssetServer;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub enum MiniAppsAction {
    Open,
    /// The assistant opened the reviewed app: show its review; Run grants it.
    OpenReviewed,
    /// Untrusted notification routing hint; backend authorization remains mandatory.
    OpenPalpo(String),
    Close,
}
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.MiniAppsPanel = #(MiniAppsPanel::register_widget(vm)) {
        width: Fill height: Fill flow: Down padding: 20 spacing: 16
        show_bg: true
        draw_bg +: {color: instance(mod.widgets.RINX_PAGE) pixel: fn() {return self.color}}
        header := View {width: Fill height: Fit flow: Right spacing: 12
            close := RinxButton {width: 44 text: "" icon_walk: Walk{width: 20 height: 20} draw_icon.svg: ICON_ARROW_BACK}
            RinxPageTitle {text: "Mini apps"}
        }
        library := MiniAppLibrary {visible: false}
        catalog := View {width: Fill height: Fill flow: Down spacing: 12
            catalog_title := RinxLabel {text: #(crate::i18n::tr("Built-in apps")) i18n_text: "Built-in apps"}
            catalog_list := PortalList {width: Fill height: Fill
                App := RoundedView {width: Fill height: 104 padding: 16 spacing: 12 flow: Right align: Align{y: 0.5}
                    draw_bg +: {color: RINX_SURFACE border_color: RINX_BORDER border_size: 1 border_radius: theme.corner_radius}
                    Icon {width: 28 height: 28 draw_icon +: {svg: ICON_EDIT color: RINX_ACCENT}}
                    copy := View {width: Fill height: Fit flow: Down spacing: 6
                        name := RinxLabel {width: Fill draw_text.text_style: theme.font_bold{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
                        subtitle := RinxHint {width: Fill text: ""}
                    }
                    launch := RinxPrimaryButton {text: #(crate::i18n::tr("Open")) i18n_text: "Open"}
                }
            }
            browse_hub := RinxButton {width: Fill text: "App Hub"}
            import_app := RinxButton {width: Fill text: #(crate::i18n::tr("Import an app")) i18n_text: "Import an app"}
        }
        import_form := View {visible: false width: Fill height: Fit flow: Down spacing: 8
            bundle_path := View {width: Fill height: Fit
                path := TextInput {width: Fill empty_text: "OctoSense bundle folder"}
            }
            room := TextInput {width: Fill empty_text: "Room ID to allow (optional)"}
            assistant_status := Label {width: Fill draw_text.color: mod.widgets.RINX_MUTED text: ""}
            core := View {width: Fill height: Fit flow: Down spacing: 6
                local_family := TextInput {width: Fill empty_text: "Assistant on this device: provider (e.g. deepseek)"}
                local_model := TextInput {width: Fill empty_text: "Model"}
                local_base_url := TextInput {width: Fill empty_text: "Base URL (optional)"}
                local_key := TextInput {width: Fill is_password: true empty_text: "API key (kept in Rinx's own runtime)"}
                local_buttons := View {width: Fill height: Fit spacing: 8
                    use_local := RinxButton {text: "Use this device"}
                    turn_off := RinxButton {text: "Turn assistant off"}
                }
                endpoint := TextInput {width: Fill empty_text: "Octos server URL"}
                profile := TextInput {width: Fill empty_text: "Octos profile"}
                token := TextInput {width: Fill is_password: true empty_text: "Octos access token"}
                connect := RinxButton {text: "Connect Octos"}
            }
            buttons := View {width: Fill height: Fit spacing: 8
                review := RinxButton {text: "Review bundle"}
                run := RinxButton {text: "Run"}
            }
        }
        notice := Label {width: Fill height: Fit draw_text.color: mod.widgets.RINX_INK text: ""}
        approval := View {visible: false width: Fill height: Fit flow: Down spacing: 8
            details := Label {width: Fill draw_text.color: mod.widgets.RINX_INK}
            buttons := View {width: Fill height: Fit spacing: 8
                allow := RinxButton {text: "Allow once"}
                deny := RinxButton {text: "Deny"}
            }
        }
        app_content := View {visible: false width: Fill height: Fill
            card := Splash {width: Fill height: Fill}
        }
    }
}
type Provider = Arc<dyn OctosProvider>;
struct Pending {
    reply: Option<(usize, u64)>,
    target: Option<String>,
    receiver: Receiver<ServiceEvent>,
    started: Instant,
    turn: bool,
}
struct Approval {
    id: String,
    message: String,
}
#[derive(Script, Widget)]
pub struct MiniAppsPanel {
    #[deref]
    view: View,
    #[rust]
    open: bool,
    #[rust]
    showing_catalog: bool,
    #[rust]
    showing_hub: bool,
    #[rust]
    return_to_hub: bool,
    #[rust]
    reviewed_room: String,
    #[rust]
    review_notice: String,
    #[rust]
    package: Option<Package>,
    #[rust]
    lease: Option<Lease>,
    #[rust]
    provider: Option<Provider>,
    #[rust]
    palpo: Option<super::palpo::PalpoHost>,
    #[rust]
    palpo_action: Option<String>,
    #[rust]
    octos_unavailable: Option<String>,
    #[rust]
    tag: String,
    #[rust]
    state: InstanceStore,
    #[rust]
    data: Value,
    #[rust]
    pending: Vec<Pending>,
    #[rust]
    approvals: VecDeque<Approval>,
    #[rust]
    assets: Option<AssetServer>,
    #[rust]
    theme_revision: u64,
    #[rust]
    restyle_card: bool,
    #[rust]
    notice_text: String,
}
impl ScriptHook for MiniAppsPanel {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, _apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        let revision = crate::theme::snapshot_for_vm(vm).revision;
        self.restyle_card |= self.theme_revision != revision && self.lease.is_some()
            && self.package.as_ref().is_some_and(|p| !p.script);
        self.theme_revision = revision;
        if self.open {
            let cx = vm.cx_mut();
            self.view.widget(cx, ids!(library)).set_visible(cx, self.showing_hub);
            self.view.view(cx, ids!(catalog)).set_visible(cx, self.showing_catalog);
            self.view.view(cx, ids!(header)).set_visible(cx, !self.showing_hub);
            let app = !self.showing_hub && !self.showing_catalog;
            self.view.view(cx, ids!(app_content)).set_visible(cx, app);
            self.view.view(cx, ids!(import_form)).set_visible(cx, app && self.lease.is_none());
            self.view.label(cx, ids!(notice)).set_text(cx, &self.notice_text);
            self.view.view(cx, ids!(import_form.core)).set_visible(cx, !crate::octos_service::is_hosted() && self.package.as_ref().and_then(Package::builtin_id) != Some(super::palpo::APP_ID));
            let imported = self.package.as_ref().and_then(Package::builtin_id).is_none();
            self.view.view(cx, ids!(bundle_path)).set_visible(cx, imported);
            self.show_approval(cx);
            self.show_assistant_status(cx);
        }
    }
}
impl Drop for MiniAppsPanel {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.take() {
            lease.revoke();
            if let Some(provider) = self.provider.take() {
                provider.close(lease.identity());
            }
        }
    }
}
impl MiniAppsPanel {
    fn show_catalog(&mut self, cx: &mut Cx) {
        self.stop(cx);
        self.package = None;
        self.palpo_action = None;
        self.review_notice.clear();
        self.reviewed_room.clear();
        self.publish_to_assistant();
        self.showing_catalog = true;
        self.showing_hub = false;
        self.return_to_hub = false;
        self.view.mini_app_library(cx, ids!(library)).cancel_open();
        self.view.widget(cx, ids!(library)).set_visible(cx, false);
        self.view.view(cx, ids!(header)).set_visible(cx, true);
        self.view.view(cx, ids!(catalog)).set_visible(cx, true);
        self.view.view(cx, ids!(import_form)).set_visible(cx, false);
        self.view.view(cx, ids!(app_content)).set_visible(cx, false);
        self.notice(cx, "");
    }
    fn show_hub(&mut self, cx: &mut Cx) {
        self.stop(cx);
        self.package = None;
        self.palpo_action = None;
        self.review_notice.clear();
        self.reviewed_room.clear();
        self.publish_to_assistant();
        self.showing_catalog = false;
        self.showing_hub = true;
        self.return_to_hub = false;
        self.view.view(cx, ids!(catalog)).set_visible(cx, false);
        self.view.view(cx, ids!(import_form)).set_visible(cx, false);
        self.view.view(cx, ids!(app_content)).set_visible(cx, false);
        self.view.view(cx, ids!(header)).set_visible(cx, false);
        self.view.widget(cx, ids!(library)).set_visible(cx, true);
        self.view.mini_app_library(cx, ids!(library)).begin(cx);
        self.notice(cx, "");
    }
    // Returns true only at the outer catalog, where Back closes the modal.
    fn navigate_back(&mut self, cx: &mut Cx) -> bool {
        if self.showing_hub {
            if !self.view.mini_app_library(cx, ids!(library)).back_to_list(cx) {
                self.show_catalog(cx);
            }
        } else if self.return_to_hub {
            self.show_hub(cx);
        } else if self.showing_catalog {
            return true;
        } else {
            self.show_catalog(cx);
        }
        false
    }
    fn show_import(&mut self, cx: &mut Cx) {
        self.showing_hub = false;
        self.view.widget(cx, ids!(library)).set_visible(cx, false);
        self.view.view(cx, ids!(header)).set_visible(cx, true);
        self.showing_catalog = false;
        self.view.view(cx, ids!(catalog)).set_visible(cx, false);
        self.view.view(cx, ids!(import_form)).set_visible(cx, true);
        self.view.view(cx, ids!(app_content)).set_visible(cx, true);
        let imported = self.package.as_ref().and_then(Package::builtin_id).is_none();
        self.view.view(cx, ids!(bundle_path)).set_visible(cx, imported);
        self.show_assistant_status(cx);
        self.view.view(cx, ids!(import_form.core)).set_visible(cx, !crate::octos_service::is_hosted() && self.package.as_ref().and_then(Package::builtin_id) != Some(super::palpo::APP_ID));
    }
    fn open_builtin(&mut self, cx: &mut Cx, index: usize) -> Result<(), String> {
        let app = crate::system_apps::apps().get(index).ok_or("Unknown built-in app")?;
        if app.native.is_some() {
            cx.action(MiniAppsAction::Close);
            crate::system_apps::launch_native(cx, app);
        } else {
            self.stop(cx);
            self.package = None;
            let package = Package::load_builtin(&app.manifest.id, &crate::app_data_dir().join("miniapps/imports"))?;
            self.show_import(cx);
            self.view.text_input(cx, ids!(import_form.room)).set_text(cx, "");
            self.review_package(cx, package)?;
            if app.manifest.id == super::palpo::APP_ID {
                if let Some(account) = crate::sliding_sync::current_user_id()
                    && super::consent::remembered(account.as_str(), &app.manifest.integrity.bundle_blake3) {
                    self.run(cx)?;
                }
            }
        }
        Ok(())
    }

    fn notice(&mut self, cx: &mut Cx, message: &str) {
        self.notice_text = message.to_owned();
        self.view.label(cx, ids!(notice)).set_text(cx, message);
    }
    fn stop(&mut self, cx: &mut Cx) {
        if let Some(lease) = self.lease.take() {
            lease.revoke();
            if let Some(provider) = self.provider.take() {
                provider.close(lease.identity());
            }
        }
        self.palpo = None;
        self.pending.clear();
        self.assets = None;
        self.approvals.clear();
        self.view.view(cx, ids!(approval)).set_visible(cx, false);
        self.view.splash(cx, ids!(card)).set_text(cx, "");
        self.view.view(cx, ids!(import_form)).set_visible(cx, true);
        self.publish_to_assistant();
    }
    /// What the assistant's `status` and `open_mini_app` see of this screen.
    fn publish_to_assistant(&self) {
        let reviewed = self.package.as_ref().map(|package| crate::assistant::ReviewedApp {
            id: package.manifest.id.clone(),
            name: package.manifest.name.clone(),
            room: Some(self.reviewed_room.clone()).filter(|room| !room.is_empty()),
        });
        let running = self.lease.as_ref().and(self.package.as_ref()).map(|p| p.manifest.name.clone());
        crate::assistant::set_mini_apps(reviewed, running);
    }
    fn show_approval(&mut self, cx: &mut Cx) {
        if let Some(approval) = self.approvals.front() {
            self.view
                .label(cx, ids!(approval.details))
                .set_text(cx, &approval.message);
        }
        self.view
            .view(cx, ids!(approval))
            .set_visible(cx, !self.approvals.is_empty());
    }
    fn show_assistant_status(&mut self, cx: &mut Cx) {
        let status = crate::octos_service::status();
        self.view
            .label(cx, ids!(import_form.assistant_status))
            .set_text(cx, &status);
    }
    /// An explicit remote server (standalone only; hosted Rinx uses
    /// OctoSense's AI settings and offers no endpoint or key form).
    #[cfg_attr(not(feature = "octos-remote"), allow(unused_variables))]
    fn connect(&mut self, cx: &mut Cx) -> Result<(), String> {
        if crate::octos_service::is_hosted() {
            return Err("This app uses OctoSense's AI settings".into());
        }
        #[cfg(feature = "octos-remote")]
        {
            let endpoint = self.view.text_input(cx, ids!(import_form.core.endpoint)).text();
            let profile = self.view.text_input(cx, ids!(import_form.core.profile)).text();
            let token = self.view.text_input(cx, ids!(import_form.core.token)).text();
            crate::octos_service::use_remote(&endpoint, &profile, &token)?;
            self.view.text_input(cx, ids!(import_form.core.token)).set_text(cx, "");
            self.show_assistant_status(cx);
            Ok(())
        }
        #[cfg(not(feature = "octos-remote"))]
        Err("This build cannot connect to an Octos server".into())
    }
    /// Rinx's own local runtime, optionally with a new provider.
    #[cfg_attr(not(feature = "octos-local"), allow(unused_variables))]
    fn use_local(&mut self, cx: &mut Cx) -> Result<(), String> {
        if crate::octos_service::is_hosted() {
            return Err("This app uses OctoSense's AI settings".into());
        }
        #[cfg(feature = "octos-local")]
        {
            let family = self.view.text_input(cx, ids!(import_form.core.local_family)).text();
            let model = self.view.text_input(cx, ids!(import_form.core.local_model)).text();
            let base = self.view.text_input(cx, ids!(import_form.core.local_base_url)).text();
            let key = self.view.text_input(cx, ids!(import_form.core.local_key)).text();
            if family.trim().is_empty() && model.trim().is_empty() {
                crate::octos_service::use_local()?;
            } else {
                crate::octos_service::configure_local_provider(&family, &model, &base, &key)?;
            }
            self.view.text_input(cx, ids!(import_form.core.local_key)).set_text(cx, "");
            self.show_assistant_status(cx);
            Ok(())
        }
        #[cfg(not(feature = "octos-local"))]
        Err("This build has no local assistant runtime".into())
    }
    fn review(&mut self, cx: &mut Cx) -> Result<(), String> {
        let builtin = self.package.as_ref().and_then(Package::builtin_id);
        self.stop(cx);
        if builtin.is_none() { self.package = None; }
        let snapshots = crate::app_data_dir().to_owned().join("miniapps/imports");
        let package = if let Some(id) = builtin {
            Package::load_builtin(id, &snapshots)?
        } else {
            let path = self.view.text_input(cx, ids!(path)).text();
            Package::load_in(&PathBuf::from(path.trim()), &snapshots)?
        };
        self.review_package(cx, package)
    }
    fn review_package(&mut self, cx: &mut Cx, package: Package) -> Result<(), String> {
        let room = self.view.text_input(cx, ids!(import_form.room)).text();
        if !room.trim().is_empty() {
            ruma::RoomId::parse(room.trim()).map_err(|_| "Invalid Matrix room ID")?;
        }
        let services = package.manifest.capabilities.join(", ");
        let imported = package.builtin_id().is_none();
        let origin = if imported { "Local unsigned bundle" } else { crate::i18n::tr("Built-in apps") };
        self.view.view(cx, ids!(bundle_path)).set_visible(cx, imported);
        self.review_notice = format!("{} {} · {}\nServices: {}\nAllowed room: {}\nRun grants these services for this session. Octos turns may use the connected core's tools.",package.manifest.name,package.manifest.version,origin,services,if room.trim().is_empty(){"None"}else{room.trim()});
        if package.builtin_id() == Some(super::palpo::APP_ID) {
            self.review_notice = "Palpo uses your current Matrix account.\nAllow it to read your projects and requests, submit work, and perform fleet or approval actions only where your server permits.\nConfiguration downloads use Rinx's file picker. Passwords and tokens stay outside the app.\nRun remembers consent for this account and this exact bundled version.".into();
            self.view.view(cx, ids!(import_form.core)).set_visible(cx, false);
            self.view.text_input(cx, ids!(import_form.room)).set_visible(cx, false);
        } else {
            self.view.text_input(cx, ids!(import_form.room)).set_visible(cx, true);
        }
        let notice = self.review_notice.clone();
        self.notice(cx, &notice);
        self.reviewed_room = room.trim().to_string();
        self.package = Some(package);
        self.publish_to_assistant();
        Ok(())
    }
    fn run(&mut self, cx: &mut Cx) -> Result<(), String> {
        self.stop(cx);
        let package = self.package.as_ref().ok_or("Review a bundle first")?;
        package.unchanged()?;
        let account = crate::sliding_sync::current_user_id()
            .ok_or("Log in to Matrix before running a mini app")?
            .to_string();
        let room = self.view.text_input(cx, ids!(import_form.room)).text();
        if room.trim() != self.reviewed_room {
            return Err("Room access changed. Review the bundle again".into());
        }
        let room = if room.trim().is_empty() {
            None
        } else {
            Some(
                ruma::RoomId::parse(room.trim())
                    .map_err(|_| "Invalid room ID")?
                    .to_string(),
            )
        };
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let lease = super::AUTHORITY.issue(
            InstanceId {
                app: package.manifest.id.clone(),
                account: account.clone(),
                room: room.clone(),
                generation,
            },
            package.manifest.capabilities.iter().cloned().collect(),
            room.into_iter().collect(),
            Instant::now() + Duration::from_secs(3600),
        );
        self.palpo = if package.manifest.capabilities.iter().any(|c| octosense_app_contract::palpo::SERVICES.contains(&c.as_str())) {
            Some(super::palpo::PalpoHost::new(package.manifest.integrity.bundle_blake3.clone())?.with_action(self.palpo_action.take()))
        } else { None };
        if package.builtin_id() == Some(super::palpo::APP_ID) {
            super::consent::remember(&account, &package.manifest.integrity.bundle_blake3)?;
        }
        self.tag = format!("rinx-miniapp-{generation}");
        self.data = package.data.clone();
        self.state = Default::default();
        if !package.script {
            for (field, value) in octoscript_ui_l0::state_initials(&package.source) {
                self.state
                    .set_cell(octoscript_ui_l0::CARD_STATE_KEY, &field, value);
            }
        }
        let server = octosense_app_contract::AssetServer::start(&package.root)?;
        octosense_app_contract::rewrite_assets(&mut self.data, server.origin());
        let account_dir: String = account.bytes().map(|b| format!("{b:02x}")).collect();
        // The local core's default read boundary is its data root. Allocate
        // an account/app child there and then narrow the session to that child.
        // The location comes from the native host, never from bundle input.
        // The isolate's own storage stays in Rinx's data dir; the assistant's
        // workspace for this instance is its kernel request context's.
        let root = crate::app_data_dir().join("miniapps").join(account_dir);
        let mut settings = super::sandbox::IsolateSettings::for_app(&package.policy, &root);
        std::fs::create_dir_all(&settings.jail_root).map_err(|e| e.to_string())?;
        let wants_octos = package
            .manifest
            .capabilities
            .iter()
            .any(|c| octosense_app_peers::OCTOS_SERVICES.contains(&c.as_str()));
        self.provider = None;
        self.octos_unavailable = None;
        if wants_octos {
            match ContextProvider::open(&lease) {
                Ok(provider) => self.provider = Some(provider as Arc<dyn OctosProvider>),
                Err(reason) => self.octos_unavailable = Some(reason),
            }
        }
        settings.serve_bundle(server.allowlist_entry(), !package.script);
        let splash = self.view.splash(cx, ids!(card));
        super::sandbox::apply(&splash, cx, &settings);
        splash.set_host_tag(cx, Some(self.tag.clone()));
        self.assets = Some(server);
        self.lease = Some(lease);
        self.publish_to_assistant();
        self.render(cx)?;
        let calls = self.package.as_ref().unwrap().bindings.on_open.clone();
        for call in calls {
            self.binding(cx, call, "root", &Value::Null)?;
        }
        self.showing_hub = false;
        self.showing_catalog = false;
        self.view.widget(cx, ids!(library)).set_visible(cx, false);
        self.view.view(cx, ids!(header)).set_visible(cx, true);
        self.view.view(cx, ids!(app_content)).set_visible(cx, true);
        self.view.view(cx, ids!(import_form)).set_visible(cx, false);
        self.notice(
            cx,
            "Running · Back closes this app and revokes its services.",
        );
        Ok(())
    }
    fn render(&mut self, cx: &mut Cx) -> Result<(), String> {
        let package = self.package.as_ref().ok_or("No package")?;
        if package.script {
            let source = octosense_app_contract::script_source(
                &package.root,
                self.assets
                    .as_ref()
                    .ok_or("Missing bundle assets")?
                    .origin(),
            )
            .ok_or("Missing main.splash")??;
            self.view.splash(cx, ids!(card)).set_text(cx, &source);
            return Ok(());
        }
        let report = octoscript_ui_l0::realize_with_state(
            &package.source,
            &self.data,
            &self.state,
            Default::default(),
        );
        report.complete_root()?;
        self.state.prune(&report.live_keys);
        for (field, value) in report.captured {
            self.state
                .set_cell(octoscript_ui_l0::CARD_STATE_KEY, &field, value);
        }
        let card = super::presentation::prepare(
            &package.source,
            &self.data,
            &self.state,
            &package.root.join("kit"),
            &crate::theme::snapshot(cx),
        )?;
        let ui = super::presentation::embedded_ui(card)?;
        // NAV belongs to this isolate and enters the same authenticated queue
        // as imperative host.request calls. No global Notify event is trusted.
        let body = format!(
            "let NAV = fn(t, v=\"\") {{ host.request(\"rinx.event\", {{route:t, value:v}}, fn(r){{}}) }}\nwidth:Fill height:Fill flow:Down\n{ui}"
        );
        self.view.splash(cx, ids!(card)).reapply_text(cx, &body);
        Ok(())
    }
    fn dispatch(
        &mut self,
        service: &str,
        args: Value,
        reply: Option<(usize, u64)>,
        target: Option<String>,
    ) -> Result<(), String> {
        if target.is_some() && self.pending.iter().any(|p| p.target == target) {
            return Err("This action is already running".into());
        }
        if self.pending.len() >= 16 {
            return Err("Mini app already has 16 pending requests".into());
        }
        let lease = self.lease.clone().ok_or("Mini app is closed")?;
        let account = crate::sliding_sync::current_user_id().ok_or("Not logged in")?;
        lease.authorize(account.as_str(), service, None)?;
        rinx_miniapp_core::parse_arguments(&args.to_string())?;
        let (tx, rx) = mpsc::sync_channel(128);
        if service.starts_with("matrix.") {
            let service = service.to_owned();
            crate::sliding_sync::spawn_async_task(async move {
                let result = super::matrix_request(lease, service, args).await;
                let _ = tx.try_send(ServiceEvent::Complete(result));
                SignalToUI::set_ui_signal();
            });
        } else if octosense_app_contract::palpo::SERVICES.contains(&service) {
            let host = self.palpo.clone().ok_or("Palpo session is unavailable")?;
            let service = service.to_owned();
            crate::sliding_sync::spawn_async_task(async move {
                let result = super::palpo::request(host, lease, service, args).await;
                let _ = tx.try_send(ServiceEvent::Complete(result));
                SignalToUI::set_ui_signal();
            });
        } else if octosense_app_peers::OCTOS_SERVICES.contains(&service) {
            let unavailable = self
                .octos_unavailable
                .clone()
                .unwrap_or_else(|| "The assistant is unavailable".into());
            self.provider
                .as_ref()
                .ok_or(unavailable)?
                .request(lease, service, args, tx)?;
        } else {
            return Err(format!("No adapter for {service}"));
        }
        self.pending.push(Pending {
            reply,
            target,
            receiver: rx,
            started: Instant::now(),
            turn: service == "octos.turn.start",
        });
        Ok(())
    }
    fn binding(
        &mut self,
        _cx: &mut Cx,
        call: Call,
        key: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let args = super::package::arguments(&call.args, &self.data, &self.state, key, payload)?;
        self.dispatch(&call.service, args, None, Some(call.target))
    }
    fn event(&mut self, cx: &mut Cx, args: Value) -> Result<(), String> {
        let route = args["route"].as_str().ok_or("Missing event route")?;
        let mut event: Value = serde_json::from_str(
            route
                .strip_prefix("l0:")
                .ok_or("Unknown navigation event")?,
        )
        .map_err(|e| e.to_string())?;
        if event["v"] == "$$" {
            event["v"] = args["value"].clone();
        }
        let key = event["k"].as_str().ok_or("Missing instance key")?;
        let name = event["e"].as_str().ok_or("Missing event name")?;
        let package = self.package.as_ref().ok_or("No package")?;
        if package.script {
            return Err("L0 events are not available to main.splash apps".into());
        }
        let changed = octoscript_ui_l0::dispatch_with_data(
            &package.source,
            &mut self.state,
            key,
            name,
            Some(&event["v"]),
            &self.data,
        );
        let call = package.bindings.events.get(name).cloned();
        if changed {
            self.render(cx)?;
        }
        if let Some(call) = call {
            self.binding(cx, call, key, &event["v"])?;
        }
        Ok(())
    }
    fn pump(&mut self, cx: &mut Cx) {
        let Some(lease) = self.lease.clone() else {
            return;
        };
        if lease
            .check(
                &crate::sliding_sync::current_user_id()
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
            )
            .is_err()
        {
            self.stop(cx);
            self.notice(cx, "Session ended. Review and run again.");
            return;
        }
        let heap = self.view.splash(cx, ids!(card)).isolate_heap_key(cx);
        let owned_heaps: Vec<_> = heap.into_iter().collect();
        for req in take_splash_host_requests_for(&owned_heaps) {
            let result = rinx_miniapp_core::parse_arguments(&req.args_json).and_then(|args| {
                if req.service == "rinx.event" {
                    self.event(cx, args)
                } else {
                    self.dispatch(&req.service, args, Some((req.heap_key, req.req_id)), None)
                }
            });
            if req.service == "rinx.event" || result.is_err() {
                splash_host_respond(
                    cx,
                    req.heap_key,
                    req.req_id,
                    result.as_ref().map(|_| "{}").map_err(String::as_str),
                );
                if let Err(e) = result {
                    self.notice(cx, &e);
                }
            }
        }
        let mut updates = Vec::new();
        self.pending.retain_mut(|pending| {
            let mut done = false;
            loop {
                match pending.receiver.try_recv() {
                    Ok(event) => {
                        let complete = matches!(event, ServiceEvent::Complete(_));
                        updates.push((pending.reply, pending.target.clone(), pending.turn, event));
                        if complete {
                            done = true;
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        updates.push((
                            pending.reply,
                            pending.target.clone(),
                            pending.turn,
                            ServiceEvent::Complete(Err(
                                "Service connection closed before completing".into(),
                            )),
                        ));
                        done = true;
                        break;
                    }
                }
            }
            if !done && pending.started.elapsed() > Duration::from_secs(185) {
                updates.push((
                    pending.reply,
                    pending.target.clone(),
                    pending.turn,
                    ServiceEvent::Complete(Err("Service timed out".into())),
                ));
                done = true;
            }
            !done
        });
        let mut redraw = false;
        for (reply, target, turn, event) in updates {
            let (complete, result) = match event {
                ServiceEvent::Data(v) => (false, Ok(v)),
                ServiceEvent::Complete(r) => (true, r),
            };
            if !complete {
                if let Ok(value) = &result {
                    let event = &value["event"];
                    if event["kind"] == "approval_requested" {
                        if let Some(id) = event["approval_id"].as_str() {
                            if !self.approvals.iter().any(|a| a.id == id) {
                                self.approvals.push_back(Approval {
                                    id: id.to_owned(),
                                    message: format!(
                                        "Octos tool approval: {}\n{}",
                                        event["title"].as_str().unwrap_or("Tool request"),
                                        event["body"].as_str().unwrap_or("")
                                    ),
                                });
                            }
                            self.show_approval(cx);
                        }
                    }
                }
            }
            if let Some(target) = target {
                let value = match &result {
                    Ok(v) => json!({"is_ok":true,"data":v}),
                    Err(e) => json!({"is_ok":false,"error":e}),
                };
                self.data[&target] = value;
                redraw = true;
            }
            if complete {
                if turn {
                    self.approvals.clear();
                    self.show_approval(cx);
                }
                if let Some((heap, req)) = reply {
                    let text = result.as_ref().map(|v| v.to_string());
                    splash_host_respond(cx, heap, req, text.as_deref().map_err(|e| e.as_str()));
                }
                if let Err(error) = result {
                    self.notice(cx, &error);
                }
            }
        }
        if redraw {
            if let Err(e) = self.render(cx) {
                self.notice(cx, &e);
            }
        }
    }
}
impl Widget for MiniAppsPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.open {
            return;
        }
        if std::mem::take(&mut self.restyle_card) {
            if let Err(error) = self.render(cx) { self.notice(cx, &error); }
        }
        self.view.handle_event(cx, event, scope);
        // A host reply can replace dynamic children while their new areas are
        // still empty. Redraw their ancestors after the script queue is pumped.
        if self.palpo.is_some() && matches!(event, Event::Signal) { cx.redraw_all(); }
        if let Event::Actions(actions) = event {
            if self.view.button(cx, ids!(close)).clicked(actions) {
                if self.navigate_back(cx) { cx.action(MiniAppsAction::Close); }
                return;
            }
            if self.view.button(cx, ids!(browse_hub)).clicked(actions) {
                self.show_hub(cx);
                return;
            }
            for action in actions {
                if !self.showing_hub { break; }
                if let Some(action) = action.downcast_ref::<LibraryAction>() {
                    match action {
                        LibraryAction::Back => { self.navigate_back(cx); }
                        LibraryAction::Developer => {
                            self.return_to_hub = true;
                            self.show_import(cx);
                        }
                        LibraryAction::Article => {
                            cx.action(MiniAppsAction::Close);
                            crate::system_apps::open_article(cx);
                        }
                        LibraryAction::Launch(bundle, room, account) => {
                            if crate::sliding_sync::current_user_id().is_none_or(|user| user.as_str() != account) { continue; }
                            self.return_to_hub = true;
                            self.reviewed_room = room.clone().unwrap_or_default();
                            self.view.text_input(cx, ids!(import_form.room)).set_text(cx, &self.reviewed_room);
                            let result = Package::load_verified(bundle.clone(), &crate::app_data_dir().join("miniapps/imports"))
                                .and_then(|package| { self.package = Some(package); self.run(cx) });
                            if let Err(error) = result {
                                self.show_hub(cx);
                                self.notice(cx, &error);
                            } else {
                                self.view.mini_app_library(cx, ids!(library)).record(cx, &bundle.manifest.id);
                            }
                        }
                    }
                }
            }
            if self.view.button(cx, ids!(import_app)).clicked(actions) {
                self.show_import(cx);
                self.notice(cx, "Import an OctoSense bundle to review its services.");
            }
            let list = self.view.portal_list(cx, ids!(catalog_list));
            for (index, item) in list.items_with_actions(actions) {
                if !list.was_scrolling() && item.button(cx, ids!(launch)).clicked(actions) {
                    if let Err(e) = self.open_builtin(cx, index) { self.notice(cx, &e); }
                    return;
                }
            }
            let approve = self
                .view
                .button(cx, ids!(approval.buttons.allow))
                .clicked(actions);
            let deny = self
                .view
                .button(cx, ids!(approval.buttons.deny))
                .clicked(actions);
            if approve || deny {
                if let (Some(approval), Some(provider), Some(lease)) = (
                    self.approvals.pop_front(),
                    self.provider.clone(),
                    self.lease.clone(),
                ) {
                    let (tx, rx) = mpsc::sync_channel(1);
                    match provider.decide(lease, &approval.id, approve, tx) {
                        Ok(()) => self.pending.push(Pending {
                            reply: None,
                            target: None,
                            receiver: rx,
                            started: Instant::now(),
                            turn: false,
                        }),
                        Err(e) => self.notice(cx, &e),
                    }
                    self.show_approval(cx);
                }
            }
            let result = if self
                .view
                .button(cx, ids!(import_form.core.connect))
                .clicked(actions)
            {
                self.connect(cx)
            } else if self.view.button(cx, ids!(import_form.core.local_buttons.use_local)).clicked(actions) {
                self.use_local(cx)
            } else if self.view.button(cx, ids!(import_form.core.local_buttons.turn_off)).clicked(actions) {
                let result = crate::octos_service::turn_off();
                self.show_assistant_status(cx);
                result
            } else if self.view.button(cx, ids!(review)).clicked(actions) {
                self.review(cx)
            } else if self.view.button(cx, ids!(run)).clicked(actions) {
                self.run(cx)
            } else {
                Ok(())
            };
            if let Err(e) = result {
                self.stop(cx);
                self.notice(cx, &e);
            }
        }
        if let Event::BackPressed { handled } = event {
            handled.set(true);
            if self.navigate_back(cx) { cx.action(MiniAppsAction::Close); }
        }
        self.pump(cx);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let apps = crate::system_apps::apps();
                list.set_item_range(cx, 0, apps.len());
                while let Some(index) = list.next_visible_item(cx) {
                    if let Some(app) = apps.get(index) {
                        let row = list.item(cx, index, id!(App));
                        row.label(cx, ids!(copy.name)).set_text(cx, crate::i18n::tr(&app.manifest.name));
                        row.label(cx, ids!(copy.subtitle)).set_text(cx, if app.manifest.id == super::palpo::APP_ID {"Projects, resources and pending actions."} else {crate::i18n::tr("Your article, your style.")});
                        row.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        DrawStep::done()
    }
}
impl MiniAppsPanelRef {
    /// Consume Back synchronously before the underlying Rinx/shell navigation.
    pub fn back(&self, cx: &mut Cx, modal: ModalRef) {
        let mut close = false;
        if let Some(mut inner) = self.borrow_mut() {
            close = inner.navigate_back(cx);
        }
        if close { self.action(cx, modal, &MiniAppsAction::Close); }
    }

    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &MiniAppsAction) {
        if let Some(mut inner) = self.borrow_mut() {
            match action {
                MiniAppsAction::Open => {
                    inner.open = true;
                    inner.show_catalog(cx);
                    modal.open(cx);
                }
                MiniAppsAction::OpenPalpo(id) => {
                    inner.open = true;
                    inner.palpo_action = Some(id.clone());
                    if let Some(index) = crate::system_apps::apps().iter().position(|a| a.manifest.id == super::palpo::APP_ID) {
                        if let Err(error) = inner.open_builtin(cx, index) { inner.notice(cx, &error); }
                    }
                    modal.open(cx);
                }
                MiniAppsAction::OpenReviewed => {
                    inner.return_to_hub = false;
                    inner.view.mini_app_library(cx, ids!(library)).cancel_open();
                    inner.open = true;
                    inner.show_import(cx);
                    let notice = inner.review_notice.clone();
                    inner.notice(cx, &notice);
                    inner.show_assistant_status(cx);
                    inner
                        .view
                        .view(cx, ids!(import_form.core))
                        .set_visible(cx, !crate::octos_service::is_hosted());
                    modal.open(cx);
                }
                MiniAppsAction::Close => {
                    inner.view.mini_app_library(cx, ids!(library)).cancel_open();
                    inner.stop(cx);
                    inner.open = false;
                    modal.close(cx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempfile_path_for_room_test() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rinx-import-room-test-{}", std::process::id()))
    }

    #[test]
    fn native_panel_keeps_catalog_hub_and_import_back_navigation_separate() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut panel = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            crate::i18n::install(vm);
            makepad_code_editor::script_mod(vm);
            crate::shared::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            crate::miniapps::script_mod(vm);
            let value = vm.eval(script! {use mod.prelude.widgets.* use mod.widgets.* MiniAppsPanel{}});
            let panel = MiniAppsPanel::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "Mini-app panel script errors: {errors:?}");
            panel
        });
        // The hidden Hub also has a `room` DropDown. Generic ids!(room)
        // resolves it first, losing a developer-import room grant.
        let room = "!import-room:example.org";
        panel.view.text_input(&cx, ids!(import_form.room)).set_text(&mut cx, room);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/miniapps/matrix-octos-script");
        let snapshots = tempfile_path_for_room_test();
        let package = Package::load_in(&path, &snapshots).expect("example bundle");
        panel.review_package(&mut cx, package).expect("room review");
        assert_eq!(panel.reviewed_room, room);
        assert!(panel.review_notice.contains(&format!("Allowed room: {room}")));
        let _ = std::fs::remove_dir_all(snapshots);
        panel.show_catalog(&mut cx);
        assert!(panel.navigate_back(&mut cx), "Only the outer catalog closes");
        panel.show_hub(&mut cx);
        assert!(panel.showing_hub);
        assert!(!panel.navigate_back(&mut cx));
        assert!(panel.showing_catalog);
        panel.show_hub(&mut cx);
        panel.return_to_hub = true;
        panel.show_import(&mut cx);
        assert!(!panel.navigate_back(&mut cx));
        assert!(panel.showing_hub, "Developer import returns to its Hub entry point");
        assert!(!panel.navigate_back(&mut cx));
        assert!(panel.showing_catalog);
        panel.show_import(&mut cx);
        assert!(!panel.navigate_back(&mut cx));
        assert!(panel.showing_catalog, "Catalog import returns to the catalog");
    }
}
