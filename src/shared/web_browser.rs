//! In-app reader for ordinary chat links, backed by the platform browser.
use makepad_widgets::*;
use url::Url;
use std::sync::Arc;
use ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use crate::article_app::{ArticleAction, ArticlePanelWidgetRefExt};
use crate::shared::navigation_bar_button::{NavigationBarButtonAction, NavigationBarButtonWidgetExt};
use super::attachment_download::{DownloadableAttachment, MediaDownloadResult, markdown_source};
use super::web_browser_session::{ReaderSession, ReaderTab};

const CAPTION_PADDING: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

#[derive(Clone, Debug)]
pub enum WebBrowserAction {
    Open(Url),
    OpenMarkdown {
        title: String,
        source: Arc<str>,
        account: Option<OwnedUserId>,
        attachment: Option<DownloadableAttachment>,
    },
    ReadArticle {
        room: OwnedRoomId,
        event: OwnedEventId,
    },
    Restore {
        session: ReaderSession,
        account: Option<OwnedUserId>,
    },
    Close,
}

impl WebBrowserAction {
    pub fn allowed(&self) -> bool {
        !matches!(self, Self::OpenMarkdown { account, .. } | Self::Restore { account, .. } if account.as_ref() != crate::sliding_sync::current_user_id().as_ref()
            || crate::logout::logout_state_machine::is_logout_in_progress())
    }
}

#[derive(Clone, Debug)]
struct RestoredMarkdown {
    tab: LiveId,
    account: Option<OwnedUserId>,
    result: Result<Arc<str>, String>,
}

/// Matrix links are resolved by the caller first. Other schemes retain their
/// existing OS handler; only HTTP(S) pages belong in this reader.
pub fn open_chat_link(cx: &mut Cx, address: &str) -> bool {
    if let Some(id) = crate::miniapps::palpo::route(address) {
        cx.action(crate::miniapps::MiniAppsAction::OpenPalpo(id));
        return true;
    }
    let Ok(url) = Url::parse(address) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return false;
    }
    cx.action(WebBrowserAction::Open(url));
    true
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.WebBrowserIconButton = NavigationBarButton {
        width: 36 height: 36 padding: 8
        draw_bg +: {color_hover: mod.widgets.RINX_FIELD color_active: mod.widgets.RINX_PRESSED border_radius: 6}
        icon := Icon {width: 20 height: 20 draw_icon.color: mod.widgets.RINX_INK}
    }

    mod.widgets.WebBrowser = #(WebBrowser::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down
        draw_bg.color: mod.widgets.RINX_SURFACE
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(CAPTION_PADDING) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        web_tabs := TabBar {
            height: 40
            draw_bg +: {color: mod.widgets.RINX_FIELD color_dither: 0 border_size: 0}
            draw_fill +: {color: mod.widgets.RINX_FIELD color_2: vec4(-1, -1, -1, -1) border_size: 0}
            CloseableTab := Tab {
                closeable: true height: 40 margin: 0
                padding: Inset{left: 10 right: 14 top: 0 bottom: 0}
                draw_text +: {
                    color: mod.widgets.RINX_MUTED color_active: mod.widgets.RINX_INK color_hover: mod.widgets.RINX_INK
                    text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}
                }
                draw_bg +: {
                    color: mod.widgets.RINX_FIELD color_hover: mod.widgets.RINX_HOVER color_active: mod.widgets.RINX_SURFACE
                    color_2: vec4(-1, -1, -1, -1) border_size: 0 color_dither: 0
                }
                close_button +: {
                    width: 18 height: 18 margin: Inset{right: 8 left: 0}
                    draw_button +: {color: mod.widgets.RINX_MUTED color_hover: mod.widgets.RINX_INK}
                }
            }
        }
        toolbar := View {
            width: Fill height: 60 flow: Right spacing: 4 padding: Inset{left: 8 right: 8} align: Align{y: 0.5}
            web_close := mod.widgets.WebBrowserIconButton {icon.draw_icon.svg: (ICON_CLOSE)}
            web_back := mod.widgets.WebBrowserIconButton {icon.draw_icon.svg: crate_resource("self://resources/icons/web_back.svg")}
            web_forward := mod.widgets.WebBrowserIconButton {icon.draw_icon.svg: crate_resource("self://resources/icons/web_forward.svg")}
            web_reopen := mod.widgets.WebBrowserIconButton {icon.draw_icon.svg: crate_resource("self://resources/icons/article_refresh.svg")}
            address := View {
                width: Fill height: Fit flow: Down spacing: 3 padding: Inset{left: 8 right: 8}
                web_title := Label {
                    width: Fill max_lines: 1 text_overflow: Ellipsis padding: 0
                    draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_bold {font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
                }
                web_address := Label {
                    width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                    draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
                }
            }
            web_external := mod.widgets.WebBrowserIconButton {icon.draw_icon.svg: (ICON_EXTERNAL_LINK)}
        }
        SolidView {width: Fill height: 1 draw_bg.color: mod.widgets.RINX_BORDER}
        web_surface := SolidView {
            width: Fill height: Fill flow: Overlay draw_bg.color: mod.widgets.RINX_SURFACE
            native_page := View {width: Fill height: Fill}
            web_status := Label {
                visible: false width: Fill height: Fit margin: 24 flow: Flow.Right{wrap: true}
                draw_text.color: mod.widgets.RINX_MUTED
            }
        }
        web_tooltip := Tooltip {
            abs_pos: vec2(0, 0)
            content +: {padding: 7 tooltip_label +: {width: Fit draw_text.text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
        }
    }
}

#[derive(Script, Widget)]
pub struct WebBrowser {
    #[deref]
    view: View,
    #[rust]
    tabs: Vec<WebTab>,
    #[rust]
    active_tab: Option<usize>,
    #[rust]
    session_account: Option<OwnedUserId>,
    #[rust]
    preserve_session: bool,
    #[rust]
    restore_after_theme: bool,
}

impl ScriptHook for WebBrowser {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _value: ScriptValue,
    ) {
        if apply.is_script_reapply() {
            for tab in &mut self.tabs {
                if let Some(panel) = &mut tab.article {
                    let value = script_eval!(vm, { mod.widgets.ArticlePanel {padding: 0 tabbed_reader: true} });
                    panel.script_apply(vm, apply, scope, value);
                }
            }
            self.restore_after_theme = true;
        }
    }
}

