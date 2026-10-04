//! Presentation-only folding and incremental reveal. The Matrix body is never modified.
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use makepad_widgets::*;
use matrix_sdk::ruma::events::room::message::FormattedBody;
use matrix_sdk_ui::timeline::EventTimelineItem;
use unicode_segmentation::UnicodeSegmentation;
use crate::shared::html_or_plaintext::HtmlOrPlaintextWidgetExt;

const STALL: Duration = Duration::from_secs(300);

pub fn fold_preview(body: &str) -> Option<String> {
    if body.lines().count() <= 8 && body.graphemes(true).count() <= 1200 {
        return None;
    }
    let preview = body
        .trim_start()
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    if preview.is_empty() {
        return None;
    }
    Some(format!(
        "{}…",
        preview.graphemes(true).take(400).collect::<String>()
    ))
}

pub fn content_is_live(content: &serde_json::Value) -> bool {
    let content = content.get("m.new_content").unwrap_or(content);
    matches!(
        content.get("org.matrix.msc4357.live"),
        Some(serde_json::Value::Object(_)) | Some(serde_json::Value::Bool(true))
    )
}

pub fn event_is_live(event: &EventTimelineItem) -> bool {
    let edited = event.content().as_message().is_some_and(|m| m.is_edited());
    event
        .latest_edit_json()
        .or_else(|| (!edited).then(|| event.original_json()).flatten())
        .and_then(|raw| raw.get_field::<serde_json::Value>("content").ok())
        .flatten()
        .is_some_and(|content| content_is_live(&content))
}

#[derive(Default)]
struct ExpandedReplies(HashSet<String>);

pub fn clear_session(cx: &mut Cx) {
    cx.global::<ExpandedReplies>().0.clear();
}

pub fn event_is_fresh(event: &EventTimelineItem) -> bool {
    let timestamp = event
        .latest_edit_json()
        .and_then(|raw| raw.get_field::<u64>("origin_server_ts").ok())
        .flatten()
        .unwrap_or_else(|| u64::from(event.timestamp().0));
    super::approval::current_unix_time_millis().saturating_sub(timestamp) < 300_000
}

