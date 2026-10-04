//! The optional, named Space tree inside Chats. Only joined conversations are
//! listed; the existing Space lobby handles discovering and joining more rooms.
use std::collections::{HashMap, HashSet};

use makepad_widgets::*;
use matrix_sdk::{
    deserialized_responses::SyncOrStrippedState,
    ruma::{
        events::{space::child::SpaceChildEventContent, SyncStateEvent},
        OwnedRoomId, OwnedUserId,
    },
};

use crate::{
    app::SelectedRoom,
    logout::logout_confirm_modal::LogoutAction,
    shared::{
        navigation_bar_button::{NavigationBarButtonAction, NavigationBarButtonWidgetRefExt},
        room_filter_input_bar::MainFilterAction,
        unread_badge::UnreadBadgeWidgetRefExt,
    },
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};
use super::rooms_list::RoomsListAction;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.JoinedSpaces = #(JoinedSpaces::register_widget(vm)) {
        width: Fill height: Fill flow: Down
        status := Label {
            width: Fill height: Fit padding: 18
            flow: Flow.Right{wrap: true}
            draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
            text: #(crate::i18n::tr("Loading joined Spaces…")) i18n_text: "Loading joined Spaces…"
        }
        list := PortalList {
            width: Fill height: Fill flow: Down auto_tail: false
            Row := View {
                width: Fill height: 57 flow: Down
                row := NavigationBarButton {
                    width: Fill height: 56 flow: Right spacing: 10
                    align: Align{y: 0.5} padding: Inset{left: 16 right: 14}
                    draw_bg +: {color_hover: mod.widgets.RINX_BORDER color_active: mod.widgets.RINX_BORDER border_radius: 0}
                    indent := View {width: 0 height: 1}
                    disclosure := View {
                        width: 12 height: 14 flow: Overlay align: Align{x: 0.5 y: 0.5}
                        closed := View {
                            width: 8 height: 12
                            Icon {icon_walk: Walk{width: 8 height: 12} draw_icon +: {svg: ICON_CHEVRON_RIGHT color: mod.widgets.RINX_MUTED}}
                        }
                        opened := View {
                            width: 12 height: 8
                            Icon {icon_walk: Walk{width: 12 height: 8} draw_icon +: {svg: ICON_CHEVRON_DOWN color: mod.widgets.RINX_MUTED}}
                        }
                    }
                    icon := View {
                        width: 22 height: 22
                        folder := View {
                            width: 22 height: 22
                            Icon {
                                icon_walk: Walk{width: 22 height: 22}
                                draw_icon +: {svg: ICON_SQUARES color: mod.widgets.RINX_ACCENT}
                            }
                        }
                    }
                    name := Label {
                        width: Fill max_lines: 1 text_overflow: Ellipsis
                        draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12.5 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                    unread := UnreadBadge {}
                }
                SolidView {width: Fill height: 0.5 margin: Inset{left: 48} draw_bg.color: mod.widgets.RINX_BORDER}
            }
            Browse := View {
                width: Fill height: 42 flow: Down
                row := NavigationBarButton {
                    width: Fill height: Fill flow: Right align: Align{y: 0.5}
                    padding: Inset{left: 48 right: 14}
                    draw_bg +: {color_hover: mod.widgets.RINX_BORDER border_radius: 0}
                    indent := View {width: 0 height: 1}
                    name := Label {
                        text: #(crate::i18n::tr("Browse rooms ›")) i18n_text: "Browse rooms ›"
                        draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    name: RoomNameId,
    space: bool,
    children: Vec<OwnedRoomId>,
    unread: u64,
    marked: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TreeRow {
    id: OwnedRoomId,
    depth: usize,
    browse: bool,
    expanded: bool,
}

#[derive(Clone, Debug)]
struct LoadedSpaces {
    owner: OwnedUserId,
    request: u64,
    nodes: HashMap<OwnedRoomId, Node>,
    incomplete: bool,
}

#[derive(Script, ScriptHook, Widget)]
pub struct JoinedSpaces {
    #[deref]
    view: View,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    request: u64,
    #[rust]
    pending: bool,
    #[rust]
    active: bool,
    #[rust]
    loaded: bool,
    #[rust]
    incomplete: bool,
    #[rust]
    timer: Timer,
    #[rust]
    nodes: HashMap<OwnedRoomId, Node>,
    #[rust]
    expanded: HashSet<OwnedRoomId>,
    #[rust]
    rows: Vec<TreeRow>,
    #[rust]
    query: String,
}

impl JoinedSpaces {
    fn clear(&mut self, cx: &mut Cx) {
        self.owner = None;
        self.request = 0;
        self.pending = false;
        self.loaded = false;
        self.nodes.clear();
        self.rows.clear();
        self.expanded.clear();
        self.query.clear();
        self.redraw(cx);
    }

    fn refresh(&mut self, cx: &mut Cx) {
        if self.owner != current_user_id() {
            self.clear(cx);
        }
        if self.pending || !self.active {
            return;
        }
        let Some(client) = get_client() else { return };
        let Some(owner) = client.user_id().map(ToOwned::to_owned) else {
            return;
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let request = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.owner = Some(owner.clone());
        self.request = request;
        self.pending = true;
        // These reads use the SDK's synced local state store. No public room
        // directory or hierarchy membership is mistaken for joined membership.
        spawn_async_task(async move {
            let mut nodes = HashMap::new();
            let mut incomplete = false;
            for room in client.joined_rooms() {
                if crate::moments::is_moments(&room) {continue;}
                let mut children = Vec::new();
                if room.is_space() {
                    match room
                        .get_state_events_static::<SpaceChildEventContent>()
                        .await
                    {
                        Ok(events) => {
                            for raw in events {
                                if let Ok(SyncOrStrippedState::Sync(SyncStateEvent::Original(
                                    event,
                                ))) = raw.deserialize()
                                    && !event.content.via.is_empty()
                                {
                                    children.push(event.state_key);
                                }
                            }
                        }
                        Err(_) => incomplete = true,
                    }
                }
                nodes.insert(
                    room.room_id().to_owned(),
                    Node {
                        name: RoomNameId::from_room(&room).await,
                        space: room.is_space(),
                        children,
                        unread: room.num_unread_messages().max(room.num_unread_mentions()),
                        marked: room.is_marked_unread(),
                    },
                );
            }
            Cx::post_action(LoadedSpaces {
                owner,
                request,
                nodes,
                incomplete,
            });
        });
    }

    fn rebuild(&mut self, cx: &mut Cx) {
        let hidden = self
            .nodes
            .keys()
            .filter(|id| super::chat_actions::is_hidden(id))
            .cloned()
            .collect();
        self.rows = tree_rows(&self.nodes, &self.expanded, &self.query, &hidden);
        self.redraw(cx);
    }
}

impl Widget for JoinedSpaces {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        // PortalList can repeat a row for hover/down actions. React once, and
        // only to an actual click on the row's button.
        let mut clicked = HashSet::new();
        for (index, item) in self
            .view
            .portal_list(cx, ids!(list))
            .items_with_actions(&actions)
        {
            let uid = item.navigation_bar_button(cx, ids!(row)).widget_uid();
            if !clicked.insert(index)
                || !actions.iter().any(|a| {
                    a.as_widget_action().is_some_and(|a| a.widget_uid == uid)
                        && matches!(
                            a.as_widget_action().cast(),
                            NavigationBarButtonAction::Clicked
                        )
                })
            {
                continue;
            }
            let Some(row) = self.rows.get(index).cloned() else {
                continue;
            };
            let Some(node) = self.nodes.get(&row.id) else {
                continue;
            };
            if row.browse {
                cx.widget_action(
                    self.widget_uid(),
                    RoomsListAction::Selected(SelectedRoom::Space {
                        space_name_id: node.name.clone(),
                    }),
                );
            } else if node.space {
                if !self.expanded.remove(&row.id) {
                    self.expanded.insert(row.id);
                }
                self.rebuild(cx);
            } else {
                cx.widget_action(
                    self.widget_uid(),
                    RoomsListAction::Selected(SelectedRoom::JoinedRoom {
                        room_name_id: node.name.clone(),
                    }),
                );
            }
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if action.downcast_ref::<super::space_management::SpaceManagementChanged>().is_some_and(|changed|Some(&changed.owner)==current_user_id().as_ref()) {self.refresh(cx);}
                if let Some(LogoutAction::ClearAppState { .. }) = action.downcast_ref() {
                    self.clear(cx);
                } else if let Some(loaded) = action.downcast_ref::<LoadedSpaces>() {
                    if loaded.request == self.request
                        && Some(&loaded.owner) == current_user_id().as_ref()
                    {
                        self.nodes = loaded.nodes.clone();
                        self.pending = false;
                        self.loaded = true;
                        self.incomplete = loaded.incomplete;
                        self.expanded.retain(|id| self.nodes.contains_key(id));
                        self.rebuild(cx);
                    }
                } else if let Some(MainFilterAction::Changed(query)) = action.downcast_ref() {
                    self.query = query.trim().to_lowercase();
                    self.view
                        .portal_list(cx, ids!(list))
                        .set_first_id_and_scroll(0, 0.0);
                    self.rebuild(cx);
                } else if action
                    .downcast_ref::<super::chat_actions::ChatVisibilityChanged>()
                    .is_some()
                {
                    self.rebuild(cx);
                }
            }
        }
        if self.timer.is_event(event).is_some() {
            self.refresh(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let status = if !self.loaded {
            crate::i18n::tr("Loading joined Spaces…")
        } else if self.incomplete {
            crate::i18n::tr("Some Spaces could not be loaded. Retrying…")
        } else if self.rows.is_empty() && !self.query.is_empty() {
            crate::i18n::tr("No matching joined Spaces or rooms.")
        } else if self.rows.is_empty() {
            crate::i18n::tr("No joined Spaces yet. Find groups and Spaces in Discover.")
        } else {
            ""
        };
        self.view.label(cx, ids!(status)).set_text(cx, status);
        self.view
            .label(cx, ids!(status))
            .set_visible(cx, !status.is_empty());
        while let Some(widget) = self.view.draw_walk(cx, scope, walk).step() {
            let Some(mut list) = widget.borrow_mut::<PortalList>() else {
                continue;
            };
            if list.first_id() >= self.rows.len() {
                list.set_first_id_and_scroll(0, 0.0);
            }
            list.set_item_range(cx, 0, self.rows.len());
            for_next_visible(&mut list, cx, &self.rows, &self.nodes);
        }
        DrawStep::done()
    }
}

fn for_next_visible(
    list: &mut PortalList,
    cx: &mut Cx2d,
    rows: &[TreeRow],
    nodes: &HashMap<OwnedRoomId, Node>,
) {
    while let Some(index) = list.next_visible_item(cx) {
        let Some(row) = rows.get(index) else { continue };
        let node = &nodes[&row.id];
        let item = list.item(cx, index, if row.browse { id!(Browse) } else { id!(Row) });
        let indent = (row.depth.min(6) * 18) as f64;
        let mut spacer = item.view(cx, ids!(indent));
        script_apply_eval!(cx, spacer, {width: #(indent)});
        if !row.browse {
            item.label(cx, ids!(name))
                .set_text(cx, &node.name.display());
            item.view(cx, ids!(closed)).set_visible(cx, node.space && !row.expanded);
            item.view(cx, ids!(opened)).set_visible(cx, node.space && row.expanded);
            item.view(cx, ids!(folder)).set_visible(cx, node.space);
            let (unread, marked) = unread_total(nodes, &row.id);
            item.unread_badge(cx, ids!(unread))
                .update_counts(marked, 0, unread);
        }
        item.draw_all(cx, &mut Scope::empty());
    }
}

impl JoinedSpacesRef {
    pub fn set_active(&self, cx: &mut Cx, active: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.active == active {
                return;
            }
            inner.active = active;
            cx.stop_timer(inner.timer);
            if active {
                inner.timer = cx.start_interval(3.0);
                inner.refresh(cx);
            }
        }
    }
}

fn sorted_ids(
    nodes: &HashMap<OwnedRoomId, Node>,
    ids: impl Iterator<Item = OwnedRoomId>,
) -> Vec<OwnedRoomId> {
    let mut ids: Vec<_> = ids.filter(|id| nodes.contains_key(id)).collect();
    ids.sort_by_key(|id| {
        (
            !nodes[id].space,
            nodes[id].name.display().to_lowercase(),
            id.clone(),
        )
    });
    ids.dedup();
    ids
}

/// Graph traversal uses visited sets: a room can have several parents, and
/// malformed server state can contain cycles. Never double-count unread rooms.
fn reachable(nodes: &HashMap<OwnedRoomId, Node>, start: &OwnedRoomId) -> HashSet<OwnedRoomId> {
    let mut visited = HashSet::new();
    let mut todo = vec![start.clone()];
    while let Some(id) = todo.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        if let Some(node) = nodes.get(&id) {
            todo.extend(
                node.children
                    .iter()
                    .filter(|id| nodes.contains_key(*id))
                    .cloned(),
            );
        }
    }
    visited
}

fn unread_total(nodes: &HashMap<OwnedRoomId, Node>, start: &OwnedRoomId) -> (u64, bool) {
    reachable(nodes, start)
        .iter()
        .filter_map(|id| nodes.get(id))
        .filter(|n| !n.space)
        .fold((0u64, false), |(sum, marked), n| {
            (sum.saturating_add(n.unread), marked || n.marked)
        })
}

fn tree_rows(
    nodes: &HashMap<OwnedRoomId, Node>,
    expanded: &HashSet<OwnedRoomId>,
    query: &str,
    hidden: &HashSet<OwnedRoomId>,
) -> Vec<TreeRow> {
    let all = sorted_ids(
        nodes,
        nodes
            .iter()
            .filter(|(_, n)| n.space)
            .map(|(id, _)| id.clone()),
    );
    let children: HashSet<_> = nodes
        .values()
        .filter(|n| n.space)
        .flat_map(|n| n.children.iter())
        .collect();
    let mut roots: Vec<_> = all
        .iter()
        .filter(|id| !children.contains(id))
        .cloned()
        .collect();
    let mut covered = HashSet::new();
    for id in &roots {
        covered.extend(reachable(nodes, id));
    }
    // Every joined Space remains reachable, even in a cycle with no root.
    for id in &all {
        if !covered.contains(id) {
            roots.push(id.clone());
            covered.extend(reachable(nodes, id));
        }
    }
    let matches: HashSet<_> = nodes
        .iter()
        .filter(|(id, n)| {
            (query.is_empty() && !hidden.contains(*id))
                || (!query.is_empty()
                    && (n.name.display().to_lowercase().contains(query)
                        || id.as_str().to_lowercase().contains(query)))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let relevant: HashSet<_> = all
        .iter()
        .filter(|id| reachable(nodes, id).iter().any(|id| matches.contains(id)))
        .cloned()
        .collect();
    let mut rows = Vec::new();
    fn append(
        nodes: &HashMap<OwnedRoomId, Node>,
        expanded: &HashSet<OwnedRoomId>,
        relevant: &HashSet<OwnedRoomId>,
        matches: &HashSet<OwnedRoomId>,
        query: &str,
        id: &OwnedRoomId,
        depth: usize,
        inherited_match: bool,
        path: &mut HashSet<OwnedRoomId>,
        rows: &mut Vec<TreeRow>,
    ) {
        if depth > 32 || !path.insert(id.clone()) {
            return;
        }
        let node = &nodes[id];
        let show_children = !query.is_empty() && (inherited_match || matches.contains(id));
        if inherited_match || matches.contains(id) || relevant.contains(id) {
            let open = node.space && (expanded.contains(id) || !query.is_empty());
            rows.push(TreeRow {
                id: id.clone(),
                depth,
                browse: false,
                expanded: open,
            });
            if open {
                for child in sorted_ids(nodes, node.children.iter().cloned()) {
                    append(
                        nodes,
                        expanded,
                        relevant,
                        matches,
                        query,
                        &child,
                        depth + 1,
                        show_children,
                        path,
                        rows,
                    );
                }
                rows.push(TreeRow {
                    id: id.clone(),
                    depth,
                    browse: true,
                    expanded: false,
                });
            }
        }
        path.remove(id);
    }
    for id in roots {
        append(
            nodes,
            expanded,
            &relevant,
            &matches,
            query,
            &id,
            0,
            false,
            &mut HashSet::new(),
            &mut rows,
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(name: &str) -> OwnedRoomId {
        format!("!{name}:example.org").parse().unwrap()
    }
    fn node(name: &str, space: bool, children: &[&str], unread: u64) -> Node {
        Node {
            name: RoomNameId::new(matrix_sdk::RoomDisplayName::Named(name.into()), id(name)),
            space,
            children: children.iter().map(|name| id(name)).collect(),
            unread,
            marked: false,
        }
    }
    fn graph() -> HashMap<OwnedRoomId, Node> {
        [
            node("Work", true, &["Projects", "General", "NotJoined"], 99),
            node("Projects", true, &["Work", "General", "设计"], 99),
            node("Friends", true, &["General"], 0),
            node("General", false, &[], 3),
            node("设计", false, &[], 2),
        ]
        .into_iter()
        .map(|n| (n.name.room_id().clone(), n))
        .collect()
    }
    #[test]
    fn cycles_and_multiple_parents_keep_spaces_reachable_and_unreads_unique() {
        let nodes = graph();
        let expanded = nodes.keys().cloned().collect();
        let rows = tree_rows(&nodes, &expanded, "", &HashSet::new());
        assert!(rows.len() < 20);
        for name in ["Work", "Projects", "Friends", "General", "设计"] {
            assert!(rows.iter().any(|r| r.id == id(name) && !r.browse));
        }
        assert!(!rows.iter().any(|r| r.id == id("NotJoined")));
        assert_eq!(unread_total(&nodes, &id("Work")), (5, false));
        assert_eq!(unread_total(&nodes, &id("Friends")), (3, false));
    }
    #[test]
    fn search_finds_nested_cjk_room_and_retains_ancestor_context() {
        let rows = tree_rows(&graph(), &HashSet::new(), "设计", &HashSet::new());
        let names: Vec<_> = rows
            .iter()
            .filter(|r| !r.browse)
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(names, vec![id("Projects"), id("Work"), id("设计")]);
        assert!(
            rows.iter()
                .find(|r| r.id == id("Projects"))
                .unwrap()
                .expanded
        );
    }
    #[test]
    fn hidden_rooms_stay_hidden_until_search_and_collapse_hides_children() {
        let nodes = graph();
        let expanded = nodes.keys().cloned().collect();
        let hidden = HashSet::from([id("General")]);
        assert!(
            !tree_rows(&nodes, &expanded, "", &hidden)
                .iter()
                .any(|r| r.id == id("General"))
        );
        assert!(
            tree_rows(&nodes, &expanded, "general", &hidden)
                .iter()
                .any(|r| r.id == id("General"))
        );
        assert!(
            tree_rows(&nodes, &HashSet::new(), "", &hidden)
                .iter()
                .all(|r| r.depth == 0 && !r.browse)
        );
    }
}
