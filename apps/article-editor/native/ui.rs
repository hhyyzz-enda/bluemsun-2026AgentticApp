use crate::theme::Snapshot as ThemeSnapshot;
use std::collections::BTreeMap;
use article_core::editing::EditHistory;
use article_makepad::body_selection::{ArticleSelection, SelectionUpdate};
use makepad_widgets::{makepad_draw::text::selection::{Cursor, Selection}, text_input::UndoGroup};
use super::table_picker::TableSizePickerWidgetRefExt;
use article_makepad::content::Images as PreviewImages;
#[cfg(feature = "html_preview")]
use makepad_html_renderer::makepad::HtmlViewWidgetExt;
use makepad_widgets::*;
use ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use crate::{
    home::rooms_list::RoomsListRef,
    shared::navigation_bar_button::NavigationBarButtonWidgetRefExt,
    sliding_sync::{current_user_id, get_client, spawn_async_task},
    utils::RoomNameId,
};
use super::{
    document::*,
    model::{Grant, ArticlePackage},
    storage::{self, Library, Publication, Operation, OperationKind},
    backend::{self, ArticleContent},
    rich_input::ArticleRichInputWidgetRefExt,
};

#[derive(Clone, Debug)]
pub enum ArticleAction {
    Open,
    Close,
    Read {
        room: OwnedRoomId,
        event: OwnedEventId,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Details,
    Consent,
    Library,
    Edit,
    /// The Markdown writing view: full-document source with a live preview (huasheng-style).
    Write,
    Source,
    Images,
    ImageSettings,
    Link,
    Theme,
    Cover,
    Preview,
    CssPreview,
    Review,
    Rooms,
    Confirm,
    Publication,
    /// Success after publishing from the editor (atlas: 已发布).
    Published,
    Withdraw,
    Reader,
}
#[derive(Clone, Debug)]
enum ResultAction {
    RemoteImages { instance: String, key: String, images: article_makepad::content::Images },
    Imported {
        instance: String,
        result: Result<Option<Document>, String>,
    },
    #[cfg(feature = "html_preview")]
    CssPreview {
        instance: String,
        request: String,
        result: Result<super::preview::Update, String>,
    },
    Image {
        instance: String,
        document: String,
        cover: bool,
        result: Result<storage::Asset, String>,
    },
    Published {
        instance: String,
        result: Result<Publication, String>,
    },
    Shared {
        instance: String,
        result: Result<(), String>,
    },
    Read {
        instance: String,
        result: Result<ArticleContent, String>,
    },
    Media {
        instance: String,
        id: String,
        result: Result<Vec<u8>, String>,
    },
}
const TOP: f64 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };
fn tr(s: &str) -> &str {
    crate::i18n::tr(s)
}
fn color(hex: u32) -> Vec4f {
    vec4(
        ((hex >> 16) & 255) as f32 / 255.0,
        ((hex >> 8) & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
        1.0,
    )
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.ArticleLabel = RinxLabel {width: Fill height: Fit flow: Flow.Right{wrap: true} draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}}
    mod.widgets.ArticleButton = RobrixNeutralIconButton {
        grab_key_focus: false enable_long_press: true
        width: 44 height: 44 margin: 0 padding: 0 spacing: 0 text: ""
        align: Align{x: 0.5 y: 0.5}
        icon_walk: Walk{width: 20 height: 20}
        draw_icon +: {color: mod.widgets.RINX_INK}
        draw_bg +: {color: mod.widgets.RINX_SURFACE color_hover: mod.widgets.RINX_HOVER color_down: mod.widgets.RINX_SELECTED border_radius: 6}
    }
    mod.widgets.ArticlePrimary = mod.widgets.ArticleButton {width: Fill height: 46 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_ACCENT color_hover: mod.widgets.RINX_ACCENT_HOVER color_down: mod.widgets.RINX_ACCENT_DOWN} draw_icon +: {color: mod.widgets.RINX_ON_ACCENT}}
    mod.widgets.ArticleActions = View {width: Fill height: 44 flow: Right align: Align{x: 1.0 y: 0.5} spacing: 12}
    mod.widgets.ArticleInput = RinxInput {width: Fill height: 44 draw_cursor +: {color: mod.widgets.RINX_ACCENT} draw_bg +: {color: mod.widgets.RINX_SURFACE color_hover: mod.widgets.RINX_SURFACE color_focus: mod.widgets.RINX_SURFACE color_empty: mod.widgets.RINX_SURFACE border_color: mod.widgets.RINX_BORDER border_color_focus: mod.widgets.RINX_ACCENT} draw_text +: {color: mod.widgets.RINX_INK color_focus: mod.widgets.RINX_INK color_hover: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
    mod.widgets.ArticleHtml = Html {
        selectable: true width: Fill height: Fit padding: 0
        font_size: (14 * mod.widgets.RINX_TEXT_SCALE)
        font_color: mod.widgets.RINX_INK draw_text.color: mod.widgets.RINX_INK
        text_style_normal: theme.font_regular{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}
        text_style_bold: theme.font_bold{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}
        text_style_italic: theme.font_italic{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}
        text_style_bold_italic: theme.font_bold_italic{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}
        // Keep the upstream table layout explicit: reload otherwise resets
        // missing nested Walk/Layout values to empty defaults.
        table_walk: Walk{width: Fill height: Fit}
        table_layout: Layout{flow: Down}
        table_row_walk: Walk{width: Fill height: Fit}
        table_row_layout: Layout{flow: Right}
        table_cell_layout: Layout{flow: Flow.Right{wrap: true} padding: Inset{left: 6 right: 6 top: 4 bottom: 4}}
        rmath := ArticleMath {} rdiagram := ArticleDiagram {}
        rimage := ArticleImage {} remoji := ArticleEmoji {}
        rcode := ArticleCode {} rcell := ArticleCell {}
    }
    // Only plain shared Markdown inherits application reading defaults.
    // This styles upstream components; authored article templates stay separate.
    mod.widgets.RinxReaderHtml = mod.widgets.ArticleHtml {
        a := HtmlLink {color: mod.widgets.RINX_ACCENT hover_color: mod.widgets.RINX_ACCENT_HOVER pressed_color: mod.widgets.RINX_ACCENT_DOWN}
        rcode := ArticleCode {
            draw_bg +: {color: uniform(mod.widgets.RINX_CODE_BG)}
            draw_selection +: {color: mod.widgets.RINX_SELECTED}
            draw_text +: {
                reader_ink: uniform(mod.widgets.RINX_CODE_FG)
                get_color: fn() {return self.reader_ink}
            }
        }
        rcell := ArticleCell {html: Html {
            selectable: true width: Fill height: Fit padding: 0
            rmath := ArticleMath {} rimage := ArticleImage {} remoji := ArticleEmoji {}
            a := HtmlLink {color: mod.widgets.RINX_ACCENT hover_color: mod.widgets.RINX_ACCENT_HOVER pressed_color: mod.widgets.RINX_ACCENT_DOWN}
        }}
    }
    mod.widgets.ArticleRich = ArticleRichInput {width: Fill height: Fit draw_cursor +: {color: mod.widgets.RINX_ACCENT} is_multiline: true flow: Flow.Right{wrap: true} padding: Inset{top: 4 bottom: 12 left: 0 right: 0} empty_text: #(crate::i18n::tr("Write your article…")) draw_bg +: {color: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000 color_empty: #x00000000 border_size: 0} draw_text +: {color: mod.widgets.RINX_INK color_hover: mod.widgets.RINX_INK color_focus: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}} draw_bold.text_style: theme.font_bold{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)} draw_italic.text_style: theme.font_italic{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)} draw_bold_italic.text_style: theme.font_bold_italic{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}
    mod.widgets.ArticleIconButton = mod.widgets.ArticleButton {
        width: 44 height: 44 padding: 0 spacing: 0 text: ""
        align: Align{x: 0.5 y: 0.5}
        enable_long_press: true
        icon_walk: Walk{width: 20 height: 20}
        draw_icon +: {color: mod.widgets.RINX_INK}
    }
    mod.widgets.ArticleTool = mod.widgets.ArticleIconButton {width: 36 height: 36 draw_bg +: {color: #x00000000 color_hover: mod.widgets.RINX_FIELD color_down: mod.widgets.RINX_PRESSED border_size: 0}}
    // A compact theme swatch for the writing view's style panel: a tiny page with the
    // theme's paper, ink and accent, and the theme's name below.
    mod.widgets.ArticleThemeSwatch = NavigationBarButton {width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 0.0} padding: 0 spacing: 6
        draw_bg +: {color: #x00000000 color_hover: #x00000000 color_active: #x00000000}
        page := RoundedView {width: Fill height: 72 flow: Down padding: 9 spacing: 6 draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 4.0}
            heading := SolidView {width: 34 height: 3}
            line1 := SolidView {width: Fill height: 2}
            line2 := SolidView {width: Fill height: 2}
            accent := SolidView {width: Fill height: 3}
        }
        name := mod.widgets.ArticleLabel {width: Fit padding: 0 draw_text.text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
    }
    // A chat to publish to: initial avatar, name and a radio mark (atlas 发布到).
    mod.widgets.ArticleChatRow = NavigationBarButton {width: Fill height: 64 flow: Right align: Align{y: 0.5} padding: Inset{left: 12 right: 12} spacing: 12
        draw_bg +: {color_hover: mod.widgets.RINX_PAGE color_active: mod.widgets.RINX_PAGE}
        avatar := RoundedView {width: 42 height: 42 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_SELECTED border_radius: 21.0}
            initial := Label {draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_bold{font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}}
        }
        name := mod.widgets.ArticleLabel {width: Fill max_lines: 1 draw_text.text_style: theme.font_regular{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}
        radio := RoundedView {width: 22 height: 22 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.5 border_color: mod.widgets.RINX_BORDER border_radius: 11.0}
            check := Label {text: "✓" padding: 0 draw_text +: {color: mod.widgets.RINX_SURFACE text_style: theme.font_bold{font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
        }
    }
    mod.widgets.ArticleThemeTile = NavigationBarButton {width: Fill height: 210 flow: Down align: Align{x: 0.0 y: 0.0} padding: 12 spacing: 8
        draw_bg +: {color: instance(mod.widgets.RINX_SURFACE) border_size: 1.0 border_color: mod.widgets.RINX_BORDER get_color: fn() {return self.color}}
        name := mod.widgets.ArticleLabel {padding: 0 max_lines: 1 draw_text.text_style: theme.font_bold{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
        title := mod.widgets.ArticleLabel {padding: 0 max_lines: 2 draw_text.text_style: theme.font_bold{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
        sample := mod.widgets.ArticleLabel {padding: 0 max_lines: 3 draw_text.text_style: theme.font_regular{font_size: (9 * mod.widgets.RINX_TEXT_SCALE)}}
        picture := Image {width: Fill height: 65 fit: ImageFit.Biggest}
    }
    mod.widgets.ArticlePanel = #(ArticlePanel::register_widget(vm)) {
        ..mod.widgets.SolidView
        icon_tooltip: CalloutTooltip {}
        width: Fill height: Fill flow: Down draw_bg.color: mod.widgets.RINX_SURFACE
        padding: Inset{top: mod.widgets.SAFE_INSET_PAD_TOP + #(TOP) bottom: mod.widgets.SAFE_INSET_PAD_BOTTOM}
        header := SolidView {width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 8 right: 12} spacing: 8 draw_bg.color: mod.widgets.RINX_PAGE
            article_back := RobrixNeutralIconButton {enable_long_press: true width: 36 height: 44 margin: 0 padding: 12 spacing: 0 draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0} draw_icon +: {svg: ICON_CHEVRON_LEFT color: mod.widgets.RINX_INK} icon_walk: Walk{width: 8 height: 14}}
            article_heading := mod.widgets.ArticleLabel {width: Fit max_lines: 1 draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
            header_fill := View {width: Fill height: Fill}
            // The writing view's article title, centred in the header as in the atlas.
            write_title_box := View {visible: false width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                write_title := mod.widgets.ArticleInput {width: 300 height: 40 empty_text: #(crate::i18n::tr("Article title")) i18n_empty_text: "Article title" draw_text.text_style: theme.font_bold{font_size: (15 * mod.widgets.RINX_TEXT_SCALE)} draw_bg +: {border_size: 0 color: #x00000000 color_empty: #x00000000 color_hover: #x00000000 color_focus: mod.widgets.RINX_PAGE color_down: #x00000000}}
            }
            // Writing view controls, as in the redesigned editor atlas.
            write_controls := View {visible: false width: Fit height: Fill flow: Right align: Align{y: 0.5} spacing: 8
                write_style := mod.widgets.ArticleIconButton {draw_icon.color: mod.widgets.RINX_ACCENT draw_bg +: {border_size: 1.0 border_color: mod.widgets.RINX_BORDER} draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg")}}
                RoundedView {width: Fit height: 48 flow: Right padding: 2 spacing: 0 draw_bg +: {color: mod.widgets.RINX_SURFACE border_radius: 6.0}
                    write_mode_source := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_EDIT}}
                    write_mode_split := mod.widgets.ArticleIconButton {draw_icon +: {svg: crate_resource("self://resources/icons/article_split.svg")}}
                    write_mode_preview := mod.widgets.ArticleIconButton {draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
                }
                write_saved := mod.widgets.ArticleLabel {width: Fit draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
                write_publish := mod.widgets.ArticleIconButton {draw_icon.color: mod.widgets.RINX_ON_ACCENT draw_bg +: {color: mod.widgets.RINX_ACCENT color_hover: mod.widgets.RINX_ACCENT_HOVER color_down: mod.widgets.RINX_ACCENT_DOWN} draw_icon +: {svg: ICON_SEND}}
            }
            article_save := mod.widgets.ArticleIconButton {visible: false draw_icon +: {svg: crate_resource("self://resources/icons/article_save.svg")}}
            // Send this article back to the writing studio for continued
            // rewriting; the article's provenance realigns the versions.
            article_to_writing := mod.widgets.ArticleIconButton {visible: false draw_icon +: {svg: crate_resource("self://resources/icons/writing-studio.svg")}}
            article_done := mod.widgets.ArticleIconButton {visible: false draw_icon.color: mod.widgets.RINX_ACCENT draw_icon +: {svg: ICON_CHECKMARK}}
            article_close := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_CLOSE}}
            // Library: a new article, as the atlas's + in the header.
            article_new := mod.widgets.ArticleIconButton {visible: false draw_icon.color: mod.widgets.RINX_ACCENT draw_icon +: {svg: ICON_ADD}}
        }
        details := View {width: Fill height: Fill flow: Down padding: 28 spacing: 24
            // The text scrolls on short windows; the actions stay in view.
            ScrollYView {width: Fill height: Fill flow: Down spacing: 24
                View {width: Fill height: 35}
                Icon {width: 48 height: 48 draw_icon +: {svg: ICON_EDIT color: mod.widgets.RINX_ACCENT}}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article studio")) i18n_text: "Article studio" draw_text.text_style: theme.font_bold{font_size: (24 * mod.widgets.RINX_TEXT_SCALE)}}
                mod.widgets.ArticleLabel {text: "OctoSense · 2.0" draw_text.color: mod.widgets.RINX_MUTED}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Write visually. Add images, choose a theme, review and publish.")) i18n_text: "Write visually. Add images, choose a theme, review and publish."}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Sharing this app shares neither your drafts nor your account permissions.")) i18n_text: "Sharing this app shares neither your drafts nor your account permissions." draw_text.color: mod.widgets.RINX_MUTED}
            }
            article_continue := RinxPrimaryButton {width: Fill text: #(crate::i18n::tr("Continue")) i18n_text: "Continue"}
        }
        consent := View {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 24
            // The text scrolls on short windows; the actions stay in view.
            ScrollYView {width: Fill height: Fill flow: Down spacing: 24
                consent_account := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("This app requests permission to:")) i18n_text: "This app requests permission to:"}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("• Save your drafts and selected images on this device\n\n• Upload images and publish only after confirmation\n\n• Update or withdraw your own articles after confirmation")) i18n_text: "• Save your drafts and selected images on this device\n\n• Upload images and publish only after confirmation\n\n• Update or withdraw your own articles after confirmation"}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your password and session token stay with Rinx. Permission expires when you close this app or after one hour.")) i18n_text: "Your password and session token stay with Rinx. Permission expires when you close this app or after one hour." draw_text.color: mod.widgets.RINX_MUTED}
            }
            mod.widgets.ArticleActions {
                article_cancel := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_CLOSE}}
                article_allow := mod.widgets.ArticlePrimary {width: 44 height: 44 draw_icon +: {svg: ICON_CHECKMARK}}
            }
        }
        library := SolidView {visible: false width: Fill height: Fill flow: Down padding: 16 spacing: 12 draw_bg.color: mod.widgets.RINX_SURFACE
            article_search := mod.widgets.ArticleInput {draw_bg +: {color: mod.widgets.RINX_PAGE color_hover: mod.widgets.RINX_PAGE color_empty: mod.widgets.RINX_PAGE border_size: 0} empty_text: #(crate::i18n::tr("Search articles")) i18n_empty_text: "Search articles"}
            View {width: Fill height: 44 flow: Right spacing: 6
                drafts_tab := mod.widgets.ArticleButton {draw_bg +: {color: mod.widgets.RINX_PAGE} width: Fill draw_icon +: {svg: ICON_EDIT}}
                published_tab := mod.widgets.ArticleButton {draw_bg +: {color: mod.widgets.RINX_PAGE} width: Fill draw_icon +: {svg: ICON_SEND}}
                withdrawn_tab := mod.widgets.ArticleButton {draw_bg +: {color: mod.widgets.RINX_PAGE} width: Fill draw_icon +: {svg: ICON_TRASH}}
            }
            library_empty := mod.widgets.ArticleLabel {text: #(crate::i18n::tr("No articles here yet. Create your first article.")) i18n_text: "No articles here yet. Create your first article." draw_text.color: mod.widgets.RINX_MUTED}
            article_library := PortalList {width: Fill height: Fill
                // Atlas article-list: thumbnail, title, status line and a chevron.
                Entry := NavigationBarButton {width: Fill height: 104 flow: Right align: Align{y: 0.5} spacing: 14 padding: Inset{left: 4 right: 4 top: 10 bottom: 10} draw_bg +: {color_hover: mod.widgets.RINX_HOVER color_active: mod.widgets.RINX_SELECTED}
                    // "Aa" shows through while there is no cover.
                    RoundedView {width: 84 height: 84 flow: Overlay align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_FIELD border_radius: 4.0}
                        Label {text: "Aa" draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_bold{font_size: (20 * mod.widgets.RINX_TEXT_SCALE)}}}
                        thumbnail := Image {width: Fill height: Fill fit: ImageFit.CropToFill}
                    }
                    View {width: Fill height: Fit flow: Down spacing: 8
                        title := mod.widgets.ArticleLabel {max_lines: 2 draw_text.text_style: theme.font_bold{font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}
                        summary := mod.widgets.ArticleLabel {max_lines: 1 draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
                    }
                    Label {text: "›" draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (20 * mod.widgets.RINX_TEXT_SCALE)}}}
                }
            }
            article_import := mod.widgets.ArticleButton {draw_bg +: {color: mod.widgets.RINX_PAGE} draw_icon +: {svg: ICON_IMPORT}}
            article_share := mod.widgets.ArticleButton {draw_bg +: {color: mod.widgets.RINX_PAGE} width: Fill draw_icon +: {svg: ICON_SHARE}}
        }
        editor := View {visible: false width: Fill height: Fill flow: Right align: Align{x: 0.5}
            editor_sidebar := View {visible: false width: 230 height: Fill flow: Down padding: 18 spacing: 18
                sidebar_new := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_ADD}}
                sidebar_library := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_FILE}}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article outline")) i18n_text: "Article outline" draw_text.text_style: theme.font_bold{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}
                outline := mod.widgets.ArticleLabel {draw_text.color: mod.widgets.RINX_MUTED}
                View {width: Fill height: Fill}
                sidebar_share := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_SHARE}}
            }
            editor_paper := SolidView {width: Fill height: Fill flow: Down padding: Inset{left: 20 right: 20 top: 16 bottom: 12} spacing: 8 draw_bg.color: mod.widgets.RINX_SURFACE
                article_title := mod.widgets.ArticleInput {height: 52 empty_text: #(crate::i18n::tr("Article title")) i18n_empty_text: "Article title" draw_text.text_style: theme.font_bold{font_size: (18 * mod.widgets.RINX_TEXT_SCALE)} draw_bg +: {border_size: 0 color: #x00000000 color_empty: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000}}
                article_author := mod.widgets.ArticleInput {height: 34 empty_text: #(crate::i18n::tr("Author")) i18n_empty_text: "Author" draw_bg +: {border_size: 0 color: #x00000000 color_empty: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
                format_bar := ScrollXView {width: Fill height: 44 flow: Right spacing: 4
                    article_bold := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_bold.svg")}}
                    article_italic := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_italic.svg")}}
                    article_link := mod.widgets.ArticleTool {draw_icon +: {svg: ICON_LINK}}
                    article_h2 := mod.widgets.ArticleButton { width: 42 draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                    article_quote := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_quote.svg")}}
                    article_list := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_bullet.svg")}}
                    article_undo := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_undo.svg")}}
                    article_redo := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_redo.svg")}}
                    block_up := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_up.svg")}}
                    block_down := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: crate_resource("self://resources/icons/article_down.svg")}}
                    block_remove := mod.widgets.ArticleButton { width: 38 draw_icon +: {svg: ICON_TRASH}}
                }
                article_blocks := PortalList {width: Fill height: Fill
                    SourceBlock := View {width: Fill height: Fit flow: Down spacing: 6 padding: Inset{top: 8 bottom: 12}
                        rendered := View {width: Fill height: Fit body := mod.widgets.ArticleHtml {}}
                        source_editor := View {visible: false width: Fill height: Fit rich := mod.widgets.ArticleRich {}}
                        source_toggle := mod.widgets.ArticleButton {height: 28 draw_icon +: {svg: ICON_VIEW_SOURCE}}
                    }
                    TextBlock := View {width: Fill height: Fit flow: Right spacing: 8
                        prefix := Label {visible: false width: 16 height: Fit padding: Inset{top: 5} draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_regular{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}}
                        rich := mod.widgets.ArticleRich {}
                    }
                    Picture := View {width: Fill height: Fit flow: Down spacing: 6 padding: Inset{top: 8 bottom: 12}
                        picture := Image {width: Fill height: 210 fit: ImageFit.Smallest}
                        caption := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
                        image_settings := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_SETTINGS}}
                    }
                    Rule := View {width: Fill height: 30 align: Align{y: 0.5} SolidView {width: Fill height: 1 draw_bg.color: mod.widgets.RINX_BORDER}}
                }
                article_stats := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (10 * mod.widgets.RINX_TEXT_SCALE)}}}
                ScrollXView {width: Fill height: 44 flow: Right spacing: 6
                    article_add_text := mod.widgets.ArticleButton {draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                    article_images := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_ADD_PHOTO}}
                    article_theme := mod.widgets.ArticleButton {draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg")}}
                    article_cover := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_ADD_PHOTO}}
                    article_source := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_VIEW_SOURCE}}
                }
                article_preview := mod.widgets.ArticlePrimary {draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
            }
            editor_inspector := View {visible: false width: 260 height: Fill flow: Down padding: 18 spacing: 20
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Article style")) i18n_text: "Article style" draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}}
                inspector_theme := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg")}}
                inspector_html := mod.widgets.ArticleHtml {}
                mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Cover preview")) i18n_text: "Cover preview"}
                inspector_cover := Image {width: Fill height: 120 fit: ImageFit.Biggest}
                inspector_cover_button := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_ADD_PHOTO}}
                View {width: Fill height: Fill}
                inspector_preview := mod.widgets.ArticlePrimary {draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
            }
        }
        writer := View {visible: false width: Fill height: Fill flow: Overlay
          View {width: Fill height: Fill flow: Down
            View {width: Fill height: Fill flow: Right
                write_source_pane := SolidView {width: Fill height: Fill flow: Overlay draw_bg.color: mod.widgets.RINX_SURFACE
                    View {width: Fill height: Fill flow: Down
                        // Phone width: the title is edited here rather than in the header.
                        // A View toggles it: TextInput itself ignores visibility.
                        write_title_small_box := View {visible: false width: Fill height: Fit padding: Inset{left: 12 right: 12 top: 12}
                        write_title_small := mod.widgets.ArticleInput {height: 48 empty_text: #(crate::i18n::tr("Article title")) i18n_empty_text: "Article title" draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
                        }
                        write_toolbar := View {width: Fill height: 44 flow: Right align: Align{y: 0.5} padding: Inset{left: 10 right: 10} spacing: 0
                            wt_bold := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_bold.svg")}}
                            wt_italic := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_italic.svg")}}
                            wt_heading := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                            wt_quote := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_quote.svg")}}
                            wt_bullet := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_bullet.svg")}}
                            wt_numbered := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_numbered.svg")}}
                            wt_link := mod.widgets.ArticleTool {draw_icon +: {svg: ICON_LINK}}
                            wt_image := mod.widgets.ArticleTool {draw_icon +: {svg: ICON_ADD_PHOTO}}
                            wt_code := mod.widgets.ArticleTool {draw_icon +: {svg: ICON_VIEW_SOURCE}}
                            wt_table := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_table.svg")}}
                            wt_rule := mod.widgets.ArticleTool {draw_icon +: {svg: crate_resource("self://resources/icons/article_rule.svg")}}
                        }
                        SolidView {width: Fill height: 1 draw_bg.color: mod.widgets.RINX_BORDER}
                        write_source := mod.widgets.ArticleInput {height: Fill is_multiline: true flow: Flow.Right{wrap: true} padding: Inset{left: 16 right: 16 top: 12 bottom: 12} empty_text: #(crate::i18n::tr("Write Markdown here…")) i18n_empty_text: "Write Markdown here…" draw_text +: {text_style: theme.font_regular{font_size: (13 * mod.widgets.RINX_TEXT_SCALE) line_spacing: 1.5}} draw_bg +: {border_size: 0 color_focus: mod.widgets.RINX_SURFACE}}
                        // Phone width: formatting within thumb reach, as in the mobile atlas.
                        write_bottom := View {visible: false width: Fill height: Fit flow: Down padding: Inset{left: 12 right: 12 bottom: 8} spacing: 6
                            write_saved_small := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
                            RoundedView {width: Fill height: 44 flow: Right align: Align{x: 0.5 y: 0.5} padding: Inset{left: 6 right: 6} draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 6.0}
                                wb_bold := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_bold.svg")}}
                                wb_italic := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_italic.svg")}}
                                wb_heading := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                                wb_quote := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_quote.svg")}}
                                wb_bullet := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_bullet.svg")}}
                                wb_table := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_table.svg")}}
                                wb_image := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: ICON_ADD_PHOTO}}
                                wb_link := mod.widgets.ArticleTool {width: Fill draw_icon +: {svg: ICON_LINK}}
                            }
                        }
                    }
                    // Shown while image files are dragged over the source pane.
                    write_drop := RoundedView {visible: false width: Fill height: Fill margin: 10 flow: Down align: Align{x: 0.5 y: 0.5} spacing: 8
                        draw_bg +: {color: mod.widgets.RINX_SELECTED border_size: 2.0 border_color: mod.widgets.RINX_ACCENT border_radius: 10.0}
                        write_drop_title := Label {draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_bold{font_size: (18 * mod.widgets.RINX_TEXT_SCALE)}}}
                        Label {text: #(crate::i18n::tr("Compressed automatically · saved on this device")) i18n_text: "Compressed automatically · saved on this device" draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
                    }
                }
                write_divider := SolidView {width: 1 height: Fill draw_bg.color: mod.widgets.RINX_BORDER}
                write_preview_pane := SolidView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{top: 16 bottom: 16} draw_bg.color: mod.widgets.RINX_PAGE
                    write_paper := RoundedView {width: 420 height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 24 bottom: 8} draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 4.0}
                        write_list := PortalList {width: Fill height: Fill
                            Title := View {width: Fill height: Fit padding: Inset{bottom: 16}
                                write_preview_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (22 * mod.widgets.RINX_TEXT_SCALE)}}
                            }
                            Text := View {width: Fill height: Fit flow: Down padding: Inset{bottom: 12} body := mod.widgets.ArticleHtml {}}
                            // Consecutive images laid out as a tight grid, like WeChat/huasheng.
                            Gallery := View {width: Fill height: Fit flow: Down spacing: 3 padding: Inset{bottom: 12}
                                row0 := View {width: Fill height: 120 flow: Right spacing: 3 g0 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g1 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g2 := Image {width: Fill height: Fill fit: ImageFit.CropToFill}}
                                row1 := View {width: Fill height: 120 flow: Right spacing: 3 g3 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g4 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g5 := Image {width: Fill height: Fill fit: ImageFit.CropToFill}}
                                row2 := View {width: Fill height: 120 flow: Right spacing: 3 g6 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g7 := Image {width: Fill height: Fill fit: ImageFit.CropToFill} g8 := Image {width: Fill height: Fill fit: ImageFit.CropToFill}}
                            }
                        }
                    }
                }
                write_themes_panel := SolidView {visible: false width: 320 height: Fill flow: Down padding: 16 spacing: 10 draw_bg.color: mod.widgets.RINX_SURFACE
                    View {width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Article style")) i18n_text: "Article style" draw_text.text_style: theme.font_bold{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}
                        write_themes_close := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_CLOSE}}
                    }
                    write_theme_list := PortalList {width: Fill height: Fill
                        Swatches := View {width: Fill height: 108 flow: Right spacing: 10 padding: Inset{bottom: 12}
                            t0 := mod.widgets.ArticleThemeSwatch {}
                            t1 := mod.widgets.ArticleThemeSwatch {}
                            t2 := mod.widgets.ArticleThemeSwatch {}
                        }
                    }
                }
            }
            SolidView {width: Fill height: 1 draw_bg.color: mod.widgets.RINX_BORDER}
            write_stats := mod.widgets.ArticleLabel {padding: Inset{left: 16 top: 8 bottom: 8} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
          }
          // Table size picker (⊞), placed under the button that opened it.
          write_table_popup := View {visible: false width: Fill height: Fill flow: Down
            write_table_card := RoundedView {width: Fit height: Fit flow: Down padding: 14 spacing: 10 draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 8.0}
              mod.widgets.ArticleLabel {width: Fit text: #(crate::i18n::tr("Insert table")) i18n_text: "Insert table" draw_text.text_style: theme.font_bold{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
              write_table_picker := mod.widgets.TableSizePicker {}
            }
          }
          // Phone preview: opens the style sheet.
          write_style_fab_box := View {visible: false width: Fill height: Fill align: Align{x: 1.0 y: 1.0} padding: Inset{right: 16 bottom: 64}
            write_style_fab := mod.widgets.ArticleIconButton {draw_icon.color: mod.widgets.RINX_ACCENT draw_bg +: {border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 22.0} draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg")}}
          }
          // Phone style sheet (atlas: theme-sheet). Tapping a style previews it; 应用 keeps it, × restores the previous one.
          write_theme_sheet := SolidView {visible: false width: Fill height: Fill flow: Down align: Align{y: 1.0} draw_bg.color: #x00000055
            RoundedView {width: Fill height: Fit flow: Down padding: Inset{left: 16 right: 16 top: 8 bottom: 20} spacing: 12 draw_bg +: {color: mod.widgets.RINX_SURFACE border_radius: 12.0}
              View {width: Fill height: Fit align: Align{x: 0.5} RoundedView {width: 36 height: 4 draw_bg +: {color: mod.widgets.RINX_BORDER border_radius: 2.0}}}
              View {width: Fill height: 44 flow: Right align: Align{y: 0.5}
                mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Article style")) i18n_text: "Article style" draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
                theme_sheet_close := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_CLOSE}}
              }
              ScrollXView {width: Fill height: 108 flow: Right spacing: 10
                  s0 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s1 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s2 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s3 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s4 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s5 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s6 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s7 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s8 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s9 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s10 := mod.widgets.ArticleThemeSwatch {width: 68}
                  s11 := mod.widgets.ArticleThemeSwatch {width: 68}
              }
              theme_sheet_apply := mod.widgets.ArticlePrimary {height: 46 draw_icon +: {svg: ICON_CHECKMARK}}
            }
          }
          // Phone insert-image sheet (atlas: insert-image). Camera and clipboard are not offered: Rinx cannot capture or paste images here yet.
          write_image_sheet := SolidView {visible: false width: Fill height: Fill flow: Down align: Align{y: 1.0} draw_bg.color: #x00000055
            RoundedView {width: Fill height: Fit flow: Down padding: Inset{left: 16 right: 16 top: 8 bottom: 24} spacing: 12 draw_bg +: {color: mod.widgets.RINX_SURFACE border_radius: 12.0}
              View {width: Fill height: Fit align: Align{x: 0.5} RoundedView {width: 36 height: 4 draw_bg +: {color: mod.widgets.RINX_BORDER border_radius: 2.0}}}
              View {width: Fill height: 44 flow: Right align: Align{y: 0.5}
                mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Insert image")) i18n_text: "Insert image" draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
                image_sheet_close := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_CLOSE}}
              }
              RoundedView {width: Fill height: Fit flow: Down draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 8.0}
                image_sheet_pick := mod.widgets.ArticleIconButton {width: Fill height: 56 draw_icon +: {svg: ICON_ADD_PHOTO}}
              }
              RoundedView {width: Fill height: 56 flow: Right align: Align{y: 0.5} padding: Inset{left: 16 right: 8} draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 8.0}
                mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Lay out multiple images automatically")) i18n_text: "Lay out multiple images automatically" draw_text.text_style: theme.font_regular{font_size: (14 * mod.widgets.RINX_TEXT_SCALE)}}
                image_sheet_layout := ToggleFlat {text: "" draw_bg +: {size: 21 color_active: mod.widgets.RINX_ACCENT border_color_active: mod.widgets.RINX_ACCENT mark_color_active: mod.widgets.RINX_SURFACE mark_color_active_hover: mod.widgets.RINX_SURFACE}}
              }
              mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Images added one after another form a grid. Selected images stay on this device until you publish.")) i18n_text: "Images added one after another form a grid. Selected images stay on this device until you publish." draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
            }
          }
          // Desktop: confirms images added by dropping or choosing (atlas: paste-images).
          write_toast_box := View {visible: false width: Fill height: Fill align: Align{x: 0.5 y: 1.0} padding: Inset{bottom: 56}
            RoundedView {width: Fit height: 40 align: Align{y: 0.5} padding: Inset{left: 18 right: 18} draw_bg +: {color: mod.widgets.RINX_FIELD border_radius: 20.0}
              write_toast := Label {draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}}
            }
          }
          // Desktop publish sheet (atlas: publish-sheet): cover, summary and chat, then 发布.
          write_sheet := SolidView {visible: false width: Fill height: Fill align: Align{x: 0.5 y: 0.5} draw_bg.color: #x00000055
            RoundedView {width: 520 height: Fit flow: Down padding: Inset{left: 22 right: 22 top: 18 bottom: 18} spacing: 8 draw_bg +: {color: mod.widgets.RINX_SURFACE border_radius: 8.0}
              View {width: Fill height: 44 flow: Right align: Align{y: 0.5}
                mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Publish article")) i18n_text: "Publish article" draw_text.text_style: theme.font_bold{font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}}
                sheet_close := mod.widgets.ArticleIconButton {draw_icon +: {svg: ICON_CLOSE}}
              }
              View {width: Fill height: Fit flow: Right align: Align{y: 0.5}
                mod.widgets.ArticleLabel {width: Fill text: #(crate::i18n::tr("Cover")) i18n_text: "Cover" draw_text.text_style: theme.font_bold{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}
                sheet_cover_change := mod.widgets.ArticleTool {width: 44 padding: Inset{left: 8 right: 8} draw_icon.color: mod.widgets.RINX_ACCENT draw_icon +: {svg: ICON_ADD_PHOTO}}
              }
              sheet_cover := Image {width: Fill height: 150 fit: ImageFit.Biggest}
              sheet_no_cover := mod.widgets.ArticleLabel {text: #(crate::i18n::tr("This article will be published without a cover.")) i18n_text: "This article will be published without a cover." draw_text.color: mod.widgets.RINX_MUTED}
              mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Summary")) i18n_text: "Summary" draw_text.text_style: theme.font_bold{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}
              sheet_summary := mod.widgets.ArticleInput {height: 38 empty_text: #(crate::i18n::tr("One line about this article")) i18n_empty_text: "One line about this article"}
              mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Publish to")) i18n_text: "Publish to" draw_text.text_style: theme.font_bold{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}
              RoundedView {width: Fill height: 138 draw_bg +: {color: mod.widgets.RINX_SURFACE border_size: 1.0 border_color: mod.widgets.RINX_BORDER border_radius: 6.0}
                sheet_rooms := PortalList {width: Fill height: Fill
                  Chat := mod.widgets.ArticleChatRow {height: 44 spacing: 10 avatar +: {width: 28 height: 28 draw_bg +: {border_radius: 14.0} initial +: {draw_text +: {text_style: theme.font_bold{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}} name +: {draw_text +: {text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}} radio +: {width: 18 height: 18 draw_bg +: {border_radius: 9.0}} }
                }
              }
              View {width: Fill height: 44 flow: Right spacing: 12 margin: Inset{top: 6}
                sheet_cancel := mod.widgets.ArticleButton {width: Fill height: 44 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_PAGE} draw_icon +: {svg: ICON_CLOSE}}
                sheet_publish := mod.widgets.ArticlePrimary {width: Fill height: 44 draw_icon +: {svg: ICON_SEND}}
              }
            }
          }
        }
        source := View {visible: false width: Fill height: Fill flow: Down padding: 18 spacing: 12
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Markdown / HTML source · your theme and cover are preserved")) i18n_text: "Markdown / HTML source · your theme and cover are preserved"}
            article_markdown := mod.widgets.ArticleInput {height: Fill is_multiline: true flow: Flow.Right{wrap: true}}
            source_report := ScrollYView {visible: false width: Fill height: 160 flow: Down
                source_issues := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_WARNING_FG text_style: theme.font_regular{font_size: (12 * mod.widgets.RINX_TEXT_SCALE)}}}
            }
            source_apply := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_CHECKMARK}}
        }
        images := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 14
            image_pick := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_ADD_PHOTO}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Selected images stay on this device until you confirm publication.")) i18n_text: "Selected images stay on this device until you confirm publication." draw_text.color: mod.widgets.RINX_MUTED}
            image_library := PortalList {width: Fill height: Fill
                Asset := NavigationBarButton {width: Fill height: 130 flow: Right spacing: 14 padding: 10
                    picture := Image {width: 140 height: 108 fit: ImageFit.Smallest}
                    name := mod.widgets.ArticleLabel {width: Fill max_lines: 3}
                }
            }
        }
        link_page := View {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            article_link_url := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("HTTPS link")) i18n_empty_text: "HTTPS link"}
            link_apply := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_LINK}}
            link_remove := mod.widgets.ArticleButton {draw_icon +: {svg: crate_resource("self://resources/icons/article_unlink.svg")}}
        }
        image_settings := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12
            selected_image := Image {width: Fill height: 190 fit: ImageFit.Smallest}
            image_caption := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Caption")) i18n_empty_text: "Caption"}
            image_alt := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Alternative text")) i18n_empty_text: "Alternative text"}
            View {width: Fill height: 44 flow: Right spacing: 8
                image_full := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_width_full.svg")}}
                image_medium := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_width_medium.svg")}}
                image_small := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_width_small.svg")}}
            }
            image_replace := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_ADD_PHOTO}}
            image_remove := mod.widgets.ArticleButton {width: Fill draw_icon.color: mod.widgets.RINX_DANGER_FG draw_icon +: {svg: ICON_TRASH}}
            View {width: Fill height: Fill}
            image_done := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_CHECKMARK}}
        }
        themes := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 12
            theme_list := PortalList {width: Fill height: Fill
                Theme := View {width: Fill height: 222 flow: Right spacing: 10 padding: Inset{bottom: 12}
                    left := mod.widgets.ArticleThemeTile {}
                    right := mod.widgets.ArticleThemeTile {}
                }
            }
            View {width: Fill height: 44 flow: Right spacing: 8
                size_normal := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_type_small.svg")}}
                size_large := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_type_large.svg")}}
            }
            View {width: Fill height: 44 flow: Right spacing: 8
                spacing_comfortable := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_spacing_comfortable.svg")}}
                spacing_compact := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/article_spacing_compact.svg")}}
            }
            theme_done := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_CHECKMARK}}
        }
        cover := SolidView {visible: false width: Fill height: Fill draw_bg.color: mod.widgets.RINX_SURFACE
          ScrollYView {width: Fill height: Fill flow: Down padding: 16 spacing: 10
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Cover")) i18n_text: "Cover" draw_text.text_style: theme.font_bold{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
            View {width: Fill height: Fit flow: Overlay align: Align{x: 1.0 y: 1.0}
                cover_wide := Image {width: Fill height: 190 fit: ImageFit.Biggest}
                cover_pick := mod.widgets.ArticleButton {height: 32 margin: 8 draw_icon +: {svg: ICON_ADD_PHOTO}}
            }
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Share card")) i18n_text: "Share card" draw_text.text_style: theme.font_bold{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
            cover_square := Image {width: 100 height: 100 fit: ImageFit.Biggest}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Summary")) i18n_text: "Summary" draw_text.text_style: theme.font_bold{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}
            cover_summary := mod.widgets.ArticleInput {height: 85 is_multiline: true flow: Flow.Right{wrap: true} empty_text: #(crate::i18n::tr("Summary")) i18n_empty_text: "Summary"}
            View {width: Fill height: 12}
            cover_done := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_CHECKMARK}}
            // Crop and placement adjustments, below the atlas's primary action.
            View {width: Fill height: 16}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Cover focal point · horizontal / vertical")) i18n_text: "Cover focal point · horizontal / vertical" draw_text.color: mod.widgets.RINX_MUTED}
            cover_x := Slider {width: Fill height: 30 min: 0 max: 1000 step: 1}
            cover_y := Slider {width: Fill height: 30 min: 0 max: 1000 step: 1}
            cover_show := CheckBox {text: #(crate::i18n::tr("Show cover at the top of the article"))}
            cover_remove := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_TRASH}}
          }
        }
        preview := SolidView {visible: false width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: RINX_GUTTER spacing: 12 draw_bg.color: mod.widgets.RINX_SURFACE
            preview_title := mod.widgets.ArticleLabel {width: Fill{max: RINX_READING_WIDTH} draw_text.text_style: theme.font_bold{font_size: (18 * mod.widgets.RINX_TEXT_SCALE)}}
            preview_author := mod.widgets.ArticleLabel {width: Fill{max: RINX_READING_WIDTH} draw_text.color: mod.widgets.RINX_MUTED}
            article_reader := PortalList {selectable: true width: Fill{max: RINX_READING_WIDTH} height: Fill
                Text := View {width: Fill height: Fit flow: Down padding: Inset{bottom: 12} body := mod.widgets.ArticleHtml {}}
                PlainText := View {width: Fill height: Fit flow: Down padding: Inset{bottom: 12} body := mod.widgets.RinxReaderHtml {}}
                Image := View {width: Fill height: Fit flow: Down spacing: 6 padding: Inset{bottom: 16}
                    picture := Image {width: Fill height: 220 fit: ImageFit.Smallest}
                    caption := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
                }
            }
            preview_stats := mod.widgets.ArticleLabel {width: Fill{max: RINX_READING_WIDTH} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
            css_preview_open := mod.widgets.ArticleButton {width: Fill{max: RINX_READING_WIDTH} visible: #(cfg!(feature = "html_preview")) draw_icon +: {svg: ICON_HTML_FILE}}
            preview_check := mod.widgets.ArticlePrimary {width: Fill{max: RINX_READING_WIDTH} draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
        }
        css_preview := View {visible: false width: Fill height: Fill flow: Down padding: 20 spacing: 10
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Tap links to open them. Return to the editor to edit or select text.")) i18n_text: "Tap links to open them. Return to the editor to edit or select text."}
            css_preview_refresh := mod.widgets.ArticleButton {draw_icon +: {svg: crate_resource("self://resources/icons/article_refresh.svg")}}
            css_preview_bitmap := mod.widgets.HtmlView {}
        }
        review := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            review_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (22 * mod.widgets.RINX_TEXT_SCALE)}}
            review_checks := mod.widgets.ArticleLabel {}
            review_destination := mod.widgets.ArticleLabel {draw_text.color: mod.widgets.RINX_MUTED}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Images are uploaded only after your final confirmation.")) i18n_text: "Images are uploaded only after your final confirmation." draw_text.color: mod.widgets.RINX_MUTED}
            View {width: Fill height: 12}
            review_continue := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_CHEVRON_RIGHT}}
        }
        rooms := SolidView {visible: false width: Fill height: Fill flow: Down padding: 16 spacing: 12 draw_bg.color: mod.widgets.RINX_SURFACE
            article_chat_search := mod.widgets.ArticleInput {empty_text: #(crate::i18n::tr("Search chats")) i18n_empty_text: "Search chats"}
            article_rooms := PortalList {width: Fill height: Fill
                Chat := mod.widgets.ArticleChatRow {}
            }
            rooms_publish := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_SEND}}
        }
        confirm := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 20
            confirm_cover := Image {width: Fill height: 170 fit: ImageFit.Biggest}
            confirm_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (18 * mod.widgets.RINX_TEXT_SCALE)}}
            confirm_summary := mod.widgets.ArticleLabel {draw_text.color: mod.widgets.RINX_MUTED}
            publish_account := mod.widgets.ArticleLabel {}
            publish_room := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_bold{font_size: (15 * mod.widgets.RINX_TEXT_SCALE)}}}
            confirm_details := mod.widgets.ArticleLabel {}
            View {width: Fill height: 12}
            mod.widgets.ArticleActions {
                article_change := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_EDIT}}
                article_confirm := mod.widgets.ArticlePrimary {width: 44 height: 44 draw_icon +: {svg: ICON_CHECKMARK}}
            }
        }
        publication := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 18
            publication_state := mod.widgets.ArticleLabel {draw_text +: {color: mod.widgets.RINX_ACCENT text_style: theme.font_bold{font_size: (24 * mod.widgets.RINX_TEXT_SCALE)}}}
            publication_cover := Image {width: Fill height: 170 fit: ImageFit.Biggest}
            publication_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (18 * mod.widgets.RINX_TEXT_SCALE)}}
            publication_info := mod.widgets.ArticleLabel {}
            publication_read := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
            publication_edit := mod.widgets.ArticleButton {width: Fill draw_icon +: {svg: ICON_EDIT}}
            publication_withdraw := mod.widgets.ArticleButton {width: Fill draw_icon.color: mod.widgets.RINX_DANGER_FG draw_icon +: {svg: ICON_TRASH}}
            View {width: Fill height: 12}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your local draft is preserved.")) i18n_text: "Your local draft is preserved." draw_text.color: mod.widgets.RINX_MUTED}
            publication_library := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_FILE}}
        }
        published := View {visible: false width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: 32 spacing: 14
            View {width: Fill height: 90}
            RoundedView {width: 64 height: 64 align: Align{x: 0.5 y: 0.5} draw_bg +: {color: mod.widgets.RINX_ACCENT border_radius: 32.0}
                Label {text: "✓" draw_text +: {color: mod.widgets.RINX_SURFACE text_style: theme.font_bold{font_size: (28 * mod.widgets.RINX_TEXT_SCALE)}}}
            }
            mod.widgets.ArticleLabel {width: Fit text: #(crate::i18n::tr("Published")) i18n_text: "Published" draw_text.text_style: theme.font_bold{font_size: (22 * mod.widgets.RINX_TEXT_SCALE)}}
            published_info := mod.widgets.ArticleLabel {width: Fit draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (13 * mod.widgets.RINX_TEXT_SCALE)}}}
            View {width: Fill height: Fill}
            published_edit := mod.widgets.ArticlePrimary {draw_icon +: {svg: ICON_EDIT}}
            published_library := mod.widgets.ArticleButton {width: Fill height: 46 align: Align{x: 0.5 y: 0.5} draw_bg +: {border_size: 1.0 border_color: mod.widgets.RINX_BORDER} draw_icon +: {svg: ICON_FILE}}
        }
        withdraw := ScrollYView {visible: false width: Fill height: Fill flow: Down padding: 24 spacing: 24
            withdraw_title := mod.widgets.ArticleLabel {draw_text.text_style: theme.font_bold{font_size: (22 * mod.widgets.RINX_TEXT_SCALE)}}
            withdraw_room := mod.widgets.ArticleLabel {}
            View {width: Fill height: 12}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Withdraw this article?")) i18n_text: "Withdraw this article?" draw_text.text_style: theme.font_bold{font_size: (20 * mod.widgets.RINX_TEXT_SCALE)}}
            mod.widgets.ArticleLabel {text: #(crate::i18n::tr("Your local draft will remain. Copies already downloaded or forwarded cannot be recalled.")) i18n_text: "Your local draft will remain. Copies already downloaded or forwarded cannot be recalled."}
            View {width: Fill height: 12}
            mod.widgets.ArticleActions {
                withdraw_cancel := mod.widgets.ArticleButton {draw_icon +: {svg: ICON_CLOSE}}
                withdraw_confirm := mod.widgets.ArticlePrimary {width: 44 height: 44 draw_bg +: {color: mod.widgets.RINX_DANGER_FG color_hover: mod.widgets.RINX_DANGER_FG color_down: mod.widgets.RINX_DANGER_FG} draw_icon +: {svg: ICON_TRASH color: mod.widgets.RINX_DANGER_BG}}
            }
        }
        article_status := mod.widgets.ArticleLabel {padding: Inset{left: 18 right: 18 top: 4 bottom: 8} draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular{font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}}
    }
}

