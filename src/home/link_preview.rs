//! A link preview widget that provides a method to populate link preview view for setting its' children.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use makepad_widgets::*;
use crate::{LivePtr, utils, widget_ref_from_live_ptr};
use matrix_sdk::ruma::{events::room::{ImageInfo, MediaSource}, OwnedMxcUri, UInt};
use serde::{Deserialize, Deserializer};
use url::Url;

use crate::{
    home::room_screen::TimelineUpdate,
    media_cache::MediaCache,
    shared::text_or_image::{TextOrImageRef, TextOrImageWidgetRefExt},
    sliding_sync::{submit_async_request, MatrixRequest, UrlPreviewError},
};

/// Maximum number of cache entries before cleanup is triggered
const MAX_CACHE_ENTRIES_BEFORE_CLEANUP: usize = 100;
/// Maximum age for cache entries in seconds (1 hour)
const CACHE_ENTRY_MAX_AGE_SECS: u64 = 3600;
/// How many previews we show before hiding the rest behind a "show more" button.
const MAX_DEFAULT_VISIBLE_PREVIEWS: usize = 2;

/// An entry in the Link Preview cache with timestamp for cleanup.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub struct TimestampedCacheEntry {
    pub entry: LinkPreviewCacheEntry,
    pub timestamp: Instant,
}

/// An entry in the Link Preview cache.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum LinkPreviewCacheEntry {
    Requested,
    LoadedLinkPreview(LinkPreviewData),
    Failed,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.LINK_PREVIEW_MESSAGE_TEXT_STYLE = RBX_TEXT_BODY {}

    mod.widgets.LinkPreview = #(LinkPreview::register_widget(vm)) {
        width: Fill{max: 440}, height: Fit,
        flow: Down,
        font_size: mod.widgets.MESSAGE_FONT_SIZE

        previews := View {
            width: Fill height: Fit flow: Down
        }

        collapsible_buttons := View {
            width: Fill, height: Fit,
            flow: Right,
            align: Align{x: 0.5, y: 0.5},
            padding: Inset{top: 4},
            visible: false,

            expand_button := RobrixIconButton {
                width: Fit, height: Fit,
                spacing: 4,
                padding: Inset{top: 4, bottom: 4, left: 8, right: 8},
                draw_icon +: {
                    svg: (ICON_TRIANGLE_DOWN)
                    color: mod.widgets.RINX_MUTED
                }
                icon_walk: Walk{width: 10, height: 10}
                draw_text +: {
                    text_style: mod.widgets.LINK_PREVIEW_MESSAGE_TEXT_STYLE {
                        font_size: (10.0 * mod.widgets.RINX_TEXT_SCALE),
                    },
                    color: mod.widgets.RINX_MUTED,
                    color_hover: mod.widgets.RINX_MUTED,
                    color_down: mod.widgets.RINX_MUTED,
                }
                draw_bg +: {
                    color: (COLOR_BG_PREVIEW)
                    color_hover: (COLOR_BG_PREVIEW_HOVER)
                    color_down: mod.widgets.RINX_PRESSED
                    border_size: 1.0
                    border_color: mod.widgets.RINX_BORDER
                    border_color_hover: mod.widgets.RINX_BORDER
                    border_color_down: mod.widgets.RINX_BORDER
                    border_radius: 4.0
                }
                text: #(crate::i18n::tr("Show more links")) i18n_text: "Show more links"
            }

            collapse_button := RobrixIconButton {
                visible: false,
                width: Fit, height: Fit,
                spacing: 4,
                padding: Inset{top: 4, bottom: 4, left: 8, right: 8},
                draw_icon +: {
                    svg: (ICON_TRIANGLE_UP)
                    color: mod.widgets.RINX_MUTED
                }
                icon_walk: Walk{width: 10, height: 10}
                draw_text +: {
                    text_style: mod.widgets.LINK_PREVIEW_MESSAGE_TEXT_STYLE {
                        font_size: (10.0 * mod.widgets.RINX_TEXT_SCALE),
                    },
                    color: mod.widgets.RINX_MUTED,
                    color_hover: mod.widgets.RINX_MUTED,
                    color_down: mod.widgets.RINX_MUTED,
                }
                draw_bg +: {
                    color: (COLOR_BG_PREVIEW)
                    color_hover: (COLOR_BG_PREVIEW_HOVER)
                    color_down: mod.widgets.RINX_PRESSED
                    border_size: 1.0
                    border_color: mod.widgets.RINX_BORDER
                    border_color_hover: mod.widgets.RINX_BORDER
                    border_color_down: mod.widgets.RINX_BORDER
                    border_radius: 4.0
                }
                text: #(crate::i18n::tr("Show fewer links")) i18n_text: "Show fewer links"
            }
        }

        preview_template: RoundedView {
            cursor: MouseCursor.Hand
            width: Fill height: Fit flow: Down spacing: 10
            margin: Inset{top: 8}
            padding: 12
            show_bg: true
            draw_bg +: {
                color: mod.widgets.RINX_SURFACE
                border_color: mod.widgets.RINX_BORDER
                border_size: 1.0
                border_radius: 8.0
            }
            summary := View {
                width: Fill height: Fit flow: Right spacing: 12
                content_view := View {
                    width: Fill height: Fit flow: Down spacing: 6
                    inner_content_view := View {
                        width: Fill height: Fit flow: Down
                        title_label := Label {
                            width: Fill height: Fit padding: 0
                            flow: Flow.Right{wrap: true} max_lines: 2 text_overflow: Ellipsis
                            draw_text +: {
                                text_style: RBX_TEXT_BODY_STRONG {}
                                color: mod.widgets.RINX_INK
                            }
                        }
                    }
                    description_label := Label {
                        width: Fill height: Fit padding: 0
                        flow: Flow.Right{wrap: true} max_lines: 3 text_overflow: Ellipsis
                        draw_text +: {
                            text_style: RBX_TEXT_BODY {}
                            color: mod.widgets.RINX_MUTED
                        }
                    }
                }
                image_view := View {
                    visible: false width: 72 height: 72
                    image := TextOrImage {
                        width: Fill height: Fill
                        image_view +: {
                            height: Fill
                            image +: {width: Fill height: Fill fit: ImageFit.CropToFill}
                        }
                    }
                }
            }
            footer := View {
                width: Fill height: Fit flow: Down spacing: 8
                SolidView {width: Fill height: 1 draw_bg.color: mod.widgets.RINX_BORDER}
                site_name_label := Label {
                    width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                    draw_text +: {text_style: RBX_TEXT_META {} color: mod.widgets.RINX_MUTED}
                }
            }
        }
    }
}

