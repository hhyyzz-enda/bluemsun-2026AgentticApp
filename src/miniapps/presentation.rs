//! Presentation-only adapter for the existing L0 native-kit contract.
//! Packages and their verified bytes remain unchanged. Legacy painted kits
//! retain their authored palettes until they adopt semantic token references.
use crate::theme::{Snapshot, argb};
use octoscript_makepad::l0::PreparedCard;
use octoscript_ui_l0::InstanceStore;
use serde_json::Value;
use std::path::Path;

/// Override recognized roles in memory, leaving content, assets, geometry,
/// bindings, unknown tokens and package authority untouched.
fn resolve_tokens(pack: &mut Value, theme: &Snapshot) {
    for role in octosense_theme_contract::COLORS {
        if let Some(token) = pack["tokens"]
            .get_mut(*role)
            .and_then(|t| t.get_mut("value"))
        {
            *token = argb(theme.role(role)).into();
        }
    }
    if let Some(token) = pack["tokens"]
        .get_mut("shape.surface.radius")
        .and_then(|t| t.get_mut("value"))
    {
        *token = theme.radius.into();
    }
    for (role, value) in [
        ("typography.body.size", 11. * 4. / 3. * theme.text_scale),
        ("typography.heading.size", 17. * 4. / 3. * theme.text_scale),
        ("typography.caption.size", 9.5 * 4. / 3. * theme.text_scale),
        (
            "metrics.control.height",
            theme.control_height.max(24. * theme.text_scale + 20.),
        ),
        ("metrics.page.gutter", theme.page_gutter),
        ("metrics.spacing", theme.spacing_scale),
    ] {
        if let Some(token) = pack["tokens"]
            .get_mut(role)
            .and_then(|t| t.get_mut("value"))
        {
            *token = value.into();
        }
    }
    if let Some(token) = pack["tokens"]
        .get_mut("typography.family.resource")
        .and_then(|t| t.get_mut("value"))
    {
        *token = if theme
            .tokens
            .get("typography.family")
            .is_some_and(|t| t.value == "mono")
        {
            "makepad_widgets:resources/LiberationMono-Regular.ttf"
        } else {
            "makepad_widgets:resources/IBMPlexSans-Text.ttf"
        }
        .into();
    }
}

pub fn prepare(
    card: &str,
    data: &Value,
    state: &InstanceStore,
    kit_dir: &Path,
    theme: &Snapshot,
) -> Result<PreparedCard, String> {
    let report = octoscript_ui_l0::realize_with_state(card, data, state, Default::default());
    let root = report.complete_root()?;
    if !octoscript_ui_l0::kit_pack::contains(root) {
        return octoscript_makepad::l0::prepare_with_state(card, data, state, kit_dir);
    }
    if !octoscript_ui_l0::card_theme_axes(card).is_empty() {
        return Err("native kit theme axes require a registered token override".into());
    }
    let mood = octoscript_ui_l0::card_theme(card).unwrap_or_else(|| "dark".into());
    let path = kit_dir.join("native").join(&mood).join("kit.json");
    let mut pack: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    if pack["theme"] != mood {
        return Err("kit theme does not match the ledger".into());
    }
    resolve_tokens(&mut pack, theme);
    let source = octoscript_ui_l0::kit_pack::lower(root, &pack, data)?;
    let tree = octoscript_makepad::design::prepare(&source)?;
    Ok(PreparedCard {
        source,
        tree,
        native_components: true,
    })
}

/// Measured kits use window coordinates. The upstream scroll viewport converts
/// their descendants to local margins; place that viewport in the host slot.
/// This uses the standard renderer, without rewriting its generated colors or
/// coordinates. Named descendants keep their identities during reconciliation.
pub fn embedded_ui(mut card: PreparedCard) -> Result<String, String> {
    octoscript_makepad::l0::inspectable(&mut card.tree);
    if !card.native_components {
        return Ok(octoscript_makepad::to_makepad_l0_ui(&card.tree));
    }
    let w = card.tree.attrs.w.ok_or("native kit width required")?;
    let h = card.tree.attrs.h.ok_or("native kit height required")?;
    let mut viewport = octoscript_makepad::design::prepare(&format!(
        "{{t:\"stack\",variant:\"scroll_y\",w:{w},h:{h}}}"
    ))?;
    viewport.children.push(card.tree);
    let ui = octoscript_makepad::design::to_makepad_ui(&viewport)?;
    Ok(format!(
        "let RinxCardViewport = {ui}\nrinx_card := RinxCardViewport {{abs_pos: nil}}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{self, Accent, Appearance, Selection};
    use makepad_widgets::*;

    #[test]
    fn native_kit_resolves_host_roles_without_mutating_bundle_or_state() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/miniapps/theme-reference");
        let file = root.join("kit/native/light/kit.json");
        let bytes = std::fs::read(&file).unwrap();
        let card = include_str!("../../examples/miniapps/theme-reference/page.card");
        let data: Value = serde_json::from_str(include_str!(
            "../../examples/miniapps/theme-reference/data.json"
        ))
        .unwrap();
        let mut state = InstanceStore::default();
        state.set_cell(
            octoscript_ui_l0::CARD_STATE_KEY,
            "draft",
            "Saved instance text".into(),
        );
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut first = true;
        for appearance in [Appearance::Light, Appearance::Dark] {
            for accent in [Accent::Teal, Accent::Violet] {
                let snapshot = cx.with_vm(|vm| {
                    if first {
                        theme::tests::install(vm, Selection { appearance, accent });
                        first = false;
                    } else {
                        vm.with_reload(|vm| {
                            theme::tests::install(vm, Selection { appearance, accent })
                        });
                    }
                    theme::snapshot_for_vm(vm)
                });
                let prepared = prepare(card, &data, &state, &root.join("kit"), &snapshot).unwrap();
                assert!(prepared.native_components);
                assert_eq!(prepared.tree.attrs.bg, Some(argb(snapshot.surface)));
                assert_eq!(
                    prepared.tree.children[0].attrs.color,
                    Some(argb(snapshot.ink))
                );
                assert_eq!(
                    prepared.tree.children[2].children[0].attrs.text.as_deref(),
                    Some("Saved instance text")
                );
                assert_eq!(
                    prepared.tree.children[3].attrs.bg,
                    Some(argb(snapshot.accent))
                );
                assert_eq!(
                    state.get(octoscript_ui_l0::CARD_STATE_KEY, "draft"),
                    Some(&Value::from("Saved instance text"))
                );
                assert_eq!(std::fs::read(&file).unwrap(), bytes);
            }
        }
    }
}
