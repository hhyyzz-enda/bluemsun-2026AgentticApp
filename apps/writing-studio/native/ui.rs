//! The writing studio: six pages (Library / Edit / Review / Confirm / Done /
//! Publish) around one loop — select a passage, request a rewrite, compare
//! and edit the proposal, confirm exactly what changes, and only then apply.
//! The agent never writes the document; publishing is always a separate,
//! explicitly-confirmed operation with its destination shown.
//!
//! The same panel widget is the in-app modal face and the desktop window
//! card face. Both read and write the shared `model::Studio` store; a
//! half-second poll of the store's version keeps the two faces in sync.
use makepad_widgets::*;
use ruma::OwnedUserId;
use crate::shared::navigation_bar_button::NavigationBarButtonWidgetRefExt;
use crate::sliding_sync::{current_user_id, get_client, spawn_async_task};
use super::{
    agent::{active_service, RewriteService},
    diff::{self, DiffOp},
    graft,
    model::{
        self, apply_proposal, citations, discard_task, expire_if_stale, hash_text,
        missing_citations, studio, studio_version, undo_apply, ApplyOutcome, Constraints,
        Document, Grant, RewriteTask, Selection, Studio, TaskStatus,
    },
    storage,
};

/// Rooms-list style relative times for draft cards and task history lines.
fn fmt_time(secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let Ok(ms) = ruma::UInt::try_from(secs.saturating_mul(1000)) else { return String::new() };
    crate::utils::relative_format(ruma::MilliSecondsSinceUnixEpoch(ms))
        .map(|cow| cow.into_owned())
        .unwrap_or_default()
}

/// Badge id groups per history slot: [ready, applied, muted, failed].
fn badge_variants(slot: usize) -> [&'static [LiveId]; 4] {
    match slot {
        0 => [ids!(badge_ready_0), ids!(badge_applied_0), ids!(badge_muted_0), ids!(badge_failed_0)],
        1 => [ids!(badge_ready_1), ids!(badge_applied_1), ids!(badge_muted_1), ids!(badge_failed_1)],
        _ => [ids!(badge_ready_2), ids!(badge_applied_2), ids!(badge_muted_2), ids!(badge_failed_2)],
    }
}
/// Which pill a status wears: 待审阅 ink/white, 已应用 white with ink border,
/// 失败 keeps the semantic red, everything else muted grey.
fn badge_variant_for(status: &TaskStatus) -> usize {
    match status {
        TaskStatus::Ready => 0,
        TaskStatus::Applied => 1,
        TaskStatus::Failed(_) => 3,
        _ => 2,
    }
}

#[derive(Clone, Debug)]
pub enum WritingAction {
    Open,
    /// Opens (or focuses) the desktop window card face on the current task.
    OpenCard,
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Library,
    Edit,
    Review,
    Confirm,
    Done,
    Publish,
}

const TOP: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };
/// Background preparation takes this long in the mock, so "Preparing" is
/// observable before the review card returns.
const PREPARING_SECS: f64 = 2.5;
/// How often a face polls the shared store for the other face's changes.
const SYNC_SECS: f64 = 0.6;