#[derive(Script, Widget)]
pub struct LinkPreview {
    #[deref]
    view: View,
    #[live]
    preview_template: Option<LivePtr>,
    #[live]
    font_size: f32,
    #[rust]
    children: Vec<ViewRef>,
    #[rust]
    is_expanded: bool,
    #[rust]
    num_hidden_links: usize,
    /// The links that were last populated in this widget, to avoid unnecessary repopulation.
    #[rust]
    last_populated_links: Vec<Url>,
}

impl ScriptHook for LinkPreview {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        apply: &Apply,
        scope: &mut Scope,
        _value: ScriptValue,
    ) {
        if apply.is_script_reapply() {
            if let Some(template) = self.preview_template {
                for child in &mut self.children {
                    let labels: Vec<_> = [
                        ids!(title_label),
                        ids!(description_label),
                        ids!(site_name_label),
                    ]
                    .into_iter()
                    .map(|id| {
                        (
                            id,
                            child.label(vm.cx_mut(), id).text(),
                            child.label(vm.cx_mut(), id).visible(),
                        )
                    })
                    .collect();
                    let visible: Vec<_> = [ids!(image_view), ids!(footer)]
                        .into_iter()
                        .map(|id| (id, child.view(vm.cx_mut(), id).visible()))
                        .collect();
                    child.script_apply(vm, apply, scope, template);
                    for (id, text, visible) in labels {
                        let label = child.label(vm.cx_mut(), id);
                        label.set_text(vm.cx_mut(), &text);
                        label.set_visible(vm.cx_mut(), visible);
                    }
                    for (id, visible) in visible {
                        child
                            .view(vm.cx_mut(), id)
                            .set_visible(vm.cx_mut(), visible);
                    }
                }
            }
            self.update_button_and_visibility(vm.cx_mut());
        }
    }
}

