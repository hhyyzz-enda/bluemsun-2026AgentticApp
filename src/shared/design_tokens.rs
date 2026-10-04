//! Compatibility names for Rinx semantic tokens (ADR 0009).
//! Interface roles resolve from the active Makepad stylesheet at registration
//! and reapply. Brand/content colors retain their meaning. The Rust constants
//! below are legacy migration values; new runtime drawing uses theme::Snapshot.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // =========================================================================
    // 1. SURFACES — host backgrounds + interaction states
    // =========================================================================
    // Page background from the resolved host stylesheet.
    mod.widgets.RBX_BG_CANVAS         = mod.widgets.RINX_PAGE
    // Card / sheet / elevated surface.
    mod.widgets.RBX_BG_SURFACE        = mod.widgets.RINX_SURFACE
    // Subtle inset surface (grouped rows, secondary panels, table zebra).
    mod.widgets.RBX_BG_SURFACE_SUBTLE = mod.widgets.RINX_FIELD
    // Sunken surface for light code/preview insets.
    mod.widgets.RBX_BG_SUNKEN         = mod.widgets.RINX_FIELD
    // Hover wash over a surface (rows, list items).
    mod.widgets.RBX_BG_HOVER          = mod.widgets.RINX_HOVER
    // Selected row / item background (== RBX_ACCENT_SOFT).
    mod.widgets.RBX_BG_SELECTED       = mod.widgets.RINX_SELECTED
    // Pressed surface (rows, list items, ghost buttons).
    mod.widgets.RBX_BG_PRESSED        = mod.widgets.RINX_PRESSED
    // Disabled control surface.
    mod.widgets.RBX_BG_DISABLED       = mod.widgets.RINX_FIELD
    // Fully transparent + press/hover overlay washes (used by Agent Registry tiles).
    mod.widgets.RBX_TRANSPARENT       = #x00000000
    mod.widgets.RBX_HIT_HOVER         = #x00000008
    mod.widgets.RBX_HIT_DOWN          = #x00000012

    // =========================================================================
    // 2. FOREGROUND — text & icons on host surfaces
    // =========================================================================
    // Primary text (titles, body).
    mod.widgets.RBX_FG_PRIMARY    = mod.widgets.RINX_INK
    // Secondary text (subtitles, meta, helper).
    mod.widgets.RBX_FG_SECONDARY  = mod.widgets.RINX_MUTED
    // Tertiary text (timestamps, faint captions).
    mod.widgets.RBX_FG_TERTIARY   = mod.widgets.RINX_MUTED
    // Text/icon on top of an accent or dark fill.
    mod.widgets.RBX_FG_ON_ACCENT  = mod.widgets.RINX_ON_ACCENT
    // Disabled text.
    mod.widgets.RBX_FG_DISABLED   = mod.widgets.RINX_DISABLED

    // =========================================================================
    // 3. ACCENT — host selection (primary CTA, selection, focus)
    // =========================================================================
    // Primary accent for actions and focus.
    mod.widgets.RBX_ACCENT         = mod.widgets.RINX_ACCENT
    // Accent hover.
    mod.widgets.RBX_ACCENT_HOVER   = mod.widgets.RINX_ACCENT_HOVER
    // Accent pressed.
    mod.widgets.RBX_ACCENT_PRESSED = mod.widgets.RINX_ACCENT_DOWN
    // Soft accent tint (selected chip bg, highlighted row, focus ring fill).
    mod.widgets.RBX_ACCENT_SOFT    = mod.widgets.RINX_SELECTED
    // Hyperlink / inline-link color.
    mod.widgets.RBX_LINK           = mod.widgets.RINX_ACCENT
    // Hovered link. Same hue, darkened like ACCENT → ACCENT_HOVER, so a link
    // deepens on hover instead of jumping to another hue.
    mod.widgets.RBX_LINK_HOVER     = mod.widgets.RINX_ACCENT_HOVER

    // =========================================================================
    // 4. BRAND — logo colors. Use sparingly (brand entry points, app icon, the
    //    room-identity avatar). Do NOT flood functional UI with purple.
    // =========================================================================
    mod.widgets.RBX_BRAND_PURPLE  = #x572DCC // theme-content: stable brand or avatar identity.
    mod.widgets.RBX_BRAND_CYAN    = #x05CDC7 // theme-content: stable brand or avatar identity.
    mod.widgets.RBX_BRAND_BLUE    = #x2D7CFF // theme-content: stable brand or avatar identity.
    // Default room / space identity avatar fill (the teal "#" square).
    mod.widgets.RBX_IDENTITY_TEAL = #x14B8A6 // theme-content: stable brand or avatar identity.
    // DEPRECATED legacy bright-blue primary (styles.rs COLOR_ACTIVE_PRIMARY).
    // Named only for migration; new UI uses RBX_ACCENT.
    mod.widgets.RBX_LEGACY_BLUE   = mod.widgets.RINX_ACCENT

    // =========================================================================
    // 5. STROKES & DIVIDERS
    // =========================================================================
    // Default card / control border (low contrast).
    mod.widgets.RBX_STROKE_SOFT   = mod.widgets.RINX_BORDER
    // Stronger border (focused / emphasized control).
    mod.widgets.RBX_STROKE_STRONG = mod.widgets.RINX_BORDER
    // Floating-overlay border (toast / notification card). Distinctly darker than
    // STRONG so a white card reads clearly against the light canvas WITHOUT a
    // shadow. Tune this one value to make overlay edges softer / harder.
    mod.widgets.RBX_STROKE_OVERLAY = mod.widgets.RINX_BORDER
    // Hairline divider between rows (alpha black).
    mod.widgets.RBX_DIVIDER       = mod.widgets.RINX_BORDER

    // =========================================================================
    // 6. SEMANTIC STATES — fg/bg pairs. One meaning = one color, always.
    //    success = Connected / Healthy / Enabled / Active / Synced
    //    warning = Approval required / Pending / Waiting
    //    danger  = Failed / Rejected / Error / risk action
    //    info    = capability / linked object / neutral metadata highlight
    //    neutral = Idle / secondary / disabled-ish
    //    (accent badges use RBX_ACCENT_SOFT bg + RBX_ACCENT fg — not a state pair.)
    // =========================================================================
    mod.widgets.RBX_SUCCESS_FG = mod.widgets.RINX_SUCCESS_FG
    mod.widgets.RBX_SUCCESS_BG = mod.widgets.RINX_SUCCESS_BG
    mod.widgets.RBX_WARNING_FG = mod.widgets.RINX_WARNING_FG
    mod.widgets.RBX_WARNING_BG = mod.widgets.RINX_WARNING_BG
    mod.widgets.RBX_DANGER_FG  = mod.widgets.RINX_DANGER_FG
    mod.widgets.RBX_DANGER_BG  = mod.widgets.RINX_DANGER_BG
    mod.widgets.RBX_INFO_FG    = mod.widgets.RINX_INFO_FG
    mod.widgets.RBX_INFO_BG    = mod.widgets.RINX_INFO_BG
    mod.widgets.RBX_NEUTRAL_FG = mod.widgets.RINX_MUTED
    mod.widgets.RBX_NEUTRAL_BG = mod.widgets.RINX_FIELD

    // Agent framework badge colors (fg = label text, bg = pill fill). One pair
    // per supported framework, used by the Agent Registry tag pills.
    mod.widgets.RBX_FW_OCTOS_FG    = mod.widgets.RINX_MUTED
    mod.widgets.RBX_FW_OCTOS_BG    = mod.widgets.RINX_FIELD
    mod.widgets.RBX_FW_HERMES_FG   = mod.widgets.RINX_MUTED
    mod.widgets.RBX_FW_HERMES_BG   = mod.widgets.RINX_FIELD
    mod.widgets.RBX_FW_OPENCLAW_FG = mod.widgets.RINX_MUTED
    mod.widgets.RBX_FW_OPENCLAW_BG = mod.widgets.RINX_FIELD

    // =========================================================================
    // 7. DARK SURFACES — desktop nav rail + mobile login (the only dark zones
    //    in this round). Kept explicit rather than as a full theme.
    // =========================================================================
    // Desktop left navigation rail background.
    mod.widgets.RBX_NAV_BG             = mod.widgets.RINX_FIELD
    // Nav item idle foreground.
    mod.widgets.RBX_NAV_FG             = mod.widgets.RINX_MUTED
    // Nav item active foreground.
    mod.widgets.RBX_NAV_FG_ACTIVE      = mod.widgets.RINX_INK
    // Nav item hover background (between rail bg and active).
    mod.widgets.RBX_NAV_ITEM_HOVER_BG  = mod.widgets.RINX_HOVER
    // Nav item active / selected pill background.
    mod.widgets.RBX_NAV_ITEM_ACTIVE_BG = mod.widgets.RINX_SELECTED
    // Nav rail hairline / section divider.
    mod.widgets.RBX_NAV_DIVIDER        = mod.widgets.RINX_BORDER
    // Mobile login page background (deep navy).
    mod.widgets.RBX_LOGIN_BG           = mod.widgets.RINX_PAGE
    // Mobile login field / card surface on the dark page.
    mod.widgets.RBX_LOGIN_SURFACE      = mod.widgets.RINX_SURFACE

    // =========================================================================
    // 8. CODE PANEL (dark) — timeline code / SQL output (CodeOutputCard, §4.7).
    // =========================================================================
    mod.widgets.RBX_CODE_BG      = mod.widgets.RINX_CODE_BG
    mod.widgets.RBX_CODE_FG      = mod.widgets.RINX_CODE_FG
    mod.widgets.RBX_CODE_KEYWORD = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_CODE_STRING  = mod.widgets.RINX_INK
    mod.widgets.RBX_CODE_COMMENT = mod.widgets.RINX_MUTED

    // =========================================================================
    // 8b. SYNTAX PALETTE (light) — the twin of the dark RBX_CODE_* panel above,
    //     for code rendered on a *light* surface: the markdown code blocks in
    //     the timeline and the raw-event viewer. One palette, so a keyword is
    //     the same color everywhere code appears.
    // =========================================================================
    mod.widgets.RBX_SYNTAX_TEXT     = mod.widgets.RINX_CODE_FG
    mod.widgets.RBX_SYNTAX_MUTED    = mod.widgets.RINX_MUTED
    mod.widgets.RBX_SYNTAX_LITERAL  = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_SYNTAX_KEYWORD  = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_SYNTAX_STRING   = mod.widgets.RINX_INK
    mod.widgets.RBX_SYNTAX_FUNCTION = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_SYNTAX_TYPE     = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_SYNTAX_ERROR    = mod.widgets.RINX_DANGER_FG
    mod.widgets.RBX_SYNTAX_WARNING  = mod.widgets.RINX_WARNING_FG

    // =========================================================================
    // 8c. COMPOSER — the room input bar's own neutral ramp (spec §4.8). Kept
    //     separate from the page surfaces because the composer sits *on* the
    //     canvas and needs its own quiet contrast steps.
    // =========================================================================
    mod.widgets.RBX_COMPOSER_BG          = mod.widgets.RINX_FIELD   // composer / attach-row fill
    mod.widgets.RBX_COMPOSER_BG_SUNKEN   = mod.widgets.RINX_FIELD   // inset row inside the composer
    mod.widgets.RBX_COMPOSER_BG_ACTIVE   = mod.widgets.RINX_SELECTED   // active/selected inset row
    mod.widgets.RBX_COMPOSER_INPUT_BG    = mod.widgets.RINX_FIELD   // text field fill
    mod.widgets.RBX_COMPOSER_INPUT_HOVER = mod.widgets.RINX_HOVER
    mod.widgets.RBX_COMPOSER_INPUT_DOWN  = mod.widgets.RINX_PRESSED
    mod.widgets.RBX_COMPOSER_STROKE      = mod.widgets.RINX_BORDER   // text field border
    mod.widgets.RBX_COMPOSER_INK         = mod.widgets.RINX_INK   // primary glyph/icon ink
    mod.widgets.RBX_COMPOSER_INK_MUTED   = mod.widgets.RINX_MUTED   // secondary ink
    mod.widgets.RBX_COMPOSER_INK_FAINT   = mod.widgets.RINX_MUTED   // placeholder / disabled ink
    mod.widgets.RBX_COMPOSER_GREY_HOVER  = mod.widgets.RINX_HOVER   // neutral button hover
    mod.widgets.RBX_COMPOSER_GREY_DOWN   = mod.widgets.RINX_PRESSED   // neutral button pressed

    // Accent washes: the accent at low alpha, for hover/active tints over dark
    // or image backgrounds (spaces rail). Derived from RBX_ACCENT — keep in
    // step with it.
    mod.widgets.RBX_ACCENT_WASH_HOVER  = (mod.widgets.RINX_ACCENT * vec4(1., 1., 1., 0.14)) // theme-content: alpha multiplier preserves the theme accent.
    mod.widgets.RBX_ACCENT_WASH_ACTIVE = (mod.widgets.RINX_ACCENT * vec4(1., 1., 1., 0.24)) // theme-content: alpha multiplier preserves the theme accent.

    // Pressed overlay wash, one step darker than RBX_HIT_DOWN.
    mod.widgets.RBX_HIT_PRESSED = #x0000001E
    // Drop shadow under the mobile navigation bar (heavier than RBX_SHADOW,
    // because it separates two light surfaces rather than floating over one).
    mod.widgets.RBX_SHADOW_NAV  = #x00000055

    // Inline toggle text ("show 3 more") inside small-state groups: near-ink,
    // deepening on interaction.
    mod.widgets.RBX_INK_TOGGLE       = mod.widgets.RINX_INK
    mod.widgets.RBX_INK_TOGGLE_HOVER = mod.widgets.RINX_INK
    mod.widgets.RBX_INK_TOGGLE_DOWN  = mod.widgets.RINX_INK

    // Controls that sit on top of media (video player chrome), where the page
    // palette would disappear against the frame behind them.
    mod.widgets.RBX_MEDIA_CONTROL_BG       = #x111827 // theme-content: fixed overlay over arbitrary media.
    mod.widgets.RBX_MEDIA_CONTROL_BG_HOVER = #x374151 // theme-content: fixed overlay over arbitrary media.

    // Neutral icon grey for indicator glyphs drawn in a shader.
    mod.widgets.RBX_ICON_NEUTRAL = mod.widgets.RINX_MUTED
    // Mention/keyboard-focus highlight in the composer's autocomplete.
    mod.widgets.RBX_MENTION_FOCUS = mod.widgets.RINX_ACCENT
    // Near-opaque light green wash marking a freshly-arrived timeline item.
    mod.widgets.RBX_HIGHLIGHT_NEW = mod.widgets.RINX_SELECTED

    // =========================================================================
    // 9. ELEVATION — modal scrim + drop-shadow colors. Cards lean on radius +
    //    border; reserve shadow for floating layers (sheets / modals / dropdowns
    //    / composer). Blur & offset live in the recipe; the token is the color.
    // =========================================================================
    // Page scrim behind a modal / bottom sheet (navy @ ~50%).
    mod.widgets.RBX_SCRIM         = #x16233B80 // theme-content: fixed overlay over arbitrary media.
    // Card / dropdown / popup drop-shadow color (~15%).
    mod.widgets.RBX_SHADOW        = #x16233B26 // theme-content: fixed overlay over arbitrary media.
    // Sheet / modal drop-shadow color (~25%).
    mod.widgets.RBX_SHADOW_STRONG = #x16233B40 // theme-content: fixed overlay over arbitrary media.

    // =========================================================================
    // 10. FOCUS — keyboard-navigation focus indication (== accent), spec §7.1.
    //     RING/WIDTH are for components that draw their own border and can turn
    //     it accent on focus. TINT is the fallback for flat controls: the button
    //     shader insets its box by `border_size`, so switching a border on just
    //     for focus would resize the control — a tinted fill does not.
    // =========================================================================
    mod.widgets.RBX_FOCUS_RING  = mod.widgets.RINX_ACCENT
    mod.widgets.RBX_FOCUS_WIDTH = 2.0
    mod.widgets.RBX_FOCUS_TINT  = mod.widgets.RINX_ACCENT

    // =========================================================================
    // 11. RADIUS scale — bigger & softer than the legacy RADIUS_* (4/6/8).
    //     Cards lean on radius + border, not heavy shadow.
    //     NOTE: the card default (MD) is intentionally tight (8) so cards line up
    //     visually with the room composer / input bar (which uses XS = 6). Keep
    //     cards calm and crisp rather than pill-soft.
    // =========================================================================
    // Tightened scale (squarer look, per design direction): every surface gets
    // smaller corners than the original 6/8/8/16/20.
    // Extra-extra-small: tightest radius, used by Agent Registry cards/sheets.
    mod.widgets.RBX_RADIUS_XXS  = theme.corner_radius * 0.67
    mod.widgets.RBX_RADIUS_XS   = theme.corner_radius * 0.67
    mod.widgets.RBX_RADIUS_SM   = theme.corner_radius * 1
    // Card / sheet default. Shares SM's value on purpose: small surfaces and
    // cards use one calm, tight radius.
    mod.widgets.RBX_RADIUS_MD   = theme.corner_radius * 1
    mod.widgets.RBX_RADIUS_LG   = theme.corner_radius * 2
    mod.widgets.RBX_RADIUS_XL   = theme.corner_radius * 2.67
    // Fully-rounded (pill) — use on badges / chips.
    mod.widgets.RBX_RADIUS_PILL = 100.0

    // =========================================================================
    // 12. SPACING — the 4px grid SPACE_XS..SPACE_XXL (4..24) from styles.rs still
    //     applies. These add the two larger section-level steps the new layouts use.
    // =========================================================================
    mod.widgets.RBX_SPACE_2XL = 32 * mod.widgets.RINX_SPACING
    mod.widgets.RBX_SPACE_3XL = 40 * mod.widgets.RINX_SPACING

    // =========================================================================
    // 13. SIZING — control heights, list-row heights, icon & avatar sizes, the
    //     mobile bottom-tab height, and the min touch target. Anchored to the 4px
    //     rhythm. Use these instead of hardcoding 32/36/40/48/52 per screen.
    // =========================================================================
    // Control heights (buttons, inputs, segmented tabs).
    mod.widgets.RBX_CONTROL_H_SM = 32.0   // compact button / chip
    mod.widgets.RBX_CONTROL_H_MD = 36.0   // standard button / segmented tab (== SETTINGS_BUTTON_HEIGHT)
    mod.widgets.RBX_CONTROL_H_LG = mod.widgets.RINX_CONTROL_HEIGHT   // large button / text input
    // List / setting row min heights (touch-first on mobile).
    mod.widgets.RBX_ROW_H_DESKTOP = 48.0
    mod.widgets.RBX_ROW_H_MOBILE  = 52.0
    // Minimum touch target.
    mod.widgets.RBX_TAP_MIN       = 44.0
    // Mobile bottom tab bar height.
    mod.widgets.RBX_BOTTOM_TAB_H  = 56.0
    // Icon sizes.
    mod.widgets.RBX_ICON_XS = 12.0
    mod.widgets.RBX_ICON_SM = 16.0
    mod.widgets.RBX_ICON_MD = 20.0
    mod.widgets.RBX_ICON_LG = 24.0
    // Avatar sizes.
    mod.widgets.RBX_AVATAR_SM = 28.0   // inline / row
    mod.widgets.RBX_AVATAR_MD = 40.0   // message / list
    mod.widgets.RBX_AVATAR_LG = 48.0   // hero / room identity

    // =========================================================================
    // 14. TYPE SCALE — semantic TextStyle presets. Built on the app's real font
    //     providers (APP_FONT_REGULAR / APP_FONT_BOLD). Sizes match Makepad's
    //     dense scale used elsewhere in robrix2 (9–17). line_spacing matches the
    //     1.3 used by styles.rs message text, so multi-line wraps stay readable.
    // =========================================================================
    // Mobile/desktop page title (e.g. "Settings", room hero name).
    // Font providers for the RBX_TEXT_* scale. Upstream Rinx ships no
    // system-font links, so these sit on the theme fonts; the
    // `fonts/macos-system-fonts` branch swaps them for the OS's San Francisco
    // and PingFang families.
    mod.widgets.RBX_FONT_REGULAR = theme.font_regular {}
    mod.widgets.RBX_FONT_BOLD = theme.font_bold {}

    mod.widgets.RBX_TEXT_PAGE_TITLE    = mod.widgets.RBX_FONT_BOLD    { font_size: mod.widgets.RINX_TITLE_SIZE, line_spacing: 1.25 }
    // Section / group title inside a page.
    mod.widgets.RBX_TEXT_SECTION_TITLE = mod.widgets.RBX_FONT_BOLD    { font_size: 13.0 * mod.widgets.RINX_TEXT_SCALE, line_spacing: 1.25 }
    // Card title.
    mod.widgets.RBX_TEXT_CARD_TITLE    = mod.widgets.RBX_FONT_BOLD    { font_size: 12.0 * mod.widgets.RINX_TEXT_SCALE, line_spacing: 1.3 }
    // Default body / list-row title.
    mod.widgets.RBX_TEXT_BODY          = mod.widgets.RBX_FONT_REGULAR { font_size: mod.widgets.RINX_BODY_SIZE, line_spacing: 1.35 }
    // Emphasized body (selected value, key figure).
    mod.widgets.RBX_TEXT_BODY_STRONG   = mod.widgets.RBX_FONT_BOLD    { font_size: mod.widgets.RINX_BODY_SIZE, line_spacing: 1.35 }
    // Meta / caption / helper text.
    mod.widgets.RBX_TEXT_META          = mod.widgets.RBX_FONT_REGULAR { font_size: mod.widgets.RINX_META_SIZE,  line_spacing: 1.3 }
    // Badge / chip label (single line).
    mod.widgets.RBX_TEXT_BADGE         = mod.widgets.RBX_FONT_BOLD    { font_size: 9.0 * mod.widgets.RINX_TEXT_SCALE }
}

