//! Native consumer library. Development import is a separate navigation action.
use super::catalog_worker::{Command, Outcome, Worker};
use crate::shared::navigation_bar_button::NavigationBarButtonWidgetRefExt;
use makepad_widgets::*;
use crate::theme::Snapshot as ThemeSnapshot;
use rinx_miniapp_catalog::{App, Consent, Snapshot, Status, VerifiedBundle, ARTICLE_ID};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum LibraryAction {
    Back,
    Developer,
    Launch(VerifiedBundle, Option<String>, String),
    Article,
}
#[derive(Default, PartialEq)]
enum Tab {
    Recent,
    #[default]
    Mine,
    Browse,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.MiniAppLibrary = #(MiniAppLibrary::register_widget(vm)) {
        width: Fill height: Fill flow: Down padding: 16 spacing: 12
        header := View {width: Fill height: 44 flow: Right spacing: 8 align: Align{y: 0.5}
            back := RinxButton {width: 44 height: 44 text: "" icon_walk: Walk{width: 20 height: 20}
                draw_icon +: {svg: crate_resource("self://resources/icons/arrow_back.svg") color: mod.widgets.RINX_INK}}
            title := Label {width: Fill max_lines: 1 text_overflow: Ellipsis text: "Mini apps" draw_text +: {color: mod.widgets.RINX_INK text_style.font_size: (18 * mod.widgets.RINX_TEXT_SCALE)}}
            refresh := RinxButton {text: "Refresh"}
            developer := RinxButton {text: "Developer"}
        }
        status := Label {width: Fill height: Fit draw_text.color: mod.widgets.RINX_MUTED text: "Loading App Hub…"}
        collection := View {width: Fill height: Fill flow: Down spacing: 12
            tabs := View {width: Fill height: 44 flow: Right spacing: 6
                recent := RinxButton {width: Fill text: "Recent"}
                mine := RinxButton {width: Fill text: "My apps"}
                browse := RinxButton {width: Fill text: "Browse"}
            }
            search := RinxInput {width: Fill height: 40 empty_text: "Find mini apps"}
            empty := Label {visible: false width: Fill height: Fit draw_text.color: mod.widgets.RINX_MUTED}
            list := PortalList {width: Fill height: Fill
                Filler := View {width: Fill height: 84}
                App := View {width: Fill height: 84 flow: Down
                    row := NavigationBarButton {
                        width: Fill height: 76 flow: Right spacing: 12 padding: 10 align: Align{y: 0.5}
                        icon := Image {width: 48 height: 48}
                        copy := View {width: Fill height: Fit flow: Down spacing: 5
                            name := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_INK text_style.font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}
                            subtitle := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_MUTED text_style.font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                        }
                        state := Label {draw_text +: {color: mod.widgets.RINX_MUTED text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                }
            }
        }
        details := ScrollYView {visible: false width: Fill height: Fill flow: Down spacing: 16
            description := Label {width: Fill draw_text.color: mod.widgets.RINX_INK}
            publisher := Label {width: Fill draw_text.color: mod.widgets.RINX_MUTED}
            permissions := Label {width: Fill draw_text.color: mod.widgets.RINX_INK}
            account := Label {width: Fill draw_text.color: mod.widgets.RINX_MUTED}
            room_group := View {width: Fill height: Fit flow: Down spacing: 8
                Label {text: "Allow access to a conversation" draw_text.color: mod.widgets.RINX_MUTED}
                room := DropDown {width: Fill labels: ["No conversation access"]}
            }
            availability := Label {width: Fill draw_text.color: mod.widgets.RINX_MUTED}
            actions := View {width: Fill height: Fit flow: Right spacing: 12
                primary := RinxPrimaryButton {height: 44 text: "Add"}
                remove := RinxButton {height: 44 text: "Remove"}
            }
            remove_note := Label {width: Fill text: "Removing an app keeps its saved documents." draw_text.color: mod.widgets.RINX_MUTED}
            remove_confirm := View {visible: false width: Fill height: Fit flow: Right spacing: 12
                cancel := RinxButton {height: 44 text: "Cancel"}
                confirm := RinxButton {height: 44 text: "Remove app"}
            }
        }
    }
}
#[derive(Script, Widget)]
pub struct MiniAppLibrary {
    #[deref]
    view: View,
    #[rust]
    theme: ThemeSnapshot,
    #[rust]
    status_text: String,
    #[rust]
    room_labels: Vec<String>,
    #[rust]
    worker: Option<Worker>,
    #[rust]
    owner: Option<String>,
    #[rust]
    snapshot: Snapshot,
    #[rust]
    tab: Tab,
    #[rust]
    selected: Option<App>,
    #[rust]
    visible: Vec<String>,
    #[rust]
    rooms: Vec<String>,
    #[rust]
    room_grant: Option<String>,
    #[rust]
    query: String,
    #[rust]
    busy: bool,
    #[rust]
    job: u64,
    #[rust]
    icons: HashMap<WidgetUid, String>,
}
impl ScriptHook for MiniAppLibrary {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        self.theme = crate::theme::snapshot_for_vm(vm);
        if apply.is_script_reapply() {
            let cx = vm.cx_mut();
            self.view.label(cx, ids!(status)).set_text(cx, &self.status_text);
            self.view.label(cx, ids!(status)).set_visible(cx, !self.status_text.is_empty());
            if !self.room_labels.is_empty() {
                let room = self.view.drop_down(cx, ids!(details.room_group.room));
                room.set_labels(cx, self.room_labels.clone());
                let index = self.room_grant.as_ref().and_then(|id| self.rooms.iter().position(|r| r == id)).unwrap_or(0);
                room.set_selected_item(cx, index);
            }
        }
    }
}
impl MiniAppLibrary {
    fn status(&mut self, cx: &mut Cx, text: &str) {
        self.status_text = text.to_owned();
        self.view.label(cx, ids!(status)).set_text(cx, text);
        self.view
            .label(cx, ids!(status))
            .set_visible(cx, !text.is_empty());
    }
    fn request(&mut self, cx: &mut Cx, command: Command) {
        if self.busy {
            return;
        }
        self.job += 1;
        match self
            .worker
            .as_ref()
            .ok_or_else(|| "Log in to Matrix to browse mini apps".to_string())
            .and_then(|w| w.request(self.job, command))
        {
            Ok(()) => {
                self.busy = true;
                self.status(cx, "Working…");
            }
            Err(e) => {
                self.worker = None;
                self.status(cx, &e);
            }
        }
        self.view.redraw(cx);
    }
    fn begin(&mut self, cx: &mut Cx) {
        let owner = crate::sliding_sync::current_user_id().map(|u| u.to_string());
        if owner != self.owner || self.worker.is_none() {
            self.worker = None;
            self.snapshot = Snapshot::default();
            self.selected = None;
            self.query.clear();
            self.icons.clear();
            self.rooms.clear();
            self.room_grant = None;
            self.view.text_input(cx, ids!(search)).set_text(cx, "");
            self.busy = false;
            self.owner = owner;
            if let Some(account) = &self.owner {
                let key: String = account.bytes().map(|b| format!("{b:02x}")).collect();
                self.worker = Some(Worker::new(
                    crate::app_data_dir().join("miniapps/library").join(key),
                ));
            }
        }
        self.selected = None;
        self.job += 1;
        self.busy = false;
        self.request(cx, Command::Refresh);
        self.view.redraw(cx);
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.selected.take().is_some() {
            self.job += 1;
            self.busy = false;
            self.room_grant = None;
            self.view
                .view(cx, ids!(details.remove_confirm))
                .set_visible(cx, false);
            self.status(cx, "");
            self.view.redraw(cx);
        } else {
            cx.action(LibraryAction::Back);
        }
    }
    fn select(&mut self, cx: &mut Cx, id: &str) {
        if self.busy {
            return;
        }
        if id == ARTICLE_ID {
            self.request(cx, Command::Record(ARTICLE_ID.into()));
            cx.action(LibraryAction::Article);
            return;
        }
        self.selected = self.snapshot.apps.iter().find(|a| a.id == id).cloned();
        self.rooms.clear();
        self.rooms.push(String::new());
        self.room_grant = None;
        let mut joined: Vec<_> = crate::sliding_sync::get_client()
            .map(|c| {
                c.joined_rooms()
                    .into_iter()
                    .filter(|r| !r.is_space())
                    .map(|r| {
                        let name = r
                            .cached_display_name()
                            .map(|s| s.to_string())
                            .or_else(|| r.name())
                            .unwrap_or_else(|| r.room_id().to_string());
                        (name, r.room_id().to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        joined.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        let mut labels = vec!["No conversation access".into()];
        for (name, id) in joined {
            labels.push(name);
            self.rooms.push(id);
        }
        let room = self.view.drop_down(cx, ids!(details.room_group.room));
        self.room_labels = labels.clone();
        room.set_labels(cx, labels);
        room.set_selected_item(cx, 0);
        self.view
            .view(cx, ids!(details.remove_confirm))
            .set_visible(cx, false);
        self.status(cx, "");
        self.view.redraw(cx);
    }
    fn pump(&mut self, cx: &mut Cx) {
        let owner = crate::sliding_sync::current_user_id().map(|u| u.to_string());
        if self.owner != owner {
            self.begin(cx);
            return;
        }
        let replies: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.receive.try_iter().collect())
            .unwrap_or_default();
        for reply in replies {
            self.snapshot = reply.snapshot;
            if reply.job == self.job || (reply.job == 0 && reply.result.is_err()) {
                self.busy = false;
            }
            // Snapshot is account-owned and may update the library after Back,
            // but a superseded open can never start an app in another page.
            let current = reply.job == self.job || reply.job == 0;
            if current {
                match reply.result {
                    Ok(Outcome::Opened(bundle)) => {
                        if let Some(account) = &self.owner {
                            cx.action(LibraryAction::Launch(
                                bundle,
                                self.room_grant.clone(),
                                account.clone(),
                            ));
                        }
                    }
                    Ok(Outcome::Updated) => {
                        let warning = self.snapshot.warning.clone().unwrap_or_default();
                        self.status(cx, &warning);
                    }
                    Err(e) => {
                        if reply.job == 0 {
                            self.worker = None;
                        }
                        self.status(cx, &e);
                    }
                }
            }
            if let Some(selected) = &self.selected {
                self.selected = self
                    .snapshot
                    .apps
                    .iter()
                    .find(|a| a.id == selected.id)
                    .cloned();
            }
            self.icons.clear();
            self.view.redraw(cx);
        }
    }
    fn filtered(&self) -> Vec<String> {
        let query = self.query.to_lowercase();
        let mut out = Vec::new();
        if self.tab != Tab::Browse
            && "article editor".contains(&query)
            && (self.tab != Tab::Recent || self.snapshot.recent.iter().any(|id| id == ARTICLE_ID))
        {
            out.push(ARTICLE_ID.into());
        }
        for app in &self.snapshot.apps {
            if app.id == ARTICLE_ID {
                continue;
            }
            if self.tab != Tab::Browse && !app.installed {
                continue;
            }
            if self.tab == Tab::Recent && !self.snapshot.recent.contains(&app.id) {
                continue;
            }
            if format!("{} {} {}", app.name, app.subtitle, app.publisher)
                .to_lowercase()
                .contains(&query)
            {
                out.push(app.id.clone());
            }
        }
        if self.tab == Tab::Recent {
            out.sort_by_key(|id| {
                self.snapshot
                    .recent
                    .iter()
                    .position(|r| r == id)
                    .unwrap_or(usize::MAX)
            });
        }
        out
    }
}
impl Widget for MiniAppLibrary {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        self.pump(cx);
        if !self.view.visible() {
            return;
        }
        if let Event::Actions(actions) = event {
            if self.view.button(cx, ids!(header.back)).clicked(actions) {
                self.back(cx);
            }
            if self
                .view
                .button(cx, ids!(header.developer))
                .clicked(actions)
            {
                cx.action(LibraryAction::Developer);
            }
            if self.view.button(cx, ids!(header.refresh)).clicked(actions) {
                self.request(cx, Command::Refresh);
            }
            for (path, tab) in [
                (ids!(tabs.recent), Tab::Recent),
                (ids!(tabs.mine), Tab::Mine),
                (ids!(tabs.browse), Tab::Browse),
            ] {
                if self.view.button(cx, path).clicked(actions) {
                    self.tab = tab;
                    self.view
                        .portal_list(cx, ids!(list))
                        .set_first_id_and_scroll(0, 0.0);
                    self.view.redraw(cx);
                }
            }
            if let Some(query) = self.view.text_input(cx, ids!(search)).changed(actions) {
                self.query = query;
                self.view.redraw(cx);
            }
            let list = self.view.portal_list(cx, ids!(list));
            for (index, widget) in list.items_with_actions(actions) {
                if !list.was_scrolling()
                    && widget.navigation_bar_button(cx, ids!(row)).clicked(actions)
                {
                    if let Some(id) = self.visible.get(index).cloned() {
                        self.select(cx, &id);
                    }
                }
            }
            if let Some(app) = self.selected.clone() {
                if self
                    .view
                    .button(cx, ids!(details.actions.primary))
                    .clicked(actions)
                    && !self.busy
                {
                    let consent = Consent {
                        id: app.id,
                        entry: app.consent,
                    };
                    match app.status {
                        Status::Available | Status::Update => {
                            self.request(cx, Command::Install(consent))
                        }
                        Status::Installed => {
                            let selected = self
                                .view
                                .drop_down(cx, ids!(details.room_group.room))
                                .selected_item();
                            self.room_grant = self
                                .rooms
                                .get(selected)
                                .filter(|id| !id.is_empty())
                                .cloned();
                            self.request(cx, Command::Open(consent));
                        }
                        Status::Unavailable(_) => {}
                    }
                }
                if self
                    .view
                    .button(cx, ids!(details.actions.remove))
                    .clicked(actions)
                {
                    self.view
                        .view(cx, ids!(details.remove_confirm))
                        .set_visible(cx, true);
                }
                if self
                    .view
                    .button(cx, ids!(details.remove_confirm.cancel))
                    .clicked(actions)
                {
                    self.view
                        .view(cx, ids!(details.remove_confirm))
                        .set_visible(cx, false);
                }
                if self
                    .view
                    .button(cx, ids!(details.remove_confirm.confirm))
                    .clicked(actions)
                {
                    self.request(
                        cx,
                        Command::Remove(self.selected.as_ref().unwrap().id.clone()),
                    );
                    self.view
                        .view(cx, ids!(details.remove_confirm))
                        .set_visible(cx, false);
                }
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let details = self.selected.is_some();
        self.view
            .view(cx, ids!(collection))
            .set_visible(cx, !details);
        self.view.view(cx, ids!(details)).set_visible(cx, details);
        self.view
            .button(cx, ids!(header.developer))
            .set_visible(cx, !details);
        for (path, tab) in [
            (ids!(tabs.recent), Tab::Recent),
            (ids!(tabs.mine), Tab::Mine),
            (ids!(tabs.browse), Tab::Browse),
        ] {
            let mut button = self.view.button(cx, path);
            let color = if self.tab == tab {
                self.theme.accent
            } else {
                self.theme.muted
            };
            script_apply_eval!(cx, button, {draw_text +: {color: #(color)}});
        }
        let title = self
            .selected
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("Mini apps");
        self.view.label(cx, ids!(header.title)).set_text(cx, title);
        if let Some(app) = &self.selected {
            self.view
                .label(cx, ids!(details.description))
                .set_text(cx, &format!("{}\n\n{}", app.subtitle, app.description));
            self.view.label(cx, ids!(details.publisher)).set_text(
                cx,
                &format!("{} · {}\n{}", app.publisher, app.version, app.repository),
            );
            self.view
                .label(cx, ids!(details.permissions))
                .set_text(cx, &format!("Permissions\n{}", app.permissions.join("\n")));
            self.view.label(cx, ids!(details.account)).set_text(
                cx,
                &format!(
                    "Open uses your current account: {}\nAccess ends when you close the app.",
                    self.owner.as_deref().unwrap_or("Not logged in")
                ),
            );
            let (button, available, message) = match &app.status {
                Status::Available => ("Add", true, "Add this app with the permissions above."),
                Status::Update => (
                    "Update",
                    true,
                    "Review the permissions above before updating.",
                ),
                Status::Installed => (
                    "Open",
                    true,
                    "Open allows these permissions for this session.",
                ),
                Status::Unavailable(reason) => ("Unavailable", false, reason.as_str()),
            };
            self.view
                .label(cx, ids!(details.availability))
                .set_text(cx, message);
            self.view
                .button(cx, ids!(details.actions.primary))
                .set_text(cx, if self.busy { "Working…" } else { button });
            self.view
                .button(cx, ids!(details.actions.primary))
                .set_enabled(cx, available && !self.busy);
            self.view
                .button(cx, ids!(details.actions.remove))
                .set_visible(cx, app.installed);
            self.view
                .label(cx, ids!(details.remove_note))
                .set_visible(cx, app.installed);
        }
        self.visible = self.filtered();
        let empty = !details && !self.busy && self.visible.is_empty();
        self.view.label(cx, ids!(empty)).set_visible(cx, empty);
        if empty {
            let message = {
                if !self.query.is_empty() {
                    "No matching mini apps."
                } else if self.tab == Tab::Recent {
                    "Apps you open appear here. Choose My apps or Browse to start."
                } else if self.snapshot.verified {
                    "No apps are published in App Hub yet. Your built-in editor is in My apps."
                } else {
                    "App Hub is not available yet. Your built-in editor is in My apps."
                }
            };
            self.view.label(cx, ids!(empty)).set_text(cx, message);
        }
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.visible.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(id) = self.visible.get(index) else {
                        list.item(cx, index, id!(Filler)).draw_all(cx, scope);
                        continue;
                    };
                    let item = list.item(cx, index, id!(App));
                    let app = self.snapshot.apps.iter().find(|a| &a.id == id);
                    let (name, subtitle, state) = if let Some(app) = app {
                        (
                            app.name.as_str(),
                            app.subtitle.as_str(),
                            match app.status {
                                Status::Available => "Add",
                                Status::Installed => "Open",
                                Status::Update => "Update",
                                Status::Unavailable(_) => "Unavailable",
                            },
                        )
                    } else {
                        (
                            "Article editor",
                            "Write, preview and publish articles",
                            "Built in",
                        )
                    };
                    item.label(cx, ids!(row.copy.name)).set_text(cx, name);
                    item.label(cx, ids!(row.copy.subtitle))
                        .set_text(cx, subtitle);
                    item.label(cx, ids!(row.state)).set_text(cx, state);
                    let image = item.image(cx, ids!(row.icon));
                    let source = app.and_then(|a| a.icon.as_deref()).unwrap_or("");
                    let key = format!("{id}:{source}");
                    if self.icons.get(&image.widget_uid()) != Some(&key) {
                        let _ = image.load_svg_from_data(
                            cx,
                            if id == crate::system_apps::ARTICLE_ID {
                                include_bytes!("../../resources/icons/file.svg")
                            } else {
                                include_bytes!("../../resources/icons/squares_filled.svg")
                            },
                        );
                        if source.starts_with("https://") {
                            let _ = image.load_image_http_by_url_async(cx, source);
                        } else if !source.is_empty() {
                            let _ = image
                                .load_image_file_by_path_async(cx, std::path::Path::new(source));
                        }
                        self.icons.insert(image.widget_uid(), key);
                    }
                    item.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}
impl MiniAppLibraryRef {
    pub fn begin(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.begin(cx);
        }
    }
    pub fn record(&self, cx: &mut Cx, id: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.request(cx, Command::Record(id.into()));
        }
    }
    pub fn cancel_open(&self) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.job += 1;
        }
    }
    pub fn back_to_list(&self, cx: &mut Cx) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.selected.is_some() {
                inner.back(cx);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn library() -> (Cx, MiniAppLibrary) {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let panel = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            crate::i18n::install(vm);
            makepad_code_editor::script_mod(vm);
            crate::shared::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            super::script_mod(vm);
            let value =
                vm.eval(script! {use mod.prelude.widgets.* use mod.widgets.* MiniAppLibrary{}});
            let panel = MiniAppLibrary::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(
                errors.is_empty(),
                "Mini-app library script errors: {errors:?}"
            );
            panel
        });
        (cx, panel)
    }
    fn app(id: &str, installed: bool) -> App {
        App {
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            subtitle: String::new(),
            description: String::new(),
            publisher: "Test publisher".into(),
            repository: String::new(),
            permissions: vec!["Read your profile".into()],
            icon: None,
            status: if installed {
                Status::Installed
            } else {
                Status::Available
            },
            installed,
            consent: "test".into(),
        }
    }
    #[test]
    fn native_library_filters_installed_recent_and_catalog_apps() {
        let (_cx, mut panel) = library();
        panel.snapshot.apps = vec![app("counter", true), app("weather", false)];
        panel.snapshot.recent = vec!["counter".into(), ARTICLE_ID.into()];
        assert_eq!(panel.filtered(), [ARTICLE_ID, "counter"]);
        panel.tab = Tab::Recent;
        assert_eq!(panel.filtered(), ["counter", ARTICLE_ID]);
        panel.tab = Tab::Browse;
        assert_eq!(panel.filtered(), ["counter", "weather"]);
        panel.query = "WEATHER".into();
        assert_eq!(panel.filtered(), ["weather"]);
    }
    #[test]
    fn back_from_details_cancels_pending_launch_and_returns_to_library() {
        let (mut cx, mut panel) = library();
        panel.selected = Some(app("counter", true));
        panel.busy = true;
        panel.job = 5;
        panel.room_grant = Some("!private:example.org".into());
        panel.back(&mut cx);
        assert!(panel.selected.is_none());
        assert!(panel.room_grant.is_none());
        assert!(!panel.busy);
        assert_eq!(panel.job, 6);
        assert!(
            cx.new_actions.is_empty(),
            "Back from details must not close the outer mini-app modal"
        );
    }
    #[test]
    fn native_layout_fits_phone_and_desktop_widths() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        for (width, details, empty) in [
            (360.0, false, false),
            (360.0, true, false),
            (360.0, false, true),
            (1024.0, false, false),
            (1024.0, true, false),
            (1024.0, false, true),
        ] {
            let (mut cx, mut panel) = library();
            panel.snapshot.apps = vec![app("counter", true)];
            if details {
                panel.selected = Some(app("counter", true));
            }
            if empty {
                panel.snapshot.apps.clear();
                panel.tab = Tab::Browse;
            }
            let pass = DrawPass::new(&mut cx);
            pass.set_size(&mut cx, dvec2(width, 760.0));
            let mut list = DrawList2d::new(&mut cx);
            let event = DrawEvent::default();
            let mut draw = CxDraw::new(&mut cx, &event);
            let mut cx2d = Cx2d::new(&mut draw);
            cx2d.begin_pass(&pass, None);
            list.begin_always(&mut cx2d);
            cx2d.begin_root_turtle(dvec2(width, 760.0), Layout::flow_overlay());
            panel.draw_all(&mut cx2d, &mut Scope::empty());
            cx2d.end_pass_sized_turtle();
            list.end(&mut cx2d);
            cx2d.end_pass(&pass);
            drop(cx2d);
            drop(draw);
            let paths: [&[LiveId]; 4] = if details {
                [
                    ids!(header.back),
                    ids!(details.actions.primary),
                    ids!(details.actions.remove),
                    ids!(details.room_group.room),
                ]
            } else {
                [
                    ids!(header.back),
                    ids!(header.developer),
                    ids!(tabs.browse),
                    ids!(list),
                ]
            };
            for path in paths {
                let rect = panel.view.widget(&mut cx, path).area().rect(&cx);
                assert!(
                    rect.size.x > 0.0
                        && rect.pos.x >= 0.0
                        && rect.pos.x + rect.size.x <= width + 0.5,
                    "widget {path:?} exceeds {width}: {rect:?}"
                );
            }
        }
    }
}
