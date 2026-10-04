//! Bidirectional grafting between writing-studio and the article editor.
//!
//! - `send_to_article_editor`: a writing-studio document becomes a draft in
//!   the article library (title + paragraphs as markdown, citation markers
//!   kept inline), so the layout studio can typeset it — images, themes,
//!   cover — and run it down its own publishing pipeline.
//! - `pull_from_article_editor`: a draft in the article library becomes a
//!   fresh writing-studio document, so the writer can keep iterating in the
//!   studio after the layout studio produced its version. The pull is one-way
//!   (no overwrite of an existing studio document) because the article id is
//!   a hash of the studio doc id+version and cannot be reversed.
//!
//! Both directions go through the shared `article_core` LocalStore under this
//! app's own consent grant: same directory, same schema, no new capabilities.
use std::path::Path;
use article_core::{
    document::Document as ArticleDocument,
    host::{ArticleHost, Capability, ConsentGrant},
    storage::LocalStore,
};
use crate::article_app::{Operation, Publication};
use super::{
    host::RobrixWritingHost,
    model::{Document, Grant},
};

/// Stable article id: re-sending the same document at the same version
/// updates the existing library entry instead of stacking duplicates.
pub fn article_id_for(doc: &Document) -> String {
    blake3::hash(format!("writing-studio:{}:v{}", doc.id, doc.version).as_bytes())
        .to_hex()
        .to_string()
}