// Brand identity is content. Interface colors use per-instance theme snapshots.
pub const RBX_IDENTITY_TEAL: Vec4 = vec4(0.078, 0.722, 0.651, 1.0); // theme-content: stable brand or avatar identity.

// =============================================================================
// Contrast guard
// =============================================================================

#[cfg(test)]
mod tests {
    /// Every foreground/background pair the design system promises will be used
    /// together, and the text that sits on it. WCAG AA wants 4.5:1 for body
    /// text, which at this type scale (9–17px) is everything here.
    const SEMANTIC_PAIRS: &[(&str, &str, &str)] = &[
        ("RBX_FG_PRIMARY", "RBX_BG_CANVAS", "body text on the page canvas"),
        ("RBX_FG_PRIMARY", "RBX_BG_SURFACE", "body text on a card"),
        ("RBX_FG_SECONDARY", "RBX_BG_SURFACE", "subtitles / meta on a card"),
        ("RBX_FG_SECONDARY", "RBX_BG_CANVAS", "subtitles / meta on the canvas"),
        ("RBX_FG_TERTIARY", "RBX_BG_SURFACE", "timestamps on a card"),
        ("RBX_FG_TERTIARY", "RBX_BG_CANVAS", "timestamps on the canvas"),
        ("RBX_FG_ON_ACCENT", "RBX_ACCENT", "label on the primary CTA"),
        ("RBX_FG_ON_ACCENT", "RBX_ACCENT_HOVER", "label on a hovered CTA"),
        ("RBX_LINK", "RBX_BG_SURFACE", "inline link on a card"),
        ("RBX_ACCENT", "RBX_ACCENT_SOFT", "accent badge / selected chip"),
        ("RBX_SUCCESS_FG", "RBX_SUCCESS_BG", "success badge"),
        ("RBX_WARNING_FG", "RBX_WARNING_BG", "warning / pending badge"),
        ("RBX_DANGER_FG", "RBX_DANGER_BG", "danger badge"),
        ("RBX_INFO_FG", "RBX_INFO_BG", "info / capability chip"),
        ("RBX_NEUTRAL_FG", "RBX_NEUTRAL_BG", "neutral / idle badge"),
        ("RBX_NAV_FG", "RBX_NAV_BG", "idle item in the desktop nav rail"),
        ("RBX_NAV_FG_ACTIVE", "RBX_NAV_BG", "active item in the desktop nav rail"),
        ("RBX_CODE_FG", "RBX_CODE_BG", "code panel body"),
        ("RBX_CODE_KEYWORD", "RBX_CODE_BG", "code panel keyword"),
        ("RBX_CODE_STRING", "RBX_CODE_BG", "code panel string"),
        ("RBX_CODE_COMMENT", "RBX_CODE_BG", "code panel comment"),
    ];

