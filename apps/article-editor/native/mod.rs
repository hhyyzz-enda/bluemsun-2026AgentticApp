//! Host-admitted Octoscript L0 article app. No browser, token bridge or downloaded code.
mod model;
pub mod document;
use article_makepad::rich_input;
mod host;
#[cfg(feature = "html_preview")]
mod preview;
mod remote_images;
mod storage;
pub(crate) mod backend;
pub mod ui;
pub mod window;
pub mod table_picker;
pub use model::{ArticlePackage, MSGTYPE};
/// Grafting targets (writing-studio → article library) need the host's
/// publication metadata types so the shared library round-trips losslessly.
pub use storage::{Operation, Publication};
pub use ui::{ArticleAction, ArticlePanelWidgetRefExt};
pub(crate) use model::invalidate_sessions;
pub fn script_mod(vm:&mut makepad_widgets::ScriptVm) {
    article_makepad::script_mod(vm);
    #[cfg(feature = "html_preview")]
    makepad_html_renderer::makepad::script_mod(vm);
    #[cfg(not(feature = "html_preview"))]
    {
        use makepad_widgets::*;
        script_eval!(vm, {mod.widgets.HtmlView = mod.widgets.View{}});
    }
    table_picker::script_mod(vm);
    ui::script_mod(vm);
    window::script_mod(vm);
}
