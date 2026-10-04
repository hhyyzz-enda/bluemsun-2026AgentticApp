//! Account- and instance-scoped storage for the writing studio.
//!
//! The shared `article-core` store hardcodes the article editor's app id, so
//! this app keeps its own directory beside it: the same account isolation and
//! atomic replacement contract, a much smaller schema. Documents and tasks
//! rewrite whole files atomically; the decision log is an append-only JSONL
//! trail, as an audit log should be.
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use serde::{Deserialize, Serialize};
use article_core::{
    host::{ArticleHost, Capability},
    storage::atomic_write,
};
use super::{
    host::RobrixWritingHost,
    model::{Decision, Document, Grant, RewriteTask},
};

const APP_ID: &str = crate::writing_studio::APP_ID;
/// Caps mirroring the article editor's: a studio is a desk, not an archive.
const MAX_DOCUMENTS: usize = 50;
const MAX_TASKS: usize = 200;
const MAX_TEXT: usize = 100_000;
const MAX_FILE: usize = 8_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Documents {
    schema: u32,
    documents: Vec<Document>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tasks {
    schema: u32,
    tasks: Vec<RewriteTask>,
}

fn directory(root: &Path, grant: &Grant) -> PathBuf {
    root.join("mini-apps")
        .join(blake3::hash(grant.lease.owner().as_bytes()).to_hex().as_str())
        .join(APP_ID)
}

fn validate_document(doc: &Document) -> Result<(), String> {
    if !article_core::document::valid_id(&doc.id)
        || doc.title.len() > 1000
        || doc.paragraphs.iter().map(|p| p.len()).sum::<usize>() > MAX_TEXT
        || doc.paragraphs.len() > 500
    {
        return Err("Invalid writing document".into());
    }
    Ok(())
}

fn validate_task(task: &RewriteTask) -> Result<(), String> {
    if !article_core::document::valid_id(&task.id)
        || !article_core::document::valid_id(&task.doc_id)
        || task.selection.text_snapshot.len() > MAX_TEXT
        || task.request.len() > 4000
        || task.proposal.as_ref().is_some_and(|p| p.len() > MAX_TEXT)
    {
        return Err("Invalid writing task".into());
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > MAX_FILE {
                return Err("Writing studio storage exceeds its limit.".into());
            }
            serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The first-run desk: one demo document whose long, filler-heavy paragraph
/// and `[^1]`/`[^2]` citations exercise selection, background preparation,
/// citation preservation and the diff view without any setup.
fn seed_documents() -> Vec<Document> {
    vec![Document::new(
        "项目介绍（示例）".into(),
        vec![
            "OctoSense 写作工作室是一个驻留在 Rinx 内的原生写作应用。它把可编辑的改写提案放在作品旁边：人选中一段文字、提出修改请求，代理给出提案，人始终保留最终决定权。".into(),
            "这个项目的背景其实可以说非常复杂，基本上它一开始只是想做一个非常非常简单的 Markdown 编辑器，但是在很多次的讨论之后，众所周知，大家逐渐意识到真正的问题不在于写作工具本身，而在于人机协作的信任边界：代理可以读懂上下文并提出建议，但是它绝对不应该在人没有确认的情况下悄悄修改文档，所以这个段落故意写得十分冗长，方便你选中它并请求「写得更简洁」[^1]。".into(),
            "引用与研究保持出处关联：本文档的论述带有 [^1] 与 [^2] 两个引用标记，改写提案必须保留它们；缺少来源的论述会被显式标注「需要补充来源」。".into(),
            "发布永远是另一个选择。应用改写只修改本地文档，导出、发布与分享都是独立的操作，并且在执行前明确呈现目的地[^2]。".into(),
        ],
    )]
}

pub fn load_documents(root: &Path, grant: &Grant) -> Result<Vec<Document>, String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::ReadDrafts)?;
    let path = directory(root, grant).join("documents.json");
    let Some(documents): Option<Documents> = read_json(&path)? else {
        // First launch on this account: seed the demo desk.
        let seeded = seed_documents();
        save_documents(root, grant, &seeded)?;
        return Ok(seeded);
    };
    if documents.schema != 1 || documents.documents.len() > MAX_DOCUMENTS {
        return Err("Unsupported writing documents".into());
    }
    for doc in &documents.documents {
        validate_document(doc)?;
    }
    Ok(documents.documents)
}

pub fn save_documents(root: &Path, grant: &Grant, documents: &[Document]) -> Result<(), String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    for doc in documents {
        validate_document(doc)?;
    }
    let bundle = Documents { schema: 1, documents: documents.to_vec() };
    let bytes = serde_json::to_vec(&bundle).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("Writing documents exceed their storage limit.".into());
    }
    atomic_write(&directory(root, grant).join("documents.json"), &bytes)
}

pub fn load_tasks(root: &Path, grant: &Grant) -> Result<Vec<RewriteTask>, String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::ReadDrafts)?;
    let path = directory(root, grant).join("tasks.json");
    let Some(tasks): Option<Tasks> = read_json(&path)? else {
        return Ok(Vec::new());
    };
    if tasks.schema != 1 || tasks.tasks.len() > MAX_TASKS {
        return Err("Unsupported writing tasks".into());
    }
    for task in &tasks.tasks {
        validate_task(task)?;
    }
    Ok(tasks.tasks)
}

