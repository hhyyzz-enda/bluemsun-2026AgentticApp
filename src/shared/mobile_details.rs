//! Shared mobile detail-page geometry, independent of the home tab widgets.
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.DetailLabel = Label {
        draw_text +: {color: mod.widgets.RINX_INK text_style: theme.font_regular {font_size: (12.5 * mod.widgets.RINX_TEXT_SCALE)}}
    }
    mod.widgets.DetailSection = SolidView {
        width: Fill height: Fit flow: Down draw_bg.color: mod.widgets.RINX_SURFACE
    }
    mod.widgets.DetailDivider = SolidView {
        width: Fill height: 0.5 margin: Inset{left: 20} draw_bg.color: mod.widgets.RINX_BORDER
    }
    mod.widgets.DetailGap = View {width: Fill height: 8}
    mod.widgets.DetailRow = NavigationBarButton {
        width: Fill height: 56 flow: Right spacing: 12
        padding: Inset{left: 20 right: 20} align: Align{y: 0.5}
        draw_bg +: {color_hover: mod.widgets.RINX_BORDER color_active: mod.widgets.RINX_BORDER border_radius: 0}
        title := mod.widgets.DetailLabel {width: Fit max_lines: 1}
        value := mod.widgets.DetailLabel {
            width: Fill align: Align{x: 1} max_lines: 1 text_overflow: Ellipsis
            draw_text.color: mod.widgets.RINX_MUTED
        }
        chevron := View {
            width: 8 height: 14 align: Align{x: 0.5 y: 0.5}
            Icon {
                icon_walk: Walk{width: 8 height: 14}
                draw_icon +: {svg: crate_resource("self://resources/icons/mobile_chevron_right.svg") color: mod.widgets.RINX_DISABLED}
            }
        }
    }
    mod.widgets.DetailAction = NavigationBarButton {
        width: Fill height: 56 align: Align{x: 0.5 y: 0.5}
        draw_bg +: {color_hover: mod.widgets.RINX_BORDER color_active: mod.widgets.RINX_BORDER border_radius: 0}
        title := mod.widgets.DetailLabel {draw_text.color: mod.widgets.RINX_ACCENT}
    }
    mod.widgets.DetailHeader = SolidView {
        width: Fill height: 48 flow: Overlay draw_bg.color: mod.widgets.RINX_PAGE
        title := mod.widgets.DetailLabel {
            width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
            draw_text.text_style: theme.font_bold {font_size: (12.5 * mod.widgets.RINX_TEXT_SCALE)}
        }
        controls := View {
            width: Fill height: Fill flow: Right align: Align{y: 0.5}
            back := RobrixNeutralIconButton {
                width: 48 height: 48 padding: 14
                align: Align{x: 0.5 y: 0.5} spacing: 0
                draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0}
                draw_icon +: {svg: ICON_CHEVRON_LEFT color: mod.widgets.RINX_INK}
                icon_walk: Walk{width: 8 height: 14}
            }
            View {width: Fill height: 1}
            save := RobrixPositiveIconButton {
                visible: false text: #(crate::i18n::tr("Save")) i18n_text: "Save" width: 58 height: 30 margin: Inset{right: 12} padding: 0
                align: Align{x: 0.5 y: 0.5}
                icon_walk: Walk{width: 0 height: 0} spacing: 0
                draw_bg +: {color: mod.widgets.RINX_ACCENT border_size: 0 border_radius: 4}
                draw_text +: {color: mod.widgets.RINX_ON_ACCENT text_style: theme.font_regular {font_size: (11 * mod.widgets.RINX_TEXT_SCALE)}}
            }
        }
    }
    mod.widgets.DetailAvatar = Avatar {
        width: 64 height: 64
        text_view +: {draw_bg +: {pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 5.0)
            sdf.fill(self.color)
            return sdf.result
        }}}
        img_view +: {img +: {draw_bg +: {pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 5.0)
            sdf.fill(self.get_color())
            return sdf.result
        }}}}
    }
    mod.widgets.DetailContactCard = SolidView {
        width: Fill height: Fit flow: Right spacing: 20
        padding: Inset{left: 24 right: 24 top: 24 bottom: 30}
        align: Align{y: 0.5} draw_bg.color: mod.widgets.RINX_SURFACE
        avatar := mod.widgets.DetailAvatar {}
        View {
            width: Fill height: Fit flow: Down spacing: 10
            name := mod.widgets.DetailLabel {
                width: Fill max_lines: 2 text_overflow: Ellipsis
                draw_text.text_style: theme.font_bold {font_size: (16 * mod.widgets.RINX_TEXT_SCALE)}
            }
            user_id := mod.widgets.DetailLabel {
                width: Fill flow: Flow.Right{wrap: true} max_lines: 3 text_overflow: Ellipsis
                draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10.5 * mod.widgets.RINX_TEXT_SCALE)}}
            }
        }
    }
    mod.widgets.DetailNote = Label {
        width: Fill height: Fit padding: Inset{left: 20 right: 20 top: 12 bottom: 16}
        flow: Flow.Right{wrap: true}
        draw_text +: {color: mod.widgets.RINX_MUTED text_style: theme.font_regular {font_size: (10.5 * mod.widgets.RINX_TEXT_SCALE)}}
    }
}
