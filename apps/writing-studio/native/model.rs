//! Matrix identity is kept by the Rinx adapter; consent is host-independent.
//!
//! This module also holds the studio's domain model: documents, rewrite
//! tasks, the decision log and the undo path. All state transitions that
//! change a document or a task (apply / discard / undo / expire / validate)
//! are pure functions or methods here, unit-tested at the bottom.
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use article_core::host::{Capabilities, Capability, ConsentGrant};
use super::host::AUTHORITY;
use ruma::{OwnedRoomId, OwnedUserId};
use serde::{Deserialize, Serialize};

pub use article_core::bindings::SOURCE;
pub fn invalidate_sessions() { AUTHORITY.invalidate(); }

/// A reference to a built-in, reviewed package. The build is the trust root;
/// this is deliberately NOT a remotely asserted publisher signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingPackage {
    pub app_id: String,
    pub version: u32,
    pub source_hash: String,
}
impl WritingPackage {
    pub fn builtin() -> Self {
        Self {
            app_id: crate::writing_studio::APP_ID.into(),
            version: 1,
            source_hash: blake3::hash(SOURCE.as_bytes()).to_hex().to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grant {
    pub owner: OwnedUserId,
    pub instance: String,
    pub(super) lease: ConsentGrant,
}
impl Grant {
    pub fn new(owner: OwnedUserId) -> Self {
        let lease = AUTHORITY.issue(owner.to_string(), Capabilities::editor(), Duration::from_secs(3600));
        Self::from_lease(owner, lease)
    }
    pub(super) fn from_lease(owner: OwnedUserId, lease: ConsentGrant) -> Self {
        Self { owner, instance: lease.instance().into(), lease }
    }
    pub fn valid(&self, owner: Option<&ruma::UserId>) -> bool {
        AUTHORITY.valid(&self.lease, owner.map(|o| o.as_str()))
    }
    pub fn authorize(&self, capability: Capability) -> Result<(), String> {
        use article_core::host::ArticleHost;
        super::host::RobrixWritingHost::new(crate::app_data_dir()).authorize(&self.lease, capability)
    }
    pub fn revoke(&self) { self.lease.revoke(); }
}

// The room shown in Rinx now: where an applied or discarded rewrite is
// reported back. Set by the app when the room focus changes, as the
// assistant's current-room view is.
thread_local! {
    static CURRENT_ROOM: RefCell<Option<OwnedRoomId>> = const { RefCell::new(None) };
}
pub fn set_current_room(room: Option<OwnedRoomId>) {
    CURRENT_ROOM.with(|r| *r.borrow_mut() = room);
}
pub fn current_room() -> Option<OwnedRoomId> {
    CURRENT_ROOM.with(|r| r.borrow().clone())
}

// ---------------------------------------------------------------------------
// Domain model
// ---------------------------------------------------------------------------

/// A proposal stays reviewable for this long; afterwards it must be
/// regenerated from the latest document (demo TTL, ten minutes).
pub const PROPOSAL_TTL_SECS: u64 = 600;

static TX_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Transaction ids make apply idempotent: timestamp + a process-local atomic
/// counter, no uuid dependency.
pub fn new_tx_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = TX_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("tx-{nanos:x}-{n}")
}

/// FNV-1a over the bytes: a cheap, stable content hash for change detection.
pub fn hash_text(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// `[^n]` citation markers, in first-use order, without duplicates.
pub fn citations(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'^' {
            if let Some(end) = text[i + 2..].find(']') {
                let marker = &text[i..i + 3 + end];
                if marker[2..marker.len() - 1].chars().all(|c| c.is_ascii_digit())
                    && !marker[2..marker.len() - 1].is_empty()
                    && !found.iter().any(|m| m == marker)
                {
                    found.push(marker.to_owned());
                }
                i += 3 + end;
                continue;
            }
        }
        i += 1;
    }
    found
}

/// Citation markers present in `before` but missing from `after`: the agent
/// must never silently drop a source.
pub fn missing_citations(before: &str, after: &str) -> Vec<String> {
    citations(before)
        .into_iter()
        .filter(|m| !after.contains(m.as_str()))
        .collect()
}

/// One document: a title plus paragraphs (joined with a blank line in the
/// editor). `version` bumps on every mutation and guards stale proposals.
/// `modified` (epoch seconds) feeds the drafts-shelf "last edited" line.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub paragraphs: Vec<String>,
    pub version: u64,
    #[serde(default)]
    pub modified: u64,
}
impl Document {
    pub fn new(title: String, paragraphs: Vec<String>) -> Self {
        Self {
            id: article_core::document::new_id(),
            title,
            paragraphs,
            version: 1,
            modified: article_core::document::now(),
        }
    }
    /// The editor's text: paragraphs separated by one blank line.
    pub fn body(&self) -> String {
        self.paragraphs.join("\n\n")
    }
    /// Draft-card summary: the first non-empty paragraph, cut at ~40 chars.
    pub fn preview(&self, limit: usize) -> String {
        let first = self.paragraphs.iter().find(|p| !p.trim().is_empty()).map(String::as_str).unwrap_or("");
        let mut text: String = first.trim().chars().take(limit).collect();
        if first.trim().chars().count() > limit {
            text.push('…');
        }
        text
    }
    pub fn set_body(&mut self, body: &str) {
        let paragraphs: Vec<String> = body
            .split("\n\n")
            .map(|p| p.trim_matches('\n').to_owned())
            .collect();
        let paragraphs = if paragraphs.is_empty() { vec![String::new()] } else { paragraphs };
        // Only a real change bumps the version: stale-proposal detection
        // compares it against the task's snapshot.
        if self.paragraphs != paragraphs {
            self.paragraphs = paragraphs;
            self.version += 1;
            self.modified = article_core::document::now();
        }
    }
    /// Maps a byte range in the joined body to (paragraph, start, end) within
    /// that paragraph. A range spanning paragraphs is clamped to the one it
    /// starts in; a range outside the body is `None`.
    pub fn locate(&self, body_start: usize, body_end: usize) -> Option<(usize, usize, usize)> {
        let mut offset = 0usize;
        for (index, para) in self.paragraphs.iter().enumerate() {
            let para_end = offset + para.len();
            if body_start >= offset && body_start <= para_end {
                let start = body_start - offset;
                let end = body_end.saturating_sub(offset).min(para.len()).max(start);
                // Stay on char boundaries: selections from the TextInput are,
                // but the clamp above may not be.
                let (mut start, mut end) = (start, end);
                while !para.is_char_boundary(start) && start > 0 { start -= 1; }
                while !para.is_char_boundary(end) && end < para.len() { end += 1; }
                return Some((index, start, end));
            }
            offset = para_end + 2; // the "\n\n" separator
        }
        None
    }
    pub fn para_hash(&self, index: usize) -> Option<u64> {
        self.paragraphs.get(index).map(|p| hash_text(p))
    }
}

/// Rewrite constraints the person configures next to the request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    pub concise: bool,
    pub formal: bool,
    pub keep_citations: bool,
}

