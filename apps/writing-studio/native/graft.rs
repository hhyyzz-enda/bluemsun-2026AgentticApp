//! Bidirectional grafting between writing-studio and the article editor.
//!
//! - `send_to_article_editor`: a writing-studio document becomes a draft in
//!   the article library (title + paragraphs as markdown, citation markers
//!   kept inline), so the layout studio can typeset it — images, themes,
//!   cover — and run it down its own publishing pipeline. The article id is
//!   stable across versions (`blake3("writing-studio:{doc.id}")`), so
//!   re-sending the same document updates that one article instead of
//!   stacking duplicates, and the article records its `WritingSource`.
//! - `import_article`: the reverse trip. The article's `source_writing` lets
//!   the pull-back find the originating studio document and realign versions:
//!   when the writing side has not changed since it was sent, the article is
//!   merged back into the original document (`Aligned`); when both sides
//!   changed, a fresh document is created so neither side's edits are
//!   silently destroyed (`Forked`); articles without writing provenance are
//!   imported as new documents (`Imported`).
//!
//! Both directions go through the shared `article_core` LocalStore under this
//! app's own consent grant: same directory, same schema, no new capabilities.
use std::path::Path;
use article_core::{
    document::{WritingSource, Document as ArticleDocument},
    host::{ArticleHost, Capability, ConsentGrant},
    storage::LocalStore,
};
use crate::article_app::{Operation, Publication};
use super::{
    host::RobrixWritingHost,
    model::{Document, Grant},
};

/// Stable article id: re-sending the same document at any version updates
/// the same library entry instead of stacking duplicates, and the pull-back
/// path can identify the article a document was sent to.
pub fn article_id_for(doc: &Document) -> String {
    blake3::hash(format!("writing-studio:{}", doc.id).as_bytes())
        .to_hex()
        .to_string()
}

/// Field mapping: title → title; paragraphs joined by a blank line → the
/// markdown body (each paragraph becomes its own block, `[^n]` markers stay
/// inline); theme/cover/summary are left to the layout studio. The article
/// records where it came from so the pull-back can align versions.
pub fn to_article_document(doc: &Document, now: u64) -> Result<ArticleDocument, String> {
    let mut article = ArticleDocument::from_markdown(&doc.title, &doc.body())?;
    article.id = article_id_for(doc);
    article.modified = now;
    article.source_writing = Some(WritingSource {
        doc_id: doc.id.clone(),
        version: doc.version,
        at: now,
    });
    article.validate()?;
    Ok(article)
}

/// Writes (or updates) the draft in the article editor's library, authorized
/// by the writing grant (editor capabilities include Read/WriteDrafts).
pub fn send_to_article_editor(root: &Path, grant: &Grant, doc: &Document, now: u64) -> Result<String, String> {
    grant.authorize(Capability::WriteDrafts)?;
    let host = RobrixWritingHost::new(root);
    send_via(&host, &grant.lease, doc, now)
}

/// The host-generic core, unit-tested against a temporary data root.
fn send_via<H: ArticleHost>(host: &H, lease: &ConsentGrant, doc: &Document, now: u64) -> Result<String, String> {
    let article = to_article_document(doc, now)?;
    let id = article.id.clone();
    let store: LocalStore<'_, H, Publication, Operation> = LocalStore::new(host, lease);
    store.save_document(&article)?;
    Ok(id)
}

/// Removes the article that was created from a writing-studio document.
/// Cascades through the shared source-draft map so the article editor stops
/// showing the draft it would have rendered. Returns whether an article was
/// found. Safe to call even when nothing was ever sent — idempotent.
pub fn delete_article_from_editor(root: &Path, grant: &Grant, doc: &Document) -> Result<bool, String> {
    grant.authorize(Capability::WriteDrafts)?;
    let host = RobrixWritingHost::new(root);
    let article_id = article_id_for(doc);
    delete_article_via(&host, &grant.lease, &article_id)
}

/// The host-generic core, unit-tested alongside `send_via`.
fn delete_article_via<H: ArticleHost>(
    host: &H,
    lease: &ConsentGrant,
    article_id: &str,
) -> Result<bool, String> {
    let store: LocalStore<'_, H, Publication, Operation> = LocalStore::new(host, lease);
    let article_id = article_id.to_owned();
    store.update(|library| {
        let before = library.documents.len();
        library.documents.retain(|d| d.id != article_id);
        let removed = library.documents.len() != before;
        // The shared map keys on article id; drop any entry pointing at the
        // removed article so the article-editor UI stays consistent.
        library.source_drafts.remove(&article_id);
        library.source_locations.remove(&article_id);
        Ok(removed)
    })
}

