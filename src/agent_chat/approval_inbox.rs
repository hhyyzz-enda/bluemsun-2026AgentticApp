//! Authenticated, bounded discovery of the owner's private approval rooms.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use makepad_widgets::*;
use matrix_sdk::{
    RoomState,
    ruma::{OwnedRoomId, OwnedUserId},
};
use futures_util::{stream, StreamExt};
use super::{
    approval::{Namespace, current_unix_time_millis},
    approval_state::ApprovalMarkerIndex,
};
use crate::{
    sliding_sync::{get_client, current_user_id, spawn_async_task},
    utils::RoomNameId,
};

type MarkerIndexes = HashMap<&'static str, ApprovalMarkerIndex>;

#[derive(Clone, Debug)]
pub enum ApprovalInboxAction {
    Open { project: Option<OwnedRoomId> },
    Close,
    Saved,
}
#[derive(Clone, Debug)]
struct Candidate {
    room: RoomNameId,
    agents: String,
}
#[derive(Debug)]
struct Discovered {
    owner: OwnedUserId,
    request: u64,
    candidates: Vec<Candidate>,
    failed: usize,
    scanned: usize,
    indexes: MarkerIndexes,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.ApprovalInbox = #(ApprovalInbox::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down spacing: 10 padding: Inset{top: 32 left: 16 right: 16 bottom: 12} draw_bg.color: mod.widgets.RINX_PAGE
        View {width: Fill height: 40 spacing: 8
            close := RobrixNeutralIconButton {text: #(crate::i18n::tr("Back")) i18n_text: "Back"}
            refresh := RobrixNeutralIconButton {text: #(crate::i18n::tr("Refresh approval rooms")) i18n_text: "Refresh approval rooms"}
        }
        status := Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text.color: mod.widgets.RINX_INK}
        rooms := PortalList {width: Fill height: Fill
            Row := View {width: Fill height: Fit flow: Down padding: 8 spacing: 8
                title := Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text.color: mod.widgets.RINX_INK}
                detail := Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text.color: mod.widgets.RINX_MUTED}
                open := RobrixNeutralIconButton {text: #(crate::i18n::tr("Open approvals")) i18n_text: "Open approvals"}
            }
        }
    }
}
#[derive(Script, ScriptHook, Widget)]
pub struct ApprovalInbox {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    request: u64,
    #[rust]
    active: bool,
    #[rust]
    project: Option<OwnedRoomId>,
    #[rust]
    cancelled: Option<Arc<AtomicBool>>,
    #[rust]
    candidates: Vec<Candidate>,
    #[rust]
    marker_client: Option<matrix_sdk::Client>,
    #[rust]
    indexes: MarkerIndexes,
    #[rust]
    validated_at: u64,
}
fn nonce() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
impl ApprovalInbox {
    fn cancel(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
    fn load(&mut self, cx: &mut Cx) {
        self.cancel();
        self.candidates.clear();
        self.request = nonce();
        let Some(client) = get_client() else { return };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else {
            return;
        };
        let request = self.request;
        self.owner = Some(owner.clone());
        if !self
            .marker_client
            .as_ref()
            .is_some_and(crate::matrix_context::is_current)
        {
            self.indexes.clear();
        }
        self.marker_client = Some(client.clone());
        let mut indexes = self.indexes.clone();
        for index in indexes.values_mut() {
            index.begin_revalidation();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = Some(cancelled.clone());
        let project = self.project.clone();
        self.view.label(cx, ids!(status)).set_text(
            cx,
            crate::i18n::tr("Checking authenticated approval-room markers…"),
        );
        self.view.button(cx, ids!(refresh)).set_enabled(cx, false);
        spawn_async_task(async move {
            let rooms: Vec<_> = client
                .rooms()
                .into_iter()
                .filter(|r| r.state() == RoomState::Joined && !r.is_space())
                .collect();
            let joined: HashSet<_> = rooms.iter().map(|r| r.room_id().to_string()).collect();
            for index in indexes.values_mut() {
                index.retain_joined(owner.as_str(), &joined);
            }
            let overflow = rooms.len() > 4096;
            let mut failed = usize::from(overflow);
            let mut scanned = 0;
            let started = tokio::time::Instant::now();
            let mut responses=stream::iter(rooms.into_iter().take(4096).map(|room| {
                let client=client.clone(); let cancelled=cancelled.clone();
                async move {
                    if cancelled.load(Ordering::Relaxed) || !crate::matrix_context::is_current(&client) {return (room,None);}
                    let response=tokio::time::timeout(Duration::from_secs(10), client.send(matrix_sdk::ruma::api::client::state::get_state_events::v3::Request::new(room.room_id().to_owned())).with_request_config(matrix_sdk::config::RequestConfig::default().retry_limit(0))).await;
                    (room,response.ok().and_then(Result::ok))
                }
            })).buffer_unordered(2);
            loop {
                let next =
                    tokio::time::timeout_at(started + Duration::from_secs(60), responses.next())
                        .await;
                let (room, response) = match next {
                    Ok(Some(v)) => v,
                    Ok(None) => break,
                    Err(_) => {
                        failed += 1;
                        break;
                    }
                };
                if cancelled.load(Ordering::Relaxed) || !crate::matrix_context::is_current(&client)
                {
                    return;
                }
                scanned += 1;
                let Some(response) = response else {
                    failed += 1;
                    continue;
                };
                if room.state() != RoomState::Joined {
                    continue;
                }
                for namespace in Namespace::ALL {
                    let selection = super::marker_selection::select_approval_marker_raw_state(
                        response.room_state.iter(),
                        namespace,
                    );
                    let index = indexes.entry(namespace.base()).or_default();
                    if selection.observed_v2 {
                        let _ = index.observe_v2(owner.as_str(), room.room_id().as_str());
                    }
                    match selection.marker {
                        Ok(Some(event)) => {
                            let text = |key| {
                                event
                                    .get(key)
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                            };
                            if index
                                .ingest_marker(
                                    owner.as_str(),
                                    room.room_id().as_str(),
                                    text("event_id"),
                                    text("type"),
                                    text("sender"),
                                    text("state_key"),
                                    &event["content"],
                                    current_unix_time_millis(),
                                )
                                .is_err()
                            {
                                failed += 1;
                            }
                        }
                        Err(()) => {
                            failed += 1;
                        }
                        Ok(None) => {}
                    }
                }
            }
            if cancelled.load(Ordering::Relaxed) || !crate::matrix_context::is_current(&client) {
                return;
            }
            let mut by_room: HashMap<String, Vec<String>> = HashMap::new();
            for index in indexes.values() {
                for (room, agent, project_id) in index.candidates(
                    owner.as_str(),
                    project.as_deref().map(|p| p.as_str()),
                    &joined,
                    current_unix_time_millis(),
                ) {
                    by_room
                        .entry(room)
                        .or_default()
                        .push(format!("{agent} · {project_id}"));
                }
            }
            let mut candidates = Vec::new();
            for (id, mut agents) in by_room {
                if let Ok(id) = OwnedRoomId::try_from(id)
                    && let Some(room) = client.get_room(&id)
                    && room.state() == RoomState::Joined
                {
                    agents.sort();
                    agents.dedup();
                    candidates.push(Candidate {
                        room: RoomNameId::new(
                            matrix_sdk::RoomDisplayName::Named(
                                room.name().unwrap_or_else(|| id.to_string()),
                            ),
                            id,
                        ),
                        agents: agents.join("\n"),
                    });
                }
            }
            candidates.sort_by_cached_key(|c| c.room.display().to_lowercase());
            Cx::post_action(Discovered {
                owner,
                request,
                candidates,
                failed,
                scanned,
                indexes,
            });
        });
    }
}
impl Widget for ApprovalInbox {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if !self.active {
            return;
        }
        if current_user_id() != self.owner {
            self.cancel();
            cx.action(ApprovalInboxAction::Close);
            return;
        }
        let Event::Actions(actions) = event else {
            return;
        };
        if self.view.button(cx, ids!(close)).clicked(actions) {
            cx.action(ApprovalInboxAction::Close);
        }
        if self.view.button(cx, ids!(refresh)).clicked(actions) {
            self.load(cx);
        }
        for action in actions {
            if let Some(result) = action.downcast_ref::<Discovered>()
                && self.owner.as_ref() == Some(&result.owner)
                && self.request == result.request
            {
                self.candidates = result.candidates.clone();
                self.indexes = result.indexes.clone();
                self.validated_at = current_unix_time_millis();
                if let Some(app) = scope.data.get_mut::<crate::app::AppState>() {
                    app.approval_markers = self
                        .indexes
                        .get(Namespace::DEFAULT.base())
                        .cloned()
                        .unwrap_or_default();
                    app.approval_markers_by_namespace = self
                        .indexes
                        .iter()
                        .filter(|(ns, _)| **ns != Namespace::DEFAULT.base())
                        .map(|(ns, index)| (ns.to_string(), index.clone()))
                        .collect();
                    cx.action(ApprovalInboxAction::Saved);
                }
                let text = crate::i18n::format(
                    "Found {count} approval rooms. Checked {scanned} rooms; {failed} checks unavailable.",
                    &[
                        ("count", self.candidates.len().to_string()),
                        ("scanned", result.scanned.to_string()),
                        ("failed", result.failed.to_string()),
                    ],
                );
                self.view.label(cx, ids!(status)).set_text(cx, &text);
                self.view.button(cx, ids!(refresh)).set_enabled(cx, true);
                self.view.redraw(cx);
            }
        }
        for (index, item) in self
            .view
            .portal_list(cx, ids!(rooms))
            .items_with_actions(actions)
        {
            if item.button(cx, ids!(open)).clicked(actions)
                && let Some(candidate) = self.candidates.get(index)
            {
                if current_unix_time_millis().saturating_sub(self.validated_at)
                    >= super::approval_state::MARKER_FRESHNESS_MS
                    || !self
                        .marker_client
                        .as_ref()
                        .is_some_and(crate::matrix_context::is_current)
                {
                    self.load(cx);
                    return;
                }
                if get_client()
                    .and_then(|c| c.get_room(candidate.room.room_id()))
                    .is_some_and(|r| r.state() == RoomState::Joined)
                {
                    cx.action(ApprovalInboxAction::Close);
                    cx.action(crate::app::AppStateAction::NavigateToRoom {
                        room_to_close: None,
                        destination_room: crate::room::BasicRoomDetails::RoomId(
                            candidate.room.clone(),
                        ),
                    });
                }
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.borrow_mut::<PortalList>() {
                list.set_item_range(cx, 0, self.candidates.len());
                while let Some(index) = list.next_visible_item(cx) {
                    if let Some(candidate) = self.candidates.get(index) {
                        let item = list.item(cx, index, id!(Row));
                        item.label(cx, ids!(title))
                            .set_text(cx, &candidate.room.display());
                        item.label(cx, ids!(detail)).set_text(
                            cx,
                            &format!("{}\n{}", candidate.room.room_id(), candidate.agents),
                        );
                        item.draw_all(cx, scope);
                    }
                }
            }
        }
        DrawStep::done()
    }
}
impl ApprovalInboxRef {
    pub fn action(
        &self,
        cx: &mut Cx,
        modal: ModalRef,
        action: &ApprovalInboxAction,
        app: &crate::app::AppState,
    ) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        match action {
            ApprovalInboxAction::Open { project } => {
                inner.indexes = Namespace::ALL
                    .into_iter()
                    .map(|ns| {
                        (
                            ns.base(),
                            if ns == Namespace::DEFAULT {
                                app.approval_markers.clone()
                            } else {
                                app.approval_markers_by_namespace
                                    .get(ns.base())
                                    .cloned()
                                    .unwrap_or_default()
                            },
                        )
                    })
                    .collect();
                inner.marker_client = get_client();
                inner.active = true;
                inner.project = project.clone();
                inner.load(cx);
                modal.open(cx);
            }
            ApprovalInboxAction::Close => {
                inner.active = false;
                inner.cancel();
                inner.marker_client = None;
                inner.indexes.clear();
                inner.candidates.clear();
                inner.request = nonce();
                modal.close(cx);
            }
            ApprovalInboxAction::Saved => {}
        }
    }
}