    /// Read the resolved VM roles, including host aliases, rather than parsing
    /// source literals (which cannot follow a live theme).
    fn token_hex(vm: &mut makepad_widgets::ScriptVm, name: &str) -> (u8, u8, u8) {
        use makepad_widgets::*;
        let widgets = vm.module(id!(widgets));
        let value = vm.bx.heap.value(widgets, LiveId::from_str(name).into(), NoTrap);
        assert!(!value.is_nil(), "missing {name}");
        let color = Vec4f::script_from_value(vm,value);
        let argb = crate::theme::argb(color);
        ((argb >> 16) as u8, (argb >> 8) as u8, argb as u8)
    }

    fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
        fn channel(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f64 {
        let (a, b) = (relative_luminance(fg), relative_luminance(bg));
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The palette was retuned to clear WCAG AA; this keeps it there. If a new
    /// color fails, darken (or lighten) it along the same hue rather than
    /// deleting the pair — `tools/ux-harness contrast --fg .. --bg ..` prints
    /// the nearest compliant value.
    #[test]
    fn semantic_token_pairs_meet_wcag_aa() {
        use makepad_widgets::*;
        use crate::theme::{Accent, Appearance, Selection};
        let mut failures = Vec::new();
        let mut cx = Cx::new(Box::new(|_,_|{}));
        let mut first = true;
        for appearance in [Appearance::Light, Appearance::Dark] {
            for accent in [Accent::Teal, Accent::Violet] {
                cx.with_vm(|vm| {
                    if first {crate::theme::tests::install(vm,Selection{appearance,accent}); first=false;}
                    else {vm.with_reload(|vm| crate::theme::tests::install(vm,Selection{appearance,accent}));}
                    super::script_mod(vm);
                    for (fg,bg,usage) in SEMANTIC_PAIRS {
                        let ratio = contrast(token_hex(vm,fg),token_hex(vm,bg));
                        if ratio < 4.5 { failures.push(format!("{appearance:?}/{accent:?}: {fg} on {bg} = {ratio:.2}:1 ({usage})")); }
                    }
                });
            }
        }
        assert!(
            failures.is_empty(),
            "design tokens below WCAG AA 4.5:1:\n  {}",
            failures.join("\n  "),
        );
    }
}