/// Where in the document the task operates: one passage in one paragraph,
/// plus the hashes that let apply detect a meanwhile-changed document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub para_index: usize,
    pub start: usize,
    pub end: usize,
    pub text_snapshot: String,
    pub para_hash: u64,
    pub doc_version: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Preparing,
    Ready,
    Applied,
    Discarded,
    Failed(String),
    Expired,
}
impl TaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Preparing => "Preparing",
            Self::Ready => "Ready",
            Self::Applied => "Applied",
            Self::Discarded => "Discarded",
            Self::Failed(_) => "Failed",
            Self::Expired => "Expired",
        }
    }
}

/// A rewrite task: selection + request in, a reviewable proposal out, and a
/// full lifecycle the decision log can trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewriteTask {
    pub id: String,
    pub doc_id: String,
    pub selection: Selection,
    pub request: String,
    pub constraints: Constraints,
    pub proposal: Option<String>,
    pub status: TaskStatus,
    pub tx_id: String,
    pub created_at: u64,
    pub proposal_at: Option<u64>,
}
impl RewriteTask {
    pub fn new(doc: &Document, selection: Selection, request: String, constraints: Constraints, now: u64) -> Self {
        Self {
            id: article_core::document::new_id(),
            doc_id: doc.id.clone(),
            selection,
            request,
            constraints,
            proposal: None,
            status: TaskStatus::Preparing,
            tx_id: new_tx_id(),
            created_at: now,
            proposal_at: None,
        }
    }
}

/// What applying produced, kept so the person can undo back to the original
/// wording. `before`/`after` are the replaced and inserted passage texts.
#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub task_id: String,
    pub doc_id: String,
    pub para_index: usize,
    pub start: usize,
    pub before: String,
    pub after: String,
}