struct WebTab {
    id: LiveId,
    original_url: Option<Url>,
    article: Option<WidgetRef>,
    title: String,
    spawned: bool,
    location: Option<ReaderTab>,
    status: Option<String>,
}

impl WebTab {
    fn title(&self) -> String {
        if let Some(url) = &self.original_url {
            return tab_label(url);
        }
        let title = self
            .article
            .as_ref()
            .map(|panel| panel.as_article_panel().reader_title())
            .unwrap_or_default();
        if title.is_empty() {
            self.title.clone()
        } else {
            title
        }
    }

    fn close(&self, cx: &mut Cx) {
        if self.spawned {
            cx.system_browser(self.id).close();
        }
        if let Some(panel) = &self.article {
            panel
                .as_article_panel()
                .action(cx, ModalRef::default(), &ArticleAction::Close);
        }
    }
}

impl WebBrowser {
    fn session(&self) -> ReaderSession {
        let mut session = ReaderSession::default();
        for (index, tab) in self.tabs.iter().enumerate() {
            if let Some(location) = &tab.location {
                if self.active_tab.is_some_and(|active| index <= active) {
                    session.active = session.tabs.len();
                }
                session.tabs.push(location.clone());
            }
        }
        session
    }

    fn save_session(&self) {
        if self.preserve_session {
            return;
        }
        if let Some(account) = &self.session_account {
            if crate::sliding_sync::current_user_id().as_ref() == Some(account) {
                if let Err(error) = self.session().save(account) {
                    error!("Could not save reader tabs: {error}");
                }
            }
        }
    }

