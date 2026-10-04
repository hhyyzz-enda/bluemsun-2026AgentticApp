//! A portable receive boundary for hosts that transport data snapshots.
//!
//! In-process OctoSense modules already receive complete Makepad StyleSheets.
//! Out-of-process adapters may call `receive` with an initial snapshot and each
//! subscription update. This module owns no IPC, network channel, or preference.
use super::*;
use octosense_theme_contract::{ResolvedTheme, makepad::append_makepad};

pub fn receive(cx: &mut Cx, bytes: &[u8]) -> Result<bool, String> {
    let snapshot = ResolvedTheme::parse(bytes)?;
    let changed = cx.with_vm(|vm| -> Result<bool, String> {
        let key = vm.bx.heap.heap_key();
        let state = vm
            .cx_mut()
            .global::<Runtime>()
            .heaps
            .get(&key)
            .ok_or("Theme host is not initialized")?;
        if !state.hosted {
            return Err("Only a hosted instance accepts host snapshots".into());
        }
        if state.host_revision.as_ref() == Some(&snapshot.revision) {
            return Ok(false);
        }
        let existing = desktop_style::current(vm);
        let family = existing
            .as_ref()
            .and_then(|s| DesktopStyle::parse(&s.name))
            .unwrap_or_else(|| platform_family(vm));
        let mut sheet = StyleSheet::load_with_appearance(family, snapshot.dark);
        // Preserve host widget rules/icons. Always compile from a fresh base so
        // repeated delivery never multiplies font/spacing scales again.
        if let Some(existing) = existing {
            sheet.widgets = existing.widgets;
            sheet.icons = existing.icons;
        }
        append_makepad(&mut sheet.theme, &snapshot.tokens)?;
        desktop_style::install(vm, sheet);
        vm.cx_mut()
            .global::<Runtime>()
            .heaps
            .get_mut(&key)
            .unwrap()
            .host_revision = Some(snapshot.revision);
        Ok(true)
    })?;
    if changed {
        cx.request_style_reload();
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_hosts_reject_local_selection_and_receive_independently() {
        let mut first = Cx::new(Box::new(|_, _| {}));
        let mut second = Cx::new(Box::new(|_, _| {}));
        for cx in [&mut first, &mut second] {
            cx.with_vm(|vm| {
                tests_init(vm);
            });
        }
        let tokens = packages::resolve(
            &mut first,
            &octosense_theme_contract::ThemePackage::blank("Host"),
            Selection::default(),
            DesktopStyle::Macos,
        )
        .unwrap();
        let snapshot = ResolvedTheme::new("host-theme".into(), false, tokens);
        assert!(receive(&mut first, &snapshot.bytes().unwrap()).unwrap());
        assert!(!receive(&mut first, &snapshot.bytes().unwrap()).unwrap());
        assert!(receive(&mut second, &snapshot.bytes().unwrap()).unwrap());
        for cx in [&mut first, &mut second] {
            assert!(packages::current(cx).is_err());
            assert!(packages::apply(cx, packages::Preferences::default()).is_err());
            assert!(packages::preview(cx, packages::Preferences::default()).is_err());
        }
        let mut invalid = snapshot;
        invalid.tokens.insert(
            "typography.scale".into(),
            octosense_theme_contract::number(2.),
        );
        assert!(receive(&mut first, &serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    fn tests_init(vm: &mut ScriptVm) {
        super::super::tests::install(vm, Selection::default());
        init_hosted(vm);
    }
}
