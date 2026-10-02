//! Grafting into the article editor: a writing-studio document becomes a
//! draft in the article library (title + paragraphs as markdown, citation
//! markers kept inline), so the layout studio can typeset it — images,
//! themes, cover — and run it down its own publishing pipeline. The write
//! goes through the shared `article_core` LocalStore under this app's own
//! consent grant: same directory, same schema, no new capabilities.
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
}