/// One trace line: read state → proposed action → user decision → execution
/// → verification, plus failures, expiry and undo.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    pub at: u64,
    pub task_id: String,
    pub stage: String,
    pub detail: String,
}
impl Decision {
    pub fn new(task_id: &str, stage: &str, detail: String, now: u64) -> Self {
        Self { at: now, task_id: task_id.to_owned(), stage: stage.to_owned(), detail }
    }
}

/// The proposal targets a document the person may have edited since. Apply
/// only when the paragraph content and the document version both still match
/// the snapshot; anything else means "regenerate from the latest document".
pub fn validate_target(doc: &Document, task: &RewriteTask) -> Result<(), String> {
    if doc.id != task.doc_id {
        return Err("The proposal belongs to another document.".into());
    }
    if doc.version != task.selection.doc_version {
        return Err("Document changed; regenerate the proposal".into());
    }
    match doc.para_hash(task.selection.para_index) {
        Some(hash) if hash == task.selection.para_hash => Ok(()),
        _ => Err("Document changed; regenerate the proposal".into()),
    }
}

/// Marks a stale task expired. Preparing ages from creation, Ready from the
/// proposal time. Returns whether the task changed.
pub fn expire_if_stale(task: &mut RewriteTask, now: u64) -> bool {
    let stale_since = match task.status {
        TaskStatus::Preparing => Some(task.created_at),
        TaskStatus::Ready => task.proposal_at,
        _ => None,
    };
    if let Some(at) = stale_since {
        if now.saturating_sub(at) > PROPOSAL_TTL_SECS {
            task.status = TaskStatus::Expired;
            return true;
        }
    }
    false
}

#[derive(Debug)]
pub enum ApplyOutcome {
    Applied(UndoEntry),
    /// Same transaction retried: already applied, no duplicate modification.
    AlreadyApplied,
}

/// Replaces the selected passage with the reviewed proposal. Idempotent per
/// task: an already-applied task reports success without touching the
/// document again.
pub fn apply_proposal(
    doc: &mut Document,
    task: &mut RewriteTask,
    proposal: &str,
    now: u64,
) -> Result<ApplyOutcome, String> {
    if matches!(task.status, TaskStatus::Applied) {
        return Ok(ApplyOutcome::AlreadyApplied);
    }
    if expire_if_stale(task, now) {
        return Err("Proposal may have expired; regenerate from the latest document".into());
    }
    if !matches!(task.status, TaskStatus::Ready) {
        return Err("The proposal is not ready to apply.".into());
    }
    validate_target(doc, task)?;
    let citations_lost = missing_citations(&task.selection.text_snapshot, proposal);
    if !citations_lost.is_empty() {
        return Err(format!("Citation markers cannot be removed: {}", citations_lost.join(" ")));
    }
    let para = doc
        .paragraphs
        .get_mut(task.selection.para_index)
        .ok_or_else(|| "Document changed; regenerate the proposal".to_string())?;
    let (start, end) = (task.selection.start, task.selection.end);
    if end > para.len() || start > end || &para[start..end] != task.selection.text_snapshot.as_str() {
        return Err("Document changed; regenerate the proposal".into());
    }
    let undo = UndoEntry {
        task_id: task.id.clone(),
        doc_id: doc.id.clone(),
        para_index: task.selection.para_index,
        start,
        before: task.selection.text_snapshot.clone(),
        after: proposal.to_owned(),
    };
    para.replace_range(start..end, proposal);
    doc.version += 1;
    doc.modified = now;
    task.proposal = Some(proposal.to_owned());
    task.status = TaskStatus::Applied;
    Ok(ApplyOutcome::Applied(undo))
}

/// Discarding records the decision and never touches the document.
pub fn discard_task(task: &mut RewriteTask) {
    task.status = TaskStatus::Discarded;
}

/// Restores the original passage, keeping the proposal around as Ready so it
/// can be reviewed again. Verifies the applied text is still in place first.
pub fn undo_apply(doc: &mut Document, task: &mut RewriteTask, entry: &UndoEntry) -> Result<(), String> {
    if !matches!(task.status, TaskStatus::Applied) || task.id != entry.task_id {
        return Err("Nothing to undo.".into());
    }
    let para = doc
        .paragraphs
        .get_mut(entry.para_index)
        .ok_or_else(|| "Undo failed: the paragraph is gone.".to_string())?;
    let end = entry.start + entry.after.len();
    if end > para.len() || &para[entry.start..end] != entry.after.as_str() {
        return Err("Undo failed: the applied passage changed meanwhile.".into());
    }
    para.replace_range(entry.start..end, &entry.before);
    doc.version += 1;
    doc.modified = article_core::document::now();
    task.status = TaskStatus::Ready;
    Ok(())
}

