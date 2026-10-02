//! The build is the authority for Rinx's bundled app identities and native entries.
use std::sync::LazyLock;
pub use rinx_system_apps::{ARTICLE_ID, NativeApp, PackedApp, WRITING_STUDIO_ID};
static CATALOG: LazyLock<rinx_system_apps::PackedCatalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/system-apps.json")))
        .expect("build-validated system app catalog")
});
pub fn apps() -> &'static [PackedApp] {
    &CATALOG.apps
}
pub fn get(id: &str) -> Option<&'static PackedApp> {
    apps().iter().find(|a| a.manifest.id == id)
}
pub fn is_reserved(id: &str) -> bool {
    id == ARTICLE_ID || get(id).is_some()
}
pub fn launch_native(cx: &mut makepad_widgets::Cx, app: &PackedApp) {
    match app.native {
        Some(NativeApp::ArticleEditor) => cx.action(crate::article_app::ArticleAction::Open),
        Some(NativeApp::WritingStudio) => cx.action(crate::writing_studio::WritingAction::Open),
        None => {}
    }
}
/// Existing Discover/chat shortcuts use the same catalog entry as Mini apps.
pub fn open_article(cx: &mut makepad_widgets::Cx) {
    if let Some(app) = get(ARTICLE_ID) {
        launch_native(cx, app);
    }
}
/// The writing studio's Discover/navigation shortcuts take the same path.
pub fn open_writing_studio(cx: &mut makepad_widgets::Cx) {
    if let Some(app) = get(WRITING_STUDIO_ID) {
        launch_native(cx, app);
    }
}