    fn prepare_shutdown(&mut self) {
        self.save_session();
        // Window teardown must release resources without overwriting the session
        // with an empty tab list. Explicit reader/tab closes still save normally.
        self.preserve_session = true;
    }

    fn active(&self) -> Option<&WebTab> {
        self.active_tab.and_then(|index| self.tabs.get(index))
    }

    fn browser_id(&self) -> Option<SystemBrowserId> {
        self.active()
            .filter(|tab| tab.spawned)
            .map(|tab| SystemBrowserId(tab.id))
    }

    fn close(&mut self, cx: &mut Cx) {
        for tab in self.tabs.drain(..) {
            tab.close(cx);
        }
        self.active_tab = None;
        self.save_session();
        self.session_account = None;
        self.attach_document(cx);
        self.tooltip(cx, ids!(web_tooltip)).hide(cx);
        self.redraw(cx);
    }

    fn open(&mut self, cx: &mut Cx, url: Url) {
        // Browser widgets elsewhere use raw WidgetUid values. A namespaced
        // hashed ID keeps tabs out of that separate numeric ID space.
        let id = LiveId::from_str(&format!("rinx.web_browser.tab.{}", LiveId::unique().0));
        let spawned = cfg!(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "android"
        ));
        if spawned {
            cx.system_browser(id).spawn_navigable(url.as_str());
        }
        self.tabs.push(WebTab {
            id,
            location: Some(ReaderTab::Web {
                url: url.to_string(),
            }),
            original_url: Some(url),
            article: None,
            title: String::new(),
            spawned,
            status: None,
        });
        self.select_tab(cx, self.tabs.len() - 1);
    }

    fn new_article_panel(cx: &mut Cx) -> WidgetRef {
        cx.with_vm(|vm| {
            let template = script_eval!(vm, {
                use mod.prelude.widgets.*
                use mod.widgets.*
                ArticlePanel { padding: 0 tabbed_reader: true }
            });
            WidgetRef::script_from_value(vm, template)
        })
    }

    fn open_document(
        &mut self,
        cx: &mut Cx,
        panel: WidgetRef,
        title: String,
        location: Option<ReaderTab>,
    ) {
        self.tabs.push(WebTab {
            id: LiveId::from_str(&format!("rinx.document.tab.{}", LiveId::unique().0)),
            original_url: None,
            article: Some(panel),
            title,
            spawned: false,
            location,
            status: None,
        });
        self.select_tab(cx, self.tabs.len() - 1);
    }

    fn open_article(&mut self, cx: &mut Cx, room: OwnedRoomId, event: OwnedEventId) {
        let panel = Self::new_article_panel(cx);
        panel.as_article_panel().action(
            cx,
            ModalRef::default(),
            &ArticleAction::Read {
                room: room.clone(),
                event: event.clone(),
            },
        );
        self.open_document(
            cx,
            panel,
            crate::i18n::tr("Read full article").into(),
            Some(ReaderTab::Article { room, event }),
        );
    }

    fn restore(&mut self, cx: &mut Cx, session: &ReaderSession) {
        // A late/duplicate restore must not replace tabs opened in this process.
        if !self.tabs.is_empty() {
            return;
        }
        self.session_account = crate::sliding_sync::current_user_id();
        self.preserve_session = true;
        let mut selected = 0;
        for (index, location) in session.tabs.iter().enumerate() {
            match location {
                ReaderTab::Web { url } => {
                    let Ok(url) = Url::parse(url) else { continue };
                    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                        continue;
                    }
                    self.open(cx, url);
                }
                ReaderTab::Article { room, event } => {
                    self.open_article(cx, room.clone(), event.clone())
                }
                ReaderTab::Markdown { attachment } => {
                    let id = LiveId::from_str(&format!("rinx.document.tab.{}", LiveId::unique().0));
                    self.tabs.push(WebTab {
                        id,
                        original_url: None,
                        article: None,
                        title: attachment.filename.clone(),
                        spawned: false,
                        location: Some(location.clone()),
                        status: Some(crate::i18n::tr("Loading file preview...").into()),
                    });
                    self.select_tab(cx, self.tabs.len() - 1);
                    let account = self.session_account.clone();
                    crate::sliding_sync::submit_async_request(
                        crate::sliding_sync::MatrixRequest::DownloadMedia {
                            media_source: attachment.media_source.clone(),
                            filename: attachment.filename.clone(),
                            on_download_result: Box::new(move |result| {
                                let result = match result {
                                    MediaDownloadResult::Downloaded(bytes) => {
                                        markdown_source(bytes)
                                    }
                                    MediaDownloadResult::Failed(error) => Err(error),
                                    MediaDownloadResult::Cancelled => {
                                        Err("Download cancelled".into())
                                    }
                                };
                                Cx::post_action(RestoredMarkdown {
                                    tab: id,
                                    account,
                                    result,
                                });
                            }),
                        },
                    );
                }
            }
            if index <= session.active {
                selected = self.tabs.len() - 1;
            }
        }
        self.select_tab(cx, selected);
        self.preserve_session = false;
        self.save_session();
    }

    fn finish_markdown_restore(&mut self, cx: &mut Cx, result: &RestoredMarkdown) {
        if result.account != crate::sliding_sync::current_user_id()
            || crate::logout::logout_state_machine::is_logout_in_progress()
        {
            return;
        }
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == result.tab) else {
            return;
        };
        match &result.result {
            Ok(source) => {
                let panel = Self::new_article_panel(cx);
                match panel
                    .as_article_panel()
                    .read_markdown(cx, &tab.title, source)
                {
                    Ok(()) => {
                        tab.article = Some(panel);
                        tab.status = None;
                    }
                    Err(error) => tab.status = Some(error),
                }
            }
            Err(error) => tab.status = Some(error.clone()),
        }
        self.attach_document(cx);
        self.update_header(cx);
        self.redraw(cx);
    }

    fn attach_document(&mut self, cx: &mut Cx) {
        let panel = self.active().and_then(|tab| tab.article.clone());
        if let Some(mut slot) = self.view(cx, ids!(native_page)).borrow_mut() {
            slot.children.clear();
            if let Some(panel) = panel {
                slot.children.push((id!(article_page), panel));
            }
            cx.widget_tree_mark_dirty(slot.widget_uid());
            slot.redraw(cx);
        }
    }

    fn select_tab(&mut self, cx: &mut Cx, index: usize) {
        if index >= self.tabs.len() || self.active_tab == Some(index) {
            return;
        }
        if let Some(id) = self.browser_id() {
            // Detach only the native view. Its document, history, form inputs,
            // and scroll position remain alive until the tab is closed.
            cx.system_browser(id).detach();
        }
        self.active_tab = Some(index);
        self.save_session();
        self.attach_document(cx);
        self.update_header(cx);
        self.tooltip(cx, ids!(web_tooltip)).hide(cx);
        cx.hide_text_ime();
        self.redraw(cx);
    }

    fn close_tab(&mut self, cx: &mut Cx, id: LiveId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(index);
        tab.close(cx);
        if self.tabs.is_empty() {
            self.active_tab = None;
            cx.action(WebBrowserAction::Close);
        } else if let Some(active) = self.active_tab {
            // Closing a background tab must leave the current page selected.
            self.active_tab = Some(if index < active {
                active - 1
            } else if index == active {
                index.min(self.tabs.len() - 1)
            } else {
                active
            });
            self.update_header(cx);
        }
        self.attach_document(cx);
        self.tooltip(cx, ids!(web_tooltip)).hide(cx);
        self.save_session();
        self.redraw(cx);
    }

    fn update_header(&mut self, cx: &mut Cx) {
        let Some(tab) = self.active() else { return };
        let url = tab.original_url.clone();
        let title = tab.title();
        let status = tab.status.clone();
        let is_web = url.is_some();
        for path in [
            ids!(web_back),
            ids!(web_forward),
            ids!(web_reopen),
            ids!(web_external),
        ] {
            self.widget(cx, path).set_visible(cx, is_web);
        }
        self.label(cx, ids!(web_status)).set_visible(cx, false);
        let Some(url) = url else {
            if let Some(status) = status {
                self.label(cx, ids!(web_status)).set_visible(cx, true);
                self.label(cx, ids!(web_status)).set_text(cx, &status);
            }
            self.label(cx, ids!(web_title)).set_text(cx, &title);
            self.label(cx, ids!(web_address))
                .set_text(cx, crate::i18n::tr("Read full article"));
            return;
        };
        self.label(cx, ids!(web_title))
            .set_text(cx, url.host_str().unwrap_or_default());
        // The pinned Apple backend does not report subsequent navigations.
        // Label this as the opened link, never as the current page's origin.
        self.label(cx, ids!(web_address)).set_text(
            cx,
            &crate::i18n::format("Opened link: {0}", &[("0", url.to_string())]),
        );
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
        {
            self.label(cx, ids!(web_status)).set_visible(cx, true);
            self.label(cx, ids!(web_status)).set_text(
                cx,
                crate::i18n::tr(
                    "In-app browsing is unavailable on this platform. Use Open in browser.",
                ),
            );
        }
    }
}