/// Lists the article library, newest-modified first, for the pull-back
/// picker. Read-only; the import itself happens in `import_article`.
pub fn article_library(root: &Path, grant: &Grant) -> Result<Vec<ArticleDocument>, String> {
    grant.authorize(Capability::ReadDrafts)?;
    let host = RobrixWritingHost::new(root);
    let store: LocalStore<'_, RobrixWritingHost, Publication, Operation> = LocalStore::new(&host, &grant.lease);
    let mut documents = store.load()?.documents;
    documents.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(documents)
}

/// How a pulled-back article was merged into the writing desk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    /// The article's `source_writing` matched a live studio document whose
    /// version is unchanged since it was sent: the article content is merged
    /// back into that original document (same id, history preserved).
    Aligned,
    /// The source document exists but was edited since it was sent: importing
    /// into it would silently destroy those edits, so a fresh document is
    /// created. The UI marks it as a fork.
    Forked,
    /// No writing provenance, or the source document was deleted: a plain
    /// import of the article as a new document.
    Imported,
}

#[derive(Clone, Debug)]
pub struct ImportOutcome {
    pub document: Document,
    pub kind: ImportKind,
}

/// Reverse of `send_to_article_editor`: reads the article, aligns it against
/// the writing desk (`existing`), and returns the document to persist. The
/// caller persists it and records the decision.
pub fn import_article(
    root: &Path,
    grant: &Grant,
    article_id: &str,
    existing: &[Document],
    now: u64,
) -> Result<ImportOutcome, String> {
    if !article_core::document::valid_id(article_id) {
        return Err("Invalid article id".into());
    }
    grant.authorize(Capability::ReadDrafts)?;
    let host = RobrixWritingHost::new(root);
    import_via(&host, &grant.lease, article_id, existing, now)
}

/// The host-generic core, unit-tested alongside `send_via` /
/// `delete_article_via`.
fn import_via<H: ArticleHost>(
    host: &H,
    lease: &ConsentGrant,
    article_id: &str,
    existing: &[Document],
    now: u64,
) -> Result<ImportOutcome, String> {
    let store: LocalStore<'_, H, Publication, Operation> = LocalStore::new(host, lease);
    let library = store.load()?;
    let article = library
        .documents
        .iter()
        .find(|d| d.id == article_id)
        .ok_or_else(|| format!("Article {article_id} not found in editor library"))?;
    // The studio's original markdown wins over the editor's layout-time
    // serialization when one was recorded; the fallback still produces a
    // usable document for articles populated through other paths.
    let markdown = library
        .source_for(&article.id)
        .map(str::to_owned)
        .unwrap_or_else(|| article.markdown());
    let (document, kind) = align_import(article, existing, article_id, &markdown, now)?;
    Ok(ImportOutcome { document, kind })
}

/// Pure alignment: decides how an article merges into the writing desk.
///
/// - `source_writing` present and the source document found with an unchanged
///   version → merge into that document (`Aligned`), keeping its id and
///   history; title/body change only when the article actually differs.
/// - `source_writing` present but the source document moved on → fresh
///   document (`Forked`); importing into the original would destroy the
///   writer's newer edits, so neither side overwrites the other.
/// - No provenance or source deleted → plain import (`Imported`).
///
/// All returned documents carry `article_id` so re-sending stays linked to
/// the same article.
pub fn align_import(
    article: &ArticleDocument,
    existing: &[Document],
    article_id: &str,
    markdown: &str,
    now: u64,
) -> Result<(Document, ImportKind), String> {
    let title = article.title.clone();
    let paragraphs = paragraphs_from_markdown(markdown);
    let Some(source) = &article.source_writing else {
        let mut doc = Document::new(title, paragraphs);
        doc.article_id = Some(article_id.to_owned());
        doc.modified = now;
        return Ok((doc, ImportKind::Imported));
    };
    let Some(source_doc) = existing.iter().find(|d| d.id == source.doc_id) else {
        // The source document was deleted; importing fresh is the only choice.
        let mut doc = Document::new(title, paragraphs);
        doc.article_id = Some(article_id.to_owned());
        doc.modified = now;
        return Ok((doc, ImportKind::Imported));
    };
    if source_doc.version != source.version {
        // Both sides changed: never overwrite the writer's newer edits.
        let mut doc = Document::new(title, paragraphs);
        doc.article_id = Some(article_id.to_owned());
        doc.modified = now;
        return Ok((doc, ImportKind::Forked));
    }
    // Versions align: fold the article back into the original document.
    let mut aligned = source_doc.clone();
    let mut changed = false;
    if aligned.title != title {
        aligned.title = title;
        changed = true;
    }
    if aligned.paragraphs != paragraphs {
        aligned.paragraphs = paragraphs;
        changed = true;
    }
    if changed {
        aligned.version += 1;
    }
    aligned.modified = now;
    aligned.article_id = Some(article_id.to_owned());
    Ok((aligned, ImportKind::Aligned))
}