fn tr(s: &str) -> &str {
    crate::i18n::tr(s)
}
fn now() -> u64 {
    article_core::document::now()
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.WritingLabel = Label {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: #x101010 text_style: theme.font_regular{font_size: 13 line_spacing: 1.45}}}
    mod.widgets.WritingMeta = mod.widgets.WritingLabel {width: Fit draw_text +: {color: #x10101099 text_style: theme.font_regular{font_size: 10.5 line_spacing: 1.1}}}
    mod.widgets.WritingStage = mod.widgets.WritingLabel {width: Fit draw_text +: {color: #x101010 text_style: theme.font_bold{font_size: 12 line_spacing: 1.1}}}
    mod.widgets.WritingTitle = mod.widgets.WritingLabel {draw_text +: {color: #x101010 text_style: theme.font_bold{font_size: 25 line_spacing: 1.12}}}
    mod.widgets.WritingBody = mod.widgets.WritingLabel {draw_text +: {color: #x10101099 text_style: theme.font_regular{font_size: 13 line_spacing: 1.55}}}
    mod.widgets.WritingRule = SolidView {width: Fill height: 1 draw_bg.color: #x1010101f}
    mod.widgets.WritingCard = RoundedShadowView {
        width: Fill height: Fit flow: Down padding: 24 spacing: 14
        show_bg: true
        draw_bg +: {
            color: #xffffff
            border_size: 1.0
            border_color: #x1010101f
            border_radius: 20.0
            shadow_color: #x1010101f
            shadow_radius: 30.0
            shadow_offset: vec2(0.0, 12.0)
        }
    }
    mod.widgets.WritingWarmCard = mod.widgets.WritingCard {draw_bg +: {color: #xf5f5f2}}
    mod.widgets.WritingRow = NavigationBarButton {
        width: Fill height: Fit flow: Down padding: 18 spacing: 8 margin: Inset{bottom: 12}
        draw_bg +: {
            color: #xffffff color_hover: #xf5f5f2 color_active: #xf5f5f2
            border_size: 1.0 border_color: #x1010101f border_radius: 16.0
        }
    }
    // A task-history line inside a draft card: clickable, no chrome of its own.
    mod.widgets.WritingTaskLine = NavigationBarButton {
        visible: false width: Fill height: Fit flow: Right align: Align{y: 0.5}
        margin: 0 padding: Inset{left: 0 right: 0 top: 5 bottom: 5} spacing: 8
        draw_bg +: {
            color: #x00000000 color_hover: #x1010100d color_active: #x1010100d
            border_size: 0.0 border_radius: 8.0
        }
    }
    // Status pill, restyled per task state at runtime (see style_badge).
    mod.widgets.WritingBadgeReady = RobrixNeutralIconButton {
        visible: false grab_key_focus: false
        width: Fit height: Fit margin: 0 padding: Inset{left: 8 right: 8 top: 2 bottom: 3}
        icon_walk: Walk{width: 0 height: 0 margin: 0}
        draw_bg +: {
            color: #x101010 color_hover: #x101010 color_down: #x101010
            border_size: 0.0 border_radius: 9.0
        }
        draw_text +: {
            color: #xffffff color_hover: #xffffff color_down: #xffffff
            text_style: theme.font_bold{font_size: 10}
        }
    }
    mod.widgets.WritingBadgeApplied = mod.widgets.WritingBadgeReady {
        draw_bg +: {
            color: #xffffff color_hover: #xffffff color_down: #xffffff
            border_size: 1.0 border_color: #x101010 border_color_hover: #x101010 border_color_down: #x101010
        }
        draw_text +: {color: #x101010 color_hover: #x101010 color_down: #x101010}
    }
    mod.widgets.WritingBadgeMuted = mod.widgets.WritingBadgeReady {
        draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000}
        draw_text +: {color: #x10101066 color_hover: #x10101066 color_down: #x10101066}
    }
    mod.widgets.WritingBadgeFailed = mod.widgets.WritingBadgeReady {
        draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000}
        draw_text +: {color: #xc0392b color_hover: #xc0392b color_down: #xc0392b}
    }
    mod.widgets.WritingIconButton = RobrixNeutralIconButton {
        grab_key_focus: false enable_long_press: true
        width: 44 height: 44 margin: 0 padding: 0 spacing: 0 text: ""
        align: Align{x: 0.5 y: 0.5}
        icon_walk: Walk{width: 18 height: 18}
        draw_icon +: {color: #x101010}
        draw_bg +: {
            color: #xffffff
            color_hover: #xf5f5f2
            color_down: #xefefea
            border_size: 1.0
            border_color: #x1010101f
            border_color_hover: #x1010101f
            border_color_down: #x1010101f
            border_radius: 22.0
        }
    }
    mod.widgets.WritingButton = RobrixNeutralIconButton {
        width: Fit{min: FitBound.Abs(140.0)} height: 48
        align: Align{x: 0.5 y: 0.5} padding: Inset{left: 20 right: 20 top: 14 bottom: 14}
        icon_walk: Walk{width: 0 height: 0 margin: 0}
        draw_bg +: {
            color: #xffffff
            color_hover: #xf5f5f2
            color_down: #xefefea
            border_size: 1.0
            border_color: #x101010
            border_color_hover: #x101010
            border_color_down: #x101010
            border_radius: (mod.widgets.RBX_RADIUS_PILL)
        }
        draw_text +: {
            color: #x101010
            color_hover: #x101010
            color_down: #x101010
            text_style: theme.font_bold{font_size: 11}
        }
    }
    mod.widgets.WritingPrimary = RobrixNeutralIconButton {
        width: Fit{min: FitBound.Abs(140.0)} height: 48
        align: Align{x: 0.5 y: 0.5} padding: Inset{left: 20 right: 20 top: 14 bottom: 14}
        icon_walk: Walk{width: 0 height: 0 margin: 0}
        draw_bg +: {
            color: #x101010
            color_hover: #x252525
            color_down: #x363636
            border_size: 0.0
            border_color: #x101010
            border_color_hover: #x101010
            border_color_down: #x101010
            border_radius: (mod.widgets.RBX_RADIUS_PILL)
        }
        draw_text +: {
            color: #xffffff
            color_hover: #xffffff
            color_down: #xffffff
            text_style: theme.font_bold{font_size: 11}
        }
    }
    mod.widgets.WritingInput = TextInput {
        width: Fill height: Fill is_multiline: true flow: Flow.Right{wrap: true}
        padding: Inset{left: 24 right: 24 top: 24 bottom: 24}
        draw_cursor +: {color: #x101010}
        draw_bg +: {
            color: #x00000000
            color_hover: #x00000000
            color_focus: #x00000000
            color_empty: #x00000000
            color_down: #x00000000
            border_size: 0.0
            border_color: #x00000000
            border_color_focus: #x00000000
        }
        draw_text +: {
            color: #x101010
            color_focus: #x101010
            color_hover: #x101010
            text_style: theme.font_regular{font_size: 15 line_spacing: 1.6}
        }
    }
    mod.widgets.WritingLineInput = TextInput {
        width: Fill height: 44
        padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
        draw_cursor +: {color: #x101010}
        draw_bg +: {
            color: #xffffff color_hover: #xffffff color_focus: #xffffff color_empty: #ffffff
            border_size: 1.0
            border_color: #x1010101f
            border_color_focus: #x101010
            border_radius: 12.0
        }
        draw_text +: {
            color: #x101010 color_focus: #x101010 color_hover: #x101010
            text_style: theme.font_regular{font_size: 14}
        }
    }

    mod.widgets.WritingPanel = #(WritingPanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill height: Fill flow: Down draw_bg.color: #xffffff
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(TOP) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        header := SolidView {width: Fill height: 68 flow: Right align: Align{y: 0.5} padding: Inset{left: 24 right: 24 top: 12 bottom: 12} spacing: 8 draw_bg.color: #xffffff
            writing_heading := mod.widgets.WritingMeta {width: Fit max_lines: 1 text: #(crate::i18n::tr("Writing studio")) i18n_text: "Writing studio"}
            header_fill := View {width: Fill height: Fill}
            open_card := mod.widgets.WritingButton {visible: false height: 36 width: Fit{min: FitBound.Abs(120.0)} padding: Inset{left: 14 right: 14 top: 8 bottom: 8} text: #(crate::i18n::tr("Open as desktop card")) i18n_text: "Open as desktop card"}
            writing_close := mod.widgets.WritingIconButton {draw_icon +: {svg: ICON_CLOSE}}
        }
        mod.widgets.WritingRule {}

        // -- 01 · Library: the desk's documents ------------------------------
        library := ScrollYView {width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("01 / Library")) i18n_text: "01 / Library"}
            mod.widgets.WritingTitle {text: #(crate::i18n::tr("Drafts")) i18n_text: "Drafts"}
            mod.widgets.WritingBody {text: #(crate::i18n::tr("Pick a document, select a passage inside it, and request a rewrite you stay in control of.")) i18n_text: "Pick a document, select a passage inside it, and request a rewrite you stay in control of."}
            new_doc := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("New document")) i18n_text: "New document"}
            doc_list := PortalList {width: Fill height: Fill
                // A draft card: title, preview, stats, then the document's
                // rewrite history (newest first, up to three lines).
                DocRow := mod.widgets.WritingRow {
                    doc_title := mod.widgets.WritingStage {draw_text +: {text_style: theme.font_bold{font_size: 14.5}}}
                    doc_preview := mod.widgets.WritingBody {max_lines: 1}
                    doc_meta := mod.widgets.WritingMeta {}
                    mod.widgets.WritingRule {}
                    task_line_0 := mod.widgets.WritingTaskLine {
                        badge_ready_0 := mod.widgets.WritingBadgeReady {text: ""}
                        badge_applied_0 := mod.widgets.WritingBadgeApplied {text: ""}
                        badge_muted_0 := mod.widgets.WritingBadgeMuted {text: ""}
                        badge_failed_0 := mod.widgets.WritingBadgeFailed {text: ""}
                        task_request_0 := mod.widgets.WritingLabel {max_lines: 1 draw_text +: {color: #x101010cc text_style: theme.font_regular{font_size: 12}}}
                        View {width: Fill height: Fit}
                        task_time_0 := mod.widgets.WritingMeta {width: Fit}
                    }
                    task_line_1 := mod.widgets.WritingTaskLine {
                        badge_ready_1 := mod.widgets.WritingBadgeReady {text: ""}
                        badge_applied_1 := mod.widgets.WritingBadgeApplied {text: ""}
                        badge_muted_1 := mod.widgets.WritingBadgeMuted {text: ""}
                        badge_failed_1 := mod.widgets.WritingBadgeFailed {text: ""}
                        task_request_1 := mod.widgets.WritingLabel {max_lines: 1 draw_text +: {color: #x101010cc text_style: theme.font_regular{font_size: 12}}}
                        View {width: Fill height: Fit}
                        task_time_1 := mod.widgets.WritingMeta {width: Fit}
                    }
                    task_line_2 := mod.widgets.WritingTaskLine {
                        badge_ready_2 := mod.widgets.WritingBadgeReady {text: ""}
                        badge_applied_2 := mod.widgets.WritingBadgeApplied {text: ""}
                        badge_muted_2 := mod.widgets.WritingBadgeMuted {text: ""}
                        badge_failed_2 := mod.widgets.WritingBadgeFailed {text: ""}
                        task_request_2 := mod.widgets.WritingLabel {max_lines: 1 draw_text +: {color: #x101010cc text_style: theme.font_regular{font_size: 12}}}
                        View {width: Fill height: Fit}
                        task_time_2 := mod.widgets.WritingMeta {width: Fit}
                    }
                    doc_more := mod.widgets.WritingMeta {visible: false}
                }
            }
        }

        // -- 02 · Edit: draft, select, request --------------------------------
        edit := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("01 / Drafting")) i18n_text: "01 / Drafting"}
            doc_title := mod.widgets.WritingLineInput {empty_text: #(crate::i18n::tr("Untitled document")) i18n_empty_text: "Untitled document" draw_text.text_style: theme.font_bold{font_size: 16}}
            writing_source_shell := mod.widgets.WritingWarmCard {width: Fill height: 260 padding: 0 spacing: 0
                doc_body := mod.widgets.WritingInput {empty_text: #(crate::i18n::tr("Write your article…")) i18n_empty_text: "Write your article…"}
            }
            View {width: Fill height: Fit flow: Right spacing: 10
                toggle_concise := mod.widgets.WritingButton {height: 36 width: Fit{min: FitBound.Abs(96.0)} padding: Inset{left: 14 right: 14 top: 8 bottom: 8}}
                toggle_formal := mod.widgets.WritingButton {height: 36 width: Fit{min: FitBound.Abs(96.0)} padding: Inset{left: 14 right: 14 top: 8 bottom: 8}}
                toggle_citations := mod.widgets.WritingButton {height: 36 width: Fit{min: FitBound.Abs(96.0)} padding: Inset{left: 14 right: 14 top: 8 bottom: 8}}
            }
            request := mod.widgets.WritingLineInput {empty_text: #(crate::i18n::tr("Describe the rewrite, e.g. make it concise…")) i18n_empty_text: "Describe the rewrite, e.g. make it concise…"}
            request_rewrite := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("Request rewrite")) i18n_text: "Request rewrite"}
            mod.widgets.WritingRule {}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("Task history")) i18n_text: "Task history"}
            task_list := PortalList {width: Fill height: 200
                TaskRow := mod.widgets.WritingRow {
                    task_title := mod.widgets.WritingStage {}
                    task_meta := mod.widgets.WritingMeta {}
                }
            }
        }

        // -- 03 · Review: compare, edit the proposal ---------------------------
        review := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("02 / Review")) i18n_text: "02 / Review"}
            mod.widgets.WritingTitle {text: #(crate::i18n::tr("Compare the rewrite proposal")) i18n_text: "Compare the rewrite proposal"}
            mod.widgets.WritingBody {text: #(crate::i18n::tr("Review and edit this rewrite directly.")) i18n_text: "Review and edit this rewrite directly."}
            prep_card := mod.widgets.WritingWarmCard {visible: false
                mod.widgets.WritingStage {text: #(crate::i18n::tr("Preparing…")) i18n_text: "Preparing…"}
                mod.widgets.WritingBody {text: #(crate::i18n::tr("A longer draft is being prepared in the background; this card returns as soon as the proposal is ready.")) i18n_text: "A longer draft is being prepared in the background; this card returns as soon as the proposal is ready."}
                prep_engine := mod.widgets.WritingMeta {}
            }
            fail_card := mod.widgets.WritingCard {visible: false
                mod.widgets.WritingStage {text: #(crate::i18n::tr("Failed")) i18n_text: "Failed"}
                fail_reason := mod.widgets.WritingBody {}
                retry_task := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Retry")) i18n_text: "Retry"}
            }
            expired_card := mod.widgets.WritingWarmCard {visible: false
                mod.widgets.WritingStage {text: #(crate::i18n::tr("Expired")) i18n_text: "Expired"}
                mod.widgets.WritingBody {text: #(crate::i18n::tr("Proposal may have expired; regenerate from the latest document")) i18n_text: "Proposal may have expired; regenerate from the latest document"}
                regenerate := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Regenerate")) i18n_text: "Regenerate"}
            }
            compare_box := View {visible: false width: Fill height: Fit flow: Down spacing: 14
                // Side by side: original (warm) on the left, editable proposal
                // (white, ink border) on the right; the diff list spans below.
                View {width: Fill height: Fit flow: Right spacing: 20
                    mod.widgets.WritingWarmCard {width: Fill padding: 18 spacing: 10
                        mod.widgets.WritingMeta {text: #(crate::i18n::tr("Original")) i18n_text: "Original"}
                        review_original := mod.widgets.WritingLabel {draw_text +: {color: #x10101099 text_style: theme.font_regular{font_size: 13.5 line_spacing: 1.6}}}
                    }
                    mod.widgets.WritingCard {width: Fill padding: 18 spacing: 0 draw_bg +: {border_color: #x101010}
                        mod.widgets.WritingMeta {text: #(crate::i18n::tr("Proposal (editable)")) i18n_text: "Proposal (editable)"}
                        View {width: Fill height: 160
                            review_proposal := mod.widgets.WritingInput {padding: Inset{left: 0 right: 0 top: 10 bottom: 0} draw_text.text_style: theme.font_regular{font_size: 13.5 line_spacing: 1.6}}
                        }
                    }
                }
                review_citations := mod.widgets.WritingMeta {width: Fill}
                mod.widgets.WritingCard {padding: 18 spacing: 8
                    mod.widgets.WritingMeta {text: #(crate::i18n::tr("Changes")) i18n_text: "Changes"}
                    diff_adds := mod.widgets.WritingLabel {draw_text +: {color: #x101010 text_style: theme.font_bold{font_size: 12.5 line_spacing: 1.5}}}
                    diff_dels := mod.widgets.WritingLabel {draw_text +: {color: #x10101066 text_style: theme.font_regular{font_size: 12.5 line_spacing: 1.5}}}
                }
                to_confirm := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("Continue to confirm")) i18n_text: "Continue to confirm"}
            }
            review_discard := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Discard")) i18n_text: "Discard"}
        }

        // -- 04 · Confirm: exactly what changes --------------------------------
        confirm := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("03 / Confirm")) i18n_text: "03 / Confirm"}
            mod.widgets.WritingTitle {text: #(crate::i18n::tr("Confirm the change")) i18n_text: "Confirm the change"}
            mod.widgets.WritingBody {text: #(crate::i18n::tr("Apply this rewrite to the selected passage.")) i18n_text: "Apply this rewrite to the selected passage."}
            mod.widgets.WritingCard {
                mod.widgets.WritingMeta {text: #(crate::i18n::tr("Target · selected passage")) i18n_text: "Target · selected passage"}
                confirm_target := mod.widgets.WritingBody {}
                mod.widgets.WritingRule {}
                mod.widgets.WritingMeta {text: #(crate::i18n::tr("Other sections · unchanged")) i18n_text: "Other sections · unchanged"}
                mod.widgets.WritingMeta {text: #(crate::i18n::tr("Action · apply the reviewed rewrite")) i18n_text: "Action · apply the reviewed rewrite"}
            }
            preview_toggle := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Preview the document")) i18n_text: "Preview the document"}
            preview_card := mod.widgets.WritingWarmCard {visible: false
                confirm_preview := mod.widgets.WritingLabel {draw_text +: {color: #x10101099 text_style: theme.font_regular{font_size: 13 line_spacing: 1.6}}}
            }
            View {width: Fill height: Fit flow: Down spacing: 12
                apply_rewrite := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("Apply")) i18n_text: "Apply"}
                confirm_discard := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Discard")) i18n_text: "Discard"}
            }
        }

        // -- 05 · Done: applied, undo kept, publish separate -------------------
        done := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("04 / Done")) i18n_text: "04 / Done"}
            mod.widgets.WritingTitle {text: #(crate::i18n::tr("Rewrite applied")) i18n_text: "Rewrite applied"}
            mod.widgets.WritingBody {text: #(crate::i18n::tr("The document now uses your reviewed wording; publishing is still a separate choice.")) i18n_text: "The document now uses your reviewed wording; publishing is still a separate choice."}
            undo_apply := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Undo")) i18n_text: "Undo"}
            to_publish := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("Publish")) i18n_text: "Publish"}
            back_to_doc := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Back to document")) i18n_text: "Back to document"}
        }

        // -- 06 · Publish: independent, destination shown ----------------------
        publish := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 22 bottom: 16} spacing: 14
            mod.widgets.WritingMeta {text: #(crate::i18n::tr("W R I T I N G   A T E L I E R")) i18n_text: "W R I T I N G   A T E L I E R"}
            SolidView {width: 92 height: 1 draw_bg.color: #x1010101f}
            mod.widgets.WritingStage {text: #(crate::i18n::tr("05 / Publish")) i18n_text: "05 / Publish"}
            mod.widgets.WritingTitle {text: #(crate::i18n::tr("Publish")) i18n_text: "Publish"}
            mod.widgets.WritingBody {text: #(crate::i18n::tr("Publishing is independent from applying. Pick a destination; you confirm it separately.")) i18n_text: "Publishing is independent from applying. Pick a destination; you confirm it separately."}
            dest_chat := mod.widgets.WritingRow {dest_chat_label := mod.widgets.WritingStage {text: #(crate::i18n::tr("Publish to current chat")) i18n_text: "Publish to current chat"}}
            dest_article := mod.widgets.WritingRow {dest_article_label := mod.widgets.WritingStage {text: #(crate::i18n::tr("Article editor · layout studio")) i18n_text: "Article editor · layout studio"}}
            dest_file := mod.widgets.WritingRow {dest_file_label := mod.widgets.WritingStage {text: #(crate::i18n::tr("Export to file (mock)")) i18n_text: "Export to file (mock)"}}
            dest_link := mod.widgets.WritingRow {dest_link_label := mod.widgets.WritingStage {text: #(crate::i18n::tr("Share link (mock)")) i18n_text: "Share link (mock)"}}
            publish_confirm_box := mod.widgets.WritingWarmCard {visible: false
                publish_confirm_label := mod.widgets.WritingBody {}
                publish_confirm := mod.widgets.WritingPrimary {width: Fill text: #(crate::i18n::tr("Confirm destination")) i18n_text: "Confirm destination"}
            }
            publish_back := mod.widgets.WritingButton {width: Fill text: #(crate::i18n::tr("Back to document")) i18n_text: "Back to document"}
        }

        status_wrap := View {width: Fill height: Fit flow: Down padding: Inset{left: 24 right: 24 top: 8 bottom: 16} spacing: 8
            mod.widgets.WritingRule {}
            writing_status := mod.widgets.WritingLabel {draw_text +: {color: #x10101099 text_style: theme.font_regular{font_size: 11 line_spacing: 1.3}}}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct WritingPanel {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    active: bool,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    grant: Option<Grant>,
    #[rust]
    page: Page,
    #[rust]
    doc_id: Option<String>,
    #[rust]
    task_id: Option<String>,
    #[rust]
    constraints: Constraints,
    /// Lazily picked: a configured LLM wins, otherwise the offline mock.
    #[rust]
    service: Option<Box<dyn RewriteService>>,
    /// Pending "Preparing" completion for a long draft.
    #[rust]
    preparing_timer: Timer,
    /// Polls the shared store so both card faces stay in sync.
    #[rust]
    sync_timer: Timer,
    #[rust]
    seen_version: u64,
    #[rust]
    publish_dest: Option<String>,
}

/// Runs `f` with the shared store and this panel's storage grant, persisting
/// what changed. All mutations funnel through here so the on-disk files, the
/// decision trail and the sync version can never be forgotten one at a time.
fn mutate<R>(panel_grant: &Option<Grant>, f: impl FnOnce(&mut Studio) -> R) -> R {
    studio(|s| {
        if s.grant.is_none() {
            s.grant = panel_grant.clone();
        }
        let result = f(s);
        s.bump();
        persist(s);
        result
    })
}

/// Persists documents, tasks and the newest decision under the store's grant.
fn persist(s: &Studio) {
    let Some(grant) = s.grant.clone() else { return };
    let root = crate::app_data_dir();
    if let Err(e) = storage::save_documents(root, &grant, &s.documents) {
        error!("Writing studio: failed to save documents: {e}");
    }
    if let Err(e) = storage::save_tasks(root, &grant, &s.tasks) {
        error!("Writing studio: failed to save tasks: {e}");
    }
    if let Some(decision) = s.decisions.last().cloned() {
        if let Err(e) = storage::append_decision(root, &grant, &decision) {
            error!("Writing studio: failed to log the decision: {e}");
        }
    }
}

/// Writes a worker-thread rewrite result back into the shared store. The
/// version bump is what both faces notice on their next sync poll — the
/// reason the store is a process-global Mutex rather than thread-local.
fn complete_async_task(id: &str, tx_id: &str, result: Result<String, String>) {
    studio(|s| {
        let Some(tpos) = s.tasks.iter().position(|t| t.id == id) else { return };
        {
            let task = &s.tasks[tpos];
            // Stale guard: only a still-waiting task on the same transaction
            // accepts the result; discarded or regenerated tasks ignore it.
            if !matches!(task.status, TaskStatus::Preparing) || task.tx_id != tx_id {
                return;
            }
        }
        match result {
            Ok(proposal) => {
                let task = &mut s.tasks[tpos];
                task.proposal = Some(proposal);
                task.status = TaskStatus::Ready;
                task.proposal_at = Some(now());
                s.log(id, "propose", "提出行动：LLM 后台准备完成".into());
            }
            Err(e) => {
                s.tasks[tpos].status = TaskStatus::Failed(e.clone());
                s.log(id, "fail", e);
            }
        }
        s.bump();
        persist(s);
    });
}

impl WritingPanel {
    fn show(&mut self, cx: &mut Cx, page: Page) {
        self.page = page;
        self.bind(cx);
    }
    fn status(&self, cx: &mut Cx, s: &str) {
        self.label(cx, ids!(writing_status)).set_text(cx, tr(s));
    }
    /// The rewrite engine, picked on first use: configured LLM, else mock.
    fn svc(&mut self) -> &dyn RewriteService {
        &**self.service.get_or_insert_with(active_service)
    }
    fn allowed(&self) -> bool {
        self.grant
            .as_ref()
            .is_some_and(|g| g.valid(current_user_id().as_deref()))
            && !crate::logout::logout_state_machine::is_logout_in_progress()
    }

    /// Loads this account's documents and tasks into the shared store once
    /// per account; later opens (either face) reuse it.
    fn ensure_loaded(&mut self) {
        let Some(owner) = current_user_id() else { return };
        let already = studio(|s| s.loaded_for.as_deref() == Some(owner.as_str()));
        if already {
            return;
        }
        if self.grant.is_none() {
            self.grant = Some(Grant::new(owner.clone()));
        }
        let Some(grant) = self.grant.clone() else { return };
        let root = crate::app_data_dir();
        let documents = storage::load_documents(root, &grant).unwrap_or_default();
        let mut tasks = storage::load_tasks(root, &grant).unwrap_or_default();
        // Age out anything that went stale while the studio was closed.
        for task in &mut tasks {
            expire_if_stale(task, now());
        }
        studio(|s| {
            s.documents = documents;
            s.tasks = tasks;
            s.decisions.clear();
            s.undo.clear();
            s.loaded_for = Some(owner.to_string());
            s.grant = Some(grant);
            s.bump();
        });
    }

    /// Renders the current page from the shared store. Both faces call this
    /// when their poll notices the other face changed something.
    fn bind(&mut self, cx: &mut Cx) {
        self.seen_version = studio_version();
        for (id, p) in [
            (id!(library), Page::Library),
            (id!(edit), Page::Edit),
            (id!(review), Page::Review),
            (id!(confirm), Page::Confirm),
            (id!(done), Page::Done),
            (id!(publish), Page::Publish),
        ] {
            self.view(cx, &[id]).set_visible(cx, p == self.page);
        }
        let desktop = crate::home::home_screen::effective_is_desktop(cx);
        self.button(cx, ids!(open_card)).set_visible(cx, desktop && self.page != Page::Library);
        self.bind_toggles(cx);
        match self.page {
            Page::Edit => self.bind_edit(cx),
            Page::Review => self.bind_review(cx),
            Page::Confirm => self.bind_confirm(cx),
            _ => {}
        }
        self.view.redraw(cx);
    }

    fn bind_toggles(&self, cx: &mut Cx) {
        let mark = |on: bool, label: &str| {
            if on { format!("✓ {}", tr(label)) } else { tr(label).to_owned() }
        };
        self.button(cx, ids!(toggle_concise)).set_text(cx, &mark(self.constraints.concise, "More concise"));
        self.button(cx, ids!(toggle_formal)).set_text(cx, &mark(self.constraints.formal, "More formal"));
        self.button(cx, ids!(toggle_citations)).set_text(cx, &mark(self.constraints.keep_citations, "Keep citations"));
    }

    fn bind_edit(&self, cx: &mut Cx) {
        if let Some(id) = self.doc_id.clone() {
            let doc = studio(|s| s.document(&id).cloned());
            if let Some(doc) = doc {
                // Do not overwrite text the person is typing right now.
                if !cx.has_key_focus(self.text_input(cx, ids!(doc_body)).area()) {
                    self.text_input(cx, ids!(doc_body)).set_text(cx, &doc.body());
                }
                if !cx.has_key_focus(self.text_input(cx, ids!(doc_title)).area()) {
                    self.text_input(cx, ids!(doc_title)).set_text(cx, &doc.title);
                }
            }
        }
    }

    /// The review card per task status: preparing / failed / expired get
    /// their own cards; a ready proposal shows the side-by-side comparison.
    fn bind_review(&mut self, cx: &mut Cx) {
        let Some(id) = self.task_id.clone() else { return };
        // A stale proposal expires the moment it is looked at.
        mutate(&self.grant, |s| {
            if let Some(task) = s.task_mut(&id) {
                if expire_if_stale(task, now()) {
                    s.log(&id, "expire", "提案过期".into());
                }
            }
        });
        let task = studio(|s| s.task(&id).cloned());
        let Some(task) = task else { return };
        let ready = matches!(task.status, TaskStatus::Ready) && task.proposal.is_some();
        self.view(cx, ids!(prep_card)).set_visible(cx, matches!(task.status, TaskStatus::Preparing));
        if matches!(task.status, TaskStatus::Preparing) {
            // Name the engine while the person waits: Octos host, direct
            // LLM with its model, or the offline mock.
            let engine = self.svc().engine_name();
            let text = match engine.as_str() {
                "Mock" => tr("Engine · Mock").to_owned(),
                "Octos" => tr("Engine · Octos host").to_owned(),
                model => crate::i18n::format("Engine · LLM · {model}", &[("model", model.to_owned())]),
            };
            self.label(cx, ids!(prep_engine)).set_text(cx, &text);
        }
        self.view(cx, ids!(fail_card)).set_visible(cx, matches!(task.status, TaskStatus::Failed(_)));
        self.view(cx, ids!(expired_card)).set_visible(cx, matches!(task.status, TaskStatus::Expired));
        self.view(cx, ids!(compare_box)).set_visible(cx, ready);
        if let TaskStatus::Failed(reason) = &task.status {
            self.label(cx, ids!(fail_reason)).set_text(cx, reason);
        }
        if !ready {
            return;
        }
        let proposal = task.proposal.clone().unwrap_or_default();
        self.label(cx, ids!(review_original)).set_text(cx, &task.selection.text_snapshot);
        if !cx.has_key_focus(self.text_input(cx, ids!(review_proposal)).area()) {
            self.text_input(cx, ids!(review_proposal)).set_text(cx, &proposal);
        }
        // Citations: kept markers are listed; missing ones block the way to
        // Confirm, and a long unsourced passage is flagged explicitly.
        let lost = missing_citations(&task.selection.text_snapshot, &proposal);
        let kept = citations(&proposal);
        let citation_line = if !lost.is_empty() {
            format!("{}: {}", tr("Citation markers cannot be removed; restore them to continue"), lost.join(" "))
        } else if !kept.is_empty() {
            format!("{}: {}", tr("Citations kept"), kept.join(" "))
        } else if task.selection.text_snapshot.chars().count() > 150 {
            tr("Needs a source").to_owned()
        } else {
            String::new()
        };
        self.label(cx, ids!(review_citations)).set_text(cx, &citation_line);
        // Word-level diff, rendered as change runs: additions in bold ink,
        // deletions in grey (run-level strikethrough is not available in the
        // DSL, so deletions read "− …" — noted in the delivery report).
        let runs = diff::runs(diff::diff(&task.selection.text_snapshot, &proposal));
        let fmt = |op: DiffOp, prefix: &str| {
            runs.iter()
                .filter(|(o, _)| *o == op)
                .take(8)
                .map(|(_, t)| format!("{prefix}{}", t.trim()))
                .filter(|l| l.len() > prefix.len())
                .collect::<Vec<_>>()
                .join("   ")
        };
        self.label(cx, ids!(diff_adds)).set_text(cx, &fmt(DiffOp::Add, "＋ "));
        self.label(cx, ids!(diff_dels)).set_text(cx, &fmt(DiffOp::Del, "− "));
    }

    fn bind_confirm(&self, cx: &mut Cx) {
        let Some(id) = self.task_id.clone() else { return };
        let (task, doc) = studio(|s| {
            let task = s.task(&id).cloned();
            let doc = task.as_ref().and_then(|t| s.document(&t.doc_id).cloned());
            (task, doc)
        });
        let (Some(task), Some(doc)) = (task, doc) else { return };
        let excerpt: String = task.selection.text_snapshot.chars().take(80).collect();
        self.label(cx, ids!(confirm_target)).set_text(
            cx,
            &format!("{} · #{}: {}…", doc.title, task.selection.para_index + 1, excerpt),
        );
        if self.view(cx, ids!(preview_card)).visible() {
            let mut preview = doc.paragraphs.clone();
            if let Some(para) = preview.get_mut(task.selection.para_index) {
                let (start, end) = (task.selection.start.min(para.len()), task.selection.end.min(para.len()));
                if start <= end && para.is_char_boundary(start) && para.is_char_boundary(end) {
                    let proposal = task.proposal.clone().unwrap_or_default();
                    para.replace_range(start..end, &proposal);
                }
            }
            self.label(cx, ids!(confirm_preview)).set_text(cx, &preview.join("\n\n"));
        }
    }

    /// Writes the editor's title and body back into the store document. Only
    /// real changes bump the document version — that bump is what makes a
    /// stale proposal refuse to apply.
    fn sync_doc_from_editor(&mut self, cx: &mut Cx) {
        let Some(id) = self.doc_id.clone() else { return };
        let title = self.text_input(cx, ids!(doc_title)).text();
        let body = self.text_input(cx, ids!(doc_body)).text();
        mutate(&self.grant, |s| {
            if let Some(doc) = s.document_mut(&id) {
                if doc.title != title {
                    doc.title = title;
                    doc.version += 1;
                }
                doc.set_body(&body);
            }
        });
    }

    fn open_doc(&mut self, cx: &mut Cx, id: String) {
        self.doc_id = Some(id.clone());
        studio(|s| s.focus_doc = Some(id));
        self.bind_edit(cx);
        self.show(cx, Page::Edit);
    }

    /// Selection + request → task. The passage (or, with no selection, the
    /// last 200 characters as a demo) becomes the task's working scope.
    fn request_rewrite(&mut self, cx: &mut Cx) {
        if !self.allowed() {
            return;
        }
        self.sync_doc_from_editor(cx);
        let Some(doc_id) = self.doc_id.clone() else {
            self.status(cx, "Pick a document first");
            return;
        };
        let input = self.text_input(cx, ids!(doc_body));
        let cursor = input.selection();
        let (start, end) = (cursor.start().index, cursor.end().index);
        let text = input.text();
        let (start, end) = if start != end {
            (start, end)
        } else {
            let start = text.len() - text.chars().rev().take(200).map(char::len_utf8).sum::<usize>();
            (start, text.len())
        };
        if text[start..end].trim().is_empty() {
            self.status(cx, "Select some text to format.");
            return;
        }
        let request = self.text_input(cx, ids!(request)).text();
        let constraints = self.constraints;
        self.service.get_or_insert_with(active_service);
        let service: &dyn RewriteService = self.service.as_deref().unwrap();
        let task_id = mutate(&self.grant, |s| {
            let (mut task, title, para_index) = {
                let doc = s.document(&doc_id)?;
                let (para_index, ps, pe) = doc.locate(start, end)?;
                let para = &doc.paragraphs[para_index];
                let selection = Selection {
                    para_index,
                    start: ps,
                    end: pe,
                    text_snapshot: para[ps..pe].to_owned(),
                    para_hash: hash_text(para),
                    doc_version: doc.version,
                };
                (RewriteTask::new(doc, selection, request.clone(), constraints, now()), doc.title.clone(), para_index)
            };
            s.log("", "read", format!("读取状态：文档 {title} 段落 {}", para_index + 1));
            let id = task.id.clone();
            // Short drafts are proposed at once; long ones (and any network
            // call) prepare in the background and the card returns when the
            // proposal is ready.
            if !service.needs_background(&task.selection.text_snapshot) {
                let snapshot = task.selection.text_snapshot.clone();
                match service.rewrite(&snapshot, &request, &constraints) {
                    Ok(proposal) => {
                        task.proposal = Some(proposal);
                        task.status = TaskStatus::Ready;
                        task.proposal_at = Some(now());
                        s.log(&id, "propose", "提出行动：改写提案已生成".into());
                    }
                    Err(e) => {
                        task.status = TaskStatus::Failed(e.clone());
                        s.log(&id, "fail", e);
                    }
                }
            }
            s.tasks.push(task);
            s.focus_doc = Some(doc_id.clone());
            s.focus_task = Some(id.clone());
            Some(id)
        });
        let Some(task_id) = task_id else { return };
        self.task_id = Some(task_id.clone());
        self.status(cx, "");
        self.show(cx, Page::Review);
        // Background path: the mock's stand-in for a long generation.
        let preparing = studio(|s| s.task(&task_id).is_some_and(|t| matches!(t.status, TaskStatus::Preparing)));
        if preparing {
            self.preparing_timer = cx.start_timeout(PREPARING_SECS);
        }
    }

    /// Completes a preparing task. The mock runs inline; a networked service
    /// (curl may block for up to a minute) is forked onto a worker thread and
    /// its result is written back into the shared store by
    /// `complete_async_task` — the version bump turns the card from
    /// "Preparing" into review or failure on the next sync poll.
    fn finish_task(&mut self, cx: &mut Cx, id: &str) {
        let payload = mutate(&self.grant, |s| {
            let task = s.task(id)?;
            if !matches!(task.status, TaskStatus::Preparing) {
                return None;
            }
            Some((task.selection.text_snapshot.clone(), task.request.clone(), task.constraints, task.tx_id.clone()))
        });
        let Some((snapshot, request, constraints, tx_id)) = payload else {
            self.bind(cx);
            return;
        };
        let service = self.svc().fork();
        if service.runs_off_thread() {
            let id = id.to_owned();
            std::thread::spawn(move || {
                let result = service.rewrite(&snapshot, &request, &constraints);
                complete_async_task(&id, &tx_id, result);
            });
            return; // the card stays "Preparing" until the store is updated
        }
        mutate(&self.grant, |s| match service.rewrite(&snapshot, &request, &constraints) {
            Ok(proposal) => {
                let Some(task) = s.task_mut(id) else { return };
                task.proposal = Some(proposal);
                task.status = TaskStatus::Ready;
                task.proposal_at = Some(now());
                s.log(id, "propose", "提出行动：后台准备完成".into());
            }
            Err(e) => {
                if let Some(task) = s.task_mut(id) {
                    task.status = TaskStatus::Failed(e.clone());
                }
                s.log(id, "fail", e);
            }
        });
        self.bind(cx);
    }

    /// Retry runs the service again right away; regenerate additionally
    /// re-snapshots the selection so an expired proposal is rebuilt from the
    /// latest document.
    fn retry_task(&mut self, cx: &mut Cx, regenerate: bool) {
        if !self.allowed() {
            return;
        }
        let Some(id) = self.task_id.clone() else { return };
        if regenerate {
            self.sync_doc_from_editor(cx);
            mutate(&self.grant, |s| {
                let Some(tpos) = s.tasks.iter().position(|t| t.id == id) else { return };
                let doc_id = s.tasks[tpos].doc_id.clone();
                let Some(dpos) = s.documents.iter().position(|d| d.id == doc_id) else { return };
                let doc = &s.documents[dpos];
                let task = &mut s.tasks[tpos];
                if let Some(para) = doc.paragraphs.get(task.selection.para_index) {
                    let end = task.selection.end.min(para.len());
                    let start = task.selection.start.min(end);
                    task.selection.text_snapshot = para[start..end].to_owned();
                    task.selection.end = end;
                    task.selection.para_hash = hash_text(para);
                    task.selection.doc_version = doc.version;
                }
                task.status = TaskStatus::Preparing;
                task.tx_id = model::new_tx_id();
            });
        } else {
            mutate(&self.grant, |s| {
                if let Some(task) = s.task_mut(&id) {
                    task.status = TaskStatus::Preparing;
                }
            });
        }
        self.finish_task(cx, &id);
    }

    fn discard(&mut self, cx: &mut Cx) {
        let Some(id) = self.task_id.clone() else { return };
        mutate(&self.grant, |s| {
            if let Some(task) = s.task_mut(&id) {
                discard_task(task);
                s.log(&id, "discard", "用户决定：放弃改写，文档未变".into());
            }
        });
        let discarded = tr("Rewrite discarded").to_owned();
        self.report(&discarded);
        self.status(cx, &discarded);
        self.show(cx, Page::Edit);
    }

    /// The only place the document changes: double-checked target, editable
    /// proposal, idempotent transaction, undo entry, decision trail.
    fn apply(&mut self, cx: &mut Cx) {
        if !self.allowed() {
            return;
        }
        let Some(id) = self.task_id.clone() else { return };
        // Reflect whatever the person typed since the proposal was made; the
        // version/hash check below then refuses to apply onto changed text.
        self.sync_doc_from_editor(cx);
        // The card's own edits to the proposal are part of what is applied.
        let proposal = self.text_input(cx, ids!(review_proposal)).text();
        let outcome = mutate(&self.grant, |s| {
            let Some(tpos) = s.tasks.iter().position(|t| t.id == id) else {
                return Err("Task is gone.".to_string());
            };
            let proposal = if proposal.is_empty() {
                s.tasks[tpos].proposal.clone().unwrap_or_default()
            } else {
                proposal
            };
            s.log(&id, "confirm", "用户确认：应用审阅后的改写".into());
            let doc_id = s.tasks[tpos].doc_id.clone();
            let Some(dpos) = s.documents.iter().position(|d| d.id == doc_id) else {
                return Err("Document is gone.".to_string());
            };
            let result = {
                let doc = &mut s.documents[dpos];
                let task = &mut s.tasks[tpos];
                apply_proposal(doc, task, &proposal, now()).map(|o| (o, doc.version))
            };
            match result {
                Ok((ApplyOutcome::Applied(undo), version)) => {
                    s.undo.push(undo);
                    s.log(&id, "execute", "执行：改写已应用".into());
                    s.log(&id, "verify", format!("核验：文档版本 {}", version));
                    Ok(true)
                }
                Ok((ApplyOutcome::AlreadyApplied, _)) => Ok(false),
                Err(e) => {
                    s.log(&id, "fail", e.clone());
                    Err(e)
                }
            }
        });
        match outcome {
            Ok(applied) => {
                let message = if applied { tr("Rewrite applied") } else { tr("Already applied") };
                self.report(&message.to_owned());
                self.status(cx, &message.to_owned());
                self.show(cx, Page::Done);
            }
            Err(e) => {
                self.status(cx, &e);
                self.show(cx, Page::Review);
            }
        }
    }

    fn undo(&mut self, cx: &mut Cx) {
        let Some(id) = self.task_id.clone() else { return };
        let outcome = mutate(&self.grant, |s| {
            let position = s.undo.iter().rposition(|u| u.task_id == id)?;
            let entry = s.undo[position].clone();
            let tpos = s.tasks.iter().position(|t| t.id == id)?;
            let dpos = s.documents.iter().position(|d| d.id == entry.doc_id)?;
            {
                let doc = &mut s.documents[dpos];
                let task = &mut s.tasks[tpos];
                undo_apply(doc, task, &entry).ok()?;
            }
            s.undo.remove(position);
            s.log(&id, "undo", "撤销：恢复原文，提案回到待审阅".into());
            Some(())
        });
        if outcome.is_some() {
            self.status(cx, "Undone");
        }
        self.show(cx, Page::Edit);
    }

    /// Publishing is never part of applying: each destination is confirmed
    /// on its own, with the destination named at the moment of execution.
    fn publish(&mut self, cx: &mut Cx) {
        let Some(dest) = self.publish_dest.clone() else { return };
        let message = match dest.as_str() {
            "chat" => {
                let body = studio(|s| {
                    self.doc_id.as_ref().and_then(|id| s.document(id)).map(|d| d.body())
                });
                if let Some(body) = body {
                    self.report(&body);
                }
                tr("Sent")
            }
            "file" => tr("Exported (mock)"),
            "article" => {
                // Graft the whole draft into the article editor's library.
                let Some(grant) = self.grant.clone() else { return };
                let doc = studio(|s| self.doc_id.as_ref().and_then(|id| s.document(id).cloned()));
                let Some(doc) = doc else {
                    self.status(cx, "Pick a document first");
                    return;
                };
                match graft::send_to_article_editor(crate::app_data_dir(), &grant, &doc, now()) {
                    Ok(_) => {
                        // Hand over: open the layout studio on the draft.
                        cx.action(crate::article_app::ArticleAction::Open);
                        tr("Sent to the article editor; open the layout studio to continue")
                    }
                    Err(e) => {
                        // Failure stays on the publish card, reason visible.
                        self.label(cx, ids!(publish_confirm_label)).set_text(cx, &e);
                        self.status(cx, &e);
                        return;
                    }
                }
            }
            _ => tr("Link copied (mock)"),
        };
        let message = message.to_owned();
        mutate(&self.grant, |s| {
            let task = self.task_id.clone().unwrap_or_default();
            s.log(&task, "publish", format!("发布目的地：{dest}（独立确认）"));
        });
        self.status(cx, &format!("{} · {}", message, dest));
        self.publish_dest = None;
        self.view(cx, ids!(publish_confirm_box)).set_visible(cx, false);
    }

    /// Reports to the room in focus as a plain message from the user's own
    /// client. The app never holds an access token: it goes through the
    /// host's client after the grant's publish capability check.
    fn report(&self, body: &str) {
        let Some(grant) = self.grant.as_ref() else { return };
        if grant.authorize(article_core::host::Capability::Publish).is_err() {
            log!("Writing studio: outcome not reported; consent expired.");
            return;
        }
        let (Some(room_id), Some(client)) = (model::current_room(), get_client()) else {
            log!("Writing studio: no room in focus; outcome not reported: {body}");
            return;
        };
        let body = body.to_owned();
        spawn_async_task(async move {
            let Some(room) = client.get_room(&room_id) else {
                error!("Writing studio: room unavailable; outcome not reported.");
                return;
            };
            let content = ruma::events::room::message::RoomMessageEventContent::text_plain(body);
            if let Err(e) = room.send(content).await {
                error!("Writing studio: failed to report the outcome: {e}");
            }
        });
    }
}

impl Widget for WritingPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.active {
            return;
        }
        if self.owner.as_ref() != current_user_id().as_ref()
            || self
                .grant
                .as_ref()
                .is_some_and(|g| !g.valid(current_user_id().as_deref()))
            || matches!(event, Event::Shutdown)
        {
            cx.action(WritingAction::Close);
            return;
        }
        // Background preparation and the cross-face sync poll.
        if self.preparing_timer.is_event(event).is_some() {
            if let Some(id) = self.task_id.clone() {
                self.finish_task(cx, &id);
            }
        }
        if self.sync_timer.is_event(event).is_some() {
            if self.seen_version != studio_version() {
                self.bind(cx);
            }
            self.sync_timer = cx.start_timeout(SYNC_SECS);
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self.button(cx, ids!(writing_close)).clicked(actions) {
                cx.action(WritingAction::Close);
                return;
            }
            if self.button(cx, ids!(open_card)).clicked(actions) {
                cx.action(WritingAction::OpenCard);
                return;
            }
            // Library: a task-history line jumps back to Review (via the
            // shared focus so both faces agree); the card itself opens the doc.
            for (index, item) in self.portal_list(cx, ids!(doc_list)).items_with_actions(actions) {
                let lines = [ids!(task_line_0), ids!(task_line_1), ids!(task_line_2)];
                let mut jumped = false;
                for (slot, line) in lines.iter().enumerate() {
                    if !item.view(cx, *line).as_navigation_bar_button().clicked(actions) {
                        continue;
                    }
                    let target = studio(|s| {
                        let doc = s.documents.get(index)?;
                        let mut tasks = s.tasks_of(&doc.id);
                        tasks.reverse();
                        let task = tasks.get(slot)?;
                        matches!(task.status, TaskStatus::Ready | TaskStatus::Preparing)
                            .then(|| (doc.id.clone(), task.id.clone()))
                    });
                    if let Some((doc_id, task_id)) = target {
                        self.doc_id = Some(doc_id.clone());
                        self.task_id = Some(task_id.clone());
                        mutate(&self.grant, |s| {
                            s.focus_doc = Some(doc_id);
                            s.focus_task = Some(task_id);
                        });
                        self.show(cx, Page::Review);
                    }
                    jumped = true;
                    break;
                }
                if jumped {
                    break;
                }
                if item.as_navigation_bar_button().clicked(actions) {
                    let id = studio(|s| s.documents.get(index).map(|d| d.id.clone()));
                    if let Some(id) = id {
                        self.open_doc(cx, id);
                    }
                    break;
                }
            }
            for (index, item) in self.portal_list(cx, ids!(task_list)).items_with_actions(actions) {
                if item.as_navigation_bar_button().clicked(actions) {
                    let id = studio(|s| {
                        self.doc_id.as_ref().and_then(|d| {
                            let mut tasks = s.tasks_of(d);
                            tasks.reverse();
                            tasks.get(index).map(|t| t.id.clone())
                        })
                    });
                    if let Some(id) = id {
                        self.task_id = Some(id);
                        self.show(cx, Page::Review);
                    }
                    break;
                }
            }
            if self.button(cx, ids!(new_doc)).clicked(actions) {
                let id = mutate(&self.grant, |s| {
                    let doc = Document::new(tr("Untitled document").to_owned(), vec![String::new()]);
                    let id = doc.id.clone();
                    s.documents.push(doc);
                    id
                });
                self.open_doc(cx, id);
            }
            // Constraint toggles.
            let mut toggled = false;
            if self.button(cx, ids!(toggle_concise)).clicked(actions) {
                self.constraints.concise = !self.constraints.concise;
                toggled = true;
            }
            if self.button(cx, ids!(toggle_formal)).clicked(actions) {
                self.constraints.formal = !self.constraints.formal;
                toggled = true;
            }
            if self.button(cx, ids!(toggle_citations)).clicked(actions) {
                self.constraints.keep_citations = !self.constraints.keep_citations;
                toggled = true;
            }
            if toggled {
                self.bind_toggles(cx);
            }
            // Proposal edits are part of the shared draft immediately.
            if let Some(text) = self.text_input(cx, ids!(review_proposal)).changed(actions) {
                if let Some(id) = self.task_id.clone() {
                    mutate(&self.grant, |s| {
                        if let Some(task) = s.task_mut(&id) {
                            task.proposal = Some(text);
                        }
                    });
                }
            }
            match self.page {
                Page::Edit => {
                    if self.button(cx, ids!(request_rewrite)).clicked(actions) {
                        self.request_rewrite(cx);
                    }
                }
                Page::Review => {
                    if self.button(cx, ids!(to_confirm)).clicked(actions) {
                        let blocked = studio(|s| {
                            self.task_id.as_ref().is_some_and(|id| {
                                s.task(id).is_some_and(|t| {
                                    let proposal = t.proposal.clone().unwrap_or_default();
                                    !missing_citations(&t.selection.text_snapshot, &proposal).is_empty()
                                })
                            })
                        });
                        if blocked {
                            self.status(cx, "Citation markers cannot be removed; restore them to continue");
                        } else {
                            self.show(cx, Page::Confirm);
                        }
                    }
                    if self.button(cx, ids!(review_discard)).clicked(actions) {
                        self.discard(cx);
                    }
                    if self.button(cx, ids!(retry_task)).clicked(actions) {
                        self.retry_task(cx, false);
                    }
                    if self.button(cx, ids!(regenerate)).clicked(actions) {
                        self.retry_task(cx, true);
                    }
                }
                Page::Confirm => {
                    if self.button(cx, ids!(preview_toggle)).clicked(actions) {
                        let card = self.view(cx, ids!(preview_card));
                        card.set_visible(cx, !card.visible());
                        self.bind_confirm(cx);
                        self.view.redraw(cx);
                    }
                    if self.button(cx, ids!(apply_rewrite)).clicked(actions) {
                        self.apply(cx);
                    }
                    if self.button(cx, ids!(confirm_discard)).clicked(actions) {
                        self.discard(cx);
                    }
                }
                Page::Done => {
                    if self.button(cx, ids!(undo_apply)).clicked(actions) {
                        self.undo(cx);
                    }
                    if self.button(cx, ids!(to_publish)).clicked(actions) {
                        self.show(cx, Page::Publish);
                    }
                    if self.button(cx, ids!(back_to_doc)).clicked(actions) {
                        self.show(cx, Page::Edit);
                    }
                }
                Page::Publish => {
                    let dest = if self.button(cx, ids!(dest_chat)).clicked(actions)
                        || self.view(cx, ids!(dest_chat)).as_navigation_bar_button().clicked(actions)
                    {
                        Some(("chat", tr("Publish to current chat")))
                    } else if self.button(cx, ids!(dest_article)).clicked(actions)
                        || self.view(cx, ids!(dest_article)).as_navigation_bar_button().clicked(actions)
                    {
                        Some(("article", tr("Article editor · layout studio")))
                    } else if self.button(cx, ids!(dest_file)).clicked(actions)
                        || self.view(cx, ids!(dest_file)).as_navigation_bar_button().clicked(actions)
                    {
                        Some(("file", tr("Export to file (mock)")))
                    } else if self.button(cx, ids!(dest_link)).clicked(actions)
                        || self.view(cx, ids!(dest_link)).as_navigation_bar_button().clicked(actions)
                    {
                        Some(("link", tr("Share link (mock)")))
                    } else {
                        None
                    };
                    if let Some((key, label)) = dest {
                        self.publish_dest = Some(key.to_owned());
                        let title = studio(|s| {
                            self.doc_id.as_ref().and_then(|id| s.document(id)).map(|d| d.title.clone())
                        })
                        .unwrap_or_default();
                        self.label(cx, ids!(publish_confirm_label)).set_text(
                            cx,
                            &format!("{}: {} · {}", tr("Confirm destination"), label, title),
                        );
                        self.view(cx, ids!(publish_confirm_box)).set_visible(cx, true);
                        self.view.redraw(cx);
                    }
                    if self.button(cx, ids!(publish_confirm)).clicked(actions) {
                        self.publish(cx);
                    }
                    if self.button(cx, ids!(publish_back)).clicked(actions) {
                        self.show(cx, Page::Edit);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            let uid = item.widget_uid();
            let docs = uid == self.portal_list(cx, ids!(doc_list)).widget_uid();
            let tasks = uid == self.portal_list(cx, ids!(task_list)).widget_uid();
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let count = studio(|s| {
                    if docs {
                        s.documents.len()
                    } else if tasks {
                        self.doc_id.as_ref().map_or(0, |d| s.tasks_of(d).len())
                    } else {
                        0
                    }
                });
                list.set_item_range(cx, 0, count);
                while let Some(index) = list.next_visible_item(cx) {
                    if index >= count {
                        continue;
                    }
                    if docs {
                        let row = list.item(cx, index, id!(DocRow));
                        studio(|s| {
                            let Some(doc) = s.documents.get(index) else { return };
                            row.label(cx, ids!(doc_title)).set_text(cx, &doc.title);
                            row.label(cx, ids!(doc_preview)).set_text(cx, &doc.preview(40));
                            let paragraphs = crate::i18n::format("{count} paragraphs", &[("count", doc.paragraphs.len().to_string())]);
                            let modified = fmt_time(doc.modified);
                            let meta = if modified.is_empty() {
                                format!("{paragraphs} · v{}", doc.version)
                            } else {
                                format!("{paragraphs} · {}", crate::i18n::format("Last modified {time}", &[("time", modified)]))
                            };
                            row.label(cx, ids!(doc_meta)).set_text(cx, &meta);
                            // The document's rewrite history, newest first,
                            // up to three lines inside the card.
                            let mut tasks = s.tasks_of(&doc.id);
                            tasks.reverse();
                            let total = tasks.len();
                            let open = tasks.iter().filter(|t| matches!(t.status, TaskStatus::Ready | TaskStatus::Preparing)).count();
                            let lines = [ids!(task_line_0), ids!(task_line_1), ids!(task_line_2)];
                            let requests = [ids!(task_request_0), ids!(task_request_1), ids!(task_request_2)];
                            let times = [ids!(task_time_0), ids!(task_time_1), ids!(task_time_2)];
                            for slot in 0..3 {
                                let Some(task) = tasks.get(slot) else {
                                    row.view(cx, lines[slot]).set_visible(cx, false);
                                    continue;
                                };
                                row.view(cx, lines[slot]).set_visible(cx, true);
                                let variants = badge_variants(slot);
                                let shown = badge_variant_for(&task.status);
                                for (v, badge_id) in variants.iter().enumerate() {
                                    let badge = row.button(cx, badge_id);
                                    badge.set_visible(cx, v == shown);
                                    if v == shown {
                                        badge.set_text(cx, tr(task.status.label()));
                                    }
                                }
                                row.label(cx, requests[slot]).set_text(cx, &task.request);
                                row.label(cx, times[slot]).set_text(cx, &fmt_time(task.created_at));
                            }
                            let more = row.label(cx, ids!(doc_more));
                            if total == 0 {
                                more.set_visible(cx, true);
                                more.set_text(cx, tr("No tasks yet"));
                            } else if total > 3 {
                                more.set_visible(cx, true);
                                more.set_text(cx, &crate::i18n::format(
                                    "{total} tasks · {open} awaiting review",
                                    &[("total", total.to_string()), ("open", open.to_string())],
                                ));
                            } else {
                                more.set_visible(cx, false);
                            }
                        });
                        row.draw_all(cx, &mut Scope::empty());
                    } else if tasks {
                        let row = list.item(cx, index, id!(TaskRow));
                        studio(|s| {
                            if let Some(doc_id) = self.doc_id.as_ref() {
                                let mut tasks = s.tasks_of(doc_id);
                                tasks.reverse();
                                if let Some(task) = tasks.get(index) {
                                    row.label(cx, ids!(task_title)).set_text(
                                        cx,
                                        &format!("{} · {}", tr(task.status.label()), task.request),
                                    );
                                    let excerpt: String = task.selection.text_snapshot.chars().take(40).collect();
                                    row.label(cx, ids!(task_meta)).set_text(cx, &format!("#{} · {}", task.selection.para_index + 1, excerpt));
                                }
                            }
                        });
                        row.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        DrawStep::done()
    }
}

impl WritingPanelRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &WritingAction) {
        let Some(mut panel) = self.borrow_mut() else {
            return;
        };
        match action {
            WritingAction::Open => {
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                panel.active = true;
                panel.owner = current_user_id();
                panel.doc_id = None;
                panel.task_id = None;
                panel.publish_dest = None;
                if let Some(owner) = current_user_id() {
                    panel.grant = Some(Grant::new(owner));
                }
                panel.ensure_loaded();
                // Resume where the desk was left: the focused document, or the
                // library when there is none.
                let focus = studio(|s| s.focus_doc.clone());
                panel.status(cx, "");
                match focus {
                    Some(id) => panel.open_doc(cx, id),
                    None => panel.show(cx, Page::Library),
                }
                panel.sync_timer = cx.start_timeout(SYNC_SECS);
                modal.open(cx);
            }
            WritingAction::OpenCard => {
                // The second face joins the same desk: same store, same focus,
                // its own consent lease. Nothing is reset.
                if !panel.active {
                    panel.active = true;
                    panel.owner = current_user_id();
                    if let Some(owner) = current_user_id() {
                        panel.grant = Some(Grant::new(owner));
                    }
                    panel.ensure_loaded();
                    panel.sync_timer = cx.start_timeout(SYNC_SECS);
                }
                let (focus_doc, focus_task) = studio(|s| (s.focus_doc.clone(), s.focus_task.clone()));
                panel.doc_id = focus_doc.clone();
                panel.task_id = focus_task.clone();
                let page = match focus_task.as_ref().and_then(|id| studio(|s| s.task(id).map(|t| t.status.clone()))) {
                    Some(TaskStatus::Ready | TaskStatus::Preparing | TaskStatus::Failed(_) | TaskStatus::Expired) => Page::Review,
                    _ if focus_doc.is_some() => Page::Edit,
                    _ => Page::Library,
                };
                panel.status(cx, "");
                panel.show(cx, page);
            }
            WritingAction::Close => {
                // Closing revokes this face's consent, as the article editor
                // does. The shared desk stays: the other face may be open, and
                // on-disk state is already persisted.
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                // Re-issue the store's lease so the surviving face keeps a
                // valid grant for write-through.
                if let Some(owner) = current_user_id() {
                    studio(|s| {
                        if s.loaded_for.is_some() {
                            s.grant = Some(Grant::new(owner));
                        }
                    });
                }
                panel.active = false;
                panel.owner = None;
                cx.stop_timer(panel.sync_timer);
                modal.close(cx);
            }
        }
    }
}