// ---------------------------------------------------------------------------
// The shared in-memory studio: both card faces (in-app modal and the desktop
// window) read and write this one store, so they can never drift apart.
// `version` bumps on every mutation; faces poll it and rebind on change.
// ---------------------------------------------------------------------------

pub struct Studio {
    pub loaded_for: Option<String>,
    pub documents: Vec<Document>,
    pub tasks: Vec<RewriteTask>,
    pub decisions: Vec<Decision>,
    pub undo: Vec<UndoEntry>,
    pub grant: Option<Grant>,
    /// The face-shared focus: the document and task a desktop card opens on.
    pub focus_doc: Option<String>,
    pub focus_task: Option<String>,
    pub version: u64,
}
impl Studio {
    const fn empty() -> Self {
        Self {
            loaded_for: None,
            documents: Vec::new(),
            tasks: Vec::new(),
            decisions: Vec::new(),
            undo: Vec::new(),
            grant: None,
            focus_doc: None,
            focus_task: None,
            version: 0,
        }
    }
    pub fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
    pub fn document(&self, id: &str) -> Option<&Document> {
        self.documents.iter().find(|d| d.id == id)
    }
    pub fn document_mut(&mut self, id: &str) -> Option<&mut Document> {
        self.documents.iter_mut().find(|d| d.id == id)
    }
    pub fn task(&self, id: &str) -> Option<&RewriteTask> {
        self.tasks.iter().find(|t| t.id == id)
    }
    pub fn task_mut(&mut self, id: &str) -> Option<&mut RewriteTask> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }
    pub fn tasks_of(&self, doc_id: &str) -> Vec<&RewriteTask> {
        self.tasks.iter().filter(|t| t.doc_id == doc_id).collect()
    }
    pub fn log(&mut self, task_id: &str, stage: &str, detail: String) {
        self.decisions.push(Decision::new(task_id, stage, detail, article_core::document::now()));
    }
}

/// Process-global store (not thread-local): background LLM threads write
/// their results back into this same store, then bump `version` so both card
/// faces pick the change up on their next sync poll.
static STUDIO: Mutex<Studio> = Mutex::new(Studio::empty());
/// Runs `f` with exclusive access to the shared studio store.
pub fn studio<R>(f: impl FnOnce(&mut Studio) -> R) -> R {
    let mut guard = STUDIO.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}