#[derive(Default)]
struct Reveal {
    target: String,
    visible_bytes: usize,
    live: bool,
    stalled: bool,
    changed: Option<Instant>,
}
impl Reveal {
    fn update(&mut self, body: &str, live: bool, fresh: bool, now: Instant) -> bool {
        if self.target == body && self.live == live {
            return false;
        }
        let shown = &self.target[..self.visible_bytes];
        let common_bytes: usize = shown
            .graphemes(true)
            .zip(body.graphemes(true))
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len())
            .sum();
        self.visible_bytes = if live && fresh {
            common_bytes
        } else {
            body.len()
        };
        self.target = body.to_owned();
        self.live = live;
        self.stalled = live && !fresh;
        self.changed = Some(now);
        true
    }
    fn tick(&mut self, now: Instant) {
        if self
            .changed
            .is_some_and(|time| now.duration_since(time) >= STALL)
        {
            self.stalled = self.live;
            self.visible_bytes = self.target.len();
            return;
        }
        let remaining = &self.target[self.visible_bytes..];
        let count = remaining.graphemes(true).count();
        // Catch up in about a second even after a large network batch.
        self.visible_bytes += remaining
            .graphemes(true)
            .take((count / 8).max(8))
            .map(str::len)
            .sum::<usize>();
    }
    fn active(&self) -> bool {
        self.live && !self.stalled
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.AgentReply = #(AgentReply::register_widget(vm)) {
        ..mod.widgets.View
        visible: false width: Fill height: Fit flow: Down spacing: 4
        agent_reply_body := HtmlOrPlaintext {}
        stream_status := Label {
            visible: false width: Fill height: Fit
            draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}
        }
        agent_reply_fold := ButtonFlat {
            visible: false width: Fit height: 32 padding: 4
            draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct AgentReply {
    #[deref]
    view: View,
    #[rust]
    identity: String,
    #[rust]
    reveal: Reveal,
    #[rust]
    formatted: Option<FormattedBody>,
    #[rust]
    timer: Timer,
    #[rust]
    phase: usize,
    #[rust]
    rendered: Option<(String, bool)>,
}
impl AgentReply {
    fn render(&mut self, cx: &mut Cx) {
        let active = self.reveal.active();
        let preview = (!active)
            .then(|| fold_preview(&self.reveal.target))
            .flatten();
        let expanded = cx.global::<ExpandedReplies>().0.contains(&self.identity);
        let folded = preview.is_some() && !expanded;
        let body = if active {
            &self.reveal.target[..self.reveal.visible_bytes]
        } else if folded {
            preview.as_deref().unwrap()
        } else {
            &self.reveal.target
        };
        let rich = !active && !folded;
        // Avoid reparsing the same HTML on every frame.
        let key = (body.to_owned(), rich);
        if self.rendered.as_ref() != Some(&key) {
            let content = self.view.html_or_plaintext(cx, ids!(agent_reply_body));
            crate::home::room_screen::populate_text_message_content(
                cx,
                &content,
                body,
                if rich { self.formatted.as_ref() } else { None },
                None,
                None,
                None,
                None,
            );
            self.rendered = Some(key);
        }
        let toggle = self.view.button(cx, ids!(agent_reply_fold));
        toggle.set_visible(cx, preview.is_some());
        toggle.set_text(
            cx,
            crate::i18n::tr(if folded { "Show more" } else { "Show less" }),
        );
        let status = self.view.label(cx, ids!(stream_status));
        status.set_visible(cx, active || self.reveal.stalled);
        status.set_text(
            cx,
            &if self.reveal.stalled {
                crate::i18n::tr("Response paused; waiting for an update").to_owned()
            } else {
                format!(
                    "{}{}",
                    crate::i18n::tr("Receiving response"),
                    ".".repeat(self.phase % 3 + 1)
                )
            },
        );
    }
}
impl Widget for AgentReply {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if self.timer.is_event(event).is_some() {
            self.reveal.tick(Instant::now());
            self.phase += 1;
            self.timer = if self.reveal.active() {
                cx.start_timeout(0.1)
            } else {
                Timer::empty()
            };
            self.render(cx);
            self.redraw(cx);
        }
        if let Event::Actions(actions) = event {
            if self
                .view
                .button(cx, ids!(agent_reply_fold))
                .clicked(actions)
            {
                let expanded = &mut cx.global::<ExpandedReplies>().0;
                if !expanded.remove(&self.identity) {
                    if expanded.len() >= 2048 {
                        expanded.clear();
                    }
                    expanded.insert(self.identity.clone());
                }
                self.render(cx);
                self.redraw(cx);
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.render(cx);
        self.view.draw_walk(cx, scope, walk)
    }
}
impl AgentReplyRef {
    pub fn populate(
        &self,
        cx: &mut Cx,
        identity: String,
        body: &str,
        formatted: Option<&FormattedBody>,
        live: bool,
        fresh: bool,
    ) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.view.set_visible(cx, true);
        if inner.identity != identity {
            cx.stop_timer(inner.timer);
            inner.timer = Timer::empty();
            inner.identity = identity;
            inner.reveal = Reveal::default();
            inner.rendered = None;
        }
        if inner.formatted.as_ref().map(|f| (&f.body, &f.format))
            != formatted.map(|f| (&f.body, &f.format))
        {
            inner.formatted = formatted.cloned();
            inner.rendered = None;
        }
        inner.reveal.update(body, live, fresh, Instant::now());
        if inner.reveal.active() && inner.timer.0 == 0 {
            inner.timer = cx.start_timeout(0.1);
        }
        if !inner.reveal.active() {
            cx.stop_timer(inner.timer);
            inner.timer = Timer::empty();
        }
        inner.render(cx);
    }
    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            cx.stop_timer(inner.timer);
            inner.timer = Timer::empty();
            inner.view.set_visible(cx, false);
            inner.identity.clear();
            inner.reveal = Reveal::default();
            inner.rendered = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folding_is_bounded_and_preserves_graphemes() {
        assert!(fold_preview("short 中文").is_none());
        let body = "👨‍👩‍👧‍👦中文".repeat(500);
        let preview = fold_preview(&body).unwrap();
        assert_eq!(preview.graphemes(true).count(), 401);
        assert!(body.starts_with(preview.trim_end_matches('…')));
        assert_eq!(
            fold_preview(&"line\n".repeat(10)).unwrap(),
            "line\nline\nline…"
        );
    }
    #[test]
    fn reveal_handles_rewrites_completion_stalls_and_historical_messages() {
        let now = Instant::now();
        let mut r = Reveal::default();
        r.update("Hello 👨‍👩‍👧‍👦 中文", true, true, now);
        r.tick(now);
        r.update("Hi 中文", true, true, now);
        r.tick(now);
        assert!(r.target.is_char_boundary(r.visible_bytes));
        r.update("Final answer 中文", false, true, now);
        assert_eq!(r.visible_bytes, r.target.len());
        assert!(!r.active());
        r.update("unfinished", true, true, now);
        r.tick(now + STALL);
        assert!(r.stalled);
        assert!(!r.active());
        r.update("old live text", true, false, now);
        assert_eq!(r.visible_bytes, r.target.len());
        assert!(!r.active());
    }
    #[test]
    fn final_edit_removes_the_live_marker() {
        assert!(content_is_live(
            &serde_json::json!({"org.matrix.msc4357.live": {}})
        ));
        assert!(!content_is_live(
            &serde_json::json!({"org.matrix.msc4357.live": true, "m.new_content": {"body":"done"}})
        ));
        assert!(!content_is_live(
            &serde_json::json!({"org.matrix.msc4357.live": "invalid"})
        ));
    }
}
