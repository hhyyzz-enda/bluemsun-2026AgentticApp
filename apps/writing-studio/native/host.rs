//! Rinx owns the login lifecycle, filesystem root and Matrix client.
//! Shared article crates have no dependency back into this application.
use std::{path::Path, sync::LazyLock};
use article_core::host::{ArticleHost, SessionAuthority};

pub(super) static AUTHORITY: LazyLock<SessionAuthority> = LazyLock::new(SessionAuthority::default);

pub struct RobrixWritingHost<'a> { root: &'a Path }
impl<'a> RobrixWritingHost<'a> {
    pub fn new(root: &'a Path) -> Self { Self { root } }
}
impl ArticleHost for RobrixWritingHost<'_> {
    fn active_account(&self) -> Option<String> {
        if crate::logout::logout_state_machine::is_logout_in_progress() { return None; }
        crate::sliding_sync::current_user_id().map(|id| id.to_string())
    }
    fn data_root(&self) -> &Path { self.root }
    fn authority(&self) -> &SessionAuthority { &AUTHORITY }
}
