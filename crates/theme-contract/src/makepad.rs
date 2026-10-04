//! Trusted Makepad stylesheet emission from validated semantic values.
use super::*;

pub fn append_makepad(theme: &mut String, tokens: &Tokens) -> Result<(), String> {
    validate_resolved(tokens)?;
    for (role, token) in tokens {
        if token.kind == "color" {
            let name = role.replace('.', "_");
            theme.push_str(&format!("\nmod.theme.octo_{name} = #x{}\n", hex(token)?));
        }
    }
    for (role, keys) in [
        ("color.surface.page", &["color_bg_app"][..]),
        (
            "color.surface.panel",
            &["color_bg_container", "color_outset"],
        ),
        ("color.surface.field", &["color_inset"]),
        (
            "color.content.primary",
            &[
                "color_text",
                "color_text_hover",
                "color_text_focus",
                "color_text_active",
                "color_text_down",
                "color_label",
                "color_label_hover",
                "color_label_focus",
                "color_label_down",
                "color_label_active",
                "color_label_inner",
                "color_label_outer",
            ],
        ),
        ("color.content.disabled", &["color_text_disabled"]),
        (
            "color.action.primary",
            &[
                "color_focus",
                "color_ctrl_selected",
                "color_ctrl_active",
                "color_outset_active",
                "color_cursor",
                "color_text_cursor",
                "color_bevel_inset_1_focus",
                "color_bevel_inset_2_focus",
                "color_bevel_outset_1_focus",
                "color_bevel_outset_2_focus",
            ],
        ),
        ("color.action.on_primary", &["color_text_on_accent"]),
        ("color.border.default", &["color_bevel_outset_2"]),
    ] {
        for key in keys {
            theme.push_str(&format!("mod.theme.{key} = #x{}\n", hex(&tokens[role])?));
        }
    }
    for (role, keys) in [
        (
            "shape.surface.radius",
            &["corner_radius", "container_corner_radius"][..],
        ),
        ("metrics.control.height", &["octo_control_height"]),
        ("metrics.reading.width", &["octo_reading_width"]),
        ("metrics.page.gutter", &["octo_page_gutter"]),
        ("motion.duration", &["octo_motion_ms"]),
    ] {
        for key in keys {
            theme.push_str(&format!(
                "mod.theme.{key} = {:?}\n",
                tokens[role].value["value"].as_f64().unwrap()
            ));
        }
    }
    for role in ["typography.scale", "metrics.spacing"] {
        theme.push_str(&format!(
            "mod.theme.octo_{} = {:?}\n",
            role.replace('.', "_"),
            tokens[role].value.as_f64().unwrap()
        ));
    }
    let scale = tokens["typography.scale"].value.as_f64().unwrap();
    for key in [
        "font_size_base",
        "font_size_1",
        "font_size_2",
        "font_size_3",
        "font_size_4",
        "font_size_p",
        "font_size_code",
    ] {
        theme.push_str(&format!("mod.theme.{key} = mod.theme.{key} * {scale:?}\n"));
    }
    let spacing = tokens["metrics.spacing"].value.as_f64().unwrap();
    for key in ["space_factor", "space_1", "space_2", "space_3"] {
        theme.push_str(&format!(
            "mod.theme.{key} = mod.theme.{key} * {spacing:?}\n"
        ));
    }
    match tokens["typography.family"].value.as_str() {
        Some("mono")=>theme.push_str("mod.theme.font_regular = mod.theme.font_code {}\nmod.theme.font_bold = mod.theme.font_code {}\nmod.theme.font_label = mod.theme.font_code {}\nmod.theme.font_italic = mod.theme.font_code {}\nmod.theme.font_bold_italic = mod.theme.font_code {}\n"),
        // System deliberately keeps the platform provider's Chinese fallback.
        _=>{}
    }
    theme.push_str(&format!(
        "mod.theme.octo_typography_family = {:?}\n",
        tokens["typography.family"].value.as_str().unwrap()
    ));
    theme.push_str("true\n");
    Ok(())
}