type ReaderAssets = BTreeMap<String, Vec<u8>>;
type ImageBindings = std::cell::RefCell<BTreeMap<WidgetUid, String>>;

#[derive(Script, Widget)]
pub struct ArticlePanel {
    #[rust]
    appearance: ThemeSnapshot,
    #[rust]
    restore_after_theme: bool,
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    icon_tooltip: CalloutTooltip,
    #[rust]
    icon_hint: Option<WidgetUid>,
    #[rust]
    active: bool,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    grant: Option<Grant>,
    #[rust]
    page: Page,
    #[rust]
    doc: Document,
    #[rust]
    library: Library,
    #[rust]
    library_tab: usize,
    #[rust]
    entries: Vec<String>,
    #[rust]
    active_block: usize,
    #[rust]
    history: EditHistory,
    #[rust]
    body_selection: ArticleSelection,
    #[rust]
    viewport_width: f64,
    #[rust]
    icon_labels: BTreeMap<WidgetUid, String>,
    #[rust]
    css_request: String,
    #[cfg(feature = "html_preview")]
    #[rust]
    css_session: Option<super::preview::PreviewSession>,
    #[rust]
    pending: bool,
    #[rust]
    dirty: bool,
    #[rust]
    save_timer: Timer,
    #[rust]
    rooms: Vec<RoomNameId>,
    #[rust]
    sharing: bool,
    #[rust]
    share_room: Option<RoomNameId>,
    #[rust]
    share_transaction: String,
    #[rust]
    selected_publication: Option<Publication>,
    #[rust]
    operation: Option<Operation>,
    #[rust]
    selecting_cover: bool,
    #[rust]
    replacing_image: bool,
    #[rust]
    reader_only: bool,
    #[rust]
    plain_markdown: bool,
    /// A read-only panel embedded in the shared web/document tab window.
    #[live]
    tabbed_reader: bool,
    #[rust]
    reader_assets: ReaderAssets,
    #[rust] preview_images: PreviewImages,
    #[rust] preview_image_key: String,
    #[rust] images_loading: bool,
    #[rust] native_preview: Vec<article_core::markdown_render::RenderedBlock>,
    #[rust] native_editor: Vec<article_core::markdown_render::RenderedBlock>,
    #[rust] editing_source_block: Option<String>,
    #[rust] native_preview_key: String,
    #[rust] reader_select_all: bool,
    #[rust]
    image_bindings: ImageBindings,
    #[rust]
    remote_article: Option<ArticleContent>,
    /// Use the older block editor instead of the Markdown writing view.
    #[rust] block_mode: bool,
    #[rust] write_mode: WriteMode,
    /// The document whose source is loaded into `write_source`.
    #[rust] write_loaded: Option<String>,
    #[rust] write_preview: Vec<article_core::markdown_render::RenderedBlock>,
    #[rust] write_preview_key: String,
    /// The source cursor the split preview last followed.
    #[rust] write_sync_cursor: Option<usize>,
    /// Debounces re-rendering the live preview while typing.
    #[rust] write_timer: Timer,
    #[rust] write_themes_open: bool,
    /// Phone style sheet: open, with the style to restore if it is closed with ×.
    #[rust] write_theme_sheet: Option<Theme>,
    #[rust] write_image_sheet: bool,
    /// Whether images inserted one after another stay separate rather than forming a grid.
    #[rust] write_image_grid_off: bool,
    /// Images inserted since the toast last faded.
    #[rust] write_toast_count: usize,
    #[rust] write_toast_timer: Timer,
    /// Why the writing view's source could not be applied to the article, if it could not.
    /// It is still saved as typed, but cannot be published until this is fixed.
    #[rust] write_error: Option<String>,
    #[rust] write_dragging: bool,
    /// Whether the live preview shows the title above the body: not when the source
    /// already opens with a level-1 heading, which would repeat it.
    #[rust] write_title_row: bool,
    /// Publishing from the writing view: the sheet (desktop) or the 封面与摘要 →
    /// 发布到 → 已发布 steps (phone) instead of review and confirm pages.
    #[rust] publish_flow: bool,
    /// The chat chosen to publish to, as an index into `rooms`.
    #[rust] publish_choice: Option<usize>,
}

