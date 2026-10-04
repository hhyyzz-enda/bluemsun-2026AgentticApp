//! Selected-message forwarding through the normal Matrix room send API.
use std::collections::BTreeSet;
use makepad_widgets::*;
use matrix_sdk_ui::timeline::{EventTimelineItem, MsgLikeKind, TimelineDetails, TimelineItemContent};
use ruma::{
    OwnedEventId, OwnedRoomId, OwnedTransactionId, OwnedUserId, TransactionId,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use serde::{Deserialize, Serialize};
use crate::{
    home::rooms_list::RoomsListRef,
    shared::{
        navigation_bar_button::{
            NavigationBarButtonAction, NavigationBarButtonWidgetExt,
        },
        popup_list::{enqueue_popup_notification, PopupKind},
    },
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};

pub const MSGTYPE: &str = "rs.robius.robrix.forwarded_chat";
const MAX_MESSAGES: usize = 100;
const MAX_BUNDLE_BYTES: usize = 32_000;
const CAPTION_PADDING: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardMessage {
    pub event_id: OwnedEventId,
    pub sender: String,
    pub sender_id: OwnedUserId,
    pub timestamp: u64,
    pub message: MessageType,
}
impl ForwardMessage {
    pub fn from_event(event: &EventTimelineItem) -> Option<Self> {
        let TimelineItemContent::MsgLike(content) = event.content() else {
            return None;
        };
        let MsgLikeKind::Message(message) = &content.kind else {
            return None;
        };
        Some(Self {
            event_id: event.event_id()?.to_owned(),
            sender: match event.sender_profile() {
                TimelineDetails::Ready(profile) => profile
                    .display_name
                    .clone()
                    .unwrap_or_else(|| event.sender().to_string()),
                _ => event.sender().to_string(),
            },
            sender_id: event.sender().to_owned(),
            timestamp: event.timestamp().0.into(),
            message: message.msgtype().clone(),
        })
    }
    fn summary(&self) -> String {
        let kind = self.message.msgtype();
        if matches!(kind, "m.text" | "m.notice" | "m.emote") {
            self.message.body().to_owned()
        } else {
            format!(
                "[{}] {}",
                kind.strip_prefix("m.").unwrap_or(crate::i18n::tr("Message")),
                self.message.body()
            )
        }
    }
    pub fn individual(&self) -> RoomMessageEventContent {
        // New content deliberately carries no source-room relation or mention targets.
        RoomMessageEventContent::new(self.message.clone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardBundle {
    version: u8,
    title: String,
    messages: Vec<ForwardMessage>,
}
impl ForwardBundle {
    fn content(
        title: String,
        messages: Vec<ForwardMessage>,
    ) -> Result<RoomMessageEventContent, String> {
        if messages.is_empty() || messages.len() > MAX_MESSAGES {
            return Err(crate::i18n::tr("Select between 1 and 100 messages.").into());
        }
        let bundle = Self {
            version: 1,
            title,
            messages,
        };
        let body = crate::i18n::format("[Forwarded chat] {0}\n{1}", &[("0", (bundle.title).to_string()), ("1", (bundle
                .messages
                .iter()
                .map(|m| format!("{}: {}", m.sender, m.summary()))
                .collect::<Vec<_>>()
                .join("\n")).to_string())]);
        let mut data = serde_json::Map::new();
        data.insert(
            "forwarded_chat".into(),
            serde_json::to_value(&bundle).map_err(|e| e.to_string())?,
        );
        let content = RoomMessageEventContent::new(
            MessageType::new(MSGTYPE, body, data).map_err(|e| e.to_string())?,
        );
        if serde_json::to_vec(&content)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_BUNDLE_BYTES
        {
            return Err(
                crate::i18n::tr("This bundle is too large. Select fewer messages or forward individually.").into(),
            );
        }
        Ok(content)
    }
    pub fn from_message(message: &MessageType) -> Option<Self> {
        if message.msgtype() != MSGTYPE {
            return None;
        }
        let data = message.data();
        let value = data.get("forwarded_chat")?;
        if serde_json::to_vec(value).ok()?.len() > MAX_BUNDLE_BYTES {
            return None;
        }
        let bundle: Self = serde_json::from_value(value.clone()).ok()?;
        (bundle.version == 1
            && !bundle.messages.is_empty()
            && bundle.messages.len() <= MAX_MESSAGES
            && bundle.title.chars().count() <= 250)
            .then_some(bundle)
    }
}

pub fn is_timeline_event(event: &ruma::events::AnySyncTimelineEvent) -> bool {
    let ruma::events::AnySyncTimelineEvent::MessageLike(event) = event else {
        return false;
    };
    event.original_content().is_some_and(|c| {
        matches!(c,
        ruma::events::AnyMessageLikeEventContent::RoomMessage(m) if m.msgtype.msgtype() == MSGTYPE)
    })
}

#[derive(Clone, Debug)]
pub enum ForwardAction {
    Select {
        title: String,
        messages: Vec<ForwardMessage>,
        selected: OwnedEventId,
    },
    View(ForwardBundle),
    Close,
}
#[derive(Clone)]
struct Job {
    room: OwnedRoomId,
    content: RoomMessageEventContent,
    transaction: OwnedTransactionId,
}
#[derive(Debug)]
struct Sent {
    owner: OwnedUserId,
    batch: OwnedTransactionId,
    completed: usize,
    error: Option<String>,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.ForwardCard = #(ForwardCard::register_widget(vm)) {
        ..mod.widgets.View
        visible: false width: Fill height: Fit
        open_bundle := NavigationBarButton {
            width: Fill height: Fit padding: 14 flow: Down spacing: 8
            draw_bg +: {color_hover: mod.widgets.RINX_HOVER color_active: mod.widgets.RINX_HOVER border_radius: 5}
            title := Label {width: Fill max_lines: 2 draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_bold {font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
            preview := Label {width: Fill max_lines: 3 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
            Label {text: #(crate::i18n::tr("Chat history  ›")) i18n_text: "Chat history  ›" draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
        }
    }
    mod.widgets.ForwardPanel = #(ForwardPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(CAPTION_PADDING) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        draw_bg.color: mod.widgets.RINX_PAGE
        header := DetailHeader {title.text: #(crate::i18n::tr("Select Messages")) title.i18n_text: "Select Messages"}
        summary := Label {width: Fill height: Fit padding: 12 flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
        messages_view := View {
            width: Fill height: Fill
        message_list := PortalList {
            width: Fill height: Fill
            Entry := NavigationBarButton {
                width: Fill height: 100 padding: 12 flow: Right spacing: 12 align: Align{y: 0.5}
                draw_bg +: {color_hover: mod.widgets.RINX_SURFACE color_active: mod.widgets.RINX_SELECTED border_radius: 0}
                choice := Label {width: 22 draw_text +: {color: mod.widgets.RINX_ACCENT text_style.font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}
                text_content := View {width: Fill height: Fill flow: Down spacing: 8
                    sender := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_ACCENT text_style.font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                    body := Label {width: Fill max_lines: 2 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_INK text_style.font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                }
            }
            History := NavigationBarButton {
                width: Fill height: Fit padding: 16 flow: Down spacing: 8
                draw_bg +: {color_hover: mod.widgets.RINX_SURFACE color_active: mod.widgets.RINX_SURFACE border_radius: 0}
                sender := Label {width: Fill flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_ACCENT text_style.font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                body := Label {width: Fill flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style.font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
            }
        }
        }
        recipients := View {
            visible: false width: Fill height: Fill flow: Down spacing: 8 padding: 12
            recipient_search := TextInput {width: Fill height: 40 empty_text: #(crate::i18n::tr("Find a chat")) i18n_empty_text: "Find a chat"}
            modes := View {
                width: Fill height: Fit flow: Right spacing: 8
                individual := RobrixNeutralIconButton {text: #(crate::i18n::tr("✓ Individually")) i18n_text: "✓ Individually" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
                bundle := RobrixNeutralIconButton {text: #(crate::i18n::tr("As chat history")) i18n_text: "As chat history" width: Fill height: 44 spacing: 0 icon_walk: Walk{width: 0 height: 0}}
            }
            recipient_list := PortalList {
                width: Fill height: Fill
                Chat := NavigationBarButton {
                    width: Fill height: 56 padding: 12 flow: Right spacing: 12 align: Align{y: 0.5}
                    draw_bg +: {color_hover: mod.widgets.RINX_SURFACE color_active: mod.widgets.RINX_SELECTED border_radius: 0}
                    choice := Label {width: 22 draw_text +: {color: mod.widgets.RINX_ACCENT text_style.font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}
                    name := Label {width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: mod.widgets.RINX_INK text_style.font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}
                }
            }
        }
        forward_status := Label {width: Fill height: Fit padding: 12 flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_MUTED text_style.font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
        proceed := RobrixPositiveIconButton {
            text: #(crate::i18n::tr("Choose Recipients")) i18n_text: "Choose Recipients" width: Fill height: 48 margin: 12 spacing: 0 align: Align{x: 0.5 y: 0.5}
            icon_walk: Walk{width: 0 height: 0}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ForwardCard {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    bundle: Option<ForwardBundle>,
}
impl Widget for ForwardCard {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.visible {
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self
                .navigation_bar_button(cx, ids!(open_bundle))
                .clicked(actions)
            {
                if let Some(bundle) = &self.bundle {
                    cx.action(ForwardAction::View(bundle.clone()));
                }
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
impl ForwardCardRef {
    pub fn set_bundle(&self, cx: &mut Cx, bundle: Option<ForwardBundle>) {
        let Some(mut s) = self.borrow_mut() else {
            return;
        };
        s.view.set_visible(cx, bundle.is_some());
        if let Some(bundle) = &bundle {
            s.label(cx, ids!(title)).set_text(
                cx,
                &crate::i18n::format("{0} · {1} messages", &[("0", (bundle.title).to_string()), ("1", (bundle.messages.len()).to_string())]),
            );
            s.label(cx, ids!(preview)).set_text(
                cx,
                &bundle
                    .messages
                    .iter()
                    .take(3)
                    .map(|m| format!("{}: {}", m.sender, m.summary()))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        s.bundle = bundle;
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ForwardPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    title: String,
    #[rust]
    messages: Vec<ForwardMessage>,
    #[rust]
    selected: BTreeSet<OwnedEventId>,
    #[rust]
    rooms: Vec<RoomNameId>,
    #[rust]
    targets: BTreeSet<OwnedRoomId>,
    #[rust]
    choosing: bool,
    #[rust]
    viewing: bool,
    #[rust]
    bundled: bool,
    #[rust]
    pending: bool,
    #[rust]
    jobs: Vec<Job>,
    #[rust]
    completed: usize,
    #[rust]
    batch: Option<OwnedTransactionId>,
}
impl ForwardPanel {
    fn status(&self, cx: &mut Cx, text: &str) {
        self.label(cx, ids!(forward_status)).set_text(cx, text);
    }
    fn refresh(&mut self, cx: &mut Cx) {
        self.view(cx, ids!(recipients))
            .set_visible(cx, self.choosing);
        self.view(cx, ids!(messages_view))
            .set_visible(cx, !self.choosing);
        self.button(cx, ids!(proceed))
            .set_visible(cx, !self.viewing);
        self.button(cx, ids!(proceed)).set_enabled(
            cx,
            !self.pending
                && !self.selected.is_empty()
                && (!self.choosing || !self.targets.is_empty()),
        );
        self.button(cx, ids!(proceed)).set_text(
            cx,
            if self.choosing {
                if self.jobs.is_empty() {
                    crate::i18n::tr("Send")
                } else {
                    crate::i18n::tr("Retry Remaining")
                }
            } else {
                crate::i18n::tr("Choose Recipients")
            },
        );
        self.label(cx, ids!(header.title)).set_text(
            cx,
            if self.viewing {
                crate::i18n::tr("Chat History")
            } else if self.choosing {
                crate::i18n::tr("Forward To")
            } else {
                crate::i18n::tr("Select Messages")
            },
        );
        self.label(cx, ids!(summary)).set_text(
            cx,
            &if self.viewing {
                crate::i18n::format("{0} · Forwarded by a participant", &[("0", (self.title).to_string())])
            } else {
                crate::i18n::format("{0} selected · {1} recipient{2}", &[("0", (self.selected.len()).to_string()), ("1", (self.targets.len()).to_string()), ("2", (crate::i18n::plural_suffix(self.targets.len())).to_string())])
            },
        );
        self.button(cx, ids!(individual)).set_text(
            cx,
            if self.bundled {
                crate::i18n::tr("Individually")
            } else {
                crate::i18n::tr("✓ Individually")
            },
        );
        self.button(cx, ids!(bundle)).set_text(
            cx,
            if self.bundled {
                crate::i18n::tr("✓ As chat history")
            } else {
                crate::i18n::tr("As chat history")
            },
        );
        self.redraw(cx);
    }
    fn send(&mut self, cx: &mut Cx) {
        if self.pending || self.selected.is_empty() || self.targets.is_empty() {
            return;
        }
        let Some(client) = get_client() else { return };
        let Some(owner) = self.owner.clone().filter(|o| client.user_id() == Some(o)) else {
            return;
        };
        if self.jobs.is_empty() {
            let messages: Vec<_> = self
                .messages
                .iter()
                .filter(|m| self.selected.contains(&m.event_id))
                .cloned()
                .collect();
            let contents = if self.bundled {
                match ForwardBundle::content(self.title.clone(), messages) {
                    Ok(content) => vec![content],
                    Err(e) => {
                        self.status(cx, &e);
                        return;
                    }
                }
            } else {
                messages.iter().map(ForwardMessage::individual).collect()
            };
            for room in &self.targets {
                for content in &contents {
                    self.jobs.push(Job {
                        room: room.clone(),
                        content: content.clone(),
                        transaction: TransactionId::new(),
                    });
                }
            }
            self.batch = Some(TransactionId::new());
            self.completed = 0;
        }
        self.pending = true;
        self.refresh(cx);
        self.status(cx, crate::i18n::tr("Sending…"));
        let jobs = self.jobs.clone();
        let mut completed = self.completed;
        let batch = self.batch.clone().unwrap();
        spawn_async_task(async move {
            let mut error = None;
            for job in jobs.iter().skip(completed) {
                if current_user_id().as_ref() != Some(&owner) {
                    error = Some(crate::i18n::tr("The signed-in account changed.").into());
                    break;
                }
                let Some(room) = client
                    .get_room(&job.room)
                    .filter(|r| r.state() == matrix_sdk::RoomState::Joined && !r.is_space() && !crate::moments::is_moments(r))
                else {
                    error = Some(crate::i18n::tr("A destination chat is no longer joined.").into());
                    break;
                };
                match room
                    .send(job.content.clone())
                    .with_transaction_id(job.transaction.clone())
                    .await
                {
                    Ok(_) => completed += 1,
                    Err(e) => {
                        error = Some(e.to_string());
                        break;
                    }
                }
            }
            Cx::post_action(Sent {
                owner,
                batch,
                completed,
                error,
            });
        });
    }
}
impl Widget for ForwardPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.owner.is_none() {
            return;
        }
        if self.owner != current_user_id() {
            cx.action(ForwardAction::Close);
            return;
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(sent) = action.downcast_ref::<Sent>() {
                    if Some(&sent.owner) == self.owner.as_ref()
                        && self.batch.as_ref() == Some(&sent.batch)
                    {
                        self.pending = false;
                        self.completed = sent.completed;
                        if let Some(error) = &sent.error {
                            self.status(
                                cx,
                                &crate::i18n::format("Sent {0} of {1}. {2}", &[("0", (self.completed).to_string()), ("1", (self.jobs.len()).to_string()), ("2", (error).to_string())]),
                            );
                            self.refresh(cx);
                        } else {
                            enqueue_popup_notification(
                                crate::i18n::tr("Messages forwarded."),
                                PopupKind::Success,
                                Some(3.0),
                            );
                            cx.action(ForwardAction::Close);
                        }
                    }
                }
            }
            if !self.pending && self.jobs.is_empty() {
                // PortalList returns one entry per action, including hover actions
                // from the same row. Toggle each row only once per click batch.
                let mut handled_messages = BTreeSet::new();
                for (index, item) in self
                    .portal_list(cx, ids!(message_list))
                    .items_with_actions(actions)
                {
                    if !self.viewing
                        && handled_messages.insert(index)
                        && actions.iter().any(|a| {
                            matches!(
                                a.as_widget_action().widget_uid_eq(item.widget_uid()).cast(),
                                NavigationBarButtonAction::Clicked
                            )
                        })
                    {
                        if let Some(message) = self.messages.get(index) {
                            if !self.selected.remove(&message.event_id) {
                                if self.selected.len() < MAX_MESSAGES {
                                    self.selected.insert(message.event_id.clone());
                                } else {
                                    self.status(cx, crate::i18n::tr("Select up to 100 messages."));
                                }
                            }
                            self.refresh(cx);
                        }
                    }
                }
                let query = self
                    .text_input(cx, ids!(recipient_search))
                    .text()
                    .to_lowercase();
                let visible: Vec<_> = self
                    .rooms
                    .iter()
                    .filter(|r| r.display().to_lowercase().contains(&query))
                    .cloned()
                    .collect();
                let mut handled_rooms = BTreeSet::new();
                for (index, item) in self
                    .portal_list(cx, ids!(recipient_list))
                    .items_with_actions(actions)
                {
                    if handled_rooms.insert(index)
                        && actions.iter().any(|a| {
                            matches!(
                                a.as_widget_action().widget_uid_eq(item.widget_uid()).cast(),
                                NavigationBarButtonAction::Clicked
                            )
                        })
                    {
                        if let Some(room) = visible.get(index) {
                            if !self.targets.remove(room.room_id()) {
                                self.targets.insert(room.room_id().clone());
                            }
                            self.refresh(cx);
                        }
                    }
                }
                if self.button(cx, ids!(individual)).clicked(actions) {
                    self.bundled = false;
                    self.refresh(cx);
                }
                if self.button(cx, ids!(bundle)).clicked(actions) {
                    self.bundled = true;
                    self.refresh(cx);
                }
                if self
                    .text_input(cx, ids!(recipient_search))
                    .changed(actions)
                    .is_some()
                {
                    self.portal_list(cx, ids!(recipient_list))
                        .set_first_id_and_scroll(0, 0.0);
                    self.redraw(cx);
                }
            }
            if self.button(cx, ids!(proceed)).clicked(actions) && !self.pending {
                if self.choosing {
                    self.send(cx);
                } else {
                    self.choosing = true;
                    self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
                    self.status(
                        cx,
                        crate::i18n::tr("Selected messages will be shared with everyone in each chosen chat."),
                    );
                    self.refresh(cx);
                }
            }
            if self.button(cx, ids!(header.back)).clicked(actions) && !self.pending {
                if self.choosing && self.jobs.is_empty() {
                    self.choosing = false;
                    self.refresh(cx);
                } else {
                    cx.action(ForwardAction::Close);
                }
            }
        }
        if !self.pending
            && (event.back_pressed()
                || matches!(
                    event,
                    Event::KeyDown(KeyEvent {
                        key_code: KeyCode::Escape,
                        ..
                    })
                ))
        {
            cx.action(ForwardAction::Close);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            let Some(mut list) = item.borrow_mut::<PortalList>() else {
                continue;
            };
            if self.choosing {
                let query = self
                    .text_input(cx, ids!(recipient_search))
                    .text()
                    .to_lowercase();
                let visible: Vec<_> = self
                    .rooms
                    .iter()
                    .filter(|r| r.display().to_lowercase().contains(&query))
                    .collect();
                list.set_item_range(cx, 0, visible.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(room) = visible.get(index) else {
                        continue;
                    };
                    let row = list.item(cx, index, id!(Chat));
                    let encryption = get_client()
                        .and_then(|c| c.get_room(room.room_id()))
                        .map(|r| r.encryption_state());
                    let suffix = match encryption {
                        Some(state) if state.is_encrypted() => "",
                        Some(state) if state.is_unknown() => crate::i18n::tr(" · Encryption unknown"),
                        _ => crate::i18n::tr(" · Unencrypted"),
                    };
                    row.label(cx, ids!(name))
                        .set_text(cx, &format!("{}{suffix}", room.display()));
                    row.label(cx, ids!(choice)).set_text(
                        cx,
                        if self.targets.contains(room.room_id()) {
                            "✓"
                        } else {
                            "○"
                        },
                    );
                    row.draw_all(cx, scope);
                }
            } else {
                list.set_item_range(cx, 0, self.messages.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(message) = self.messages.get(index) else {
                        continue;
                    };
                    let row = list.item(
                        cx,
                        index,
                        if self.viewing {
                            id!(History)
                        } else {
                            id!(Entry)
                        },
                    );
                    let date = chrono::DateTime::from_timestamp_millis(
                        message.timestamp.min(i64::MAX as u64) as i64,
                    )
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%m/%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                    row.label(cx, ids!(sender))
                        .set_text(cx, &format!("{}  {date}", message.sender));
                    row.label(cx, ids!(body)).set_text(cx, &message.summary());
                    row.label(cx, ids!(choice)).set_text(
                        cx,
                        if self.viewing {
                            ""
                        } else if self.selected.contains(&message.event_id) {
                            "✓"
                        } else {
                            "○"
                        },
                    );
                    row.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}
impl ForwardPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &ForwardAction) {
        let Some(mut s) = self.borrow_mut() else {
            return;
        };
        if matches!(action, ForwardAction::Close) {
            s.owner = None;
            s.messages.clear();
            s.jobs.clear();
            s.batch = None;
            modal.close(cx);
            return;
        }
        s.owner = current_user_id();
        s.selected.clear();
        s.targets.clear();
        s.jobs.clear();
        s.completed = 0;
        s.batch = None;
        s.pending = false;
        s.choosing = false;
        s.bundled = false;
        s.text_input(cx, ids!(recipient_search)).set_text(cx, "");
        match action {
            ForwardAction::Select {
                title,
                messages,
                selected,
            } => {
                s.title = title.chars().take(250).collect();
                s.messages = messages.clone();
                s.selected.insert(selected.clone());
                s.viewing = false;
                s.status(cx, crate::i18n::tr("Select messages to forward. Load earlier messages in the chat to include them."));
            }
            ForwardAction::View(bundle) => {
                s.title = bundle.title.clone();
                s.messages = bundle.messages.clone();
                s.viewing = true;
                s.status(
                    cx,
                    crate::i18n::tr("Sender names and history were provided by the person who forwarded this card."),
                );
            }
            ForwardAction::Close => unreachable!(),
        }
        let first = if s.viewing {
            0
        } else {
            s.messages
                .iter()
                .position(|m| s.selected.contains(&m.event_id))
                .unwrap_or(0)
                .saturating_sub(2)
        };
        s.portal_list(cx, ids!(message_list))
            .set_first_id_and_scroll(first, 0.0);
        s.portal_list(cx, ids!(recipient_list))
            .set_first_id_and_scroll(0, 0.0);
        s.refresh(cx);
        modal.open(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(id: &str, body: &str) -> ForwardMessage {
        ForwardMessage {
            event_id: id.try_into().unwrap(),
            sender: "Alice 陈".into(),
            sender_id: ruma::user_id!("@alice:example.org").to_owned(),
            timestamp: 1234,
            message: MessageType::Text(
                ruma::events::room::message::TextMessageEventContent::plain(body),
            ),
        }
    }
    #[test]
    fn bundled_messages_preserve_order_and_have_an_interoperable_fallback() {
        let content = ForwardBundle::content(
            "Design 设计".into(),
            vec![text("$first", "First"), text("$second", "第二条")],
        )
        .unwrap();
        assert!(content.body().contains("Alice 陈: First\nAlice 陈: 第二条"));
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["msgtype"], MSGTYPE);
        let decoded: RoomMessageEventContent = serde_json::from_value(json).unwrap();
        let bundle = ForwardBundle::from_message(&decoded.msgtype).unwrap();
        assert_eq!(bundle.messages[0].event_id.as_str(), "$first");
        assert_eq!(bundle.messages[1].message.body(), "第二条");
    }
    #[test]
    fn individual_forward_is_a_new_message_without_cross_room_relations_or_mentions() {
        let item = text("$source", "hello");
        let content = item.individual();
        assert_eq!(content.body(), "hello");
        assert!(content.relates_to.is_none());
        assert!(content.mentions.is_none());
    }
    #[test]
    fn individual_forward_preserves_media_descriptors_and_mini_apps() {
        let mut item = text("$media", "");
        item.message = serde_json::from_value(serde_json::json!({"msgtype":"m.image", "body":"photo.png", "url":"mxc://example.org/photo", "info":{"mimetype":"image/png", "w":10,"h":20}})).unwrap();
        let json = serde_json::to_value(item.individual()).unwrap();
        assert_eq!(json["url"], "mxc://example.org/photo");
        assert_eq!(json["info"]["h"], 20);
        let mini = crate::mini_app::WebMiniApp::new("Demo", "https://example.org/app").unwrap();
        item.message = mini.message().msgtype;
        assert_eq!(
            crate::mini_app::WebMiniApp::from_message(&item.individual().msgtype).unwrap(),
            mini
        );
    }
    #[test]
    fn bundle_limits_reject_empty_oversized_and_unsupported_data() {
        assert!(ForwardBundle::content("Chat".into(), vec![]).is_err());
        assert!(
            ForwardBundle::content(
                "Chat".into(),
                vec![text("$large", &"x".repeat(MAX_BUNDLE_BYTES))]
            )
            .is_err()
        );
        assert!(
            ForwardBundle::content("Chat".into(), vec![text("$many", "ok"); MAX_MESSAGES + 1])
                .is_err()
        );
        let mut content = serde_json::to_value(
            ForwardBundle::content("Chat".into(), vec![text("$valid", "ok")]).unwrap(),
        )
        .unwrap();
        content["forwarded_chat"]["version"] = 99.into();
        let content: RoomMessageEventContent = serde_json::from_value(content).unwrap();
        assert!(ForwardBundle::from_message(&content.msgtype).is_none());
    }
}