pub fn save_tasks(root: &Path, grant: &Grant, tasks: &[RewriteTask]) -> Result<(), String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    let mut kept: Vec<RewriteTask> = tasks.to_vec();
    if kept.len() > MAX_TASKS {
        let excess = kept.len() - MAX_TASKS;
        kept.drain(..excess);
    }
    for task in &kept {
        validate_task(task)?;
    }
    let bundle = Tasks { schema: 1, tasks: kept };
    let bytes = serde_json::to_vec(&bundle).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("Writing tasks exceed their storage limit.".into());
    }
    atomic_write(&directory(root, grant).join("tasks.json"), &bytes)
}

/// Appends one decision to the JSONL trail. Appends are inherently atomic
/// enough for a single-writer desktop app; the whole-file rewrites above use
/// `atomic_write` because corruption there would lose work, while a truncated
/// last log line loses one line of audit.
pub fn append_decision(root: &Path, grant: &Grant, decision: &Decision) -> Result<(), String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    let mut line = serde_json::to_vec(decision).map_err(|e| e.to_string())?;
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory(root, grant).join("decisions.jsonl"))
        .map_err(|e| e.to_string())?;
    file.write_all(&line).map_err(|e| e.to_string())
}

/// Reads the full decision log: a Vec in JSONL order. Malformed lines are
/// skipped (they're an audit trail; a bad line is the last thing to crash on).
/// Missing file → empty Vec. Used by `rewrite_decisions` and any future
/// "show audit" UI.
pub fn read_decisions(root: &Path, grant: &Grant) -> Result<Vec<Decision>, String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::ReadDrafts)?;
    let path = directory(root, grant).join("decisions.jsonl");
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut decisions = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(d) = serde_json::from_str::<Decision>(trimmed) {
            decisions.push(d);
        }
    }
    Ok(decisions)
}

/// Rewrites the decision log keeping only entries whose `task_id` passes the
/// predicate. The log is append-only in the steady state; this is the
/// housekeeping path used by `delete_task` / `delete_document`. We rewrite the
/// whole file with `atomic_write` — the same protection as documents.json.
/// Returns how many lines were kept.
pub fn rewrite_decisions<F: Fn(&str) -> bool>(
    root: &Path,
    grant: &Grant,
    keep: F,
) -> Result<usize, String> {
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    let kept = read_decisions(root, grant)?
        .into_iter()
        .filter(|d| keep(&d.task_id))
        .collect::<Vec<_>>();
    let mut bytes = Vec::new();
    for d in &kept {
        let mut line = serde_json::to_vec(d).map_err(|e| e.to_string())?;
        line.push(b'\n');
        bytes.extend_from_slice(&line);
    }
    let path = directory(root, grant).join("decisions.jsonl");
    atomic_write(&path, &bytes)?;
    Ok(kept.len())
}

/// Removes a document by id. Also removes its tasks' decision-log entries and
/// undo entries are cleared by the caller (they live in memory only). Returns
/// whether the document existed.
pub fn delete_document(root: &Path, grant: &Grant, doc_id: &str) -> Result<bool, String> {
    if !article_core::document::valid_id(doc_id) {
        return Err("Invalid document id".into());
    }
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    let mut docs = load_documents(root, grant)?;
    let before = docs.len();
    docs.retain(|d| d.id != doc_id);
    if docs.len() == before {
        return Ok(false);
    }
    save_documents(root, grant, &docs)?;
    // Cascade: drop tasks for this document and any decision-log entries that
    // pointed at them. Undo entries live in memory and are cleared by the
    // caller through `Studio::delete_document`.
    let mut tasks = load_tasks(root, grant)?;
    let dropped_task_ids: Vec<String> = tasks
        .iter()
        .filter(|t| t.doc_id == doc_id)
        .map(|t| t.id.clone())
        .collect();
    tasks.retain(|t| t.doc_id != doc_id);
    save_tasks(root, grant, &tasks)?;
    if !dropped_task_ids.is_empty() {
        rewrite_decisions(root, grant, |tid| !dropped_task_ids.iter().any(|d| d == tid))?;
    }
    Ok(true)
}

/// Removes one task by id and prunes its decision-log entries. Returns whether
/// the task existed. Undo entries are the caller's job (in-memory).
pub fn delete_task(root: &Path, grant: &Grant, task_id: &str) -> Result<bool, String> {
    if !article_core::document::valid_id(task_id) {
        return Err("Invalid task id".into());
    }
    let host = RobrixWritingHost::new(root);
    host.authorize(&grant.lease, Capability::WriteDrafts)?;
    let mut tasks = load_tasks(root, grant)?;
    let before = tasks.len();
    tasks.retain(|t| t.id != task_id);
    if tasks.len() == before {
        return Ok(false);
    }
    save_tasks(root, grant, &tasks)?;
    rewrite_decisions(root, grant, |tid| tid != task_id)?;
    Ok(true)
}

/// Tests are gated behind `cfg(test)` at the bottom of the file (alongside
/// the other unit tests); the helpers `temp_root` and `seed_grant` build a
/// throwaway directory + SessionAuthority per test.

/// UI-facing helper: drops every decision-log entry whose `task_id` is in
/// `dropped_task_ids`. Used after `Studio::delete_document` /
/// `Studio::delete_task` to keep the on-disk JSONL consistent with the
/// in-memory state (`persist()` only appends the newest decision, it never
/// rewrites older entries). Returns how many entries survive.
pub fn delete_decisions_for(
    root: &Path,
    grant: &Grant,
    dropped_task_ids: &[String],
) -> Result<usize, String> {
    if dropped_task_ids.is_empty() {
        return Ok(read_decisions(root, grant)?.len());
    }
    rewrite_decisions(root, grant, |tid| !dropped_task_ids.iter().any(|d| d == tid))
}