impl Widget for WebBrowser {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if std::mem::take(&mut self.restore_after_theme) {
            self.attach_document(cx);
            self.update_header(cx);
        }
        if matches!(event, Event::QuitRequested(_) | Event::Shutdown) {
            self.prepare_shutdown();
        }
        if matches!(event, Event::Shutdown) {
            self.close(cx);
            return;
        }
        if self.tabs.is_empty() {
            return;
        }
        let active = self.active().map(|tab| tab.id);
        let mut generated = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let close_active = generated.iter().any(|action| {
            matches!(
                action.downcast_ref::<ArticleAction>(),
                Some(ArticleAction::Close)
            )
        });
        generated.retain(|action| {
            !matches!(
                action.downcast_ref::<ArticleAction>(),
                Some(ArticleAction::Close)
            )
        });
        cx.extend_actions(generated);
        if close_active {
            if let Some(id) = active {
                self.close_tab(cx, id);
            }
        }
        let background: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| Some(tab.id) != active)
            .filter_map(|tab| tab.article.clone().map(|panel| (tab.id, panel)))
            .collect();
        for (id, panel) in background {
            let mut generated = cx.capture_actions(|cx| {
                panel
                    .as_article_panel()
                    .handle_background_event(cx, event, scope)
            });
            let close = generated.iter().any(|action| {
                matches!(
                    action.downcast_ref::<ArticleAction>(),
                    Some(ArticleAction::Close)
                )
            });
            generated.retain(|action| {
                !matches!(
                    action.downcast_ref::<ArticleAction>(),
                    Some(ArticleAction::Close)
                )
            });
            cx.extend_actions(generated);
            if close {
                self.close_tab(cx, id);
            }
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(result) = action.downcast_ref::<RestoredMarkdown>() {
                    self.finish_markdown_restore(cx, result);
                }
            }
            if self.active().is_some_and(|tab| tab.article.is_some()) {
                self.update_header(cx);
            }
            let tab_bar = self.tab_bar(cx, ids!(web_tabs));
            for action in actions {
                let Some(action) = action.as_widget_action() else {
                    continue;
                };
                if action.widget_uid != tab_bar.widget_uid() {
                    continue;
                }
                match action.cast() {
                    TabBarAction::TabWasPressed(id) => {
                        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
                            self.select_tab(cx, index);
                        }
                    }
                    TabBarAction::TabCloseWasPressed(id) => self.close_tab(cx, id),
                    _ => {}
                }
            }
            for (path, text) in [
                (ids!(web_close), "Close"),
                (ids!(web_back), "Back"),
                (ids!(web_forward), "Forward"),
                (ids!(web_reopen), "Reopen link"),
                (ids!(web_external), "Open in browser"),
            ] {
                let button = self.navigation_bar_button(cx, path);
                if let Some(action) = actions.find_widget_action(button.widget_uid()) {
                    match action.cast() {
                        NavigationBarButtonAction::HoverIn { widget_rect } => {
                            let pos = dvec2(
                                widget_rect.pos.x.max(8.0),
                                (widget_rect.pos.y - 26.0).max(0.0),
                            );
                            self.tooltip(cx, ids!(web_tooltip)).show_with_options(
                                cx,
                                pos,
                                crate::i18n::tr(text),
                            );
                        }
                        NavigationBarButtonAction::HoverOut => {
                            self.tooltip(cx, ids!(web_tooltip)).hide(cx)
                        }
                        _ => {}
                    }
                }
            }
            if self
                .navigation_bar_button(cx, ids!(web_close))
                .clicked(actions)
            {
                cx.action(WebBrowserAction::Close);
            }
            if self
                .navigation_bar_button(cx, ids!(web_back))
                .clicked(actions)
            {
                if let Some(id) = self.browser_id() {
                    cx.system_browser(id).history_go(-1);
                }
            }
            if self
                .navigation_bar_button(cx, ids!(web_forward))
                .clicked(actions)
            {
                if let Some(id) = self.browser_id() {
                    cx.system_browser(id).history_go(1);
                }
            }
            if self
                .navigation_bar_button(cx, ids!(web_reopen))
                .clicked(actions)
            {
                if let Some(tab) = self.active().filter(|tab| tab.spawned) {
                    // Apple's backend ignores set_url when it equals the last
                    // requested URL, even after a page navigates itself.
                    cx.system_browser(tab.id).close();
                    if let Some(url) = &tab.original_url {
                        cx.system_browser(tab.id).spawn_navigable(url.as_str());
                    }
                    self.redraw(cx);
                }
            }
            if self
                .navigation_bar_button(cx, ids!(web_external))
                .clicked(actions)
            {
                if let Some(tab) = self.active() {
                    if let Some(url) = &tab.original_url {
                        crate::utils::open_url(url.as_str());
                    }
                }
            }
        }
        if event.back_pressed()
            || matches!(
                event,
                Event::KeyUp(KeyEvent {
                    key_code: KeyCode::Escape,
                    ..
                })
            )
        {
            cx.action(WebBrowserAction::Close);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut bar) = item.as_tab_bar().borrow_mut() {
                let walk = bar.walk(cx);
                bar.begin(cx, self.active_tab, walk);
                for tab in &self.tabs {
                    let title = tab.title();
                    let label = shorten_title(&title);
                    bar.draw_tab(cx, tab.id, &label, id!(CloseableTab));
                }
                bar.end(cx);
            }
        }
        if let Some(id) = self.browser_id() {
            let area = self.view(cx, ids!(web_surface)).area();
            cx.system_browser(id).update(area, true);
        }
        DrawStep::done()
    }
}