/// Splits the joined markdown body into paragraphs on the blank-line rule the
/// editor uses, the same one `Document::body` joins on. Empty input becomes
/// one empty paragraph so `paragraphs` is never empty (matching
/// `Document::set_body`'s invariant).
fn paragraphs_from_markdown(markdown: &str) -> Vec<String> {
    let split: Vec<String> = markdown
        .split("\n\n")
        .map(|p| p.trim_matches('\n').to_owned())
        .collect();
    if split.is_empty() {
        vec![String::new()]
    } else {
        split
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use article_core::host::{Capabilities, SessionAuthority};
    use std::{path::PathBuf, sync::LazyLock, time::Duration};

    static TEST_AUTHORITY: LazyLock<SessionAuthority> = LazyLock::new(SessionAuthority::default);

    struct TestHost {
        root: PathBuf,
        account: String,
    }
    impl ArticleHost for TestHost {
        fn active_account(&self) -> Option<String> {
            Some(self.account.clone())
        }
        fn data_root(&self) -> &Path {
            &self.root
        }
        fn authority(&self) -> &SessionAuthority {
            &TEST_AUTHORITY
        }
    }

    fn fixture() -> (TestHost, ConsentGrant, Document) {
        let root = std::env::temp_dir().join(format!("rinx-graft-{}", article_core::document::new_id()));
        let host = TestHost { root, account: "@writer:test".into() };
        let lease = TEST_AUTHORITY.issue("@writer:test".into(), Capabilities::editor(), Duration::from_secs(60));
        let doc = Document::new(
            "示例标题".into(),
            vec!["第一段，带来源。[^1]".into(), "第二段，更长的内容与引用[^2]。".into()],
        );
        (host, lease, doc)
    }

    fn load_ids(host: &TestHost, lease: &ConsentGrant) -> Vec<String> {
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(host, lease);
        store.load().unwrap().documents.into_iter().map(|d| d.id).collect()
    }

    #[test]
    fn sent_document_is_readable_from_the_article_library() {
        let (host, lease, doc) = fixture();
        let id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        let library = store.load().unwrap();
        let article = library.documents.iter().find(|d| d.id == id).expect("article stored");
        assert_eq!(article.title, "示例标题");
        let markdown = article.markdown();
        assert!(markdown.contains("[^1]"), "citation markers survive: {markdown}");
        assert!(markdown.contains("第二段"));
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn resending_updates_the_same_article_and_records_provenance() {
        let (host, lease, doc) = fixture();
        let first = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // A newer version is still the same article: the id is version-free,
        // so re-sending updates in place instead of stacking a duplicate.
        let mut newer = doc.clone();
        newer.version += 1;
        let second = send_via(&host, &lease, &newer, 1_700_000_100).unwrap();
        assert_eq!(first, second, "same doc → same article id across versions");
        assert_eq!(load_ids(&host, &lease).iter().filter(|id| *id == &first).count(), 1);
        // The provenance records the version at send time.
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        let article = store
            .load()
            .unwrap()
            .documents
            .into_iter()
            .find(|d| d.id == first)
            .expect("article stored");
        let source = article.source_writing.expect("provenance recorded");
        assert_eq!(source.doc_id, doc.id);
        assert_eq!(source.version, 2);
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn deleting_the_only_article_empties_the_library() {
        let (host, lease, doc) = fixture();
        let id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // Confirm it landed first.
        assert!(load_ids(&host, &lease).contains(&id));
        // Delete it: returns true (was found) and library is now empty.
        assert!(delete_article_via(&host, &lease, &id).unwrap());
        assert!(load_ids(&host, &lease).is_empty());
        // Idempotent: deleting again returns false, still empty.
        assert!(!delete_article_via(&host, &lease, &id).unwrap());
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn deleting_one_article_leaves_the_others() {
        let (host, lease, _doc) = fixture();
        // Two distinct documents have distinct ids, so their article ids are
        // distinct too — no collision under the version-free id scheme.
        let doc_a = Document::new("第一篇".into(), vec!["A 段。".into(), "A 段二。".into()]);
        let doc_b = Document::new("第二篇".into(), vec!["B 段。".into(), "B 段二。".into()]);
        let id_a = send_via(&host, &lease, &doc_a, 1_700_000_000).unwrap();
        let id_b = send_via(&host, &lease, &doc_b, 1_700_000_001).unwrap();
        assert_ne!(id_a, id_b);
        assert_eq!(load_ids(&host, &lease).len(), 2);
        assert!(delete_article_via(&host, &lease, &id_a).unwrap());
        let remaining = load_ids(&host, &lease);
        assert_eq!(remaining, vec![id_b.clone()]);
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pull_back_aligns_into_the_original_document() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        let outcome = import_via(&host, &lease, &article_id, &[doc.clone()], 1_700_000_500).unwrap();
        assert_eq!(outcome.kind, ImportKind::Aligned);
        assert_eq!(outcome.document.id, doc.id, "aligns back into the original document");
        assert_eq!(outcome.document.title, doc.title);
        assert_eq!(outcome.document.paragraphs, doc.paragraphs);
        assert_eq!(outcome.document.article_id.as_deref(), Some(article_id.as_str()));
        assert_eq!(outcome.document.version, doc.version, "unchanged content → no version bump");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pull_back_aligns_and_bumps_version_when_article_differs() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // The layout studio edits the article after the send.
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.update(|library| {
            if let Some(article) = library.documents.iter_mut().find(|d| d.id == article_id) {
                article.title = "排版后的标题".into();
                article.blocks[0].text = "排版改动后的正文。".into();
            }
            Ok(())
        }).unwrap();
        let outcome = import_via(&host, &lease, &article_id, &[doc.clone()], 1_700_000_500).unwrap();
        assert_eq!(outcome.kind, ImportKind::Aligned);
        assert_eq!(outcome.document.id, doc.id);
        assert_eq!(outcome.document.title, "排版后的标题");
        assert_eq!(outcome.document.version, doc.version + 1, "layout edits bump the version");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pull_back_forks_when_both_sides_changed() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // The writer keeps editing after sending: the source document's
        // version moves on, so merging back would destroy those edits.
        let mut writer = doc.clone();
        writer.version += 1;
        let outcome = import_via(&host, &lease, &article_id, &[writer.clone()], 1_700_000_500).unwrap();
        assert_eq!(outcome.kind, ImportKind::Forked);
        assert_ne!(outcome.document.id, doc.id, "a fork is a fresh document");
        assert_eq!(outcome.document.title, doc.title);
        assert_eq!(outcome.document.paragraphs, doc.paragraphs);
        assert_eq!(outcome.document.article_id.as_deref(), Some(article_id.as_str()));
        assert_eq!(outcome.document.version, 1);
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pull_back_imports_articles_without_writing_provenance() {
        let (host, lease, _doc) = fixture();
        // No send: build an article through the editor's own import path so it
        // has no `source_writing` at all.
        let article = article_core::document::Document::from_markdown("独立文章", "第一段。\n\n第二段。").unwrap();
        let article_id = article.id.clone();
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.save_document(&article).unwrap();
        let outcome = import_via(&host, &lease, &article_id, &[], 1_700_000_500).unwrap();
        assert_eq!(outcome.kind, ImportKind::Imported);
        assert_eq!(outcome.document.title, "独立文章");
        assert_eq!(outcome.document.paragraphs, vec!["第一段。".to_string(), "第二段。".to_string()]);
        assert_eq!(outcome.document.article_id.as_deref(), Some(article_id.as_str()));
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn importing_unknown_article_errors() {
        let (host, lease, _doc) = fixture();
        let err = import_via(&host, &lease, "does-not-exist", &[], 1_700_000_000).unwrap_err();
        assert!(err.contains("does-not-exist"), "error names the missing id: {err}");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pull_back_prefers_source_draft_when_present() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // Inject a source draft as if the editor had recorded it.
        let custom = "第一段 from source.\n\n第二段 from source with citation[^1].";
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.update(|library| {
            library.source_drafts.insert(article_id.clone(), custom.into());
            Ok(())
        }).unwrap();
        let outcome = import_via(&host, &lease, &article_id, &[doc.clone()], 1_700_000_500).unwrap();
        assert_eq!(outcome.document.paragraphs, vec!["第一段 from source.".to_string(), "第二段 from source with citation[^1].".to_string()]);
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn importing_with_empty_body_still_produces_one_paragraph() {
        let (host, lease, _doc) = fixture();
        // No send: build an article with empty markdown via direct insert so
        // we exercise the empty-body fallback (send would never produce
        // empty body — Document::new requires paragraphs).
        let empty_article = article_core::document::Document::from_markdown("空标题", "").unwrap();
        let article_id = empty_article.id.clone();
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.save_document(&empty_article).unwrap();
        let outcome = import_via(&host, &lease, &article_id, &[], 1_700_000_500).unwrap();
        assert_eq!(outcome.document.title, "空标题");
        assert_eq!(outcome.document.paragraphs.len(), 1, "empty body → one empty paragraph, matching Document invariants");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn paragraphs_from_markdown_splits_on_blank_line_and_strips_newlines() {
        let input = "第一段。\n\n第二段。\n\n\n第三段。";
        assert_eq!(paragraphs_from_markdown(input), vec!["第一段。", "第二段。", "第三段。"]);
        assert_eq!(paragraphs_from_markdown(""), vec![""]);
    }
}
