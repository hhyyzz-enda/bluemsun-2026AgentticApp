//! Room-scoped search and attachment browsing over SDK-decrypted history.
use std::collections::{BTreeMap, BTreeSet};
use futures_util::future::{AbortHandle, Abortable};
use makepad_widgets::*;
use matrix_sdk::{deserialized_responses::TimelineEvent, media::MediaFormat, room::MessagesOptions};
use ruma::{
    OwnedEventId, OwnedUserId,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use crate::{
    media_cache::{MediaCache, MediaCacheEntry},
    shared::{
        attachment_download::{
            DownloadKind, DownloadableAttachment, media_source_mxc, start_attachment_download, start_attachment_share,
        },
        navigation_bar_button::NavigationBarButtonAction,
        text_or_image::{TextOrImageAction, TextOrImageWidgetRefExt, TextOrImageWidgetExt},
    },
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};
use super::back_swipe::BackSwipe;

const CAPTION_PADDING: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryFilter {
    #[default]
    Messages,
    Media,
    Files,
    Audio,
}

#[derive(Clone, Debug)]
pub enum RoomHistoryAction {
    Open {
        room: RoomNameId,
        filter: HistoryFilter,
    },
    Jump {
        room: RoomNameId,
        event: OwnedEventId,
    },
    Close,
}

#[derive(Clone, Debug)]
struct HistoryMessage {
    id: OwnedEventId,
    sender: OwnedUserId,
    name: String,
    timestamp: u64,
    content: MessageType,
}
impl HistoryMessage {
    fn attachment(&self) -> Option<DownloadableAttachment> {
        let (source, name, size, kind) = match &self.content {
            MessageType::Image(m) => (
                &m.source,
                m.filename(),
                m.info.as_ref().and_then(|i| i.size),
                DownloadKind::Image,
            ),
            MessageType::Video(m) => (
                &m.source,
                m.filename(),
                m.info.as_ref().and_then(|i| i.size),
                DownloadKind::Video,
            ),
            MessageType::File(m) => (
                &m.source,
                m.filename(),
                m.info.as_ref().and_then(|i| i.size),
                DownloadKind::File,
            ),
            MessageType::Audio(m) => (
                &m.source,
                m.filename(),
                m.info.as_ref().and_then(|i| i.size),
                DownloadKind::Audio,
            ),
            _ => return None,
        };
        Some(DownloadableAttachment {
            media_source: source.clone(),
            filename: name.into(),
            size: size.map(u64::from),
            kind,
        })
    }
    fn matches(&self, query: &str, filter: HistoryFilter) -> bool {
        let kind_matches = match filter {
            HistoryFilter::Messages => true,
            HistoryFilter::Media => {
                matches!(self.content, MessageType::Image(_) | MessageType::Video(_))
            }
            HistoryFilter::Files => matches!(self.content, MessageType::File(_)),
            HistoryFilter::Audio => matches!(self.content, MessageType::Audio(_)),
        };
        kind_matches
            && (self.content.body().to_lowercase().contains(query)
                || self.name.to_lowercase().contains(query)
                || self
                    .attachment()
                    .is_some_and(|a| a.filename.to_lowercase().contains(query)))
    }
    fn metadata(&self) -> String {
        let date =
            chrono::DateTime::from_timestamp_millis(self.timestamp.min(i64::MAX as u64) as i64)
                .map(|d| {
                    d.with_timezone(&chrono::Local)
                        .format("%Y/%m/%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
        let kind = match self.content {
            MessageType::Image(_) => crate::i18n::tr("Photo"),
            MessageType::Video(_) => crate::i18n::tr("Video"),
            MessageType::File(_) => crate::i18n::tr("File"),
            MessageType::Audio(_) => crate::i18n::tr("Audio"),
            _ => crate::i18n::tr("Message"),
        };
        format!("{} · {date} · {kind}", self.name)
    }
}

#[derive(Default)]
struct HistoryIndex {
    messages: BTreeMap<OwnedEventId, HistoryMessage>,
    edits: BTreeMap<OwnedEventId, Vec<HistoryMessage>>,
    redacted: BTreeSet<OwnedEventId>,
    seen: BTreeSet<OwnedEventId>,
    locked: usize,
}
impl HistoryIndex {
    fn insert(&mut self, event: &TimelineEvent, names: &BTreeMap<OwnedUserId, String>) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(event.kind.raw().json().get())
        else {
            return;
        };
        self.insert_value(value, names, event.kind.is_utd());
    }
    fn insert_value(
        &mut self,
        value: serde_json::Value,
        names: &BTreeMap<OwnedUserId, String>,
        locked: bool,
    ) {
        let Some(id) = value
            .get("event_id")
            .and_then(|v| v.as_str())
            .and_then(|s| OwnedEventId::try_from(s).ok())
        else {
            return;
        };
        if !self.seen.insert(id.clone()) {
            return;
        }
        if locked {
            self.locked += 1;
            return;
        }
        if value.pointer("/unsigned/redacted_because").is_some() {
            self.redacted.insert(id);
            return;
        }
        if value["type"] == "m.room.redaction" {
            if let Some(target) = value
                .get("redacts")
                .or_else(|| value.pointer("/content/redacts"))
                .and_then(|v| v.as_str())
                .and_then(|s| OwnedEventId::try_from(s).ok())
            {
                self.redacted.insert(target);
            }
            return;
        }
        if value["type"] != "m.room.message" {
            return;
        }
        let Some(sender) = value
            .get("sender")
            .and_then(|v| v.as_str())
            .and_then(|s| OwnedUserId::try_from(s).ok())
        else {
            return;
        };
        let Some(timestamp) = value.get("origin_server_ts").and_then(|v| v.as_u64()) else {
            return;
        };
        let content = &value["content"];
        let replacement = (content
            .pointer("/m.relates_to/rel_type")
            .and_then(|v| v.as_str())
            == Some("m.replace"))
        .then(|| {
            content
                .pointer("/m.relates_to/event_id")
                .and_then(|v| v.as_str())
                .and_then(|s| OwnedEventId::try_from(s).ok())
        })
        .flatten();
        let body = if replacement.is_some() {
            &content["m.new_content"]
        } else {
            content
        };
        let Ok(content) = serde_json::from_value::<RoomMessageEventContent>(body.clone()) else {
            return;
        };
        let message = HistoryMessage {
            id: id.clone(),
            name: names
                .get(&sender)
                .cloned()
                .unwrap_or_else(|| sender.to_string()),
            sender,
            timestamp,
            content: content.msgtype,
        };
        if let Some(target) = replacement {
            self.edits.entry(target).or_default().push(message);
        } else {
            self.messages.insert(id, message);
        }
    }
    fn results(
        &self,
        query: &str,
        filter: HistoryFilter,
        cutoff: Option<u64>,
    ) -> Vec<HistoryMessage> {
        let query = query.trim().to_lowercase();
        let mut result = Vec::new();
        for message in self.messages.values() {
            if self.redacted.contains(&message.id) || cutoff.is_some_and(|t| message.timestamp <= t)
            {
                continue;
            }
            let mut message = message.clone();
            if let Some(edit) = self.edits.get(&message.id).and_then(|edits| {
                edits
                    .iter()
                    .filter(|e| e.sender == message.sender && !self.redacted.contains(&e.id))
                    .max_by_key(|e| e.timestamp)
            }) {
                message.content = edit.content.clone();
            }
            if message.matches(&query, filter) {
                result.push(message);
            }
        }
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
        result
    }
}

#[derive(Debug)]
struct HistoryPage {
    owner: OwnedUserId,
    request: u64,
    result: Result<
        (
            Vec<TimelineEvent>,
            BTreeMap<OwnedUserId, String>,
            Option<String>,
            bool,
        ),
        String,
    >,
    last: bool,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.RoomHistoryPanel = #(RoomHistoryPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down draw_bg.color: mod.widgets.RINX_PAGE
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(CAPTION_PADDING) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        header := DetailHeader {title.text: #(crate::i18n::tr("Search Chat History")) title.i18n_text: "Search Chat History"}
        room_name := Label {width: Fill height: Fit margin: 12 max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
        browser := View {
            width: Fill height: Fill flow: Down spacing: 8
            search_bar := View {width: Fill height: 40 flow: Right margin: Inset{left: 12 right: 12} spacing: 8
                history_query := TextInput {width: Fill height: Fill empty_text: #(crate::i18n::tr("Search messages or file names")) i18n_empty_text: "Search messages or file names"}
                search_history := RobrixNeutralIconButton {text: #(crate::i18n::tr("Search")) i18n_text: "Search" width: 64 height: Fill spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            }
            filters := View {width: Fill height: 36 flow: Right margin: Inset{left: 12 right: 12} spacing: 4
                all_filter := RobrixNeutralIconButton {text: #(crate::i18n::tr("All")) i18n_text: "All" width: Fill height: Fill spacing: 0 icon_walk: Walk{width: 0 height: 0}}
                media_filter := RobrixNeutralIconButton {text: #(crate::i18n::tr("Media")) i18n_text: "Media" width: Fill height: Fill spacing: 0 icon_walk: Walk{width: 0 height: 0}}
                file_filter := RobrixNeutralIconButton {text: #(crate::i18n::tr("Files")) i18n_text: "Files" width: Fill height: Fill spacing: 0 icon_walk: Walk{width: 0 height: 0}}
                audio_filter := RobrixNeutralIconButton {text: #(crate::i18n::tr("Audio")) i18n_text: "Audio" width: Fill height: Fill spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            }
            history_status := Label {width: Fill height: Fit margin: Inset{left: 12 right: 12} flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
            results := PortalList {
                width: Fill height: Fill
                Entry := NavigationBarButton {
                    width: Fill height: 96 flow: Right padding: 12 spacing: 12 align: Align{y: 0.5}
                    draw_bg +: {color_hover: mod.widgets.RINX_SURFACE color_active: mod.widgets.RINX_SELECTED border_radius: 0}
                    thumbnail := TextOrImage {visible: false width: 64 height: 64
                        image_view +: {height: Fill image +: {height: Fill fit: ImageFit.Smallest}}
                        text_view +: {height: Fill label +: {max_lines: 2 draw_text.text_style.font_size: (9 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                    View {width: Fill height: Fit flow: Down spacing: 8
                        metadata := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (9 * mod.widgets.RINX_TEXT_SCALE)}}}
                        body := Label {width: Fill max_lines: 2 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
                    }
                }
            }
            history_more := RobrixNeutralIconButton {text: #(crate::i18n::tr("Load Older")) i18n_text: "Load Older" width: Fill height: 42 margin: 12 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
        }
        detail := ScrollYView {
            visible: false width: Fill height: Fill flow: Down padding: 16 spacing: 16
            detail_metadata := Label {width: Fill flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
            preview := TextOrImage {visible: false width: Fill height: 260
                image_view +: {height: Fill image +: {height: Fill fit: ImageFit.Smallest}}
            }
            detail_body := Label {width: Fill flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
            detail_hint := Label {width: Fill flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
            view_in_chat := RobrixPositiveIconButton {text: #(crate::i18n::tr("View in Chat")) i18n_text: "View in Chat" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            post_text_to_moments := RobrixNeutralIconButton {text: #(crate::i18n::tr("Post to Moments")) i18n_text: "Post to Moments" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            attachment_actions := View {width: Fill height: Fit flow: Right spacing: 12
                download_attachment := RobrixNeutralIconButton {text: #(crate::i18n::tr("Download")) i18n_text: "Download" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
                share_attachment := RobrixNeutralIconButton {text: #(crate::i18n::tr("Share")) i18n_text: "Share" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct RoomHistoryPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    room: Option<RoomNameId>,
    #[rust]
    filter: HistoryFilter,
    #[rust]
    index: HistoryIndex,
    #[rust]
    matches: Vec<HistoryMessage>,
    #[rust]
    detail: Option<HistoryMessage>,
    #[rust]
    cursor: Option<String>,
    #[rust]
    exhausted: bool,
    #[rust]
    busy: bool,
    #[rust]
    request: u64,
    #[rust]
    abort: Option<AbortHandle>,
    #[rust]
    error: Option<String>,
    #[rust]
    media: Option<MediaCache>,
    #[rust]
    back_swipe: BackSwipe,
}

impl RoomHistoryPanel {
    fn cancel(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
        self.busy = false;
        self.request = next_request();
    }
    fn reset(&mut self) {
        self.cancel();
        self.owner = None;
        self.room = None;
        self.index = HistoryIndex::default();
        self.matches.clear();
        self.detail = None;
        self.cursor = None;
        self.exhausted = false;
        self.error = None;
        self.media = None;
        self.back_swipe = Default::default();
    }
    fn load(&mut self, cx: &mut Cx, pages: usize) {
        if self.busy || self.exhausted {
            return;
        }
        let Some(client) = get_client() else { return };
        let Some(owner) = self.owner.clone().filter(|o| client.user_id() == Some(o)) else {
            return;
        };
        let Some(room) = self
            .room
            .as_ref()
            .and_then(|r| client.get_room(r.room_id()))
        else {
            return;
        };
        self.error = None;
        self.busy = true;
        self.request = next_request();
        let request = self.request;
        let mut cursor = self.cursor.clone();
        let (abort, registration) = AbortHandle::new_pair();
        self.abort = Some(abort);
        spawn_async_task(async move {
            let _ = Abortable::new(
                async move {
                    for page in 0..pages {
                        if current_user_id().as_ref() != Some(&owner) {
                            break;
                        }
                        let mut options = MessagesOptions::backward();
                        options.from = cursor.clone();
                        options.limit = ruma::uint!(100);
                        match room.messages(options).await {
                            Ok(response) => {
                                let done = response.end.is_none()
                                    || response.end == cursor
                                    || response.chunk.is_empty();
                                cursor = response.end;
                                let senders: BTreeSet<_> = response
                                    .chunk
                                    .iter()
                                    .filter_map(|e| e.kind.parse_sender())
                                    .collect();
                                let mut names = BTreeMap::new();
                                for sender in senders {
                                    if let Ok(Some(member)) = room.get_member_no_sync(&sender).await
                                    {
                                        if let Some(name) = member.display_name() {
                                            names.insert(sender, name.to_owned());
                                        }
                                    }
                                }
                                Cx::post_action(HistoryPage {
                                    owner: owner.clone(),
                                    request,
                                    result: Ok((response.chunk, names, cursor.clone(), done)),
                                    last: done || page + 1 == pages,
                                });
                                if done {
                                    break;
                                }
                            }
                            Err(error) => {
                                Cx::post_action(HistoryPage {
                                    owner: owner.clone(),
                                    request,
                                    result: Err(error.to_string()),
                                    last: true,
                                });
                                break;
                            }
                        }
                    }
                },
                registration,
            )
            .await;
        });
        self.refresh(cx, false);
    }
    fn refresh(&mut self, cx: &mut Cx, scroll_top: bool) {
        let cutoff = self
            .room
            .as_ref()
            .and_then(|r| super::chat_actions::cleared_through(r.room_id()));
        self.matches = self.index.results(
            &self.text_input(cx, ids!(history_query)).text(),
            self.filter,
            cutoff,
        );
        if scroll_top {
            self.portal_list(cx, ids!(results))
                .set_first_id_and_scroll(0, 0.0);
        }
        let status = if let Some(error) = &self.error {
            crate::i18n::format("Could not load history: {error}. Retry to continue.", &[("error", (error).to_string())])
        } else {
            crate::i18n::format("{0} result{1} · {2} messages searched. {3}{4}", &[("0", (self.matches.len()).to_string()), ("1", (crate::i18n::plural_suffix(self.matches.len())).to_string()), ("2", (self.index.messages.len()).to_string()), ("3", (if self.busy {
                    crate::i18n::tr("Searching history…")
                } else if self.exhausted {
                    crate::i18n::tr("All available history checked.")
                } else {
                    crate::i18n::tr("Older history is available.")
                }).to_string()), ("4", (if self.index.locked > 0 {
                    crate::i18n::format(" {0} encrypted messages are unavailable on this device.", &[("0", (self.index.locked).to_string())])
                } else {
                    String::new()
                }).to_string())])
        };
        self.label(cx, ids!(history_status)).set_text(cx, &status);
        self.button(cx, ids!(history_more)).set_text(
            cx,
            if self.busy {
                crate::i18n::tr("Stop")
            } else if self.error.is_some() {
                crate::i18n::tr("Retry")
            } else {
                crate::i18n::tr("Load Older")
            },
        );
        self.button(cx, ids!(history_more))
            .set_visible(cx, self.busy || !self.exhausted);
        self.view(cx, ids!(browser))
            .set_visible(cx, self.detail.is_none());
        self.view(cx, ids!(detail))
            .set_visible(cx, self.detail.is_some());
        self.label(cx, ids!(header.title)).set_text(
            cx,
            if self.detail.is_some() {
                crate::i18n::tr("Message Details")
            } else if self.filter == HistoryFilter::Messages {
                crate::i18n::tr("Search Chat History")
            } else {
                crate::i18n::tr("Shared Attachments")
            },
        );
        for (id, filter, label) in [
            (ids!(all_filter), HistoryFilter::Messages, crate::i18n::tr("All")),
            (ids!(media_filter), HistoryFilter::Media, crate::i18n::tr("Media")),
            (ids!(file_filter), HistoryFilter::Files, crate::i18n::tr("Files")),
            (ids!(audio_filter), HistoryFilter::Audio, crate::i18n::tr("Audio")),
        ] {
            self.button(cx, id).set_text(
                cx,
                &format!("{}{label}", if self.filter == filter { "✓ " } else { "" }),
            );
        }
        self.redraw(cx);
    }
    fn show_detail(&mut self, cx: &mut Cx, message: HistoryMessage) {
        self.label(cx, ids!(detail_metadata))
            .set_text(cx, &message.metadata());
        self.label(cx, ids!(detail_body))
            .set_text(cx, message.content.body());
        self.button(cx, ids!(post_text_to_moments)).set_visible(cx,
            current_user_id().as_ref() == Some(&message.sender)
                && matches!(message.content, MessageType::Text(_)));
        let attachment = message.attachment();
        self.view(cx, ids!(attachment_actions))
            .set_visible(cx, attachment.is_some());
        let hint = attachment
            .as_ref()
            .map(|a| {
                crate::i18n::format("{0}{1} · Open the original message to use its media controls.", &[("0", (a.filename).to_string()), ("1", (a.size
                        .map(|size| crate::i18n::format(" · {size} bytes", &[("size", (size).to_string())]))
                        .unwrap_or_default()).to_string())])
            })
            .unwrap_or_default();
        self.label(cx, ids!(detail_hint)).set_text(cx, &hint);
        self.detail = Some(message);
        self.refresh(cx, false);
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.detail.take().is_some() {
            self.refresh(cx, false);
        } else {
            cx.action(RoomHistoryAction::Close);
        }
    }
}

fn next_request() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Widget for RoomHistoryPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.owner.is_none() {
            return;
        }
        if self.owner != current_user_id() {
            self.reset();
            cx.action(RoomHistoryAction::Close);
            return;
        }
        if event.back_pressed()
            || matches!(
                event,
                Event::KeyDown(KeyEvent {
                    key_code: KeyCode::Escape,
                    ..
                })
            )
            || matches!(event, Event::Scroll(e) if self.back_swipe.update(e))
        {
            self.back(cx);
            return;
        }
        self.view.handle_event(cx, event, scope);
        if matches!(event, Event::Signal) {
            self.redraw(cx);
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(page) = action.downcast_ref::<HistoryPage>() {
                    if self.owner.as_ref() != Some(&page.owner) || self.request != page.request {
                        continue;
                    }
                    match &page.result {
                        Ok((events, names, cursor, exhausted)) => {
                            for event in events {
                                self.index.insert(event, names);
                            }
                            self.cursor = cursor.clone();
                            self.exhausted = *exhausted;
                        }
                        Err(error) => self.error = Some(error.clone()),
                    }
                    if page.last {
                        self.busy = false;
                        self.abort = None;
                    }
                    self.refresh(cx, false);
                }
                if action
                    .downcast_ref::<super::chat_actions::ChatVisibilityChanged>()
                    .is_some()
                {
                    self.detail = None;
                    self.refresh(cx, false);
                }
            }
            if self.button(cx, ids!(header.back)).clicked(actions) {
                self.back(cx);
                return;
            }
            if self.detail.is_some() {
                if self.button(cx, ids!(post_text_to_moments)).clicked(actions) {
                    if let Some(message) = &self.detail {
                        cx.action(RoomHistoryAction::Close);
                        cx.action(crate::moments::ui::MomentsAction::Compose {text: message.content.body().to_owned()});
                    }
                }
                if self.button(cx, ids!(view_in_chat)).clicked(actions) {
                    if let (Some(room), Some(message)) = (&self.room, &self.detail) {
                        cx.action(RoomHistoryAction::Jump {
                            room: room.clone(),
                            event: message.id.clone(),
                        });
                        cx.action(RoomHistoryAction::Close);
                    }
                }
                if let Some(attachment) = self.detail.as_ref().and_then(|m| m.attachment()) {
                    if self.button(cx, ids!(download_attachment)).clicked(actions) {
                        start_attachment_download(attachment.clone(), None);
                    }
                    if self.button(cx, ids!(share_attachment)).clicked(actions) {
                        start_attachment_share(attachment, None);
                    }
                }
                return;
            }
            if self
                .text_input(cx, ids!(history_query))
                .changed(actions)
                .is_some()
            {
                self.refresh(cx, true);
            }
            if self
                .text_input(cx, ids!(history_query))
                .returned(actions)
                .is_some()
                || self.button(cx, ids!(search_history)).clicked(actions)
            {
                self.cancel();
                self.index = HistoryIndex::default();
                self.cursor = None;
                self.exhausted = false;
                self.refresh(cx, true);
                self.load(cx, 10);
            }
            if self.button(cx, ids!(history_more)).clicked(actions) {
                if self.busy {
                    self.cancel();
                    self.refresh(cx, false);
                } else {
                    self.load(cx, 10);
                }
            }
            for (id, filter) in [
                (ids!(all_filter), HistoryFilter::Messages),
                (ids!(media_filter), HistoryFilter::Media),
                (ids!(file_filter), HistoryFilter::Files),
                (ids!(audio_filter), HistoryFilter::Audio),
            ] {
                if self.button(cx, id).clicked(actions) {
                    self.filter = filter;
                    self.refresh(cx, true);
                }
            }
            let list = self.portal_list(cx, ids!(results));
            for (index, row) in list.items_with_actions(actions) {
                let clicked = actions.iter().any(|a| {
                    matches!(
                        a.as_widget_action().widget_uid_eq(row.widget_uid()).cast(),
                        NavigationBarButtonAction::Clicked
                    )
                }) || actions.iter().any(|a| {
                    matches!(
                        a.as_widget_action()
                            .widget_uid_eq(row.text_or_image(cx, ids!(thumbnail)).widget_uid())
                            .cast(),
                        TextOrImageAction::Clicked(_)
                    )
                });
                if clicked && !list.was_scrolling() {
                    if let Some(message) = self.matches.get(index).cloned() {
                        self.show_detail(cx, message);
                    }
                    break;
                }
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.owner.is_none() {
            return DrawStep::done();
        }
        if self.owner != current_user_id() {
            self.reset();
            return DrawStep::done();
        }
        if let Some(message) = self.detail.clone() {
            let preview = self.text_or_image(cx, ids!(preview));
            preview.set_visible(cx, matches!(message.content, MessageType::Image(_)));
            if let MessageType::Image(image) = &message.content {
                if let Some(cache) = self.media.as_mut() {
                    match cache.try_get_media_or_fetch(&image.source, MediaFormat::File) {
                        (MediaCacheEntry::Loaded(data), format) => {
                            let variant = if matches!(format, MediaFormat::File) {"full"} else {"thumb"};
                            let key = format!("{}#history-{variant}", media_source_mxc(&image.source));
                            if preview.show_image(cx, Some(image.source.clone()), |cx, img| {
                                crate::utils::load_image_with_cache_key(&img, cx, std::path::Path::new(&key), data)
                                    .map(|()| img.size_in_pixels(cx).unwrap_or_default())
                            }).is_err() {preview.show_text(cx, crate::i18n::tr("Preview unavailable. Download the image to view it."));}
                        }
                        (MediaCacheEntry::Requested, _) => preview.show_text(cx, crate::i18n::tr("Loading original photo…")),
                        (MediaCacheEntry::Failed(_), _) => preview.show_text(cx, crate::i18n::tr("Could not load the photo. Download it or reopen to retry.")),
                    }
                }
            }
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let Some(mut list) = step.borrow_mut::<PortalList>() else {
                continue;
            };
            list.set_item_range(cx, 0, self.matches.len());
            while let Some(index) = list.next_visible_item(cx) {
                let Some(message) = self.matches.get(index) else {
                    continue;
                };
                let row = list.item(cx, index, id!(Entry));
                row.label(cx, ids!(metadata))
                    .set_text(cx, &message.metadata());
                row.label(cx, ids!(body))
                    .set_text(cx, message.content.body());
                let preview = row.text_or_image(cx, ids!(thumbnail));
                preview.set_visible(cx, matches!(message.content, MessageType::Image(_)));
                if let MessageType::Image(image) = &message.content {
                    if let Some(cache) = self.media.as_mut() {
                        super::room_screen::populate_image_message_content(
                            cx,
                            &preview,
                            image.info.as_deref(),
                            image.source.clone(),
                            image.body.as_str(),
                            cache,
                        );
                    }
                }
                row.draw_all(cx, scope);
            }
        }
        DrawStep::done()
    }
}
impl RoomHistoryPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &RoomHistoryAction) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        match action {
            RoomHistoryAction::Close => {
                inner.reset();
                modal.close(cx);
            }
            RoomHistoryAction::Open { room, filter } => {
                inner.reset();
                inner.owner = current_user_id();
                inner.room = Some(room.clone());
                inner.filter = *filter;
                inner.media = Some(MediaCache::new(None));
                inner.text_input(cx, ids!(history_query)).set_text(cx, "");
                inner
                    .label(cx, ids!(room_name))
                    .set_text(cx, &room.display());
                inner.refresh(cx, true);
                modal.open(cx);
                inner.load(cx, 1);
            }
            RoomHistoryAction::Jump { .. } => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(id: &str, body: &str, time: u64) -> serde_json::Value {
        serde_json::json!({"type":"m.room.message", "event_id":id, "sender":"@alice:example.org", "origin_server_ts":time, "content":{"msgtype":"m.text", "body":body}})
    }
    fn insert(index: &mut HistoryIndex, value: serde_json::Value) {
        index.insert_value(value, &BTreeMap::new(), false);
    }
    #[test]
    fn backwards_pages_apply_latest_authorized_edit_and_deduplicate() {
        let mut index = HistoryIndex::default();
        let mut edit = message("$edit", "* corrected", 200);
        edit["content"]["m.relates_to"] =
            serde_json::json!({"rel_type":"m.replace","event_id":"$original"});
        edit["content"]["m.new_content"] =
            serde_json::json!({"msgtype":"m.text","body":"Corrected 中文"});
        insert(&mut index, edit.clone());
        insert(&mut index, edit.clone());
        let mut forged = edit;
        forged["event_id"] = "$forged".into();
        forged["sender"] = "@mallory:example.org".into();
        forged["origin_server_ts"] = 300.into();
        forged["content"]["m.new_content"]["body"] = "forged".into();
        insert(&mut index, forged);
        insert(&mut index, message("$original", "obsolete", 100));
        assert_eq!(
            index
                .results("CORRECTED 中文", HistoryFilter::Messages, None)
                .len(),
            1
        );
        assert!(
            index
                .results("obsolete", HistoryFilter::Messages, None)
                .is_empty()
        );
        assert!(
            index
                .results("forged", HistoryFilter::Messages, None)
                .is_empty()
        );
        assert_eq!(index.seen.len(), 3);
    }
    #[test]
    fn redacted_edits_revert_and_redacted_messages_disappear() {
        let mut index = HistoryIndex::default();
        insert(&mut index, message("$original", "original", 100));
        let mut edit = message("$edit", "* edited", 200);
        edit["content"]["m.relates_to"] =
            serde_json::json!({"rel_type":"m.replace","event_id":"$original"});
        edit["content"]["m.new_content"] = serde_json::json!({"msgtype":"m.text","body":"edited"});
        insert(&mut index, edit);
        insert(
            &mut index,
            serde_json::json!({"type":"m.room.redaction", "event_id":"$redact_edit", "content":{"redacts":"$edit"}}),
        );
        assert_eq!(
            index
                .results("original", HistoryFilter::Messages, None)
                .len(),
            1
        );
        insert(
            &mut index,
            serde_json::json!({"type":"m.room.redaction", "event_id":"$redact_original", "redacts":"$original"}),
        );
        assert!(index.results("", HistoryFilter::Messages, None).is_empty());
    }
    #[test]
    fn attachment_filters_filename_search_and_clear_cutoff_apply_together() {
        let mut index = HistoryIndex::default();
        for (id, kind, time) in [
            ("$image", "m.image", 100),
            ("$video", "m.video", 110),
            ("$file", "m.file", 120),
            ("$audio", "m.audio", 130),
        ] {
            let mut value = message(id, "Caption", time);
            value["content"] = serde_json::json!({"msgtype":kind, "body":"Caption", "filename":"项目.PDF", "url":"mxc://example.org/data"});
            insert(&mut index, value);
        }
        assert_eq!(index.results("", HistoryFilter::Media, None).len(), 2);
        assert_eq!(
            index.results("项目.pdf", HistoryFilter::Files, None).len(),
            1
        );
        assert_eq!(index.results("", HistoryFilter::Audio, None).len(), 1);
        assert!(
            index
                .results("", HistoryFilter::Media, Some(115))
                .is_empty()
        );
        assert!(
            index
                .results("", HistoryFilter::Files, Some(120))
                .is_empty()
        );
        assert_eq!(index.results("", HistoryFilter::Audio, Some(120)).len(), 1);
    }
    #[test]
    fn undecryptable_and_redacted_bodies_are_not_searchable() {
        let mut index = HistoryIndex::default();
        index.insert_value(message("$locked", "secret", 100), &BTreeMap::new(), true);
        let mut redacted = message("$redacted", "removed", 101);
        redacted["unsigned"]["redacted_because"] = serde_json::json!({});
        insert(&mut index, redacted);
        assert_eq!(index.locked, 1);
        assert!(index.results("", HistoryFilter::Messages, None).is_empty());
    }
}