impl WebBrowserRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &WebBrowserAction) {
        if !action.allowed() {
            return;
        }
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        if inner.tabs.is_empty()
            && !matches!(
                action,
                WebBrowserAction::Close | WebBrowserAction::Restore { .. }
            )
        {
            inner.session_account = crate::sliding_sync::current_user_id();
            inner.preserve_session = false;
        }
        match action {
            WebBrowserAction::Open(url) => {
                inner.open(cx, url.clone());
                modal.open(cx);
            }
            WebBrowserAction::Close => {
                inner.close(cx);
                modal.close(cx);
            }
            WebBrowserAction::OpenMarkdown {
                title,
                source,
                attachment,
                ..
            } => {
                let panel = WebBrowser::new_article_panel(cx);
                if let Err(error) = panel.as_article_panel().read_markdown(cx, title, source) {
                    crate::shared::popup_list::enqueue_popup_notification(
                        error,
                        crate::shared::popup_list::PopupKind::Error,
                        None,
                    );
                    if inner.tabs.is_empty() {
                        cx.action(WebBrowserAction::Close);
                    }
                    return;
                }
                inner.open_document(
                    cx,
                    panel,
                    title.clone(),
                    attachment
                        .clone()
                        .map(|attachment| ReaderTab::Markdown { attachment }),
                );
                modal.open(cx);
            }
            WebBrowserAction::ReadArticle { room, event } => {
                inner.open_article(cx, room.clone(), event.clone());
                modal.open(cx);
            }
            WebBrowserAction::Restore { session, .. } => {
                inner.restore(cx, session);
                if !inner.tabs.is_empty() {
                    modal.open(cx);
                }
            }
        }
    }

    pub fn session(&self) -> ReaderSession {
        self.borrow()
            .map(|inner| inner.session())
            .unwrap_or_default()
    }

    pub fn prepare_shutdown(&self) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.prepare_shutdown();
        }
    }

    pub fn browser_id(&self) -> Option<SystemBrowserId> {
        self.borrow().and_then(|inner| inner.browser_id())
    }
}