pub fn studio_version() -> u64 {
    studio(|s| s.version)
}
/// Drops all in-memory state (logout / account switch). On-disk documents,
/// tasks and the decision log survive in the account's directory.
pub fn clear_studio() {
    studio(|s| *s = Studio::empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document {
            id: "doc-1".into(),
            title: "Demo".into(),
            paragraphs: vec![
                "第一段保持不动。".to_string(),
                "这是一个非常冗长的段落，其实可以说它基本上包含了太多的废话与引用[^1]。".to_string(),
            ],
            version: 7,
            modified: 0,
        }
    }
    fn task(doc: &Document) -> RewriteTask {
        let para = &doc.paragraphs[1];
        let selection = Selection {
            para_index: 1,
            start: 0,
            end: para.len(),
            text_snapshot: para.clone(),
            para_hash: hash_text(para),
            doc_version: doc.version,
        };
        let mut task = RewriteTask::new(doc, selection, "写得更简洁".into(), Constraints::default(), 1000);
        task.proposal = Some("这个段落更简洁，引用[^1]仍在。".into());
        task.status = TaskStatus::Ready;
        task.proposal_at = Some(1000);
        task
    }

    #[test]
    fn apply_replaces_selection_and_is_idempotent() {
        let mut doc = doc();
        let mut task = task(&doc);
        let first = apply_proposal(&mut doc, &mut task, "这个段落更简洁，引用[^1]仍在。", 1001).unwrap();
        let undo = match first {
            ApplyOutcome::Applied(undo) => undo,
            _ => panic!("first apply must apply"),
        };
        assert_eq!(doc.paragraphs[1], "这个段落更简洁，引用[^1]仍在。");
        assert_eq!(doc.version, 8);
        assert_eq!(task.status, TaskStatus::Applied);
        assert_eq!(undo.before, "这是一个非常冗长的段落，其实可以说它基本上包含了太多的废话与引用[^1]。");
        // A retry with the same task succeeds without changing the document.
        let retry = apply_proposal(&mut doc, &mut task, "别的文本", 1002).unwrap();
        assert!(matches!(retry, ApplyOutcome::AlreadyApplied));
        assert_eq!(doc.paragraphs[1], "这个段落更简洁，引用[^1]仍在。");
        assert_eq!(doc.version, 8);
    }

    #[test]
    fn discard_keeps_the_document() {
        let mut doc = doc();
        let mut task = task(&doc);
        discard_task(&mut task);
        assert_eq!(task.status, TaskStatus::Discarded);
        assert_eq!(doc.paragraphs[1], "这是一个非常冗长的段落，其实可以说它基本上包含了太多的废话与引用[^1]。");
        // A discarded task cannot be applied later.
        assert!(apply_proposal(&mut doc, &mut task, "x", 1001).is_err());
    }

    #[test]
    fn undo_restores_the_original_wording() {
        let mut doc = doc();
        let mut task = task(&doc);
        let original = doc.paragraphs[1].clone();
        let outcome = apply_proposal(&mut doc, &mut task, "这个段落更简洁，引用[^1]仍在。", 1001).unwrap();
        let ApplyOutcome::Applied(undo) = outcome else { panic!() };
        undo_apply(&mut doc, &mut task, &undo).unwrap();
        assert_eq!(doc.paragraphs[1], original);
        assert_eq!(task.status, TaskStatus::Ready);
    }

    #[test]
    fn expired_proposals_cannot_be_applied() {
        let mut doc = doc();
        let mut task = task(&doc);
        let result = apply_proposal(&mut doc, &mut task, "这个段落更简洁，引用[^1]仍在。", 1000 + PROPOSAL_TTL_SECS + 1);
        assert!(result.is_err());
        assert_eq!(task.status, TaskStatus::Expired);
        // expire_if_stale reports the transition exactly once.
        assert!(!expire_if_stale(&mut task, 1000 + PROPOSAL_TTL_SECS + 2));
    }

    #[test]
    fn changed_selection_blocks_apply() {
        let mut doc = doc();
        let mut task = task(&doc);
        // The person edits the paragraph after the proposal was made.
        doc.set_body("第一段保持不动。\n\n被手动改过的段落。");
        let result = apply_proposal(&mut doc, &mut task, "这个段落更简洁，引用[^1]仍在。", 1001);
        assert_eq!(result.unwrap_err(), "Document changed; regenerate the proposal");
        assert_eq!(doc.paragraphs[1], "被手动改过的段落。");
        assert_eq!(task.status, TaskStatus::Ready);
    }

    #[test]
    fn citations_are_extracted_and_missing_ones_detected() {
        assert_eq!(citations("甲[^1]乙[^2]丙[^1]"), vec!["[^1]".to_string(), "[^2]".to_string()]);
        assert_eq!(missing_citations("甲[^1]乙[^2]", "甲[^1]"), vec!["[^2]".to_string()]);
        assert!(missing_citations("甲[^1]", "甲[^1]乙[^3]").is_empty());
    }

    #[test]
    fn dropped_citations_block_apply() {
        let mut doc = doc();
        let mut task = task(&doc);
        let result = apply_proposal(&mut doc, &mut task, "这个段落更简洁，引用丢了。", 1001);
        assert!(result.unwrap_err().contains("[^1]"));
        assert_eq!(task.status, TaskStatus::Ready);
    }

    #[test]
    fn locate_maps_body_ranges_to_paragraphs() {
        let doc = doc();
        let body = doc.body();
        let start = doc.paragraphs[0].len() + 2;
        let (para, s, e) = doc.locate(start, start + 6).unwrap();
        assert_eq!(para, 1);
        assert_eq!(&doc.paragraphs[1][s..e], &body[start..start + 6]);
        // A range starting in paragraph 0 but crossing into 1 clamps to 0.
        // Byte 3 is the first boundary after "第" (CJK chars are 3 bytes).
        let (para, s, e) = doc.locate(3, start + 3).unwrap();
        assert_eq!(para, 0);
        assert_eq!((s, e), (3, doc.paragraphs[0].len()));
    }
}