/// The rendered image ids of a block that holds only images (at least two), such as
/// `![](a) ![](b) ![](c)` on one line; such blocks are drawn as a gallery grid.
fn gallery_images(html: &str) -> Option<Vec<String>> {
    let mut ids = Vec::new();
    let mut rest = html.trim();
    for tag in ["<p>", "</p>"] { rest = rest.trim_start_matches(tag).trim_end_matches(tag); }
    let mut rest = rest.trim();
    while let Some(after) = rest.strip_prefix("<rimage>") {
        let end = after.find("</rimage>")?;
        ids.push(after[..end].split(':').next()?.to_owned());
        rest = after[end + "</rimage>".len()..].trim_start_matches(|c: char| c.is_whitespace() || c == '\u{a0}');
        rest = rest.trim_start_matches("<br>").trim_start_matches("<br/>").trim_start();
    }
    (rest.is_empty() && ids.len() >= 2).then_some(ids)
}

/// Colours a style swatch as a tiny page in `theme`, outlined in its accent when selected.
fn style_swatch(cx: &mut Cx, swatch: &WidgetRef, theme: Theme, selected: bool) {
    let (paper, ink, accent) = theme.colors();
    let (paper, ink, accent) = (color(paper), color(ink), color(accent));
    let mut faint = ink; faint.w = 0.35;
    let border = if selected { accent } else { color(0xe0e0e0) };
    let name_ink = if selected { accent } else { color(0x333333) };
    swatch.label(cx, ids!(name)).set_text(cx, tr(theme.name()));
    let mut page = swatch.view(cx, ids!(page));
    script_apply_eval!(cx, page, {draw_bg +: {color: #(paper) border_color: #(border) border_size: #(if selected {2.0} else {1.0})}});
    let mut heading = swatch.view(cx, ids!(heading));
    script_apply_eval!(cx, heading, {draw_bg +: {color: #(ink)}});
    for id in [id!(line1), id!(line2)] {
        let mut line = swatch.view(cx, &[id]);
        script_apply_eval!(cx, line, {draw_bg +: {color: #(faint)}});
    }
    let mut bar = swatch.view(cx, ids!(accent));
    script_apply_eval!(cx, bar, {draw_bg +: {color: #(accent)}});
    let mut name = swatch.label(cx, ids!(name));
    script_apply_eval!(cx, name, {draw_text +: {color: #(name_ink)}});
}

/// The rendered preview block that shows source line `line` (1-based).
fn preview_item_for_line(blocks: &[article_core::markdown_render::RenderedBlock], line: usize) -> Option<usize> {
    if blocks.is_empty() { return None; }
    Some(blocks.partition_point(|b| b.source_line <= line).saturating_sub(1))
}

/// The text that inserts `image` after `before` as its own paragraph. With `grid`,
/// an image right after another one joins that paragraph, which renders as a grid.
fn image_insertion(before: &str, image: &str, grid: bool) -> String {
    let line = before.rsplit('\n').next().unwrap_or("").trim();
    let after_image = line.starts_with("![") && line.ends_with(')');
    let prefix = if grid && after_image {
        "\n"
    } else if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let suffix = if grid { "" } else { "\n\n" };
    format!("{prefix}{image}{suffix}")
}

/// The clipboard's image as PNG bytes, if it holds an image and no text.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn clipboard_png() -> Option<Vec<u8>> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    if clipboard.get_text().is_ok_and(|t| !t.trim().is_empty()) { return None; }
    let image = clipboard.get_image().ok()?;
    let rgba = ::image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes.into_owned())?;
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, ::image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

/// Hex digits of an image's ID shown in the writing view's source.
const SHORT_ASSET: usize = 8;

/// Rewrites every `asset:<hex>` reference in `source` with `map`.
fn map_assets(source: &str, map: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("asset:") {
        let (head, tail) = rest.split_at(at + "asset:".len());
        out.push_str(head);
        let hex = tail.len() - tail.trim_start_matches(|c: char| c.is_ascii_hexdigit()).len();
        let id = &tail[..hex];
        out.push_str(&map(id).unwrap_or_else(|| id.to_owned()));
        rest = &tail[hex..];
    }
    out.push_str(rest);
    out
}

/// Shortens full image IDs to their first digits wherever that prefix is unambiguous.
fn shorten_assets(source: &str, known: &[String]) -> String {
    map_assets(source, |id| {
        let short = id.get(..SHORT_ASSET)?;
        (id.len() == 64 && known.iter().filter(|k| k.starts_with(short)).all(|k| k == id)).then(|| short.to_owned())
    })
}

/// Expands shortened image IDs back to the one known ID they start with.
fn expand_assets(source: &str, known: &[String]) -> String {
    map_assets(source, |id| {
        if id.len() < SHORT_ASSET || id.len() >= 64 { return None; }
        let mut matches = known.iter().filter(|k| k.starts_with(id));
        let full = matches.next()?;
        matches.next().is_none().then(|| full.clone())
    })
}

/// How long ago `secs` (Unix time) was, for the article list.
fn relative_time(secs: u64) -> String {
    let age = now().saturating_sub(secs);
    match age {
        ..60 => tr("Just now").to_owned(),
        60..120 => tr("1 min ago").to_owned(),
        120..3600 => crate::i18n::format("{mins} mins ago", &[("mins", (age / 60).to_string())]),
        3600..7200 => tr("1 hour ago").to_owned(),
        7200..86400 => crate::i18n::format("{hours} hours ago", &[("hours", (age / 3600).to_string())]),
        86400..172800 => tr("Yesterday").to_owned(),
        _ => crate::i18n::format("{0} days ago", &[("0", (age / 86400).to_string())]),
    }
}

/// What the writing view shows: the Markdown source, both side by side, or the preview.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum WriteMode {
    Source,
    #[default]
    Split,
    Preview,
}
impl ArticlePanel {
    fn navigate_anchor(&mut self,cx:&mut Cx,fragment:&str) {
        self.prepare_native_preview();
        let anchor=article_core::render::decode_fragment(fragment);
        let blocks=if self.page==Page::Edit {&self.native_editor}else{&self.native_preview};
        let index=if anchor.is_empty(){Some(0)}else{blocks.iter().position(|b|b.anchors.iter().any(|id|id==&anchor))};
        if let Some(index)=index {
            let editing=self.page==Page::Edit;
            let cover=usize::from(!editing && self.doc.cover.as_ref().is_some_and(|c|c.show_in_article));
            self.portal_list(cx,if editing {ids!(article_blocks)}else{ids!(article_reader)}).set_first_id_and_scroll(index+cover,0.0);
            self.redraw(cx);
        }
    }
    /// Applies full-document Markdown/HTML source to the document, keeping its theme,
    /// cover and image settings, and saves it. The source is then the document's own.
    fn apply_source(&mut self, cx: &mut Cx, text: &str) -> Result<(), String> {
        let mut state = super::model::Draft {
            title: self.doc.title.clone(),
            markdown: String::new(),
        };
        super::model::apply_input(&mut state, "markdown_changed", text)?;
        let doc = if self.doc.is_html_source() {
            Document::from_html(&state.title, &state.markdown)
        } else {
            Document::from_markdown(&state.title, &state.markdown)
        }?;
        let mut blocks = doc.blocks;
        let mut matched_images = std::collections::HashSet::new();
        for block in &mut blocks {
            if block.kind == BlockKind::Image {
                if let Some((index, old)) = self.doc.blocks.iter().enumerate().find(|(index, old)| {
                    !matched_images.contains(index) && old.kind == BlockKind::Image && old.asset == block.asset
                }) {
                    matched_images.insert(index);
                    block.id = old.id.clone();
                    block.caption = old.caption.clone();
                    block.width = old.width;
                }
            }
        }
        // Commit the import and source removal before navigating:
        // opening the image library can reload storage immediately.
        // A failed write must leave both editing forms intact.
        let mut imported = self.doc.clone();
        imported.blocks = blocks;
        imported.reference_definitions = doc.reference_definitions;
        imported.retain_source(&state.markdown);
        imported.modified = now();
        let grant = self.grant.as_ref().ok_or("Authorization expired")?;
        storage::save_document_with_source(crate::app_data_dir(), grant, &imported, None)?;
        self.checkpoint();
        self.doc = imported;
        self.library.clear_source(&self.doc.id);
        self.dirty = false;
        self.operation = None;
        cx.stop_timer(self.save_timer);
        Ok(())
    }
    /// Fills a chat row: initial avatar, name, and the radio mark when choosing.
    fn style_chat_row(&self, cx: &mut Cx, row: &WidgetRef, name: &str, selected: bool) {
        row.label(cx, ids!(name)).set_text(cx, name);
        let initial: String = name.chars().find(|c| !c.is_whitespace()).map(|c| c.to_uppercase().collect()).unwrap_or_default();
        row.label(cx, ids!(initial)).set_text(cx, &initial);
        row.view(cx, ids!(radio)).set_visible(cx, self.publish_flow);
        let (fill, border) = if selected { (color(0x07c160), color(0x07c160)) } else { (color(0xffffff), color(0xc8c8c8)) };
        let mut radio = row.view(cx, ids!(radio));
        script_apply_eval!(cx, radio, {draw_bg +: {color: #(fill) border_color: #(border)}});
        row.label(cx, ids!(check)).set_visible(cx, selected);
    }
    /// Fills a Gallery row with up to nine images in a tight grid of square cells:
    /// two or four images use two columns, others three.
    fn draw_gallery(&self, cx: &mut Cx, row: &WidgetRef, ids: &[String]) {
        const GAP: f64 = 3.0;
        let count = ids.len().min(9);
        let columns = if matches!(count, 2 | 4) { 2 } else { 3 };
        let inner = if self.viewport_width >= 960.0 { 420.0 - 48.0 } else { self.viewport_width - 48.0 };
        let side = ((inner - GAP * (columns - 1) as f64) / columns as f64).floor().max(40.0);
        let cells = [id!(g0), id!(g1), id!(g2), id!(g3), id!(g4), id!(g5), id!(g6), id!(g7), id!(g8)];
        for (r, row_id) in [id!(row0), id!(row1), id!(row2)].into_iter().enumerate() {
            let line = row.view(cx, &[row_id]);
            line.set_visible(cx, r * columns < count);
            if let Some(mut view) = line.borrow_mut() { view.walk.height = Size::Fixed(side); }
            for c in 0..3 {
                let image = line.image(cx, &[cells[r * 3 + c]]);
                let index = r * columns + c;
                // Cells beyond the column count are hidden; so are empty trailing cells,
                // which keep their share of the row (see the widths below).
                image.set_visible(cx, c < columns);
                let Some(id) = ids.get(index).filter(|_| c < columns && index < count) else {
                    image.set_visible(cx, false);
                    continue;
                };
                if self.image_bindings.borrow().get(&image.widget_uid()).is_some_and(|key| key == id) { continue; }
                if let Some(preview) = self.preview_images.values().find(|p| &p.id == id) {
                    if image.load_image_from_data(cx, &preview.png).is_ok() {
                        self.image_bindings.borrow_mut().insert(image.widget_uid(), id.clone());
                    }
                }
            }
            // Keep columns aligned in a partly filled row: fixed cell widths.
            for c in 0..3 {
                let image = line.image(cx, &[cells[r * 3 + c]]);
                if let Some(mut inner_image) = image.borrow_mut() { inner_image.walk.width = Size::Fixed(side); }
            }
        }
    }
    /// Shows the writing view's panes for the current mode and window width,
    /// and renders the live preview of the current source.
    fn layout_writer(&mut self, cx: &mut Cx2d, wide: bool) {
        // Narrow windows (mobile) switch between editing and previewing; split needs room.
        let mode = match self.write_mode {
            WriteMode::Split if !wide => WriteMode::Source,
            mode => mode,
        };
        let source = mode != WriteMode::Preview;
        let preview = mode != WriteMode::Source;
        self.view(cx, ids!(write_source_pane)).set_visible(cx, source);
        self.view(cx, ids!(write_preview_pane)).set_visible(cx, preview);
        self.view(cx, ids!(write_divider)).set_visible(cx, source && preview);
        self.view(cx, ids!(write_themes_panel)).set_visible(cx, self.write_themes_open && wide);
        self.button(cx, ids!(write_mode_split)).set_visible(cx, wide);
        // Phone width follows the mobile atlas: back, 编辑|预览 and 发布 in the header;
        // title above the source; formatting at the bottom; no stats while editing.
        self.label(cx, ids!(article_heading)).set_visible(cx, wide);
        self.button(cx, ids!(write_style)).set_visible(cx, wide);
        self.label(cx, ids!(write_saved)).set_visible(cx, wide);
        self.view(cx, ids!(write_title_box)).set_visible(cx, wide);
        self.view(cx, ids!(header_fill)).set_visible(cx, !wide);
        self.view(cx, ids!(write_title_small_box)).set_visible(cx, !wide);
        self.view(cx, ids!(write_toolbar)).set_visible(cx, wide);
        self.view(cx, ids!(write_bottom)).set_visible(cx, !wide);
        self.label(cx, ids!(write_stats)).set_visible(cx, wide || mode == WriteMode::Preview);
        let theme_sheet = self.write_theme_sheet.is_some() && !wide;
        self.view(cx, ids!(write_theme_sheet)).set_visible(cx, theme_sheet);
        self.view(cx, ids!(write_image_sheet)).set_visible(cx, self.write_image_sheet && !wide);
        self.view(cx, ids!(write_style_fab_box)).set_visible(cx, !wide && mode == WriteMode::Preview && !theme_sheet);
        if theme_sheet {
            for (index, &theme) in Theme::ALL.iter().enumerate() {
                let swatch = self.widget(cx, &[LiveId::from_str(&format!("s{index}"))]);
                style_swatch(cx, &swatch, theme, theme == self.doc.theme);
            }
        }
        self.view(cx, ids!(write_toast_box)).set_visible(cx, self.write_toast_count > 0);
        if self.write_toast_count > 0 {
            let toast = crate::i18n::format("{0} images inserted", &[("0", self.write_toast_count.to_string())]);
            self.label(cx, ids!(write_toast)).set_text(cx, &toast);
        }
        let paper_width = if wide {
            Size::Fixed(420.0)
        } else {
            Size::fill()
        };
        let (paper, _, accent) = self.document_colors();
        let paper = color(paper);
        let mut sheet = self.view(cx, ids!(write_paper));
        script_apply_eval!(cx, sheet, {width: #(paper_width) draw_bg +: {color: #(paper)}});
        for (id, selected) in [(id!(write_mode_source), mode == WriteMode::Source), (id!(write_mode_split), mode == WriteMode::Split), (id!(write_mode_preview), mode == WriteMode::Preview)] {
            let mut button = self.button(cx, &[id]);
            let (bg, ink) = if selected { (color(0xe9f8ef), color(0x07a858)) } else { (color(0xffffff), color(0x555555)) };
            script_apply_eval!(cx, button, {draw_bg +: {color: #(bg)}});
            if let Some(mut button) = button.borrow_mut() { button.draw_icon.color = ink; }
        }
        let accent = color(accent);
        for id in [id!(write_style), id!(write_style_fab)] {
            if let Some(mut button) = self.button(cx, &[id]).borrow_mut() {
                button.draw_icon.color = accent;
            }
        }
        let saved = tr(if self.dirty { "Editing…" } else if self.write_error.is_some() { "Saved as source only" } else { "Saved" });
        let (stats_ink, stats) = match &self.write_error {
            Some(e) => (color(0xc05a00), Some(crate::i18n::format("Can't publish yet: {0}", &[("0", tr(e).to_owned())]))),
            None => (color(0x888888), None),
        };
        let mut stats_label = self.label(cx, ids!(write_stats));
        script_apply_eval!(cx, stats_label, {draw_text +: {color: #(stats_ink)}});
        if let Some(stats) = stats {
            stats_label.set_text(cx, &stats);
            stats_label.set_visible(cx, true);
        }
        self.label(cx, ids!(write_saved)).set_text(cx, saved);
        self.label(cx, ids!(write_saved_small)).set_text(cx, saved);
        if preview {
            let text = self.write_source_full(cx);
            self.prepare_write_preview(&text);
        }
    }
    /// Loads the document's source into the writing view.
    fn load_write_source(&mut self, cx: &mut Cx) {
        let source = self.library.source_for(&self.doc.id).map(str::to_owned).unwrap_or_else(|| self.doc.markdown());
        self.text_input(cx, ids!(write_source)).set_text(cx, &shorten_assets(&source, &self.known_assets()));
        self.text_input(cx, ids!(write_title)).set_text(cx, &self.doc.title);
        self.text_input(cx, ids!(write_title_small)).set_text(cx, &self.doc.title);
        self.write_loaded = Some(self.doc.id.clone());
        self.write_error = None;
        self.write_preview_key.clear();
        self.portal_list(cx, ids!(write_list)).set_first_id_and_scroll(0, 0.0);
    }
    /// Saves the writing view: applies its source into the document when it is valid,
    /// otherwise keeps it as a source draft. Returns false if nothing could be saved.
    fn flush_write(&mut self, cx: &mut Cx) -> bool {
        if !self.dirty { return true; }
        let text = self.write_source_full(cx);
        match self.apply_source(cx, &text) {
            Ok(()) => {
                self.write_error = None;
                self.status(cx, "Draft saved on this device");
                self.refresh_document(cx);
                self.ensure_preview_images();
                true
            }
            // Unsupported or invalid source stays a draft, exactly as typed.
            Err(e) => {
                self.write_error = Some(e);
                self.view.redraw(cx);
                self.save(cx)
            }
        }
    }
    /// Records an edit of the writing view's source.
    fn write_changed(&mut self, cx: &mut Cx, text: String) {
        let text = expand_assets(&text, &self.known_assets());
        self.library.source_drafts.insert(self.doc.id.clone(), text);
        self.doc.modified = now();
        self.dirty = true;
        self.operation = None;
        cx.stop_timer(self.save_timer);
        self.save_timer = cx.start_timeout(0.7);
        cx.stop_timer(self.write_timer);
        self.write_timer = cx.start_timeout(0.25);
        self.status(cx, "Unsaved changes");
    }
    /// Replaces the selection in the source with `text` (or inserts it at the cursor).
    fn insert_write_text(&mut self, cx: &mut Cx, text: &str) {
        let input = self.text_input(cx, ids!(write_source));
        let selection = input.selection();
        let range = selection.start().index..selection.end().index;
        let _ = input.replace_range(cx, range.clone(), text, UndoGroup::New);
        let end = range.start + text.len();
        input.set_selection(cx, Selection {
            anchor: Cursor { index: end, prefer_next_row: false },
            cursor: Cursor { index: end, prefer_next_row: false },
        });
        let text = input.text();
        self.write_changed(cx, text);
    }
    /// Wraps the selection in `before`/`after`, or inserts `placeholder` wrapped and selected.
    fn wrap_write_selection(&mut self, cx: &mut Cx, before: &str, after: &str, placeholder: &str) {
        let input = self.text_input(cx, ids!(write_source));
        let selection = input.selection();
        let (start, end) = (selection.start().index, selection.end().index);
        let inner = if start == end { tr(placeholder).to_string() } else { input.selected_text() };
        let _ = input.replace_range(cx, start..end, &format!("{before}{inner}{after}"), UndoGroup::New);
        let inner_start = start + before.len();
        input.set_selection(cx, Selection {
            anchor: Cursor { index: inner_start, prefer_next_row: false },
            cursor: Cursor { index: inner_start + inner.len(), prefer_next_row: false },
        });
        let text = input.text();
        self.write_changed(cx, text);
    }
    /// Adds `prefix` at the start of every line the selection touches.
    fn prefix_write_lines(&mut self, cx: &mut Cx, prefix: impl Fn(usize) -> String) {
        let input = self.text_input(cx, ids!(write_source));
        let text = input.text();
        let selection = input.selection();
        let start = text[..selection.start().index].rfind('\n').map_or(0, |i| i + 1);
        let end = text[selection.end().index..].find('\n').map_or(text.len(), |i| selection.end().index + i);
        let lines: Vec<String> = text[start..end].split('\n').enumerate().map(|(i, line)| format!("{}{line}", prefix(i))).collect();
        let replaced = lines.join("\n");
        let _ = input.replace_range(cx, start..end, &replaced, UndoGroup::New);
        let caret = Cursor { index: start + replaced.len(), prefer_next_row: false };
        input.set_selection(cx, Selection { anchor: caret, cursor: caret });
        let text = input.text();
        self.write_changed(cx, text);
    }
    /// Image IDs the writing view's source can refer to by a short prefix.
    fn known_assets(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.library.assets.keys().cloned().collect();
        ids.extend(self.doc.asset_ids());
        ids.sort();
        ids.dedup();
        ids
    }
    /// The writing view's source with full image IDs, as stored and parsed.
    fn write_source_full(&self, cx: &mut Cx) -> String {
        expand_assets(&self.text_input(cx, ids!(write_source)).text(), &self.known_assets())
    }
    fn write_image_text(&self, cx: &mut Cx, image: &str) -> String {
        let input = self.text_input(cx, ids!(write_source));
        let text = input.text();
        image_insertion(&text[..input.selection().start().index], image, !self.write_image_grid_off)
    }
    /// Keeps the part of the article being edited in view in the split preview.
    fn follow_write_cursor(&mut self, cx: &mut Cx) {
        let input = self.text_input(cx, ids!(write_source));
        let cursor = input.selection().cursor.index;
        if self.write_sync_cursor == Some(cursor) { return; }
        self.write_sync_cursor = Some(cursor);
        let text = input.text();
        let line = text[..cursor.min(text.len())].matches('\n').count() + 1;
        let Some(item) = preview_item_for_line(&self.write_preview, line) else { return };
        let item = item + usize::from(self.write_title_row);
        let list = self.portal_list(cx, ids!(write_list));
        let first = list.first_id();
        // Leave the preview alone while the block is already in view.
        if item < first || item >= first + list.visible_items().saturating_sub(1).max(1) {
            list.set_first_id_and_scroll(item, 0.0);
            self.view.redraw(cx);
        }
    }
    /// Opens the table size picker under the button that opened it, or closes it.
    fn toggle_table_popup(&mut self, cx: &mut Cx, button: LiveId) {
        let popup = self.view(cx, ids!(write_table_popup));
        if popup.visible() {
            self.close_table_popup(cx);
            return;
        }
        self.widget(cx, ids!(write_table_picker)).as_table_size_picker().reset(cx);
        // The popup overlays the whole writing view, whose top-left is the source pane's.
        let origin = self.view(cx, ids!(write_source_pane)).area().rect(cx).pos;
        let rect = self.button(cx, &[button]).area().rect(cx);
        let desktop = button == id!(wt_table);
        if let Some(mut view) = popup.borrow_mut() {
            // Desktop: drop down from the toolbar button. Phone: centred above the bottom toolbar.
            view.layout.align = if desktop { Align { x: 0.0, y: 0.0 } } else { Align { x: 0.5, y: 1.0 } };
            view.layout.padding = if desktop {
                Inset { left: (rect.pos.x - origin.x - 12.0).max(8.0), top: rect.pos.y + rect.size.y - origin.y + 4.0, right: 0.0, bottom: 0.0 }
            } else {
                Inset { left: 0.0, top: 0.0, right: 0.0, bottom: 72.0 }
            };
        }
        popup.set_visible(cx, true);
        self.view.redraw(cx);
    }
    fn close_table_popup(&mut self, cx: &mut Cx) {
        self.view(cx, ids!(write_table_popup)).set_visible(cx, false);
        self.view.redraw(cx);
    }
    fn set_icon_hint(&mut self, cx: &mut Cx, id: LiveId, label: &str) {
        let uid = self.button(cx, &[id]).widget_uid();
        self.icon_labels.insert(uid, label.to_owned());
    }
    fn hide_icon_hint(&mut self, cx: &mut Cx) {
        if self.icon_hint.take().is_some() {
            self.icon_tooltip.hide(cx);
        }
    }
    fn icon_hints(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::MouseMove(_) | Event::LongPress(_) => {}
            Event::MouseDown(_) | Event::MouseUp(_) | Event::MouseLeave(_)
            | Event::TouchUpdate(_) | Event::ClearHover | Event::Scroll(_)
            | Event::KeyDown(_) | Event::BackPressed { .. } => {
                self.hide_icon_hint(cx);
                return;
            }
            _ => return,
        }
        let mut hint = None;
        'buttons:
        for (id, label) in [
            (id!(article_back), "Back"),
            (id!(article_continue), "Continue with Rinx"),
            (id!(article_allow), "Allow and open"),
            (id!(article_cancel), "Cancel"),
            (id!(drafts_tab), "Drafts"),
            (id!(published_tab), "Published"),
            (id!(withdrawn_tab), "Withdrawn"),
            (id!(article_import), "Import Markdown or HTML file"),
            (id!(article_share), "Share app"),
            (id!(sidebar_new), "New article"),
            (id!(sidebar_library), "My articles"),
            (id!(sidebar_share), "Share app"),
            (id!(article_h2), "Heading"),
            (id!(article_quote), "Quote"),
            (id!(article_list), "Bulleted list"),
            (id!(article_undo), "Undo"),
            (id!(article_redo), "Redo"),
            (id!(block_up), "Move up"),
            (id!(block_down), "Move down"),
            (id!(block_remove), "Remove block"),
            (id!(source_toggle), "Edit source"),
            (id!(image_settings), "Image settings"),
            (id!(article_add_text), "Text"),
            (id!(article_images), "Images"),
            (id!(article_theme), "Style"),
            (id!(article_cover), "Cover"),
            (id!(article_source), "Source"),
            (id!(article_preview), "Full preview"),
            (id!(inspector_theme), "Choose theme"),
            (id!(inspector_cover_button), "Cover and summary"),
            (id!(inspector_preview), "Full preview"),
            (id!(theme_sheet_apply), "Apply"),
            (id!(sheet_cover_change), "Change"),
            (id!(sheet_cancel), "Cancel"),
            (id!(sheet_publish), "Publish"),
            (id!(source_apply), "Apply and return to visual editing"),
            (id!(image_pick), "Choose from device"),
            (id!(link_apply), "Apply link"),
            (id!(link_remove), "Remove link"),
            (id!(image_full), "Full width"),
            (id!(image_medium), "Medium width"),
            (id!(image_small), "Small width"),
            (id!(image_replace), "Replace image"),
            (id!(image_remove), "Remove image"),
            (id!(image_done), "Done"),
            (id!(size_normal), "Standard type"),
            (id!(size_large), "Large type"),
            (id!(spacing_comfortable), "Comfortable"),
            (id!(spacing_compact), "Compact"),
            (id!(theme_done), "Done"),
            (id!(cover_pick), "Choose cover image"),
            (id!(cover_done), "Done"),
            (id!(cover_remove), "Remove cover"),
            (id!(css_preview_open), "HTML/CSS preview (experimental)"),
            (id!(preview_check), "Publication review"),
            (id!(css_preview_refresh), "Refresh preview"),
            (id!(review_continue), "Continue"),
            (id!(rooms_publish), "Publish"),
            (id!(article_confirm), "Confirm publish"),
            (id!(article_change), "Back to editing"),
            (id!(publication_read), "Read full article"),
            (id!(publication_edit), "Edit article"),
            (id!(publication_withdraw), "Withdraw article"),
            (id!(publication_library), "My articles"),
            (id!(published_edit), "Continue editing"),
            (id!(published_library), "Back to article list"),
            (id!(withdraw_confirm), "Confirm withdrawal"),
            (id!(withdraw_cancel), "Cancel"),
            (id!(image_sheet_pick), "Choose from device"),
            (id!(article_save), "Save"),
            (id!(article_done), "Done"),
            (id!(article_close), "Close"),
            (id!(article_new), "New article"),
            (id!(write_style), "Article style"),
            (id!(write_mode_source), "Edit"),
            (id!(write_mode_split), "Split"),
            (id!(write_mode_preview), "Preview"),
            (id!(write_publish), "Publish"),
            (id!(write_style_fab), "Article style"),
            (id!(write_themes_close), "Close"),
            (id!(theme_sheet_close), "Close"),
            (id!(image_sheet_close), "Close"),
            (id!(sheet_close), "Close"),
            (id!(article_bold), "Bold"),
            (id!(article_italic), "Italic"),
            (id!(article_link), "Link"),
            (id!(wt_bold), "Bold"),
            (id!(wt_italic), "Italic"),
            (id!(wt_heading), "Heading"),
            (id!(wt_quote), "Quote"),
            (id!(wt_bullet), "Bulleted list"),
            (id!(wt_numbered), "Numbered list"),
            (id!(wt_link), "Link"),
            (id!(wt_image), "Image"),
            (id!(wt_code), "Code"),
            (id!(wt_table), "Insert table"),
            (id!(wt_rule), "Horizontal rule"),
            (id!(wb_bold), "Bold"),
            (id!(wb_italic), "Italic"),
            (id!(wb_heading), "Heading"),
            (id!(wb_quote), "Quote"),
            (id!(wb_bullet), "Bulleted list"),
            (id!(wb_link), "Link"),
            (id!(wb_image), "Image"),
            (id!(wb_table), "Insert table"),
        ] {
            for button in self.view.widgets(cx, &[&[id]]).iter() {
                if button.borrow::<Button>().is_none() { continue; }
                let area = button.area();
                if !button.visible() || !area.is_valid(cx) { continue; }
                // Observe the button's completed hit routing. Calling hits here
                // can claim input for controls behind a sheet or hidden page.
                let targeted = match event {
                    Event::MouseMove(e) => e.handled.get() == area && area.clipped_rect(cx).contains(e.abs),
                    Event::LongPress(e) => cx.fingers.touch_capture_area(e.uid) == Some(area),
                    _ => false,
                };
                if targeted {
                    let uid = button.widget_uid();
                    hint = Some((uid, self.icon_labels.get(&uid).cloned().unwrap_or_else(|| tr(label).to_owned()), area.rect(cx)));
                    break 'buttons;
                }
            }
        }
        if let Some((uid, text, rect)) = hint {
            if self.icon_hint != Some(uid) {
                self.icon_tooltip.show_with_options(cx, &text, rect,
                    CalloutTooltipOptions {position: TooltipPosition::Top, ..Default::default()}, false);
                self.icon_hint = Some(uid);
            }
        } else {
            self.hide_icon_hint(cx);
        }
    }
    fn set_write_mode(&mut self, cx: &mut Cx, mode: WriteMode) {
        self.write_mode = mode;
        self.view.redraw(cx);
    }
    fn handle_write_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some(text) = self.text_input(cx, ids!(write_source)).changed(actions) {
            self.write_changed(cx, text);
        }
        if let Some(title) = self.text_input(cx, ids!(write_title)).changed(actions) {
            self.text_input(cx, ids!(write_title_small)).set_text(cx, &title);
            self.doc.title = title;
            self.changed(cx);
        }
        if let Some(title) = self.text_input(cx, ids!(write_title_small)).changed(actions) {
            self.text_input(cx, ids!(write_title)).set_text(cx, &title);
            self.doc.title = title;
            self.changed(cx);
        }
        for (id, before, after, placeholder) in [
            (id!(wt_bold), "**", "**", "bold text"),
            (id!(wb_bold), "**", "**", "bold text"),
            (id!(wt_italic), "*", "*", "italic text"),
            (id!(wb_italic), "*", "*", "italic text"),
            (id!(wt_code), "`", "`", "code"),
            (id!(wt_link), "[", "](https://)", "link text"),
            (id!(wb_link), "[", "](https://)", "link text"),
        ] {
            if self.button(cx, &[id]).clicked(actions) { self.wrap_write_selection(cx, before, after, placeholder); }
        }
        if self.button(cx, ids!(wt_heading)).clicked(actions) || self.button(cx, ids!(wb_heading)).clicked(actions) { self.prefix_write_lines(cx, |_| "## ".into()); }
        if self.button(cx, ids!(wt_quote)).clicked(actions) || self.button(cx, ids!(wb_quote)).clicked(actions) { self.prefix_write_lines(cx, |_| "> ".into()); }
        if self.button(cx, ids!(wt_bullet)).clicked(actions) || self.button(cx, ids!(wb_bullet)).clicked(actions) { self.prefix_write_lines(cx, |_| "- ".into()); }
        if self.button(cx, ids!(wt_numbered)).clicked(actions) { self.prefix_write_lines(cx, |i| format!("{}. ", i + 1)); }
        if self.button(cx, ids!(wt_rule)).clicked(actions) { self.insert_write_text(cx, "\n\n---\n\n"); }
        for id in [id!(wt_table), id!(wb_table)] {
            if self.button(cx, &[id]).clicked(actions) {
                self.toggle_table_popup(cx, id);
            }
        }
        if let Some((rows, cols)) = self.widget(cx, ids!(write_table_picker)).as_table_size_picker().picked(actions) {
            self.close_table_popup(cx);
            let table = super::table_picker::markdown_table(rows, cols, |i| crate::i18n::format("Column {0}", &[("0", i.to_string())]));
            let input = self.text_input(cx, ids!(write_source));
            let text = input.text();
            let before = &text[..input.selection().start().index.min(text.len())];
            let prefix = if before.is_empty() || before.ends_with("\n\n") { "" } else if before.ends_with('\n') { "\n" } else { "\n\n" };
            self.insert_write_text(cx, &format!("{prefix}{table}\n\n"));
        }
        let pick_image = self.button(cx, ids!(wt_image)).clicked(actions)
            || self.button(cx, ids!(image_sheet_pick)).clicked(actions);
        if pick_image {
            self.write_image_sheet = false;
            self.selecting_cover = false;
            self.replacing_image = false;
            self.pick(cx);
            self.view.redraw(cx);
        }
        if self.button(cx, ids!(wb_image)).clicked(actions) {
            self.write_image_sheet = true;
            self.check_box(cx, ids!(image_sheet_layout)).set_active(cx, !self.write_image_grid_off, Animate::No);
            self.view.redraw(cx);
        }
        if self.button(cx, ids!(image_sheet_close)).clicked(actions) {
            self.write_image_sheet = false;
            self.view.redraw(cx);
        }
        if let Some(on) = self.check_box(cx, ids!(image_sheet_layout)).changed(actions) {
            self.write_image_grid_off = !on;
        }
        if self.button(cx, ids!(write_style_fab)).clicked(actions) {
            self.write_theme_sheet = Some(self.doc.theme);
            self.view.redraw(cx);
        }
        if let Some(previous) = self.write_theme_sheet {
            for (index, &theme) in Theme::ALL.iter().enumerate() {
                let id = LiveId::from_str(&format!("s{index}"));
                if self.widget(cx, &[id]).as_navigation_bar_button().clicked(actions) && theme != self.doc.theme {
                    self.doc.theme = theme;
                    self.changed(cx);
                }
            }
            if self.button(cx, ids!(theme_sheet_apply)).clicked(actions) {
                if self.doc.theme != previous {
                    // One undo step for the whole sheet visit.
                    let chosen = self.doc.theme;
                    self.doc.theme = previous;
                    self.checkpoint();
                    self.doc.theme = chosen;
                }
                self.write_theme_sheet = None;
                self.view.redraw(cx);
            }
            if self.button(cx, ids!(theme_sheet_close)).clicked(actions) {
                if self.doc.theme != previous {
                    self.doc.theme = previous;
                    self.changed(cx);
                }
                self.write_theme_sheet = None;
                self.view.redraw(cx);
            }
        }
        for (id, mode) in [(id!(write_mode_source), WriteMode::Source), (id!(write_mode_split), WriteMode::Split), (id!(write_mode_preview), WriteMode::Preview)] {
            if self.button(cx, &[id]).clicked(actions) { self.set_write_mode(cx, mode); }
        }
        if self.button(cx, ids!(write_style)).clicked(actions) {
            self.write_themes_open = !self.write_themes_open;
            self.view.redraw(cx);
        }
        if self.button(cx, ids!(write_themes_close)).clicked(actions) {
            self.write_themes_open = false;
            self.view.redraw(cx);
        }
        for (index, item) in self.portal_list(cx, ids!(write_theme_list)).items_with_actions(actions) {
            for (offset, id) in [id!(t0), id!(t1), id!(t2)].into_iter().enumerate() {
                if item.navigation_bar_button(cx, &[id]).clicked(actions) {
                    if let Some(t) = Theme::ALL.get(index * 3 + offset) {
                        self.checkpoint();
                        self.doc.theme = *t;
                        self.changed(cx);
                    }
                }
            }
        }
        if self.button(cx, ids!(write_publish)).clicked(actions) {
            self.start_publish(cx);
        }
        if self.view(cx, ids!(write_sheet)).visible() {
            if self.button(cx, ids!(sheet_close)).clicked(actions) || self.button(cx, ids!(sheet_cancel)).clicked(actions) {
                self.close_publish_sheet(cx);
            }
            if let Some(summary) = self.text_input(cx, ids!(sheet_summary)).changed(actions) {
                self.doc.summary = summary;
                self.changed(cx);
            }
            if self.button(cx, ids!(sheet_cover_change)).clicked(actions) {
                self.choose_images(cx, true, false);
            }
            for (index, item) in self.portal_list(cx, ids!(sheet_rooms)).items_with_actions(actions) {
                if item.as_navigation_bar_button().clicked(actions) {
                    self.publish_choice = Some(index);
                    self.view.redraw(cx);
                }
            }
            if self.button(cx, ids!(sheet_publish)).clicked(actions) {
                self.publish_to_choice(cx);
            }
        }
    }
    /// Accepts image files dragged onto the source pane and imports them at the cursor.
    fn handle_write_drop(&mut self, cx: &mut Cx, event: &Event) {
        fn images(items: &[DragItem]) -> Vec<String> {
            items.iter().filter_map(|item| match item {
                DragItem::FilePath { path, internal_id: None } => {
                    let lower = path.to_lowercase();
                    [".png", ".jpg", ".jpeg"].iter().any(|ext| lower.ends_with(ext)).then(|| path.clone())
                }
                _ => None,
            }).collect()
        }
        // The pane itself draws nothing, so hit-test the source input that fills it.
        let area = self.text_input(cx, ids!(write_source)).area();
        match event.drag_hits(cx, area) {
            DragHit::Drag(hit) if hit.state == DragState::Out => self.write_drop_ended(cx),
            DragHit::Drag(hit) => {
                let count = images(&hit.items).len();
                if count > 0 {
                    *hit.response.lock().unwrap() = DragResponse::Copy;
                    if !self.write_dragging {
                        self.write_dragging = true;
                        self.label(cx, ids!(write_drop_title)).set_text(cx, &tr("Release to insert {0} images").replace("{0}", &count.to_string()));
                        self.view(cx, ids!(write_drop)).set_visible(cx, true);
                        self.view.redraw(cx);
                    }
                }
            }
            DragHit::Drop(hit) => {
                self.write_drop_ended(cx);
                let paths = images(&hit.items);
                let Some(grant) = self.grant.clone() else { return };
                for path in paths {
                    let grant = grant.clone();
                    let document = self.doc.id.clone();
                    spawn_async_task(async move {
                        let path = std::path::PathBuf::from(path);
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image").to_owned();
                        let result = storage::import_image(crate::app_data_dir(), &grant, &path, &name);
                        Cx::post_action(ResultAction::Image { instance: grant.instance, document, cover: false, result });
                    });
                }
            }
            DragHit::DragEnd => self.write_drop_ended(cx),
            DragHit::NoHit => if matches!(event, Event::Drag(_)) { self.write_drop_ended(cx) },
        }
    }
    fn write_drop_ended(&mut self, cx: &mut Cx) {
        if self.write_dragging {
            self.write_dragging = false;
            self.view(cx, ids!(write_drop)).set_visible(cx, false);
            self.view.redraw(cx);
        }
    }
    /// Renders the live preview of the writing view's current source.
    fn prepare_write_preview(&mut self, source: &str) {
        let key = format!("{}:{}:{:?}:{}:{}", self.doc.id, blake3::hash(source.as_bytes()), self.doc.theme, self.doc.large_type, self.preview_images.len());
        if key == self.write_preview_key { return; }
        let mut images = (*self.preview_images).clone();
        for id in source.split("asset:").skip(1).map(|rest| rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>()) {
            let key = format!("asset:{id}");
            if id.is_empty() || images.contains_key(&key) { continue; }
            let bytes = self.reader_assets.get(&id).cloned().or_else(|| self.grant.as_ref().and_then(|g| storage::asset_bytes(crate::app_data_dir(), g, &id).ok()));
            if let Some(image) = bytes.and_then(|b| article_makepad::content::prepare_image(&b).ok()) { images.insert(key, image); }
        }
        for (url, id) in &self.doc.resource_bindings {
            if let Some(image) = images.get(&format!("asset:{id}")).cloned() { images.insert(url.clone(), image); }
        }
        self.preview_images = std::sync::Arc::new(images);
        let mut renderer = article_makepad::content::NativeRenderer {
            images: &self.preview_images,
            size: if self.doc.large_type { 16.0 } else { 14.0 },
            ink: self.document_colors().1,
        };
        self.write_preview = article_core::markdown_render::render(source, &mut renderer);
        self.write_title_row = !source.trim_start().starts_with("# ");
        self.write_preview_key = key;
    }
    fn document_colors(&self) -> (u32, u32, u32) {
        if self.plain_markdown {
            let hex = |color| crate::theme::argb(color) & 0xffffff;
            (
                hex(self.appearance.page),
                hex(self.appearance.ink),
                hex(self.appearance.accent),
            )
        } else {
            self.doc.theme.colors()
        }
    }
    fn style_reader_html(&self, cx: &mut Cx, mut html: HtmlRef) {
        article_makepad::presentation::style_html(cx, html.clone(), &self.doc);
        if self.plain_markdown {
            let s = &self.appearance;
            script_apply_eval!(cx, html, {
                font_color: #(s.ink) font_size: #(14. * s.text_scale)
                draw_text.color: #(s.ink)
                draw_block +: {table_header_bg_color: #(s.field) table_border_color: #(s.border) code_color: #(s.field) line_color: #(s.border)}
            });
        }
    }
    fn prepare_native_preview(&mut self) {
        if self.doc.is_html_source() && self.page!=Page::Edit { return; }
        let source=self.doc.markdown();
        let key=format!("{}:{}:{}:{:?}:{}:{}:{}",self.doc.id,blake3::hash(source.as_bytes()),self.preview_image_key,self.doc.theme,self.preview_images.len(),self.doc.large_type,self.page==Page::Edit);
        if key==self.native_preview_key { return; }
        let mut images=(*self.preview_images).clone();
        for id in self.doc.asset_ids() {
            let key=format!("asset:{id}");
            if !images.contains_key(&key) {
                let bytes=self.reader_assets.get(&id).cloned().or_else(||self.grant.as_ref().and_then(|g|storage::asset_bytes(crate::app_data_dir(),g,&id).ok()));
                if let Some(image)=bytes.and_then(|b|article_makepad::content::prepare_image(&b).ok()) { images.insert(key,image); }
            }
        }
        for (url,id) in &self.doc.resource_bindings {
            if let Some(image)=images.get(&format!("asset:{id}")).cloned() {images.insert(url.clone(),image);}
        }
        self.preview_images = std::sync::Arc::new(images);
        let mut renderer = article_makepad::content::NativeRenderer {
            images: &self.preview_images,
            size: (if self.doc.large_type { 16.0 } else { 14.0 })
                * if self.plain_markdown {
                    self.appearance.text_scale as f32
                } else {
                    1.0
                },
            ink: self.document_colors().1,
        };
        if self.page == Page::Edit {
            self.native_editor =
                article_core::markdown_render::render_editor(&self.doc, &mut renderer);
        } else {
            self.native_preview=article_core::markdown_render::render(&source,&mut renderer);
        }
        self.native_preview_key=key;
    }
    fn ensure_preview_images(&mut self) {
        let Some(grant) = self.grant.as_ref() else { return; };
        let urls = article_core::render::image_requests(&self.doc);
        let key = format!("{}:{}", self.doc.id, blake3::hash(urls.join("\n").as_bytes()).to_hex());
        if self.preview_image_key == key { return; }
        self.preview_image_key = key.clone();
        self.preview_images = Default::default();
        self.images_loading = !urls.is_empty();
        if urls.is_empty() { return; }
        let instance = grant.instance.clone();
        spawn_async_task(async move {
            let images = super::remote_images::load(urls).await;
            Cx::post_action(ResultAction::RemoteImages { instance, key, images });
        });
    }

    #[cfg(feature = "html_preview")]
    fn start_css_preview(&mut self, cx: &mut Cx) {
        if !self.editable() || self.pending { return; }
        self.ensure_preview_images();
        if self.images_loading {
            self.show(cx, Page::CssPreview);
            self.status(cx, "Loading article images…");
            return;
        }
        let Some(grant) = self.grant.clone() else { return; };
        let area = self.portal_list(cx, ids!(article_reader)).area();
        let options = makepad_html_renderer::RenderOptions {
            width_css: (self.viewport_width - 40.0).clamp(64.0, 2048.0) as u32,
            scale: (cx.get_dpi_factor_of(&area) as f32).clamp(1.0, 2.0),
            max_height_css: 8192,
            ..Default::default()
        };
        self.css_request = new_id();
        let request = self.css_request.clone();
        let instance = grant.instance.clone();
        let document = self.doc.clone();
        self.css_session.take();
        self.pending = true;
        self.show(cx, Page::CssPreview);
        self.html_view(cx, ids!(css_preview_bitmap)).clear(cx);
        self.html_view(cx, ids!(css_preview_bitmap)).scroll_to(cx, 0.0);
        self.status(cx, "Rendering HTML/CSS preview…");
        self.css_session = Some(super::preview::start(document, self.preview_images.clone(), grant, options, move |result| {
            Cx::post_action(ResultAction::CssPreview { instance: instance.clone(), request: request.clone(), result });
        }));
    }
    fn status(&self, cx: &mut Cx, s: &str) {
        let label = self.label(cx, ids!(article_status));
        label.set_text(cx, tr(s));
        // The writing view shows its save state in the header; keep other messages.
        let routine = s.is_empty() || matches!(s, "Draft saved on this device" | "Unsaved changes");
        label.set_visible(cx, self.page != Page::Write || !routine);
    }
    fn source_report(&self, cx: &mut Cx, source: &str) {
        use article_core::markdown::{inspect, Effect, Feature};
        let issues = if self.doc.is_html_source() { Vec::new() } else { inspect(source) };
        self.view(cx, ids!(source_report)).set_visible(cx, !issues.is_empty());
        let chinese = crate::i18n::language() == crate::i18n::Language::Chinese;
        let mut lines = vec![tr("Images, tables, highlighted code, emoji, math and supported diagrams render in the editor and preview. Use Edit source on a block to change its Markdown. Metadata stays in Source.").to_owned()];
        for issue in issues {
            let feature = match issue.feature {
                Feature::Html => "Embedded HTML",
                Feature::InlineCode => "Inline code",
                Feature::CodeBlock => "Code blocks",
                Feature::ExternalImage => "Images outside the article library",
                Feature::NonHttpsLink => "Links other than HTTPS",
                Feature::NestedList => "Nested lists",
                Feature::NestedQuote => "Nested quotes",
                Feature::HeadingLevel => "Heading levels other than H2/H3",
                Feature::OrderedListStart => "Custom list start numbers",
                Feature::Table => "Markdown tables",
                Feature::TaskList => "Task lists",
                Feature::Strikethrough => "Strikethrough",
                Feature::FrontMatter => "YAML metadata",
                Feature::Math => "Math formulas",
                Feature::TableOfContents => "Table of contents markers",
                Feature::PageBreak => "Page break markers",
                Feature::EmojiShortcode => "Emoji shortcodes",
                Feature::LinkedImage => "Linked images",
                Feature::Footnote => "Footnotes",
                Feature::Alert => "Alerts",
                Feature::DescriptionList => "Description lists",
            };
            let effect = match issue.effect {
                Effect::SourceBlock => "kept in a source block",
                Effect::Literal => "displayed as source text",
            };
            lines.push(if chinese {
                format!("第 {} 行：{}（{}）", issue.line, tr(feature), tr(effect))
            } else {
                format!("Line {}: {} ({})", issue.line, tr(feature), tr(effect))
            });
        }
        self.label(cx, ids!(source_issues)).set_text(cx, &lines.join("\n"));
    }
    fn allowed(&self) -> bool {
        self.grant
            .as_ref()
            .is_some_and(|g| g.valid(current_user_id().as_deref()))
            && !crate::logout::logout_state_machine::is_logout_in_progress()
    }
    fn editable(&self) -> bool {
        self.allowed() && !self.reader_only
    }
    fn checkpoint(&mut self) { self.history.checkpoint(&self.doc); }
    fn changed(&mut self, cx: &mut Cx) {
        self.doc.modified = now();
        self.dirty = true;
        self.operation = None;
        cx.stop_timer(self.save_timer);
        self.save_timer = cx.start_timeout(0.7);
        self.status(cx, "Unsaved changes");
        self.refresh_document(cx);
        if self.page == Page::Edit { self.ensure_preview_images(); }
    }
    fn save(&mut self, cx: &mut Cx) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(g) = &self.grant else { return false };
        match storage::save_document_with_source(crate::app_data_dir(), g, &self.doc, self.library.source_for(&self.doc.id)) {
            Ok(()) => {
                self.dirty = false;
                self.status(cx, "Draft saved on this device");
                true
            }
            Err(e) => {
                self.status(cx, &e);
                false
            }
        }
    }
    fn load_library(&mut self, cx: &mut Cx) {
        if let Some(g) = &self.grant {
            match storage::load(crate::app_data_dir(), g) {
                Ok(lib) => self.library = lib,
                Err(e) => self.status(cx, &e),
            }
        }
        self.filter_library(cx);
    }
    fn filter_library(&mut self, cx: &mut Cx) {
        let query = self
            .text_input(cx, ids!(article_search))
            .text()
            .to_lowercase();
        self.entries = if self.library_tab == 0 {
            self.library
                .documents
                .iter()
                .filter(|d| {
                    d.title.to_lowercase().contains(&query)
                        || d.summary.to_lowercase().contains(&query)
                })
                .map(|d| d.id.clone())
                .collect()
        } else {
            self.library
                .publications
                .iter()
                .filter(|p| {
                    p.withdrawn == (self.library_tab == 2)
                        && p.document.title.to_lowercase().contains(&query)
                })
                .map(|p| p.id.clone())
                .collect()
        };
        self.label(cx, ids!(library_empty))
            .set_visible(cx, self.entries.is_empty());
        for (id, tab) in [
            (id!(drafts_tab), 0),
            (id!(published_tab), 1),
            (id!(withdrawn_tab), 2),
        ] {
            let button = self.button(cx, &[id]);
            let ink = if tab == self.library_tab {
                self.appearance.accent
            } else {
                self.appearance.muted
            };
            if let Some(mut button) = button.borrow_mut() { button.draw_icon.color = ink; }
        }
        self.portal_list(cx, ids!(article_library))
            .set_first_id_and_scroll(0, 0.0);
        self.view.redraw(cx);
    }
    fn show(&mut self, cx: &mut Cx, page: Page) {
        self.hide_icon_hint(cx);
        #[cfg(feature = "html_preview")]
        if self.page == Page::CssPreview && page != Page::CssPreview {
            self.css_session.take();
            self.css_request.clear();
            self.pending = false;
            self.html_view(cx, ids!(css_preview_bitmap)).clear(cx);
        }
        let page = if page == Page::Edit && !self.block_mode { Page::Write } else { page };
        if page == Page::Write && (self.page != Page::Write || self.write_loaded.as_deref() != Some(self.doc.id.as_str())) {
            self.load_write_source(cx);
        }
        self.page = page;
        self.view(cx, ids!(header)).set_visible(cx, !self.tabbed_reader);
        self.native_preview_key.clear();
        self.reader_select_all=false;
        if matches!(page, Page::Edit | Page::Write | Page::Preview | Page::Reader) { self.ensure_preview_images(); }
        if page != Page::Write {
            self.write_theme_sheet = None;
            self.write_image_sheet = false;
        }
        self.restore_page_ui(cx);
        self.status(cx, "");
        self.refresh_document(cx);
        self.view.redraw(cx);
    }
    /// Restore runtime visibility and labels after a stylesheet reapply without
    /// navigating, reloading source text, clearing selection, or issuing work.
    fn restore_page_ui(&mut self, cx: &mut Cx) {
        let page = self.page;
        self.view(cx, ids!(header)).set_visible(cx, !self.tabbed_reader);
        let writing = page == Page::Write;
        let publishing = self.publish_flow && matches!(page, Page::Cover | Page::Rooms | Page::Published);
        self.view(cx, ids!(write_controls)).set_visible(cx, writing);
        self.view(cx, ids!(write_title_box)).set_visible(cx, writing);
        self.view(cx, ids!(header_fill)).set_visible(cx, !writing);
        self.button(cx, ids!(article_close)).set_visible(cx, !writing && !publishing && page != Page::Published);
        self.button(cx, ids!(article_new)).set_visible(cx, page == Page::Library);
        // The writing view hides the heading at phone width; other pages always show it.
        self.label(cx, ids!(article_heading)).set_visible(cx, true);
        let header_bg = if writing { self.appearance.surface } else { self.appearance.page };
        let mut header = self.view(cx, ids!(header));
        script_apply_eval!(cx, header, {draw_bg +: {color: #(header_bg)}});
        self.button(cx, ids!(article_done)).set_visible(
            cx,
            matches!(page, Page::Theme | Page::Cover | Page::ImageSettings) && !publishing,
        );
        for (id, p) in [
            (id!(details), Page::Details),
            (id!(consent), Page::Consent),
            (id!(library), Page::Library),
            (id!(editor), Page::Edit),
            (id!(writer), Page::Write),
            (id!(source), Page::Source),
            (id!(images), Page::Images),
            (id!(image_settings), Page::ImageSettings),
            (id!(link_page), Page::Link),
            (id!(themes), Page::Theme),
            (id!(cover), Page::Cover),
            (id!(preview), Page::Preview),
            (id!(css_preview), Page::CssPreview),
            (id!(review), Page::Review),
            (id!(rooms), Page::Rooms),
            (id!(confirm), Page::Confirm),
            (id!(publication), Page::Publication),
            (id!(published), Page::Published),
            (id!(withdraw), Page::Withdraw),
        ] {
            self.view(cx, &[id]).set_visible(
                cx,
                p == page || (p == Page::Preview && page == Page::Reader),
            );
        }
        let heading = match page {
            Page::Details => "App details",
            Page::Consent => "Authorize app",
            Page::Library => "My articles",
            Page::Edit => "Edit article",
            Page::Write => "Article editor",
            Page::Source => "Markdown / HTML source",
            Page::Images => "Insert images",
            Page::ImageSettings => "Image settings",
            Page::Link => "Link",
            Page::Theme => "Article style",
            Page::Cover => "Cover and summary",
            Page::Preview => "Full preview",
            Page::CssPreview => "HTML/CSS preview (experimental)",
            Page::Review => "Publication review",
            Page::Rooms => if self.publish_flow { "Publish to" } else { "Choose chat" },
            Page::Confirm => {
                if self.sharing {
                    "Confirm share"
                } else if self
                    .operation
                    .as_ref()
                    .is_some_and(|o| o.kind == OperationKind::Update)
                {
                    "Confirm update"
                } else {
                    "Confirm publish"
                }
            }
            Page::Publication => "Publication record",
            Page::Published => "Article editor",
            Page::Withdraw => "Withdraw article",
            Page::Reader => "Read full article",
        };
        self.label(cx, ids!(article_heading))
            .set_text(cx, tr(heading));
        self.button(cx, ids!(article_save))
            .set_visible(cx, page == Page::Edit);
        self.button(cx, ids!(article_to_writing))
            .set_visible(cx, page == Page::Edit);
        self.button(cx, ids!(preview_check))
            .set_visible(cx, page == Page::Preview && !self.reader_only);
        self.button(cx, ids!(css_preview_open)).set_visible(cx, cfg!(feature = "html_preview") && self.doc.is_html_source() && page == Page::Preview && !self.reader_only);
        self.apply_document_paint(cx);
    }
    fn refresh_document(&mut self, cx: &mut Cx) {
        let (chars, minutes, images) = self.doc.stats();
        let stats = format!(
            "{} · {} {} · {} {}",
            chars,
            minutes,
            tr("min read"),
            images,
            tr("images")
        );
        self.label(cx, ids!(article_stats)).set_text(cx, &stats);
        self.label(cx, ids!(preview_stats)).set_text(cx, &stats);
        let grouped = chars.to_string().as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",");
        self.label(cx, ids!(write_stats)).set_text(cx, &crate::i18n::format("{0} chars · about {1} min · {2} images", &[("0", grouped), ("1", minutes.to_string()), ("2", images.to_string())]));
        self.label(cx, ids!(preview_title))
            .set_text(cx, &self.doc.title);
        self.label(cx, ids!(preview_author))
            .set_text(cx, &self.doc.author);
        let outline = self
            .doc
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading2 | BlockKind::Heading3))
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        self.label(cx, ids!(outline)).set_text(cx, &outline);
        self.html(cx, ids!(inspector_html)).set_text(
            cx,
            &format!(
                "<h2>{}</h2><p>{}</p><blockquote>{}</blockquote>",
                escape(tr(self.doc.theme.name())),
                escape(tr("Your article, your style.")),
                escape(tr("Only style changes; content stays the same."))
            ),
        );
        self.apply_document_paint(cx);
        self.load_cover(cx, ids!(inspector_cover), false);
        self.view.redraw(cx);
    }
    // Authored article paper and ink are content, independent of app chrome.
    // Reapply them without loading a cover or changing the document/history.
    fn apply_document_paint(&mut self, cx: &mut Cx) {
        let (paper, ink, _) = self.document_colors();
        let paper = color(paper);
        let ink = color(ink);
        let mut editor = self.view(cx, ids!(editor_paper));
        script_apply_eval!(cx,editor,{draw_bg +: {color: #(paper)}});
        let mut preview = self.view(cx, ids!(preview));
        script_apply_eval!(cx,preview,{draw_bg +: {color: #(paper)}});
        for id in [id!(preview_title), id!(preview_author)] {
            let mut label = self.label(cx, &[id]);
            script_apply_eval!(cx,label,{draw_text +: {color: #(ink)}});
        }
    }
    fn bind(&mut self, cx: &mut Cx) {
        self.body_selection.reset();
        self.editing_source_block = None;
        let state = super::model::Draft {
            title: self.doc.title.clone(),
            markdown: self.library.source_for(&self.doc.id).map(str::to_owned).unwrap_or_else(|| self.doc.markdown()),
        };
        match super::model::realize_editor(
            &state,
            crate::i18n::language() == crate::i18n::Language::Chinese,
        ) {
            Ok(fields) => {
                for (field, id) in fields
                    .iter()
                    .zip([id!(article_title), id!(article_markdown)])
                {
                    let input = self.text_input(cx, &[id]);
                    input.set_text(cx, &field.text);
                    input.set_empty_text(cx, field.placeholder.clone());
                }
            }
            Err(error) => self.status(cx, &error),
        }
        self.text_input(cx, ids!(article_author))
            .set_text(cx, &self.doc.author);
        self.active_block = 0;
        self.portal_list(cx, ids!(article_blocks))
            .set_first_id_and_scroll(0, 0.0);
        self.refresh_document(cx);
    }
    fn new_document(&mut self, cx: &mut Cx) {
        if self.dirty && !self.save(cx) {
            return;
        }
        self.doc = Document::default();
        self.selected_publication = None;
        self.operation = None;
        self.history.clear();
        self.bind(cx);
        self.save(cx);
        self.show(cx, Page::Edit);
    }
    fn load_asset(&self, cx: &mut Cx, widget: ImageRef, id: Option<&str>) {
        widget.set_visible(cx, id.is_some());
        let Some(id) = id else { return };
        if self
            .image_bindings
            .borrow()
            .get(&widget.widget_uid())
            .is_some_and(|key| key == id)
        {
            return;
        }
        let result = if let Some(bytes) = self.reader_assets.get(id) {
            widget.load_image_from_data(cx, bytes)
        } else if let Some(path) = self
            .grant
            .as_ref()
            .and_then(|g| storage::asset_bytes(crate::app_data_dir(), g, id).ok())
        {
            widget.load_image_from_data(cx, &path)
        } else {
            return;
        };
        if result.is_ok() {
            self.image_bindings
                .borrow_mut()
                .insert(widget.widget_uid(), id.into());
        }
    }
    fn cover_image(&self, cx: &mut Cx, widget: ImageRef, cover: &Cover, square: bool) {
        widget.set_visible(cx, true);
        let key = format!(
            "{}:{}:{}:{}",
            cover.asset, cover.focal_x, cover.focal_y, square
        );
        if self.image_bindings.borrow().get(&widget.widget_uid()) == Some(&key) {
            return;
        }
        let bytes = self.reader_assets.get(&cover.asset).cloned().or_else(|| {
            self.grant
                .as_ref()
                .and_then(|g| storage::asset_bytes(crate::app_data_dir(), g, &cover.asset).ok())
        });
        if let Some(bytes) = bytes.and_then(|bytes| crop_cover(&bytes, cover, square).ok()) {
            if widget.load_image_from_data(cx, &bytes).is_ok() {
                self.image_bindings
                    .borrow_mut()
                    .insert(widget.widget_uid(), key);
            }
        }
    }
    fn load_cover(&self, cx: &mut Cx, path: &[LiveId], square: bool) {
        let widget = self.image(cx, path);
        if let Some(cover) = &self.doc.cover {
            self.cover_image(cx, widget, cover, square);
        } else {
            widget.set_visible(cx, false);
        }
    }
    fn open_cover(&mut self, cx: &mut Cx) {
        self.text_input(cx, ids!(cover_summary))
            .set_text(cx, &self.doc.summary);
        if let Some(c) = &self.doc.cover {
            self.slider(cx, ids!(cover_x))
                .set_value(cx, c.focal_x as f64);
            self.slider(cx, ids!(cover_y))
                .set_value(cx, c.focal_y as f64);
            self.check_box(cx, ids!(cover_show))
                .set_active(cx, c.show_in_article, Animate::No);
        }
        self.show(cx, Page::Cover);
        self.load_cover(cx, ids!(cover_wide), false);
        self.load_cover(cx, ids!(cover_square), true);
        self.set_icon_hint(cx, id!(cover_pick), tr(if self.doc.cover.is_some() { "Change cover" } else { "Choose cover image" }));
        self.set_icon_hint(cx, id!(cover_done), tr(if self.publish_flow { "Next step" } else { "Done" }));
    }
    fn choose_images(&mut self, cx: &mut Cx, cover: bool, replace: bool) {
        self.selecting_cover = cover;
        self.replacing_image = replace;
        self.load_library(cx);
        self.show(cx, Page::Images);
    }
    fn use_image(&mut self, cx: &mut Cx, asset: storage::Asset) {
        self.checkpoint();
        if self.selecting_cover {
            self.doc.cover = Some(Cover {
                asset: asset.id,
                focal_x: 500,
                focal_y: 500,
                show_in_article: true,
            });
            self.changed(cx);
            if self.publish_flow && self.viewport_width >= 960.0 {
                self.show(cx, Page::Edit);
                self.open_publish_sheet(cx);
            } else {
                self.open_cover(cx);
            }
        } else if !self.block_mode && !self.replacing_image {
            let alt = asset.name.replace(['[', ']'], "");
            let image = shorten_assets(&format!("![{alt}](asset:{})", asset.id), &self.known_assets());
            let text = self.write_image_text(cx, &image);
            self.insert_write_text(cx, &text);
            self.write_toast_count += 1;
            cx.stop_timer(self.write_toast_timer);
            self.write_toast_timer = cx.start_timeout(2.5);
            self.show(cx, Page::Edit);
        } else {
            if self.replacing_image {
                if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                    b.asset = Some(asset.id);
                }
            } else {
                let mut block = Block::new(BlockKind::Image, "");
                block.asset = Some(asset.id);
                block.alt = asset.name;
                let index = (self.active_block + 1).min(self.doc.blocks.len());
                self.doc.blocks.insert(index, block);
                self.active_block = index;
            }
            self.changed(cx);
            self.show(cx, Page::Edit);
        }
    }
    fn pick(&mut self, cx: &mut Cx) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if !self.selecting_cover && !self.replacing_image && self.page == Page::Write {
            self.pick_many();
            return;
        }
        let Some(grant) = self.grant.clone() else {
            return;
        };
        let document = self.doc.id.clone();
        let cover = self.selecting_cover;
        let result = robius_file_picker::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_image(move |result| {
                let result = match result {
                    Ok(None) => return,
                    Ok(Some(file)) => {
                        file.into_local_file()
                            .map_err(|e| e.to_string())
                            .and_then(|file| {
                                storage::import_image(
                                    crate::app_data_dir(),
                                    &grant,
                                    file.path(),
                                    file.display_name().unwrap_or("image"),
                                )
                            })
                    }
                    Err(e) => Err(e.to_string()),
                };
                Cx::post_action(ResultAction::Image {
                    instance: grant.instance,
                    document,
                    cover,
                    result,
                });
            });
        if let Err(e) = result {
            self.status(cx, &e.to_string())
        }
    }
    /// Imports an image from the clipboard into the writing view, when the clipboard
    /// holds an image and no text (desktop only).
    fn paste_image(&mut self, _cx: &mut Cx) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let Some(grant) = self.grant.clone() else { return };
            let document = self.doc.id.clone();
            std::thread::spawn(move || {
                let Some(png) = clipboard_png() else { return };
                let result = storage::import_image_bytes(crate::app_data_dir(), &grant, &png, "pasted image.png");
                Cx::post_action(ResultAction::Image { instance: grant.instance, document, cover: false, result });
            });
        }
    }
    /// Picks one or more images to insert into the writing view (desktop).
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn pick_many(&mut self) {
        let Some(grant) = self.grant.clone() else { return };
        let document = self.doc.id.clone();
        let dialog = move || {
            let files = rfd::FileDialog::new()
                .add_filter(tr("Images"), &["png", "jpg", "jpeg"])
                .pick_files()
                .unwrap_or_default();
            // Import in order on one thread, so the images are inserted in the order picked.
            std::thread::spawn(move || {
                for path in files {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image").to_owned();
                    let result = storage::import_image(crate::app_data_dir(), &grant, &path, &name);
                    Cx::post_action(ResultAction::Image { instance: grant.instance.clone(), document: document.clone(), cover: false, result });
                }
            });
        };
        // As robius-file-picker does: run the modal dialog from the main queue, outside this event.
        #[cfg(target_os = "macos")]
        dispatch2::DispatchQueue::main().exec_async(dialog);
        #[cfg(not(target_os = "macos"))]
        std::thread::spawn(dialog);
    }
    fn import_file(&mut self, cx: &mut Cx) {
        if self.dirty && !self.save(cx) { return; }
        let Some(grant) = self.grant.clone().filter(|_| self.editable()) else { return };
        self.pending = true;
        let result = robius_file_picker::FileDialog::new()
            .add_filter("Markdown and HTML", &["md", "markdown", "html", "htm", "txt"])
            .pick_file(move |picked| {
                let result = match picked {
                    Ok(None) => Ok(None),
                    Ok(Some(file)) => file.into_local_file().map_err(|e| e.to_string()).and_then(|file| {
                        let name = file.display_name().or_else(|| file.path().file_name().and_then(|s| s.to_str())).unwrap_or("article.md");
                        storage::read_import(&grant, file.path(), name).map(Some)
                    }),
                    Err(error) => Err(error.to_string()),
                };
                Cx::post_action(ResultAction::Imported { instance: grant.instance, result });
            });
        if let Err(error) = result {
            self.pending = false;
            self.status(cx, &error.to_string());
        }
    }
    fn preview(&mut self, cx: &mut Cx) {
        if let Err(e) = self.doc.ready() {
            self.status(cx, &e);
            return;
        }
        if !self.save(cx) {
            return;
        }
        self.portal_list(cx, ids!(article_reader))
            .set_first_id_and_scroll(0, 0.0);
        self.show(cx, Page::Preview);
    }
    /// Checks the article can be published: title and body ready, image files present.
    fn publish_ready(&self) -> Result<(), String> {
        self.doc.ready()?;
        let g = self.grant.as_ref().ok_or("Authorization expired")?;
        for id in self.doc.asset_ids() {
            storage::asset_bytes(crate::app_data_dir(), g, &id)?;
        }
        Ok(())
    }
    /// Starts publishing from the writing view: the publish sheet on desktop, or the
    /// 封面与摘要 step on phones (followed by 发布到 and 已发布).
    fn start_publish(&mut self, cx: &mut Cx) {
        // Always re-apply, so a source that was saved but could not be applied is caught.
        self.dirty = true;
        if !self.flush_write(cx) || self.write_error.is_some() { return; }
        if let Err(e) = self.publish_ready() {
            self.status(cx, &e);
            return;
        }
        self.publish_flow = true;
        self.sharing = false;
        self.operation = None;
        self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
        // Default to the chat this article was last published to.
        let previous = self.selected_publication.as_ref().filter(|p| !p.withdrawn).map(|p| p.room.clone());
        self.publish_choice = previous.and_then(|room| self.rooms.iter().position(|r| r.room_id() == &room));
        if self.viewport_width >= 960.0 {
            self.open_publish_sheet(cx);
        } else {
            self.open_cover(cx);
        }
    }
    fn open_publish_sheet(&mut self, cx: &mut Cx) {
        self.text_input(cx, ids!(sheet_summary)).set_text(cx, &self.doc.summary);
        self.load_cover(cx, ids!(sheet_cover), false);
        self.label(cx, ids!(sheet_no_cover)).set_visible(cx, self.doc.cover.is_none());
        self.view(cx, ids!(write_sheet)).set_visible(cx, true);
        self.view.redraw(cx);
    }
    fn close_publish_sheet(&mut self, cx: &mut Cx) {
        self.view(cx, ids!(write_sheet)).set_visible(cx, false);
        self.publish_flow = false;
        self.view.redraw(cx);
    }
    /// Publishes to the chosen chat with the same operation and upload as the
    /// review/confirm path; choosing the chat and pressing 发布 is the confirmation.
    fn publish_to_choice(&mut self, cx: &mut Cx) {
        if self.pending { return; }
        let Some(room) = self.publish_choice.and_then(|i| self.rooms.get(i)).cloned() else {
            self.status(cx, "Choose a chat to publish to.");
            return;
        };
        self.prepare_operation(room.room_id().to_owned(), room.display().into_owned());
        self.send(cx);
    }
    fn show_published(&mut self, cx: &mut Cx, p: &Publication) {
        self.selected_publication = Some(p.clone());
        self.publish_flow = false;
        self.view(cx, ids!(write_sheet)).set_visible(cx, false);
        self.show(cx, Page::Published);
        self.label(cx, ids!(published_info)).set_text(cx, &tr("Sent to {0}").replace("{0}", &p.room_name));
    }
    fn review(&mut self, cx: &mut Cx) {
        let result = self.publish_ready();
        if let Err(e) = result {
            self.status(cx, &e);
            return;
        }
        self.show(cx, Page::Review);
        self.label(cx, ids!(review_title))
            .set_text(cx, &self.doc.title);
        self.label(cx, ids!(review_checks)).set_text(
            cx,
            &format!(
                "✓ {}\n\n✓ {}\n\n{}",
                tr("Title and body are ready"),
                tr("Image files are available"),
                if self.doc.cover.is_some() {
                    tr("Cover and summary reviewed")
                } else {
                    tr("This article will be published without a cover.")
                }
            ),
        );
        self.label(cx, ids!(review_destination)).set_text(
            cx,
            &self
                .selected_publication
                .as_ref()
                .filter(|p| !p.withdrawn)
                .map(|p| p.room_name.clone())
                .unwrap_or_else(|| tr("Choose a chat in the next step.").into()),
        );
    }
    fn choose_room(&mut self, cx: &mut Cx, sharing: bool) {
        self.sharing = sharing;
        self.operation = None;
        self.share_room = None;
        self.share_transaction = new_id();
        self.rooms = cx.get_global::<RoomsListRef>().mini_app_share_rooms();
        self.text_input(cx, ids!(article_chat_search))
            .set_text(cx, "");
        self.show(cx, Page::Rooms);
    }
    fn prepare(&mut self, cx: &mut Cx, room: OwnedRoomId, name: String) {
        self.prepare_operation(room, name);
        self.confirm(cx);
    }
    fn prepare_operation(&mut self, room: OwnedRoomId, name: String) {
        let p = self
            .selected_publication
            .as_ref()
            .filter(|p| !p.withdrawn && p.room == room);
        self.operation = Some(Operation {
            id: new_id(),
            kind: if p.is_some() {
                OperationKind::Update
            } else {
                OperationKind::Publish
            },
            document: self.doc.clone(),
            room,
            room_name: name,
            publication: p.map(|p| p.id.clone()),
            root: p.map(|p| p.root.clone()),
            version: p.map(|p| p.version + 1).unwrap_or(1),
            uploaded: BTreeMap::new(),
            redacted: vec![],
            confirmed: None,
            finished: false,
        });
    }
    fn confirm(&mut self, cx: &mut Cx) {
        self.show(cx, Page::Confirm);
        let saved = self.operation.as_ref().is_some_and(|op| {
            self.library
                .outbox
                .iter()
                .any(|saved| saved.id == op.id && !saved.finished)
        });
        self.set_icon_hint(cx, id!(article_change),
            tr(if saved {
                "Keep editing a copy"
            } else {
                "Back to editing"
            }),
        );
        let label = if self.sharing {
            "Confirm share"
        } else if self
            .operation
            .as_ref()
            .is_some_and(|o| o.kind == OperationKind::Update)
        {
            "Confirm update"
        } else {
            "Confirm publish"
        };
        self.set_icon_hint(cx, id!(article_confirm), tr(label));
        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
        self.label(cx, ids!(confirm_title)).set_text(
            cx,
            if self.sharing {
                tr("Article studio")
            } else {
                &self.doc.title
            },
        );
        self.label(cx, ids!(confirm_summary)).set_text(
            cx,
            if self.sharing {
                tr("Sharing this app shares neither your drafts nor your account permissions.")
            } else {
                &self.doc.summary
            },
        );
        self.label(cx, ids!(publish_account))
            .set_text(cx, self.owner.as_ref().map(|u| u.as_str()).unwrap_or(""));
        let room = if self.sharing {
            self.share_room.as_ref().map(|r| r.display().into_owned())
        } else {
            self.operation.as_ref().map(|o| o.room_name.clone())
        };
        self.label(cx, ids!(publish_room))
            .set_text(cx, &room.unwrap_or_default());
        self.label(cx, ids!(confirm_details)).set_text(
            cx,
            &self
                .operation
                .as_ref()
                .map(|o| {
                    format!(
                        "{} {} · {} {}",
                        tr("Version"),
                        o.version,
                        o.document.asset_ids().len(),
                        tr("images")
                    )
                })
                .unwrap_or_else(|| {
                    tr("Each recipient opens this app with their own account.").into()
                }),
        );
        if self.sharing {
            self.image(cx, ids!(confirm_cover)).set_visible(cx, false);
        } else {
            self.load_cover(cx, ids!(confirm_cover), false);
        }
    }
    fn send(&mut self, cx: &mut Cx) {
        if self.pending || !self.editable() {
            return;
        }
        let (Some(client), Some(grant)) = (get_client(), self.grant.clone()) else {
            return;
        };
        self.pending = true;
        self.button(cx, ids!(article_confirm))
            .set_enabled(cx, false);
        self.status(cx, "Uploading and publishing…");
        if self.sharing {
            let Some(room) = self.share_room.clone() else {
                self.pending = false;
                return;
            };
            let transaction = self.share_transaction.clone();
            spawn_async_task(async move {
                let result = async {
                    if !grant.valid(client.user_id()) {
                        return Err("Authorization expired".into());
                    }
                    let room = backend::writable(&client, &grant, room.room_id()).await?;
                    if room.state() != matrix_sdk::RoomState::Joined {
                        return Err("This chat is no longer joined.".into());
                    }
                    room.send(ArticlePackage::builtin().message())
                        .with_transaction_id(ruma::OwnedTransactionId::from(transaction))
                        .with_request_config(
                            matrix_sdk::config::RequestConfig::new().retry_limit(0),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(())
                }
                .await;
                Cx::post_action(ResultAction::Shared {
                    instance: grant.instance,
                    result,
                });
            });
        } else if let Some(op) = self.operation.clone() {
            spawn_async_task(async move {
                let instance = grant.instance.clone();
                use article_core::host::ArticlePublisher;
                let result = super::host::RobrixPublisher.publish(grant.lease, op).await;
                Cx::post_action(ResultAction::Published { instance, result });
            });
        } else {
            self.pending = false;
        }
    }
    fn publication(&mut self, cx: &mut Cx, p: Publication) {
        self.doc = p.document.clone();
        self.selected_publication = Some(p.clone());
        self.show(cx, Page::Publication);
        self.label(cx, ids!(publication_state)).set_text(
            cx,
            tr(if p.withdrawn {
                "Withdrawn"
            } else {
                "Published"
            }),
        );
        self.label(cx, ids!(publication_title))
            .set_text(cx, &p.document.title);
        self.label(cx, ids!(publication_info)).set_text(
            cx,
            &format!("{}\n\n{} {}", p.room_name, tr("Version"), p.version),
        );
        self.button(cx, ids!(publication_withdraw))
            .set_visible(cx, !p.withdrawn);
        self.load_cover(cx, ids!(publication_cover), false);
    }
    fn back(&mut self, cx: &mut Cx) {
        if self.pending && self.page != Page::CssPreview {
            return;
        }
        match self.page {
            Page::Details => cx.action(ArticleAction::Close),
            Page::Consent => self.show(cx, Page::Details),
            Page::Library | Page::Reader => cx.action(ArticleAction::Close),
            Page::Edit => {
                if self.save(cx) {
                    self.load_library(cx);
                    self.show(cx, Page::Library)
                }
            }
            Page::Write => {
                if self.flush_write(cx) {
                    self.load_library(cx);
                    self.show(cx, Page::Library)
                }
            }
            Page::Source => {
                if self.save(cx) {
                    self.show(cx, Page::Edit);
                }
            },
            Page::CssPreview => {
                self.pending = false;
                self.css_request.clear();
                #[cfg(feature = "html_preview")]
                self.html_view(cx, ids!(css_preview_bitmap)).clear(cx);
                self.show(cx, Page::Preview);
            },
            Page::Confirm => {
                self.load_library(cx);
                self.show(cx, Page::Library);
            }
            Page::Publication | Page::Published => {
                self.load_library(cx);
                self.show(cx, Page::Library)
            }
            Page::Cover if self.publish_flow => {
                self.publish_flow = false;
                self.show(cx, Page::Edit);
            }
            Page::Rooms if self.publish_flow => self.open_cover(cx),
            Page::Withdraw => {
                if let Some(p) = self.selected_publication.clone() {
                    self.publication(cx, p)
                }
            }
            _ => self.show(cx, Page::Edit),
        }
    }
}

impl ScriptHook for ArticlePanel {
    fn on_after_apply(
        &mut self,
        vm: &mut ScriptVm,
        _apply: &Apply,
        _scope: &mut Scope,
        _value: ScriptValue,
    ) {
        let appearance = crate::theme::snapshot_for_vm(vm);
        self.restore_after_theme |= self.active && self.appearance.revision != appearance.revision;
        if self.plain_markdown && self.appearance.revision != appearance.revision {
            self.native_preview_key.clear();
            self.write_preview_key.clear();
        }
        self.appearance = appearance;
    }
}
impl Widget for ArticlePanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if std::mem::take(&mut self.restore_after_theme) { self.restore_page_ui(cx); }
        if !self.active {
            return;
        }
        // Flush the debounce before the window/app stops delivering timers.
        // save() rechecks the current account and consent before writing.
        if self.dirty && matches!(event, Event::Background | Event::WindowCloseRequested(_) | Event::Shutdown) {
            self.save(cx);
        }
        if self.owner.as_ref() != current_user_id().as_ref()
            || self
                .grant
                .as_ref()
                .is_some_and(|g| !g.valid(current_user_id().as_deref()))
            || matches!(event, Event::Shutdown)
        {
            cx.action(ArticleAction::Close);
            return;
        }
        if self.page == Page::Edit && self.editable() && self.editing_source_block.is_none() {
            let list = self.portal_list(cx, ids!(article_blocks));
            match self.body_selection.handle_event(cx, event, &list, &mut self.doc, &mut self.history) {
                SelectionUpdate::Changed => { self.changed(cx); return; }
                SelectionUpdate::Handled => { return; }
                SelectionUpdate::Pass => {}
            }
        }
        if self.page == Page::Write && self.view(cx, ids!(write_table_popup)).visible() {
            // The picker closes on Escape or a press anywhere outside it (its buttons toggle it).
            let outside = match event {
                Event::KeyDown(key) => key.key_code == KeyCode::Escape,
                Event::MouseDown(e) => {
                    let inside = |id: LiveId, this: &Self, cx: &mut Cx| this.widget(cx, &[id]).area().rect(cx).contains(e.abs);
                    !inside(id!(write_table_card), self, cx) && !inside(id!(wt_table), self, cx) && !inside(id!(wb_table), self, cx)
                }
                _ => false,
            };
            if outside { self.close_table_popup(cx); }
        }
        if self.page == Page::Write {
            if let Event::KeyDown(key) = event {
                let source = self.text_input(cx, ids!(write_source));
                if key.modifiers.is_primary() && cx.has_key_focus(source.area()) {
                    match key.key_code {
                        KeyCode::KeyB => { self.wrap_write_selection(cx, "**", "**", "bold text"); return; }
                        KeyCode::KeyI => { self.wrap_write_selection(cx, "*", "*", "italic text"); return; }
                        KeyCode::KeyK => { self.wrap_write_selection(cx, "[", "](https://)", "link text"); return; }
                        // Text pastes as usual; an image on the clipboard is imported at the cursor.
                        KeyCode::KeyV => self.paste_image(cx),
                        _ => {}
                    }
                }
            }
        }
        if matches!(self.page,Page::Preview|Page::Reader) && !self.doc.is_html_source() {
            if let Event::KeyDown(key)=event {
                if key.key_code==KeyCode::KeyA && key.modifiers.is_primary() {
                    self.reader_select_all=true;
                    self.redraw(cx);
                    return;
                }
            }
            if let Event::TextCopy(copy)=event {
                if self.reader_select_all {
                    *copy.response.borrow_mut()=Some(self.native_preview.iter().map(|b|b.text.as_str()).collect::<Vec<_>>().join("\n\n"));
                    return;
                }
            }
            if matches!(event,Event::MouseDown(_)) {self.reader_select_all=false;}
        }
        // Keep labels in this panel's own pass/window, and ignore tooltip
        // actions from other Rinx windows.
        if !matches!(event, Event::Actions(_)) {
            self.icon_tooltip.handle_event(cx, event, scope);
        }
        self.view.handle_event(cx, event, scope);
        self.icon_hints(cx, event);
        if self.page == Page::Write && self.write_mode == WriteMode::Split
            && matches!(event, Event::KeyDown(_) | Event::MouseUp(_) | Event::TextInput(_))
        {
            self.follow_write_cursor(cx);
        }
        if self.page == Page::Edit && self.editing_source_block.is_none() && matches!(event, Event::MouseDown(_) | Event::MouseUp(_)) {
            let list = self.portal_list(cx, ids!(article_blocks));
            self.body_selection.after_event(cx, &list, &self.doc);
        }
        if self.save_timer.is_event(event).is_some() && self.dirty && !self.pending {
            if self.page == Page::Write { self.flush_write(cx); } else { self.save(cx); }
        }
        if self.write_toast_timer.is_event(event).is_some() {
            self.write_toast_count = 0;
            self.view.redraw(cx);
        }
        if self.write_timer.is_event(event).is_some() && self.page == Page::Write {
            if self.write_mode == WriteMode::Split {
                // Render the edit now, so the preview can follow the cursor into it.
                let text = self.write_source_full(cx);
                self.prepare_write_preview(&text);
                self.write_sync_cursor = None;
                self.follow_write_cursor(cx);
            }
            self.view.redraw(cx);
        }
        if self.page == Page::Write { self.handle_write_drop(cx, event); }
        if let Event::Actions(actions) = event {
            for action in actions {
                if matches!(self.page, Page::Edit | Page::Preview | Page::Reader) {
                    if let HtmlLinkAction::Clicked { url, .. } = action.as_widget_action().cast() {
                        {
                            // Multiple readers/editor panels may coexist. Only
                            // follow links emitted by this panel's own widgets.
                            let mut uid = action.as_widget_action().map(|a| a.widget_uid);
                            while uid.is_some_and(|uid| uid != self.widget_uid()) {
                                uid = uid.and_then(|uid| cx.widget_tree().parent_of(uid));
                            }
                            if uid.is_none() { continue; }
                        }
                        if let Some(anchor)=url.strip_prefix('#') {
                            self.navigate_anchor(cx,anchor);
                        } else if url.is_empty() {
                            self.navigate_anchor(cx,"");
                        } else if article_core::render::valid_link(&url) && matches!(url::Url::parse(&url).ok().as_ref().map(url::Url::scheme),Some("http"|"https"|"mailto")) {
                            if !self.tabbed_reader || !crate::shared::web_browser::open_chat_link(cx, &url) {
                                crate::utils::open_url(&url);
                            }
                        } else if article_core::render::valid_link(&url) {
                            if !self.reader_only {
                                if let Some(grant)=self.grant.as_ref() {
                                    match storage::read_relative(grant,&self.doc.id,&url) {
                                        Ok(document)=>{
                                            self.doc=document;self.load_library(cx);self.bind(cx);
                                            self.portal_list(cx,ids!(article_reader)).set_first_id_and_scroll(0,0.0);
                                            self.show(cx,Page::Preview);
                                            if let Some((_,anchor))=url.split_once('#') {self.navigate_anchor(cx,anchor);}
                                        }
                                        Err(error)=>self.status(cx,&error),
                                    }
                                }
                            }
                        }
                    }
                }
                let Some(result) = action.downcast_ref::<ResultAction>() else {
                    continue;
                };
                let instance = match result {
                    #[cfg(feature = "html_preview")]
                    ResultAction::CssPreview { instance, .. } => instance,
                    ResultAction::RemoteImages { instance, .. }
                    | ResultAction::Image { instance, .. }
                    | ResultAction::Imported { instance, .. }
                    | ResultAction::Published { instance, .. }
                    | ResultAction::Shared { instance, .. }
                    | ResultAction::Read { instance, .. }
                    | ResultAction::Media { instance, .. } => instance,
                };
                if !self.grant.as_ref().is_some_and(|g| &g.instance == instance) {
                    continue;
                }
                match result {
                    ResultAction::RemoteImages { key, images, .. } => {
                        if key != &self.preview_image_key || !self.allowed() { continue; }
                        self.preview_images = images.clone();
                        self.native_preview_key.clear();
                        self.images_loading = false;
                        self.view.redraw(cx);
                        #[cfg(feature = "html_preview")]
                        if self.page == Page::CssPreview { self.start_css_preview(cx); }
                    }
                    ResultAction::Imported { result, .. } => {
                        self.pending = false;
                        if !self.editable() { continue; }
                        match result {
                            Ok(Some(document)) => {
                                let Some(grant) = &self.grant else { continue };
                                if let Err(error) = storage::save_document_with_source(crate::app_data_dir(), grant, document, None) {
                                    self.status(cx, &error);
                                    continue;
                                }
                                self.doc = document.clone();
                                self.dirty = false;
                                self.selected_publication = None;
                                self.operation = None;
                                self.history.clear();
                                self.load_library(cx);
                                self.bind(cx);
                                self.portal_list(cx, ids!(article_reader)).set_first_id_and_scroll(0, 0.0);
                                self.show(cx, Page::Preview);
                                self.status(cx, "File imported and saved on this device");
                            }
                            Ok(None) => (),
                            Err(error) => self.status(cx, error),
                        }
                    }
                    #[cfg(feature = "html_preview")]
                    ResultAction::CssPreview { request, result, .. } => {
                        if *request != self.css_request || self.page != Page::CssPreview || !self.allowed() { continue; }
                        self.pending = false;
                        match result {
                            Ok(super::preview::Update::Rendered(bitmap)) => {
                                self.html_view(cx, ids!(css_preview_bitmap)).set_rendered(cx, bitmap);
                                self.status(cx, if bitmap.clipped {
                                    "Preview reached its length limit. Return to Full preview to read the entire article."
                                } else { "HTML/CSS preview ready" });
                            }
                            Ok(super::preview::Update::Interaction(action)) => match action {
                                makepad_html_renderer::HtmlAction::OpenLink { url } => {
                                    if validate_link(url).is_ok() { crate::utils::open_url(url); }
                                }
                                makepad_html_renderer::HtmlAction::ScrollTo { y_css } => {
                                    self.html_view(cx, ids!(css_preview_bitmap)).scroll_to(cx, *y_css);
                                }
                                _ => (),
                            },
                            Err(error) => {
                                self.css_session.take();
                                self.status(cx, error);
                            }
                        }
                    }
                    ResultAction::Image {
                        document,
                        cover,
                        result,
                        ..
                    } if document == &self.doc.id => match result {
                        Ok(asset) => {
                            self.selecting_cover = *cover;
                            // Known before the source refers to it, so its short ID resolves.
                            self.library.assets.insert(asset.id.clone(), asset.clone());
                            self.use_image(cx, asset.clone());
                        }
                        Err(e) => self.status(cx, e),
                    },
                    ResultAction::Published { result, .. } => {
                        self.pending = false;
                        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
                        self.button(cx, ids!(withdraw_confirm))
                            .set_enabled(cx, true);
                        match result {
                            Ok(p) => {
                                self.load_library(cx);
                                if self.publish_flow {
                                    let p = p.clone();
                                    self.show_published(cx, &p);
                                } else {
                                    self.publication(cx, p.clone());
                                }
                            }
                            Err(e) => {
                                self.load_library(cx);
                                self.set_icon_hint(cx, id!(article_change), tr("Keep editing a copy"));
                                self.status(cx, e);
                            }
                        }
                    }
                    ResultAction::Shared { result, .. } => {
                        self.pending = false;
                        self.button(cx, ids!(article_confirm)).set_enabled(cx, true);
                        match result {
                            Ok(()) => {
                                self.load_library(cx);
                                self.show(cx, Page::Library);
                                self.status(cx, "Mini app sent");
                            }
                            Err(e) => self.status(cx, e),
                        }
                    }
                    ResultAction::Read { result, .. } => {
                        self.pending = false;
                        match result {
                            Ok(article) => {
                                self.doc = article.document.clone();
                                self.remote_article = Some(article.clone());
                                self.show(cx, Page::Reader);
                                if let (Some(client), Some(grant)) =
                                    (get_client(), self.grant.clone())
                                {
                                    for (id, asset) in &article.assets {
                                        let id = id.clone();
                                        let asset = asset.clone();
                                        let client = client.clone();
                                        let instance = grant.instance.clone();
                                        let grant = grant.clone();
                                        spawn_async_task(async move {
                                            let result =
                                                backend::download_image(client, grant, asset).await;
                                            Cx::post_action(ResultAction::Media {
                                                instance,
                                                id,
                                                result,
                                            });
                                        });
                                    }
                                }
                            }
                            Err(e) => self.status(cx, e),
                        }
                    }
                    ResultAction::Media { id, result, .. } => match result {
                        Ok(bytes) => {
                            self.reader_assets.insert(id.clone(), bytes.clone());
                            self.native_preview_key.clear();
                            self.view.redraw(cx);
                        }
                        Err(e) => self.status(cx, e),
                    },
                    _ => (),
                }
            }
            if self.button(cx, ids!(article_close)).clicked(actions)
                || self.button(cx, ids!(article_cancel)).clicked(actions)
            {
                if !self.dirty || self.save(cx) {
                    cx.action(ArticleAction::Close)
                }
                return;
            }
            if self.button(cx, ids!(article_back)).clicked(actions) {
                self.back(cx);
                return;
            }
            if self.pending {
                return;
            }
            #[cfg(feature = "html_preview")]
            if self.page == Page::CssPreview && self.allowed() {
                let view = self.html_view(cx, ids!(css_preview_bitmap));
                if let Some(session) = &self.css_session {
                    if let Some((x, y)) = view.activation(actions) {
                        self.pending = session.activate(x, y);
                    } else if let Some((x, y, delta)) = view.horizontal_scroll(actions) {
                        self.pending = session.scroll_horizontal(x, y, delta);
                    }
                }
            }
            #[cfg(feature = "html_preview")]
            if (self.page == Page::Preview && self.button(cx, ids!(css_preview_open)).clicked(actions))
                || (self.page == Page::CssPreview && self.button(cx, ids!(css_preview_refresh)).clicked(actions)) {
                self.start_css_preview(cx);
                return;
            }
            if self.page == Page::Details
                && self.button(cx, ids!(article_continue)).clicked(actions)
            {
                if let Some(owner) = current_user_id() {
                    self.owner = Some(owner.clone());
                    self.show(cx, Page::Consent);
                    self.label(cx, ids!(consent_account))
                        .set_text(cx, owner.as_str());
                } else {
                    self.status(cx, "Sign in to Rinx to authorize this app.");
                }
            }
            if self.page == Page::Consent && self.button(cx, ids!(article_allow)).clicked(actions) {
                if let Some(owner) = current_user_id().filter(|u| Some(u) == self.owner.as_ref()) {
                    self.grant = Some(Grant::new(owner));
                    self.reader_only = false;
                    self.load_library(cx);
                    self.show(cx, Page::Library);
                    if self.library.legacy_source.is_some() {
                        self.status(cx,"Your earlier Markdown draft is preserved. Open Markdown source to review unsupported formatting.");
                    }
                }
            }
            if !self.editable() {
                return;
            }
            if self.button(cx, ids!(article_done)).clicked(actions) {
                self.show(cx, Page::Edit);
                return;
            }
            if self.button(cx, ids!(article_save)).clicked(actions) {
                self.save(cx);
            }
            // Round-trip: continue rewriting in the writing studio. Save first
            // so the pulled-back article carries the latest layout edits; the
            // article's provenance realigns the writing-side versions.
            if self.button(cx, ids!(article_to_writing)).clicked(actions) {
                if self.save(cx) {
                    let id = self.doc.id.clone();
                    cx.action(crate::writing_studio::WritingAction::ImportArticle { article_id: id });
                }
            }
            if self.page == Page::Library {
                if self.button(cx, ids!(article_import)).clicked(actions) {
                    self.import_file(cx);
                }
                for (id, tab) in [
                    (id!(drafts_tab), 0),
                    (id!(published_tab), 1),
                    (id!(withdrawn_tab), 2),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.library_tab = tab;
                        self.filter_library(cx);
                    }
                }
                if self
                    .text_input(cx, ids!(article_search))
                    .changed(actions)
                    .is_some()
                {
                    self.filter_library(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_library))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(id) = self.entries.get(index).cloned() {
                            if self.library_tab == 0 {
                                if let Some(doc) =
                                    self.library.documents.iter().find(|d| d.id == id).cloned()
                                {
                                    self.doc = doc;
                                    self.selected_publication = self
                                        .library
                                        .publications
                                        .iter()
                                        .rev()
                                        .find(|p| p.document_id == id && !p.withdrawn)
                                        .cloned();
                                    self.operation = self
                                        .library
                                        .outbox
                                        .iter()
                                        .find(|o| !o.finished && o.document.id == id)
                                        .cloned();
                                    self.history.clear();
                                    self.bind(cx);
                                    if let Some(operation) = self.operation.clone() {
                                        self.doc = operation.document.clone();
                                        self.sharing = false;
                                        if operation.kind == OperationKind::Withdraw {
                                            self.show(cx, Page::Withdraw);
                                            self.label(cx, ids!(withdraw_title))
                                                .set_text(cx, &self.doc.title);
                                            self.label(cx, ids!(withdraw_room))
                                                .set_text(cx, &operation.room_name);
                                        } else {
                                            self.confirm(cx);
                                        }
                                        self.status(cx,"A publication is pending. Retry uses the saved transaction.");
                                    } else {
                                        self.show(cx, Page::Edit);
                                    }
                                }
                            } else if let Some(p) = self
                                .library
                                .publications
                                .iter()
                                .find(|p| p.id == id)
                                .cloned()
                            {
                                self.publication(cx, p);
                            }
                        }
                        break;
                    }
                }
                if self.button(cx, ids!(article_new)).clicked(actions) {
                    self.new_document(cx);
                }
                if self.button(cx, ids!(article_share)).clicked(actions) {
                    self.choose_room(cx, true);
                }
            }
            if self.page == Page::Edit {
                if let Some(title) = self.text_input(cx, ids!(article_title)).changed(actions) {
                    let mut state = super::model::Draft {
                        title: self.doc.title.clone(),
                        markdown: String::new(),
                    };
                    match super::model::apply_input(&mut state, "title_changed", &title) {
                        Ok(()) => {
                            self.checkpoint();
                            self.doc.title = state.title;
                            self.changed(cx)
                        }
                        Err(e) => {
                            self.text_input(cx, ids!(article_title))
                                .set_text(cx, &self.doc.title);
                            self.status(cx, &e);
                        }
                    }
                }
                if let Some(author) = self.text_input(cx, ids!(article_author)).changed(actions) {
                    self.checkpoint();
                    self.doc.author = author;
                    self.changed(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_blocks))
                    .items_with_actions(actions)
                {
                    if index >= self.doc.blocks.len() {
                        continue;
                    }
                    self.active_block = index;
                    if item.button(cx, ids!(source_toggle)).clicked(actions) {
                        let id = &self.doc.blocks[index].id;
                        self.editing_source_block = if self.editing_source_block.as_ref() == Some(id) { None } else { Some(id.clone()) };
                        self.body_selection.reset();
                        cx.set_key_focus(Area::Empty);
                        self.redraw(cx);
                    }
                    let input = item.article_rich_input(cx, ids!(rich));
                    if input.changed(actions).is_some() {
                        if let Some((text, marks)) = input.content() {
                            self.checkpoint();
                            self.doc.blocks[index].text = text;
                            self.doc.blocks[index].marks = marks;
                            self.changed(cx);
                        }
                    }
                    if item.button(cx, ids!(image_settings)).clicked(actions) {
                        self.text_input(cx, ids!(image_caption))
                            .set_text(cx, &self.doc.blocks[index].caption);
                        self.text_input(cx, ids!(image_alt))
                            .set_text(cx, &self.doc.blocks[index].alt);
                        let picture = self.image(cx, ids!(selected_image));
                        self.load_asset(cx, picture, self.doc.blocks[index].asset.as_deref());
                        self.show(cx, Page::ImageSettings);
                    }
                }
                for (id, bold) in [(id!(article_bold), true), (id!(article_italic), false)] {
                    if self.button(cx, &[id]).clicked(actions) {
                        if let Some(selection) = self.body_selection.selection {
                            let (start, _) = selection.ordered();
                            let flags = self.doc.blocks[start.block].flags_at(start.byte);
                            self.checkpoint();
                            for (index, block) in self.doc.blocks.iter_mut().enumerate() {
                                if let Some(range) = selection.range(index, block.text.len()).filter(|r| !r.is_empty()) {
                                    let _ = block.format(range, if bold { Some(!flags.0) } else { None },
                                        if bold { None } else { Some(!flags.1) }, None);
                                }
                            }
                            self.changed(cx);
                            continue;
                        }
                        if self.doc.blocks.get(self.active_block).is_some_and(|b| matches!(b.kind, BlockKind::Markdown | BlockKind::Html)) {
                            self.status(cx, "Edit this block's formatting in its source.");
                            continue;
                        }
                        let item = self.portal_list(cx, ids!(article_blocks)).item(
                            cx,
                            self.active_block,
                            id!(TextBlock),
                        );
                        let input = item.article_rich_input(cx, ids!(rich));
                        self.checkpoint();
                        if input.toggle_format(cx, bold) {
                            if let Some((text, marks)) = input.content() {
                                if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                                    b.text = text;
                                    b.marks = marks;
                                }
                                self.changed(cx);
                            }
                        } else {
                            self.status(cx, "Select some text to format.");
                        }
                    }
                }
                for (id, kind) in [
                    (id!(article_h2), BlockKind::Heading2),
                    (id!(article_quote), BlockKind::Quote),
                    (id!(article_list), BlockKind::Bullet),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        if self.doc.blocks.get(self.active_block).is_some_and(|b| matches!(b.kind, BlockKind::Markdown | BlockKind::Html)) {
                            self.status(cx, "Edit this block's formatting in its source.");
                            continue;
                        }
                        self.checkpoint();
                        if let Some(block) = self.doc.blocks.get_mut(self.active_block) {
                            if block.kind != BlockKind::Image {
                                block.kind = if block.kind == kind {
                                    BlockKind::Paragraph
                                } else {
                                    kind
                                };
                            }
                        }
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_add_text)).clicked(actions) {
                    self.checkpoint();
                    let pos = (self.active_block + 1).min(self.doc.blocks.len());
                    self.doc
                        .blocks
                        .insert(pos, Block::new(BlockKind::Paragraph, ""));
                    self.active_block = pos;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_up)).clicked(actions) && self.active_block > 0 {
                    self.checkpoint();
                    self.doc
                        .blocks
                        .swap(self.active_block, self.active_block - 1);
                    self.active_block -= 1;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_down)).clicked(actions)
                    && self.active_block + 1 < self.doc.blocks.len()
                {
                    self.checkpoint();
                    self.doc
                        .blocks
                        .swap(self.active_block, self.active_block + 1);
                    self.active_block += 1;
                    self.changed(cx);
                }
                if self.button(cx, ids!(block_remove)).clicked(actions)
                    && self.active_block < self.doc.blocks.len()
                {
                    self.checkpoint();
                    self.doc.blocks.remove(self.active_block);
                    if self.doc.blocks.is_empty() {
                        self.doc.blocks.push(Block::new(BlockKind::Paragraph, ""));
                    }
                    self.active_block = self.active_block.min(self.doc.blocks.len() - 1);
                    self.changed(cx);
                }
                if self.button(cx, ids!(article_undo)).clicked(actions) {
                    if self.history.undo(&mut self.doc) {
                        self.bind(cx);
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_redo)).clicked(actions) {
                    if self.history.redo(&mut self.doc) {
                        self.bind(cx);
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(article_link)).clicked(actions) {
                    if self.doc.blocks.get(self.active_block).is_some_and(|b| matches!(b.kind, BlockKind::Markdown | BlockKind::Html)) {
                        self.status(cx, "Edit this block's formatting in its source.");
                        return;
                    }
                    let item = self.portal_list(cx, ids!(article_blocks)).item(
                        cx,
                        self.active_block,
                        id!(TextBlock),
                    );
                    let input = item.article_rich_input(cx, ids!(rich));
                    if input.has_selection() {
                        self.text_input(cx, ids!(article_link_url))
                            .set_text(cx, "https://");
                        self.show(cx, Page::Link);
                    } else {
                        self.status(cx, "Select some text to format.");
                    }
                }
                if self.button(cx, ids!(article_images)).clicked(actions) {
                    self.choose_images(cx, false, false);
                }
                if self.button(cx, ids!(article_theme)).clicked(actions)
                    || self.button(cx, ids!(inspector_theme)).clicked(actions)
                {
                    self.show(cx, Page::Theme);
                }
                if self.button(cx, ids!(article_cover)).clicked(actions)
                    || self
                        .button(cx, ids!(inspector_cover_button))
                        .clicked(actions)
                {
                    self.open_cover(cx);
                }
                if self.button(cx, ids!(article_source)).clicked(actions) {
                    let source = self
                        .library
                        .source_for(&self.doc.id)
                        .map(str::to_owned)
                        .unwrap_or_else(|| self.doc.markdown());
                    self.text_input(cx, ids!(article_markdown))
                        .set_text(cx, &source);
                    self.source_report(cx, &source);
                    self.show(cx, Page::Source);
                }
                if self.button(cx, ids!(article_preview)).clicked(actions)
                    || self.button(cx, ids!(inspector_preview)).clicked(actions)
                {
                    self.preview(cx);
                }
                if self.button(cx, ids!(sidebar_new)).clicked(actions) {
                    self.new_document(cx);
                }
                if self.button(cx, ids!(sidebar_library)).clicked(actions) {
                    self.back(cx);
                }
                if self.button(cx, ids!(sidebar_share)).clicked(actions) {
                    if self.save(cx) {
                        self.choose_room(cx, true);
                    }
                }
            }
            if self.page == Page::Link {
                let apply = self.button(cx, ids!(link_apply)).clicked(actions);
                let remove = self.button(cx, ids!(link_remove)).clicked(actions);
                if apply || remove {
                    let url = if remove {
                        None
                    } else {
                        Some(self.text_input(cx, ids!(article_link_url)).text())
                    };
                    let item = self.portal_list(cx, ids!(article_blocks)).item(
                        cx,
                        self.active_block,
                        id!(TextBlock),
                    );
                    let input = item.article_rich_input(cx, ids!(rich));
                    match input.apply_link(cx, url) {
                        Ok(()) => {
                            self.checkpoint();
                            if let Some((text, marks)) = input.content() {
                                self.doc.blocks[self.active_block].text = text;
                                self.doc.blocks[self.active_block].marks = marks;
                            }
                            self.changed(cx);
                            self.show(cx, Page::Edit);
                        }
                        Err(error) => self.status(cx, &error),
                    }
                }
            }
            if self.page == Page::Source {
                if let Some(text) = self.text_input(cx, ids!(article_markdown)).changed(actions) {
                    self.source_report(cx, &text);
                    self.library.source_drafts.insert(self.doc.id.clone(), text);
                    self.doc.modified = now();
                    self.dirty = true;
                    cx.stop_timer(self.save_timer);
                    self.save_timer = cx.start_timeout(0.7);
                    self.status(cx, "Unsaved changes");
                }
            }
            if self.page == Page::Source && self.button(cx, ids!(source_apply)).clicked(actions) {
                let text = self.text_input(cx, ids!(article_markdown)).text();
                match self.apply_source(cx, &text) {
                    Ok(()) => {
                        self.bind(cx);
                        self.show(cx, Page::Edit);
                    }
                    Err(e) => self.status(cx, &e),
                }
            }
            if self.page == Page::Write {
                self.handle_write_actions(cx, actions);
            }
            if self.page == Page::Images {
                if self.button(cx, ids!(image_pick)).clicked(actions) {
                    self.pick(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(image_library))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if let Some(asset) = self.library.assets.values().nth(index).cloned() {
                            self.use_image(cx, asset)
                        }
                        break;
                    }
                }
            }
            if self.page == Page::ImageSettings {
                for (id, caption) in [(id!(image_caption), true), (id!(image_alt), false)] {
                    if let Some(text) = self.text_input(cx, &[id]).changed(actions) {
                        self.checkpoint();
                        if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                            if caption {
                                b.caption = text
                            } else {
                                b.alt = text
                            }
                        }
                        self.changed(cx);
                    }
                }
                for (id, width) in [
                    (id!(image_full), 100),
                    (id!(image_medium), 75),
                    (id!(image_small), 50),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        if let Some(b) = self.doc.blocks.get_mut(self.active_block) {
                            b.width = width;
                        }
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(image_replace)).clicked(actions) {
                    self.choose_images(cx, false, true);
                }
                if self.button(cx, ids!(image_remove)).clicked(actions) {
                    self.checkpoint();
                    if self.active_block < self.doc.blocks.len() {
                        self.doc.blocks.remove(self.active_block);
                    }
                    if self.doc.blocks.is_empty() {
                        self.doc.blocks.push(Block::new(BlockKind::Paragraph, ""));
                    }
                    self.active_block = 0;
                    self.changed(cx);
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(image_done)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Theme {
                for (index, item) in self
                    .portal_list(cx, ids!(theme_list))
                    .items_with_actions(actions)
                {
                    for (offset, id) in [id!(left), id!(right)].into_iter().enumerate() {
                        if item.navigation_bar_button(cx, &[id]).clicked(actions) {
                            if let Some(t) = Theme::ALL.get(index * 2 + offset) {
                                self.checkpoint();
                                self.doc.theme = *t;
                                self.changed(cx);
                            }
                        }
                    }
                }
                for (id, large) in [(id!(size_normal), false), (id!(size_large), true)] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        self.doc.large_type = large;
                        self.changed(cx);
                    }
                }
                for (id, compact) in [
                    (id!(spacing_comfortable), false),
                    (id!(spacing_compact), true),
                ] {
                    if self.button(cx, &[id]).clicked(actions) {
                        self.checkpoint();
                        self.doc.compact = compact;
                        self.changed(cx);
                    }
                }
                if self.button(cx, ids!(theme_done)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Cover {
                if self.button(cx, ids!(cover_pick)).clicked(actions) {
                    self.choose_images(cx, true, false);
                }
                if let Some(summary) = self.text_input(cx, ids!(cover_summary)).changed(actions) {
                    self.checkpoint();
                    self.doc.summary = summary;
                    self.changed(cx);
                }
                let x = self.slider(cx, ids!(cover_x)).slided(actions);
                let y = self.slider(cx, ids!(cover_y)).slided(actions);
                if x.is_some() || y.is_some() {
                    if let Some(c) = &mut self.doc.cover {
                        if let Some(x) = x {
                            c.focal_x = x.clamp(0.0, 1000.0) as u16
                        }
                        if let Some(y) = y {
                            c.focal_y = y.clamp(0.0, 1000.0) as u16
                        }
                    }
                    self.changed(cx);
                    self.load_cover(cx, ids!(cover_wide), false);
                    self.load_cover(cx, ids!(cover_square), true);
                }
                if let Some(value) = self.check_box(cx, ids!(cover_show)).changed(actions) {
                    self.checkpoint();
                    if let Some(c) = &mut self.doc.cover {
                        c.show_in_article = value;
                    }
                    self.changed(cx);
                }
                if self.button(cx, ids!(cover_remove)).clicked(actions) {
                    self.checkpoint();
                    self.doc.cover = None;
                    self.changed(cx);
                    self.open_cover(cx);
                }
                if self.button(cx, ids!(cover_done)).clicked(actions) {
                    if self.publish_flow {
                        self.text_input(cx, ids!(article_chat_search)).set_text(cx, "");
                        self.show(cx, Page::Rooms);
                    } else {
                        self.show(cx, Page::Edit);
                    }
                }
            }
            if self.page == Page::Preview && self.button(cx, ids!(preview_check)).clicked(actions) {
                self.review(cx);
            }
            if self.page == Page::Review && self.button(cx, ids!(review_continue)).clicked(actions)
            {
                self.sharing = false;
                if let Some(p) = self.selected_publication.clone().filter(|p| !p.withdrawn) {
                    self.prepare(cx, p.room, p.room_name)
                } else {
                    self.choose_room(cx, false)
                }
            }
            if self.page == Page::Rooms && self.button(cx, ids!(rooms_publish)).clicked(actions) {
                self.publish_to_choice(cx);
            }
            if self.page == Page::Published {
                if self.button(cx, ids!(published_edit)).clicked(actions) {
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(published_library)).clicked(actions) {
                    self.load_library(cx);
                    self.show(cx, Page::Library);
                }
            }
            if self.page == Page::Rooms {
                if let Some(query) = self
                    .text_input(cx, ids!(article_chat_search))
                    .changed(actions)
                {
                    self.rooms = cx
                        .get_global::<RoomsListRef>()
                        .mini_app_share_rooms()
                        .into_iter()
                        .filter(|r| r.display().to_lowercase().contains(&query.to_lowercase()))
                        .collect();
                    self.publish_choice = None;
                    self.view.redraw(cx);
                }
                for (index, item) in self
                    .portal_list(cx, ids!(article_rooms))
                    .items_with_actions(actions)
                {
                    if item.as_navigation_bar_button().clicked(actions) {
                        if self.publish_flow {
                            self.publish_choice = Some(index);
                            self.view.redraw(cx);
                            break;
                        }
                        if let Some(room) = self.rooms.get(index).cloned() {
                            if self.sharing {
                                self.share_room = Some(room);
                                self.confirm(cx)
                            } else {
                                self.prepare(
                                    cx,
                                    room.room_id().to_owned(),
                                    room.display().into_owned(),
                                )
                            }
                        }
                        break;
                    }
                }
            }
            if self.page == Page::Confirm {
                if self.button(cx, ids!(article_confirm)).clicked(actions) {
                    self.send(cx);
                }
                if self.button(cx, ids!(article_change)).clicked(actions) {
                    self.load_library(cx);
                    if self.operation.as_ref().is_some_and(|op| {
                        self.library
                            .outbox
                            .iter()
                            .any(|saved| saved.id == op.id && !saved.finished)
                    }) {
                        self.doc.id = new_id();
                        self.selected_publication = None;
                    }
                    self.operation = None;
                    self.bind(cx);
                    self.show(cx, Page::Edit);
                }
            }
            if self.page == Page::Publication {
                if self.button(cx, ids!(publication_library)).clicked(actions) {
                    self.load_library(cx);
                    self.show(cx, Page::Library);
                }
                if self.button(cx, ids!(publication_edit)).clicked(actions) {
                    if self
                        .selected_publication
                        .as_ref()
                        .is_some_and(|p| p.withdrawn)
                    {
                        self.doc.id = new_id();
                        self.selected_publication = None;
                    }
                    self.bind(cx);
                    self.show(cx, Page::Edit);
                }
                if self.button(cx, ids!(publication_read)).clicked(actions) {
                    self.show(cx, Page::Preview);
                }
                if self.button(cx, ids!(publication_withdraw)).clicked(actions) {
                    if let Some(p) = self.selected_publication.clone() {
                        self.show(cx, Page::Withdraw);
                        self.label(cx, ids!(withdraw_title))
                            .set_text(cx, &p.document.title);
                        self.label(cx, ids!(withdraw_room))
                            .set_text(cx, &p.room_name);
                    }
                }
            }
            if self.page == Page::Withdraw {
                if self.button(cx, ids!(withdraw_cancel)).clicked(actions) {
                    self.back(cx);
                }
                if self.button(cx, ids!(withdraw_confirm)).clicked(actions) {
                    if let Some(p) = self.selected_publication.clone() {
                        self.sharing = false;
                        self.operation = self
                            .library
                            .outbox
                            .iter()
                            .find(|o| {
                                !o.finished
                                    && o.kind == OperationKind::Withdraw
                                    && o.publication.as_ref() == Some(&p.id)
                            })
                            .cloned()
                            .or_else(|| {
                                Some(Operation {
                                    id: new_id(),
                                    kind: OperationKind::Withdraw,
                                    document: p.document.clone(),
                                    room: p.room.clone(),
                                    room_name: p.room_name.clone(),
                                    publication: Some(p.id),
                                    root: Some(p.root),
                                    version: p.version,
                                    uploaded: p.assets,
                                    redacted: vec![],
                                    confirmed: None,
                                    finished: false,
                                })
                            });
                        self.button(cx, ids!(withdraw_confirm))
                            .set_enabled(cx, false);
                        self.send(cx);
                        self.status(cx, "Withdrawing article and its revisions…");
                    }
                }
            }
        }
        if event.back_pressed()
            || matches!(
                event,
                Event::KeyUp(KeyEvent {
                    key_code: KeyCode::Escape,
                    ..
                })
            )
        {
            self.back(cx);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let available = cx.turtle().inner_rect().size.x;
        self.viewport_width = available;
        let wide = available >= 960.0;
        let paper_width = if wide {
            Size::Fixed((available - 490.0).min(self.appearance.reading_width))
        } else {
            Size::fill()
        };
        let mut paper = self.view(cx, ids!(editor_paper));
        script_apply_eval!(cx, paper, {width: #(paper_width)});
        self.view(cx, ids!(editor_sidebar)).set_visible(cx, wide);
        self.view(cx, ids!(editor_inspector)).set_visible(cx, wide);
        if matches!(self.page,Page::Edit|Page::Preview|Page::Reader) {self.prepare_native_preview();}
        if self.page == Page::Write { self.layout_writer(cx, wide); }
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            let uid = item.widget_uid();
            let library = uid == self.portal_list(cx, ids!(article_library)).widget_uid();
            let blocks = uid == self.portal_list(cx, ids!(article_blocks)).widget_uid();
            let reader = uid == self.portal_list(cx, ids!(article_reader)).widget_uid();
            let themes = uid == self.portal_list(cx, ids!(theme_list)).widget_uid();
            let swatches = uid == self.portal_list(cx, ids!(write_theme_list)).widget_uid();
            let writing = uid == self.portal_list(cx, ids!(write_list)).widget_uid();
            let images = uid == self.portal_list(cx, ids!(image_library)).widget_uid();
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let cover = self.doc.cover.as_ref().filter(|c| c.show_in_article);
                let count = if library {
                    self.entries.len()
                } else if blocks {
                    self.doc.blocks.len()
                } else if reader {
                    (if self.doc.is_html_source(){self.doc.blocks.len()}else{self.native_preview.len()}) + usize::from(cover.is_some())
                } else if themes {
                    Theme::ALL.len().div_ceil(2)
                } else if writing {
                    usize::from(self.write_title_row) + self.write_preview.len()
                } else if swatches {
                    Theme::ALL.len().div_ceil(3)
                } else if images {
                    self.library.assets.len()
                } else {
                    self.rooms.len()
                };
                list.set_item_range(cx, 0, count);
                while let Some(index) = list.next_visible_item(cx) {
                    if index >= count {
                        continue;
                    }
                    if library {
                        let Some(id) = self.entries.get(index) else {
                            continue;
                        };
                        let (doc, status) = if self.library_tab == 0 {
                            let Some(d) = self.library.documents.iter().find(|d| &d.id == id)
                            else {
                                continue;
                            };
                            (d, format!("{} · {}", tr("Draft"), relative_time(d.modified)))
                        } else {
                            let Some(p) = self.library.publications.iter().find(|p| &p.id == id)
                            else {
                                continue;
                            };
                            let mut status = crate::i18n::format("Published to {0}", &[("0", p.room_name.clone())]);
                            if p.version > 1 {
                                status.push_str(&format!(" · {} {}", tr("Version"), p.version));
                            }
                            (&p.document, status)
                        };
                        let row = list.item(cx, index, id!(Entry));
                        row.label(cx, ids!(title)).set_text(
                            cx,
                            if doc.title.is_empty() {
                                tr("Untitled article")
                            } else {
                                &doc.title
                            },
                        );
                        row.label(cx, ids!(summary)).set_text(cx, &status);
                        let thumbnail = row.image(cx, ids!(thumbnail));
                        self.load_asset(
                            cx,
                            thumbnail,
                            doc.cover.as_ref().map(|c| c.asset.as_str()),
                        );
                        row.draw_all(cx, &mut Scope::empty());
                    } else if blocks {
                        let Some(block) = self.doc.blocks.get(index) else {
                            continue;
                        };
                        let template = match block.kind {
                            BlockKind::Markdown | BlockKind::Html => id!(SourceBlock),
                            BlockKind::Image => id!(Picture),
                            BlockKind::Divider => id!(Rule),
                            _ => id!(TextBlock),
                        };
                        let row = list.item(cx, index, template);
                        if block.kind == BlockKind::Image {
                            let picture = row.image(cx, ids!(picture));
                            self.load_asset(cx, picture, block.asset.as_deref());
                            row.label(cx, ids!(caption)).set_text(cx, &block.caption);
                            let width = (cx.turtle().inner_rect().size.x - 12.0)
                                * (block.width as f64 / 100.0);
                            let mut image = row.image(cx, ids!(picture));
                            script_apply_eval!(cx,image,{width: #(width)});
                        } else if matches!(block.kind, BlockKind::Markdown | BlockKind::Html) {
                            let editing = self.editing_source_block.as_ref() == Some(&block.id);
                            let rendered = self.native_editor.get(index);
                            let empty = rendered.is_none_or(|b| b.html.trim().is_empty());
                            row.view(cx, ids!(rendered)).set_visible(cx, !editing && !empty);
                            row.view(cx, ids!(source_editor)).set_visible(cx, editing);
                            self.icon_labels.insert(row.button(cx, ids!(source_toggle)).widget_uid(), tr(if editing { "Render block" } else if empty { "Edit metadata" } else { "Edit source" }).to_owned());
                            if editing {
                                let input = row.article_rich_input(cx, ids!(rich));
                                article_makepad::presentation::style_input(cx, input.clone(), &self.doc, block);
                                self.body_selection.apply_to_input(cx, index, &input, &self.doc);
                            } else if let Some(rendered) = rendered {
                                let mut html = row.html(cx, ids!(body));
                                self.style_reader_html(cx, html.clone());
                                html.set_text(
                                    cx,
                                    &article_makepad::content::native_html(&rendered.html),
                                );
                            }
                        } else if block.kind != BlockKind::Divider {
                            let prefix = row.label(cx, ids!(prefix));
                            prefix.set_text(
                                cx,
                                match block.kind {
                                    BlockKind::Quote => "❝",
                                    BlockKind::Bullet => "•",
                                    BlockKind::Numbered => "1.",
                                    _ => "",
                                },
                            );
                            prefix.set_visible(
                                cx,
                                matches!(
                                    block.kind,
                                    BlockKind::Quote | BlockKind::Bullet | BlockKind::Numbered
                                ),
                            );
                            let input = row.article_rich_input(cx, ids!(rich));
                            article_makepad::presentation::style_input(cx, input.clone(), &self.doc, block);
                            let empty_body = article_makepad::presentation::show_body_placeholder(&self.doc, index);
                            input.set_empty_text(cx, if empty_body { tr("Write your article…").to_owned() } else { String::new() });
                            self.body_selection.apply_to_input(cx, index, &input, &self.doc);
                        }
                        cx.global::<article_makepad::content_view::DrawingImages>().0=self.preview_images.clone();
                        row.draw_all(cx, &mut Scope::empty());
                        cx.global::<article_makepad::content_view::DrawingImages>().0=Default::default();
                        let input = row.article_rich_input(cx, ids!(rich));
                        self.body_selection.after_draw(cx, index, &input);
                    } else if reader {
                        let cover_count = usize::from(cover.is_some());
                        if !self.doc.is_html_source() && index >= cover_count {
                            if let Some(block) = self.native_preview.get(index - cover_count) {
                                let row = list.item(cx, index, if self.plain_markdown {id!(PlainText)} else {id!(Text)});
                                let mut html = row.html(cx, ids!(body));
                                self.style_reader_html(cx, html.clone());
                                html.set_text(
                                    cx,
                                    &article_makepad::content::native_html(&block.html),
                                );
                                cx.global::<article_makepad::content_view::DrawingImages>()
                                    .0 = self.preview_images.clone();
                                let previous_len = row.selection_text_len();
                                if self.reader_select_all {
                                    row.selection_select_all();
                                }
                                row.draw_all(cx, &mut Scope::empty());
                                if self.reader_select_all
                                    && row.selection_text_len() != previous_len
                                {
                                    row.selection_select_all();
                                    row.redraw(cx);
                                }
                                cx.global::<article_makepad::content_view::DrawingImages>().0=Default::default();
                            }
                            continue;
                        }
                        let block = if cover.is_some() {
                            if index == 0 {
                                None
                            } else {
                                self.doc.blocks.get(index - 1)
                            }
                        } else {
                            self.doc.blocks.get(index)
                        };
                        let picture =
                            block.is_none() || block.is_some_and(|b| b.kind == BlockKind::Image);
                        let row =
                            list.item(cx, index, if picture { id!(Image) } else { id!(Text) });
                        if picture {
                            let asset = block
                                .and_then(|b| b.asset.as_deref())
                                .or_else(|| cover.map(|c| c.asset.as_str()));
                            let mut image = row.image(cx, ids!(picture));
                            if let Some(cover) = cover.filter(|_| block.is_none()) {
                                self.cover_image(cx, image.clone(), cover, false);
                            } else {
                                self.load_asset(cx, image.clone(), asset);
                            }
                            let full_width = cx.turtle().inner_rect().size.x;
                            let width =
                                full_width * block.map(|b| b.width as f64 / 100.0).unwrap_or(1.0);
                            let dimensions = asset.and_then(|id| {
                                self.library
                                    .assets
                                    .get(id)
                                    .map(|a| (a.width, a.height))
                                    .or_else(|| {
                                        self.remote_article.as_ref().and_then(|a| {
                                            a.assets
                                                .get(id)
                                                .map(|a| (a.asset.width, a.asset.height))
                                        })
                                    })
                            });
                            let ratio = if block.is_none() {
                                2.35
                            } else {
                                dimensions
                                    .map(|(w, h)| w as f64 / h.max(1) as f64)
                                    .unwrap_or(1.8)
                            };
                            let height = (width / ratio).min(600.0);
                            script_apply_eval!(cx, image, {width: #(width) height: #(height)});
                            row.label(cx, ids!(caption))
                                .set_text(cx, block.map(|b| b.caption.as_str()).unwrap_or(""));
                        } else if let Some(block) = block {
                            let mut html = row.html(cx, ids!(body));
                            self.style_reader_html(cx, html.clone());
                            let mut renderer = article_makepad::content::NativeRenderer {
                                images: &self.preview_images,
                                size: if self.doc.large_type { 16.0 } else { 14.0 },
                                ink: self.document_colors().1,
                            };
                            html.set_text(cx, &article_makepad::content::native_html(&self.doc.block_html_with_renderer(block, &mut renderer)));
                        }
                        cx.global::<article_makepad::content_view::DrawingImages>().0 = self.preview_images.clone();
                        row.draw_all(cx, &mut Scope::empty());
                        cx.global::<article_makepad::content_view::DrawingImages>().0 = Default::default();
                    } else if swatches {
                        let row = list.item(cx, index, id!(Swatches));
                        for (offset, id) in [id!(t0), id!(t1), id!(t2)].into_iter().enumerate() {
                            let swatch = row.widget(cx, &[id]);
                            let Some(&theme) = Theme::ALL.get(index * 3 + offset) else { swatch.set_visible(cx, false); continue };
                            swatch.set_visible(cx, true);
                            style_swatch(cx, &swatch, theme, theme == self.doc.theme);
                        }
                        row.draw_all(cx, &mut Scope::empty());
                        continue;
                    } else if writing {
                        let index = if self.write_title_row { index } else { index + 1 };
                        if index == 0 {
                            let row = list.item(cx, index, id!(Title));
                            let ink = color(self.document_colors().1);
                            let mut title = row.label(cx, ids!(write_preview_title));
                            title.set_text(cx, &self.doc.title);
                            script_apply_eval!(cx, title, {draw_text +: {color: #(ink)}});
                            row.draw_all(cx, &mut Scope::empty());
                        } else if let Some(ids) = self.write_preview.get(index - 1).and_then(|b| gallery_images(&b.html)) {
                            let row = list.item(cx, index, id!(Gallery));
                            self.draw_gallery(cx, &row, &ids);
                            row.draw_all(cx, &mut Scope::empty());
                        } else if let Some(block) = self.write_preview.get(index - 1) {
                            let row = list.item(cx, index, id!(Text));
                            let mut html = row.html(cx, ids!(body));
                            self.style_reader_html(cx, html.clone());
                            html.set_text(cx, &article_makepad::content::native_html(&block.html));
                            cx.global::<article_makepad::content_view::DrawingImages>().0 = self.preview_images.clone();
                            row.draw_all(cx, &mut Scope::empty());
                            cx.global::<article_makepad::content_view::DrawingImages>().0 = Default::default();
                        }
                        continue;
                    } else if themes {
                        let row = list.item(cx, index, id!(Theme));
                        for (offset, id) in [id!(left), id!(right)].into_iter().enumerate() {
                            let Some(&theme) = Theme::ALL.get(index * 2 + offset) else { continue };
                            let mut tile = row.widget(cx, &[id]);
                            tile.label(cx, ids!(name)).set_text(
                                cx,
                                &format!(
                                    "{}{}",
                                    if theme == self.doc.theme { "✓ " } else { "" },
                                    tr(theme.name())
                                ),
                            );
                            tile.label(cx, ids!(title)).set_text(
                                cx,
                                if self.doc.title.is_empty() {
                                    tr("Your article, your style.")
                                } else {
                                    &self.doc.title
                                },
                            );
                            tile.label(cx, ids!(sample))
                                .set_text(cx, tr("A thoughtful layout makes reading a pleasure."));
                            let image = tile.image(cx, ids!(picture));
                            let asset =
                                self.doc
                                    .cover
                                    .as_ref()
                                    .map(|c| c.asset.as_str())
                                    .or_else(|| {
                                        self.doc.blocks.iter().find_map(|b| b.asset.as_deref())
                                    });
                            self.load_asset(cx, image, asset);
                            let (bg, ink, accent) = theme.colors();
                            let bg = color(bg);
                            let ink = color(ink);
                            let border = color(if theme == self.doc.theme {
                                accent
                            } else {
                                0xe0e0e0
                            });
                            script_apply_eval!(cx, tile, {draw_bg +: {color: #(bg) border_color: #(border)} name +: {draw_text +: {color: #(ink)}} title +: {draw_text +: {color: #(ink)}} sample +: {draw_text +: {color: #(ink)}}});
                        }
                        row.draw_all(cx, &mut Scope::empty());
                    } else if images {
                        if let Some(asset) = self.library.assets.values().nth(index) {
                            let row = list.item(cx, index, id!(Asset));
                            let picture = row.image(cx, ids!(picture));
                            self.load_asset(cx, picture, Some(&asset.id));
                            row.label(cx, ids!(name)).set_text(cx, &asset.name);
                            row.draw_all(cx, &mut Scope::empty());
                        }
                    } else if let Some(room) = self.rooms.get(index) {
                        let row = list.item(cx, index, id!(Chat));
                        self.style_chat_row(cx, &row, &room.display(), self.publish_flow && self.publish_choice == Some(index));
                        row.draw_all(cx, &mut Scope::empty());
                    }
                }
            }
        }
        self.icon_tooltip.draw_all(cx, scope);
        DrawStep::done()
    }
}
use article_core::assets::crop_cover;
impl ArticlePanelRef {
    /// Opens a downloaded Markdown document using the existing native reader.
    pub fn read_markdown(&self, cx: &mut Cx, title: &str, source: &str) -> Result<(), String> {
        let document = Document::from_markdown(title, source)?;
        self.action(cx, ModalRef::default(), &ArticleAction::Open);
        let Some(mut panel) = self.borrow_mut() else { return Err("Article reader is unavailable".into()) };
        panel.reader_only = true;
        panel.plain_markdown = true;
        panel.grant = current_user_id().map(Grant::reader);
        panel.doc = document;
        panel.show(cx, Page::Reader);
        Ok(())
    }

    pub fn reader_title(&self) -> String {
        self.borrow().map(|panel| panel.doc.title.clone()).unwrap_or_default()
    }

    /// Hidden tabs receive their asynchronous document/image results, never
    /// another tab's pointer, keyboard, or link actions.
    pub fn handle_background_event(&self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Event::Actions(actions) = event {
            let results: ActionsBuf = actions.iter()
                .filter_map(|action| action.downcast_ref::<ResultAction>().cloned())
                .map(|result| Box::new(result) as Action).collect();
            if !results.is_empty() {
                if let Some(mut panel) = self.borrow_mut() {
                    panel.handle_event(cx, &Event::Actions(results), scope);
                }
            }
        }
    }

    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &ArticleAction) {
        let Some(mut panel) = self.borrow_mut() else {
            return;
        };
        match action {
            ArticleAction::Open | ArticleAction::Read { .. } => {
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                panel.css_request.clear();
                #[cfg(feature = "html_preview")]
                panel.css_session.take();
                #[cfg(feature = "html_preview")]
                panel.html_view(cx, ids!(css_preview_bitmap)).clear(cx);
                panel.active = true;
                panel.owner = current_user_id();
                panel.plain_markdown = false;
                panel.reader_only = matches!(action, ArticleAction::Read { .. });
                panel.doc = Document::default();
                panel.library = Library::default();
                panel.pending = false;
                panel.dirty = false;
                panel.operation = None;
                panel.selected_publication = None;
                panel.reader_assets.clear();
                panel.icon_labels.clear();
                panel.preview_images = Default::default();
                panel.preview_image_key.clear();
                panel.images_loading = false;
                panel.image_bindings.borrow_mut().clear();
                panel.remote_article = None;
                panel.sharing = false;
                panel.show(cx, Page::Details);
                modal.open(cx);
                if let ArticleAction::Read { room, event } = action {
                    if let (Some(owner), Some(client)) = (current_user_id(), get_client()) {
                        let grant = Grant::reader(owner);
                        let instance = grant.instance.clone();
                        panel.grant = Some(grant.clone());
                        panel.pending = true;
                        panel.show(cx, Page::Reader);
                        panel.status(cx, "Loading article…");
                        let room = room.clone();
                        let event = event.clone();
                        spawn_async_task(async move {
                            let result = backend::read_article(client, grant, room, event).await;
                            Cx::post_action(ResultAction::Read { instance, result });
                        });
                    }
                }
            }
            ArticleAction::Close => {
                if panel.dirty && panel.editable() && !panel.save(cx) {
                    return;
                }
                panel.hide_icon_hint(cx);
                if let Some(g) = panel.grant.take() {
                    g.revoke()
                }
                panel.css_request.clear();
                #[cfg(feature = "html_preview")]
                panel.css_session.take();
                #[cfg(feature = "html_preview")]
                panel.html_view(cx, ids!(css_preview_bitmap)).clear(cx);
                panel.active = false;
                panel.pending = false;
                panel.owner = None;
                panel.doc = Document::default();
                panel.library = Library::default();
                panel.reader_assets.clear();
                panel.preview_images = Default::default();
                panel.preview_image_key.clear();
                panel.images_loading = false;
                panel.image_bindings.borrow_mut().clear();
                panel.remote_article = None;
                panel.operation = None;
                panel.selected_publication = None;
                cx.stop_timer(panel.save_timer);
                modal.close(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_assets, image_insertion, preview_item_for_line, shorten_assets};

    #[test]
    fn theme_reapply_keeps_unsaved_article_source_selection_history_and_scroll() {
        use super::*;
        use crate::theme::{Accent, Appearance, Selection};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let register = |vm: &mut ScriptVm, selection| {
            crate::theme::tests::install(vm, selection);
            crate::i18n::install(vm);
            makepad_code_editor::script_mod(vm);
            crate::shared::script_mod(vm);
            crate::article_app::script_mod(vm);
        };
        let mut panel = cx.with_vm(|vm| {
            register(vm, Selection::default());
            vm.bx.captured_errors = Some(Vec::new());
            let value = vm.eval(script! { use mod.widgets.* ArticlePanel{} });
            let panel = ArticlePanel::script_from_value(vm, value);
            assert!(vm.take_errors().is_empty());
            panel
        });
        panel.active = true;
        panel.doc = Document::from_markdown("Original", "# Heading\n\nUnsaved text").unwrap();
        panel.history.checkpoint(&panel.doc);
        panel.doc.title = "Unsaved title".into();
        panel.show(&mut cx, Page::Write);
        let input = panel.text_input(&mut cx, ids!(write_source));
        input.set_text(&mut cx, "# Edited source\n\nStill unsaved");
        input.set_selection(&mut cx, makepad_widgets::text::selection::Selection {
            anchor: makepad_widgets::text::selection::Cursor {index: 2, prefer_next_row: false},
            cursor: makepad_widgets::text::selection::Cursor {index: 8, prefer_next_row: false},
        });
        let uid = input.widget_uid();
        let selected = input.selection();
        let doc_before = serde_json::to_value(&panel.doc).unwrap();
        let list = panel.portal_list(&mut cx, ids!(article_blocks));
        list.set_first_id_and_scroll(3, -21.);
        for selection in [Selection {appearance: Appearance::Dark, accent: Accent::Teal},
            Selection {appearance: Appearance::Dark, accent: Accent::Violet}, Selection::default()] {
            cx.with_vm(|vm| vm.with_reload(|vm| {
                register(vm, selection);
                let value = vm.eval(script! {use mod.widgets.* ArticlePanel{}});
                panel.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), value);
            }));
            assert!(panel.restore_after_theme);
            panel.restore_page_ui(&mut cx);
            assert!(matches!(panel.page, Page::Write));
            assert_eq!(panel.text_input(&mut cx, ids!(write_source)).widget_uid(),uid);
            assert_eq!(input.text(), "# Edited source\n\nStill unsaved");
            assert_eq!(input.selection().anchor.index, selected.anchor.index);
            assert_eq!(input.selection().cursor.index, selected.cursor.index);
            assert_eq!(serde_json::to_value(&panel.doc).unwrap(),doc_before);
            assert_eq!(list.first_id(),3);
            assert_eq!(list.scroll_position(),-21.);
        }
        assert!(panel.history.undo(&mut panel.doc));
        assert_eq!(panel.doc.title, "Original");
    }

    #[test]
    fn the_preview_follows_the_block_holding_the_cursor() {
        struct Plain;
        impl article_core::render::Renderer for Plain {}
        let blocks = article_core::markdown_render::render("# A\n\ntext\n\n## B\n\nmore", &mut Plain);
        let lines: Vec<usize> = blocks.iter().map(|b| b.source_line).collect();
        assert_eq!(preview_item_for_line(&blocks, 1), Some(0), "{lines:?}");
        assert_eq!(preview_item_for_line(&blocks, 4), Some(1), "{lines:?}");
        assert_eq!(preview_item_for_line(&blocks, 7), Some(blocks.len() - 1), "{lines:?}");
        assert_eq!(preview_item_for_line(&[], 3), None);
    }

    #[test]
    fn image_ids_are_short_in_the_source_and_full_when_stored() {
        let a = format!("abcdef01{}", "1".repeat(56));
        let b = format!("abcdef01{}", "2".repeat(56));
        let c = format!("12345678{}", "3".repeat(56));
        let known = vec![a.clone(), b.clone(), c.clone()];
        let source = format!("![x](asset:{c}) ![y](asset:{a})");
        // Unambiguous prefixes shorten; ones shared with another image stay full.
        let short = shorten_assets(&source, &known);
        assert_eq!(short, format!("![x](asset:12345678) ![y](asset:{a})"));
        assert_eq!(expand_assets(&short, &known), source);
        // Unknown or ambiguous prefixes are left for the parser to reject.
        assert_eq!(expand_assets("asset:abcdef01 asset:99999999", &known), "asset:abcdef01 asset:99999999");
        assert_eq!(expand_assets("asset:1234567", &known), "asset:1234567");
        let _ = b;
    }

    #[test]
    fn images_in_a_row_share_a_paragraph_only_with_the_grid_on() {
        let img = "![b](asset:b)";
        assert_eq!(image_insertion("", img, true), img);
        assert_eq!(image_insertion("Text", img, true), format!("\n\n{img}"));
        assert_eq!(image_insertion("Text\n", img, true), format!("\n{img}"));
        assert_eq!(image_insertion("![a](asset:a)", img, true), format!("\n{img}"));
        assert_eq!(image_insertion("![a](asset:a)", img, false), format!("\n\n{img}\n\n"));
        assert_eq!(image_insertion("![a](asset:a)\n\n", img, false), format!("{img}\n\n"));
    }
}