fn tab_label(url: &Url) -> String {
    let path = url.path().trim_end_matches('/');
    let label = format!("{}{}", url.host_str().unwrap_or("Web"), path);
    shorten_title(&label)
}

fn shorten_title(title: &str) -> String {
    if title.chars().count() > 32 {
        format!("{}…", title.chars().take(31).collect::<String>())
    } else {
        title.into()
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    fn browser(cx: &mut Cx) -> WebBrowserRef {
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            crate::app::register_widgets(vm);
            let value = script_eval!(vm, { mod.widgets.WebBrowser {} });
            WidgetRef::script_from_value(vm, value).as_web_browser()
        })
    }

    #[test]
    fn restore_keeps_order_and_selection_ignores_duplicates_and_unsafe_schemes() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let browser = browser(&mut cx);
        let action = WebBrowserAction::Restore {
            account: None,
            session: ReaderSession {
                active: 2,
                tabs: vec![
                    ReaderTab::Web {
                        url: "https://example.org/first".into(),
                    },
                    ReaderTab::Web {
                        url: "file:///tmp/private".into(),
                    },
                    ReaderTab::Web {
                        url: "https://example.org/second".into(),
                    },
                ],
            },
        };
        browser.action(&mut cx, ModalRef::default(), &action);
        browser.action(&mut cx, ModalRef::default(), &action);
        let session = browser.session();
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active, 1);
        assert!(matches!(&session.tabs[1], ReaderTab::Web { url } if url.ends_with("/second")));
        let first = browser.borrow().unwrap().tabs[0].id;
        browser.borrow_mut().unwrap().close_tab(&mut cx, first);
        assert_eq!(browser.session().active, 0);
        assert_eq!(browser.session().tabs.len(), 1);
    }

    #[test]
    fn another_accounts_restore_is_rejected() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let browser = browser(&mut cx);
        browser.action(
            &mut cx,
            ModalRef::default(),
            &WebBrowserAction::Restore {
                account: Some("@other:example.org".try_into().unwrap()),
                session: ReaderSession {
                    active: 0,
                    tabs: vec![ReaderTab::Web {
                        url: "https://example.org".into(),
                    }],
                },
            },
        );
        assert!(browser.session().tabs.is_empty());
    }

    #[test]
    fn late_markdown_download_cannot_reopen_a_closed_tab_or_switch_the_active_tab() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let browser = browser(&mut cx);
        let id = LiveId::unique();
        let mut inner = browser.borrow_mut().unwrap();
        inner.tabs.push(WebTab {
            id,
            original_url: None,
            article: None,
            title: "notes.md".into(),
            spawned: false,
            location: None,
            status: Some("Loading".into()),
        });
        inner.open(&mut cx, "https://example.org".parse().unwrap());
        let result = RestoredMarkdown {
            tab: id,
            account: None,
            result: Ok("# Notes\n\nRestored.".into()),
        };
        inner.finish_markdown_restore(&mut cx, &result);
        assert!(inner.tabs[0].article.is_some());
        assert_eq!(inner.active_tab, Some(1));
        inner.close_tab(&mut cx, id);
        inner.finish_markdown_restore(&mut cx, &result);
        assert_eq!(inner.tabs.len(), 1);
        assert!(inner.active().unwrap().original_url.is_some());
    }
}
