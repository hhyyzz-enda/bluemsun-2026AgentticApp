//! Host-admitted native writing app. No browser, token bridge or downloaded code.
mod agent;
mod diff;
mod graft;
mod host;
mod model;
mod storage;
pub mod ui;
pub mod window;
pub use model::WritingPackage;
pub use ui::{WritingAction, WritingPanelWidgetRefExt};
pub(crate) use model::{clear_studio, invalidate_sessions, set_current_room};
pub const APP_ID: &str = "org.octosense.writing-studio";
pub fn script_mod(vm: &mut makepad_widgets::ScriptVm) {
    ui::script_mod(vm);
    window::script_mod(vm);
}