impl Widget for LinkPreview {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // clear hovers on every link preview card and the show more/fewer buttons
        if let Event::ClearHover = event {
            for item in self.children.iter() {
                reset_hover(cx, item);
            }
            self.view.button(cx, ids!(collapsible_buttons.expand_button)).reset_hover(cx);
            self.view.button(cx, ids!(collapsible_buttons.collapse_button)).reset_hover(cx);
        }

        // Handle collapsible button clicks
        if let Event::Actions(actions) = event {
            let expand_btn = self.view.button(cx, ids!(collapsible_buttons.expand_button));
            let collapse_btn = self.view.button(cx, ids!(collapsible_buttons.collapse_button));
            if expand_btn.clicked(actions) || collapse_btn.clicked(actions) {
                self.is_expanded = !self.is_expanded;
                self.update_button_and_visibility(cx);
                cx.redraw_all();
            }
        }

        for (index, view) in self.children.iter().enumerate().filter(|(_, view)| view.visible()) {
            match event.hits(cx, view.area()) {
                Hit::FingerHoverIn(_) | Hit::FingerDown(_) => {
                    let mut view = view.clone();
                    script_apply_eval!(cx, view, {
                        draw_bg.color: mod.widgets.RINX_HOVER
                    });
                }
                Hit::FingerHoverOut(_) => {
                    reset_hover(cx, view);
                }
                Hit::FingerUp(fe) => {
                    // return to normal bg color
                    reset_hover(cx, view);
                    if fe.is_over && fe.is_primary_hit() && fe.was_tap() {
                        if let Some(link) = self.last_populated_links.get(index) {
                            cx.widget_action(
                                view.widget_uid(),
                                HtmlLinkAction::Clicked {
                                    url: link.to_string(),
                                    key_modifiers: fe.modifiers,
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Previews are instantiated outside the message's widget tree. Carry its
        // body size explicitly, including mobile overrides and live theme reloads.
        for child in &self.children {
            for path in [ids!(title_label), ids!(description_label)] {
                if let Some(mut label) = child.label(cx, path).borrow_mut() {
                    label.draw_text.text_style.font_size = self.font_size;
                }
            }
        }
        self.view.draw_walk(cx, scope, walk)
    }
}

impl LinkPreview {
    fn update_button_and_visibility(&mut self, cx: &mut Cx) {
        for (index, child) in self.children.iter().enumerate() {
            child.set_visible(cx, self.is_expanded || index < MAX_DEFAULT_VISIBLE_PREVIEWS);
        }
        if self.num_hidden_links > 0 {
            self.view.view(cx, ids!(collapsible_buttons)).set_visible(cx, true);
            let expand_btn = self.view.button(cx, ids!(collapsible_buttons.expand_button));
            let collapse_btn = self.view.button(cx, ids!(collapsible_buttons.collapse_button));
            if self.is_expanded {
                expand_btn.set_visible(cx, false);
                collapse_btn.set_visible(cx, true);
            } else {
                expand_btn.set_text(cx, &format!("Show {} more links", self.num_hidden_links));
                expand_btn.set_visible(cx, true);
                collapse_btn.set_visible(cx, false);
            }
            expand_btn.reset_hover(cx);
            collapse_btn.reset_hover(cx);
        } else {
            self.view.view(cx, ids!(collapsible_buttons)).set_visible(cx, false);
        }
    }

    fn sync_children(&mut self, cx: &mut Cx) {
        // Cards must belong to the view's layout and widget tree. Drawing them
        // before the view left its area covering only the expand/collapse button.
        if let Some(mut previews) = self.view.view(cx, ids!(previews)).borrow_mut() {
            previews.children = self.children.iter().enumerate()
                .map(|(index, child)| (LiveId::from_num(id!(preview).0, index as u64), WidgetRef::clone(child)))
                .collect();
            cx.widget_tree_mark_dirty(previews.widget_uid());
            previews.redraw(cx);
        }
    }
}

impl LinkPreviewRef {
    /// Clears any displayed link preview(s), resetting this widget to its empty state.
    ///
    /// Needed for messages that never show link previews (e.g. redacted messages).
    pub fn clear(&mut self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.children.clear();
            inner.sync_children(cx);
            inner.last_populated_links.clear();
            inner.is_expanded = false;
            inner.num_hidden_links = 0;
            inner.update_button_and_visibility(cx);
            inner.redraw(cx);
        }
    }

    /// Populates the link previews below a message.
    ///
    /// Returns `true` if every preview was fully drawn.
    pub fn populate_below_message<F>(
        &mut self,
        cx: &mut Cx,
        links: &[url::Url],
        media_cache: &mut MediaCache,
        link_preview_cache: &mut LinkPreviewCache,
        populate_image_fn: &F,
    ) -> bool
    where
        F: Fn(&mut Cx, &TextOrImageRef, Option<&ImageInfo>, MediaSource, &str, &mut MediaCache) -> bool,
    {
        let accepted_links = previewable_links(links);

        let did_links_change = match self.borrow() {
            Some(inner) => inner.last_populated_links != accepted_links,
            None => return true,
        };
        if did_links_change {
            if let Some(mut inner) = self.borrow_mut() {
                let num_links = accepted_links.len();
                // Reuse as many old link preview child instances as we can.
                inner.children.truncate(num_links);
                while inner.children.len() < num_links {
                    let child = widget_ref_from_live_ptr(cx, inner.preview_template).as_view();
                    inner.children.push(child);
                }
                // Reset each preview's per-link visual state: a reused one keeps its old
                // hover color and image, and a new one shows TextOrImage's default view.
                for item in inner.children.iter() {
                    reset_hover(cx, item);
                    item.text_or_image(cx, ids!(image)).clear(cx);
                }
                inner.last_populated_links = accepted_links;
                inner.is_expanded = false;
                inner.num_hidden_links = num_links.saturating_sub(MAX_DEFAULT_VISIBLE_PREVIEWS);
                inner.sync_children(cx);
                inner.update_button_and_visibility(cx);
            }
        }

        let Some(inner) = self.borrow() else { return true };
        let mut all_drawn = true;
        for (view, link) in inner.children.iter().zip(inner.last_populated_links.iter()) {
            let entry = link_preview_cache.get_or_fetch_link_preview(link.as_str());
            all_drawn &= populate_preview_item(cx, view, entry, link, media_cache, populate_image_fn);
        }
        all_drawn
    }
}

fn previewable_links(links: &[Url]) -> Vec<Url> {
    let mut accepted = Vec::new();
    for link in links {
        if !matches!(link.scheme(), "http" | "https") || link.host_str().is_none()
            || link.host_str().is_some_and(|host|
                ["matrix.to", "matrix.io"].iter().any(|skip|
                    host == *skip || host.strip_suffix(skip).is_some_and(|prefix| prefix.ends_with('.'))
                )
            )
        {
            continue;
        }
        if !accepted.contains(link) {
            accepted.push(link.clone());
        }
    }
    accepted
}

fn reset_hover(cx: &mut Cx, item: &ViewRef) {
    let mut item = item.clone();
    script_apply_eval!(cx, item, {
        draw_bg.color: mod.widgets.RINX_SURFACE
    });
}

/// Populates a single link preview with whatever metadata has been fetched so far.
///
/// Returns `true` if the link preview was fully drawn.
fn populate_preview_item<F>(
    cx: &mut Cx,
    view: &ViewRef,
    entry: LinkPreviewCacheEntry,
    link: &Url,
    media_cache: &mut MediaCache,
    populate_image_fn: &F,
) -> bool
where
    F: Fn(&mut Cx, &TextOrImageRef, Option<&ImageInfo>, MediaSource, &str, &mut MediaCache) -> bool,
{
    let title_link = view.label(cx, ids!(title_label));
    let site_name_label = view.label(cx, ids!(site_name_label));
    let description_label = view.label(cx, ids!(description_label));
    let image_view = view.view(cx, ids!(image_view));
    let text_or_image = view.text_or_image(cx, ids!(image));

    let host = link.host_str().unwrap_or_default();
    let footer = view.view(cx, ids!(footer));
    let description = |value: &str| utils::replace_linebreaks_separators(value, false).trim().to_owned();
    let data = match entry {
        LinkPreviewCacheEntry::LoadedLinkPreview(data) => data,
        LinkPreviewCacheEntry::Requested | LinkPreviewCacheEntry::Failed => {
            title_link.set_text(cx, host);
            description_label.set_text(cx, if matches!(entry, LinkPreviewCacheEntry::Requested) {
                crate::i18n::tr("Loading preview…")
            } else {
                crate::i18n::tr("Open webpage")
            });
            description_label.set_visible(cx, true);
            footer.set_visible(cx, false);
            image_view.set_visible(cx, false);
            return matches!(entry, LinkPreviewCacheEntry::Failed);
        }
    };

    let title = data.title.as_deref().map(description).filter(|s| !s.is_empty());
    title_link.set_text(cx, title.as_deref().unwrap_or(host));
    let body = data.description.as_deref().map(description).filter(|s| !s.is_empty());
    description_label.set_text(cx, body.as_deref().unwrap_or(""));
    description_label.set_visible(cx, body.is_some());
    let site = data.site_name.as_deref().map(description).filter(|s| !s.is_empty());
    site_name_label.set_text(cx, &match site {
        Some(site) if !site.eq_ignore_ascii_case(host) => format!("{site} · {host}"),
        _ => host.to_owned(),
    });
    footer.set_visible(cx, title.is_some() || body.is_some() || data.image.is_some());

    let Some(image) = &data.image else {
        image_view.set_visible(cx, false);
        return true;
    };
    let mut image_info = ImageInfo::default();
    image_info.mimetype = data.image_type.clone();
    image_info.size = data.image_size;
    let source = MediaSource::Plain(OwnedMxcUri::from(image.clone()));
    let fully_drawn = populate_image_fn(cx, &text_or_image, Some(&image_info), source, "", media_cache);
    // Only show the image area once there's an actual image in it. Anything else means
    // it's still loading or couldn't be fetched, and an error message doesn't belong here.
    image_view.set_visible(cx, text_or_image.status().is_image());
    fully_drawn
}

/// The data structure from the link preview API, "/_matrix/client/v1/media/preview_url"
#[derive(Clone, Debug, Deserialize, Default)]
pub struct LinkPreviewData {
    #[serde(rename = "og:description")]
    pub description: Option<String>,
    /// The size of the image in bytes, if available
    #[serde(rename = "matrix:image:size", default, deserialize_with = "deserialize_lenient_uint")]
    pub image_size: Option<UInt>,
    /// The URL of the image
    #[serde(rename = "og:image")]
    pub image: Option<String>,
    /// The type of the image
    #[serde(rename = "og:image:type")]
    pub image_type: Option<String>,
    /// The name of the site
    #[serde(rename = "og:site_name")]
    pub site_name: Option<String>,
    /// The URL of the site
    #[serde(rename = "og:url")]
    pub url: Option<String>,
    /// The title of the site
    #[serde(rename = "og:title")]
    pub title: Option<String>,
}

/// Deserializes an optional [`UInt`] that is either a JSON number or a JSON string.
///
/// Some homeservers encode the numeric preview fields as strings, so we handle both.
fn deserialize_lenient_uint<'de, D>(deserializer: D) -> Result<Option<UInt>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(UInt),
        String(String),
    }
    Ok(match Option::<NumberOrString>::deserialize(deserializer)? {
        Some(NumberOrString::Number(n)) => Some(n),
        Some(NumberOrString::String(s)) => s.parse::<u64>().ok().and_then(|n| UInt::try_from(n).ok()),
        None => None,
    })
}

/// The cache for link previews, specific to a given timeline.
pub struct LinkPreviewCache {
    /// The actual cached data.
    cache: BTreeMap<String, Arc<Mutex<TimestampedCacheEntry>>>,
    /// A channel to send updates to a particular timeline when a link preview request has completed.
    timeline_update_sender: Option<crossbeam_channel::Sender<TimelineUpdate>>,
}

impl LinkPreviewCache {
    /// Creates a new link preview cache that will optionally send updates
    /// when a link preview request has completed.
    pub const fn new(
        timeline_update_sender: Option<crossbeam_channel::Sender<TimelineUpdate>>,
    ) -> Self {
        Self {
            cache: BTreeMap::new(),
            timeline_update_sender,
        }
    }

    /// Cache metadata that has already been fetched, without requesting it again.
    pub fn insert(&mut self, url: &Url, data: LinkPreviewData) {
        self.cache.insert(url.to_string(), Arc::new(Mutex::new(TimestampedCacheEntry {
            entry: LinkPreviewCacheEntry::LoadedLinkPreview(data),
            timestamp: Instant::now(),
        })));
    }

    /// Sets a new timeline update sender, e.g., for when the backend re-created this room's timeline.
    pub fn set_timeline_update_sender(
        &mut self,
        sender: crossbeam_channel::Sender<TimelineUpdate>,
    ) {
        self.timeline_update_sender = Some(sender);
    }

    /// Fetches the link preview for the specified URL.
    pub fn get_or_fetch_link_preview(&mut self, url: &str) -> LinkPreviewCacheEntry {
        // Clean up old entries periodically
        if self.cache.len() > MAX_CACHE_ENTRIES_BEFORE_CLEANUP {
            self.cleanup_old_entries(Duration::from_secs(CACHE_ENTRY_MAX_AGE_SECS));
        }

        if let Some(entry) = self.cache.get(url) {
            return entry.lock().unwrap().entry.clone();
        }
        let entry_ref = Arc::new(Mutex::new(TimestampedCacheEntry {
            entry: LinkPreviewCacheEntry::Requested,
            timestamp: Instant::now(),
        }));
        self.cache.insert(url.to_owned(), entry_ref.clone());
        submit_async_request(MatrixRequest::GetUrlPreview {
            url: url.to_owned(),
            on_fetched: insert_into_cache,
            destination: entry_ref,
            update_sender: self.timeline_update_sender.clone(),
        });
        LinkPreviewCacheEntry::Requested
    }

    /// Removes all `Requested` and `Failed` entries from the link preview cache,
    /// allowing them to be re-fetched.
    ///
    /// This should be called when the app transitions from offline back to online,
    /// because any in-flight requests that were submitted while offline have likely
    /// failed, leaving stale entries that permanently block re-fetching.
    pub fn clear_all_pending_and_failed_requests(&mut self) {
        self.cache.retain(|_, entry| {
            if let Ok(guard) = entry.lock() {
                matches!(guard.entry, LinkPreviewCacheEntry::LoadedLinkPreview(_))
            } else {
                true // Keep entries we can't lock
            }
        });
    }

    /// Removes cache entries older than the specified duration
    pub fn cleanup_old_entries(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.cache.retain(|_url, entry| {
            if let Ok(timestamped_entry) = entry.lock() {
                now.duration_since(timestamped_entry.timestamp) < max_age
            } else {
                true // Keep entries we can't lock
            }
        });
    }
}

/// Insert data into a previously-requested media cache entry.
fn insert_into_cache(
    value_ref: Arc<Mutex<TimestampedCacheEntry>>,
    data: Result<LinkPreviewData, UrlPreviewError>,
    update_sender: Option<crossbeam_channel::Sender<TimelineUpdate>>,
) {
    let new_entry = match data {
        Ok(data) => LinkPreviewCacheEntry::LoadedLinkPreview(data),
        Err(e) => {
            error!("Homeserver link preview failed: {e}");
            LinkPreviewCacheEntry::Failed
        }
    };

    if let Ok(mut timestamped_entry) = value_ref.lock() {
        timestamped_entry.entry = new_entry;
        timestamped_entry.timestamp = Instant::now();
    }

    if let Some(sender) = update_sender {
        // Reuse TimelineUpdate MediaFetched to trigger redraw in the timeline.
        let _ = sender.send(TimelineUpdate::LinkPreviewFetched);
    }
    SignalToUI::set_ui_signal();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_urls_preserve_order_and_only_skip_actual_matrix_domains() {
        let links: Vec<Url> = [
            "https://example.org/article", "mailto:reader@example.org", "ftp://example.org/file",
            "https://matrix.to/#/@reader:example.org", "https://sub.matrix.io/room",
            "http://example.org/page", "https://example.org/article", "https://notmatrix.to/article",
            "matrix:u/reader:example.org", "file:///tmp/article.html",
        ].into_iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(previewable_links(&links).iter().map(Url::as_str).collect::<Vec<_>>(), vec![
            "https://example.org/article", "http://example.org/page", "https://notmatrix.to/article",
        ]);
    }

    #[test]
    fn homeserver_metadata_accepts_string_image_sizes_and_missing_fields() {
        let data: LinkPreviewData = serde_json::from_value(serde_json::json!({
            "og:title": "An article", "og:description": "中文 description",
            "og:image": "mxc://example.org/image", "matrix:image:size": "1234",
        })).unwrap();
        assert_eq!(data.title.as_deref(), Some("An article"));
        assert_eq!(data.description.as_deref(), Some("中文 description"));
        assert_eq!(data.image_size.map(u64::from), Some(1234));
        let empty: LinkPreviewData = serde_json::from_str("{}").unwrap();
        assert!(empty.title.is_none() && empty.image.is_none());
    }
}