/// Field mapping: title → title; paragraphs joined by a blank line → the
/// markdown body (each paragraph becomes its own block, `[^n]` markers stay
/// inline); theme/cover/summary are left to the layout studio.
pub fn to_article_document(doc: &Document, now: u64) -> Result<ArticleDocument, String> {
    let mut article = ArticleDocument::from_markdown(&doc.title, &doc.body())?;
    article.id = article_id_for(doc);
    article.modified = now;
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

/// Reverse of `send_to_article_editor`: takes an article from the editor's
/// library and creates a new writing-studio document from its current content.
///
/// The article id is a hash of the studio doc id and version, so we cannot
/// recover which studio document a given article came from — instead of
/// risking an overwrite that would silently destroy the writer's in-progress
/// edits, this returns a brand-new document with a fresh id and
/// `version = 1`. Re-sending it to the editor produces a new article (its id
/// is derived from the fresh doc id), so the two stay cleanly linked by
/// content rather than by shared identity.
///
/// Markdown source: `library.source_for(article_id)` (the original markdown
/// the studio sent over, when one was recorded) wins over `article.markdown()`
/// (the editor's layout-time serialization). Falling back is deliberate —
/// an article may have lost its source draft if the editor was populated
/// through a different path (file import, server message), and the pull must
/// still produce a usable document.
pub fn pull_from_article_editor(
    root: &Path,
    grant: &Grant,
    article_id: &str,
    now: u64,
) -> Result<Document, String> {
    if !article_core::document::valid_id(article_id) {
        return Err("Invalid article id".into());
    }
    grant.authorize(Capability::ReadDrafts)?;
    let host = RobrixWritingHost::new(root);
    pull_via(&host, &grant.lease, article_id, now)
}

/// The host-generic core, unit-tested alongside `send_via` /
/// `delete_article_via`. Returns the new document; persistence into the
/// studio's own `documents.json` is the integration entry point's job.
fn pull_via<H: ArticleHost>(
    host: &H,
    lease: &ConsentGrant,
    article_id: &str,
    now: u64,
) -> Result<Document, String> {
    let store: LocalStore<'_, H, Publication, Operation> = LocalStore::new(host, lease);
    let library = store.load()?;
    let article = library
        .documents
        .iter()
        .find(|d| d.id == article_id)
        .ok_or_else(|| format!("Article {article_id} not found in editor library"))?;
    let markdown = library
        .source_for(&article.id)
        .map(str::to_owned)
        .unwrap_or_else(|| article.markdown());
    let title = article.title.clone();
    let mut doc = Document::new(title, paragraphs_from_markdown(&markdown));
    doc.modified = now;
    Ok(doc)
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
    fn resending_same_version_updates_instead_of_duplicating() {
        let (host, lease, doc) = fixture();
        let first = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        let second = send_via(&host, &lease, &doc, 1_700_000_100).unwrap();
        assert_eq!(first, second, "same doc + same version → same article id");
        assert_eq!(load_ids(&host, &lease).iter().filter(|id| *id == &first).count(), 1);
        // A newer version is deliberately a new entry, not an overwrite.
        let mut newer = doc.clone();
        newer.version += 1;
        assert_ne!(article_id_for(&newer), first);
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
        // Two distinct documents — `article_id_for` keys on doc.id + version,
        // so cloning with only a title change would hash to the same id and
        // overwrite instead of stacking.
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
    fn pulling_a_sent_article_returns_a_fresh_doc_with_same_title_and_body() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        let pulled = pull_via(&host, &lease, &article_id, 1_700_000_500).unwrap();
        assert_eq!(pulled.title, doc.title, "title carries over from the article");
        assert_eq!(pulled.paragraphs, doc.paragraphs, "paragraphs carry over from the article body");
        assert_eq!(pulled.version, 1, "a pulled-back document starts at version 1");
        assert_ne!(pulled.id, doc.id, "a pulled-back document gets a fresh id (no silent overwrite)");
        assert_ne!(pulled.id, article_id, "the pulled doc id is its own id, not the article id");
        assert_eq!(pulled.modified, 1_700_000_500, "modified is the timestamp the caller passed in");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pulling_unknown_article_errors() {
        let (host, lease, _doc) = fixture();
        let err = pull_via(&host, &lease, "does-not-exist", 1_700_000_000).unwrap_err();
        assert!(err.contains("does-not-exist"), "error names the missing id: {err}");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pulling_prefers_source_draft_when_present() {
        let (host, lease, doc) = fixture();
        let article_id = send_via(&host, &lease, &doc, 1_700_000_000).unwrap();
        // Inject a source draft as if the editor had recorded it.
        let custom = "第一段 from source.\n\n第二段 from source with citation[^1].";
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.update(|library| {
            library.source_drafts.insert(article_id.clone(), custom.into());
            Ok(())
        }).unwrap();
        let pulled = pull_via(&host, &lease, &article_id, 1_700_000_500).unwrap();
        assert_eq!(pulled.paragraphs, vec!["第一段 from source.".to_string(), "第二段 from source with citation[^1].".to_string()]);
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn pulling_with_empty_body_still_produces_one_paragraph() {
        let (host, lease, _doc) = fixture();
        // No send: build an article with empty markdown via direct insert so
        // we exercise the empty-body fallback (send would never produce
        // empty body — Document::new requires paragraphs).
        let empty_article = article_core::document::Document::from_markdown("空标题", "").unwrap();
        let article_id = empty_article.id.clone();
        let store: LocalStore<'_, TestHost, Publication, Operation> = LocalStore::new(&host, &lease);
        store.save_document(&empty_article).unwrap();
        let pulled = pull_via(&host, &lease, &article_id, 1_700_000_500).unwrap();
        assert_eq!(pulled.title, "空标题");
        assert_eq!(pulled.paragraphs.len(), 1, "empty body → one empty paragraph, matching Document invariants");
        std::fs::remove_dir_all(host.data_root()).ok();
    }

    #[test]
    fn paragraphs_from_markdown_splits_on_blank_line_and_strips_newlines() {
        let input = "第一段。\n\n第二段。\n\n\n第三段。";
        assert_eq!(paragraphs_from_markdown(input), vec!["第一段。", "第二段。", "第三段。"]);
        assert_eq!(paragraphs_from_markdown(""), vec![""]);
    }
}
