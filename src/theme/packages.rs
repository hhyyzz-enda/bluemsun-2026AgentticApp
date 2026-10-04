//! Customer appearance transactions. Preview never writes the startup selection.
use super::*;
use octosense_theme_contract::{self as contract, ThemePackage, Token, Tokens};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub selection: Selection,
    #[serde(default)]
    pub package: Option<ThemePackage>,
    #[serde(default)]
    pub follow_system: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stored {
    pub current: Preferences,
    #[serde(default)]
    pub previous: Option<Preferences>,
}

pub fn load(cx: &mut Cx, dir: &Path, family: DesktopStyle) -> Stored {
    // An interrupted first render must never trap startup in the new theme.
    if dir.join("theme-rollback.json").exists() {
        let Ok(bytes) = read_bounded(&dir.join("theme-rollback.json")) else {
            return Stored::default();
        };
        if bytes.len() <= contract::MAX_BYTES * 3 {
            if let Ok(old) = serde_json::from_slice::<Stored>(&bytes) {
                if stylesheet(cx, &old.current, family).is_ok() {
                    if save(dir, &old).is_ok() {
                        let _ = std::fs::remove_file(dir.join("theme-rollback.json"));
                    }
                    return old;
                }
            }
        }
        return Stored::default();
    }
    let stored = read_bounded(&dir.join("theme-state.json"))
        .ok()
        .filter(|b| b.len() <= contract::MAX_BYTES * 3)
        .and_then(|b| serde_json::from_slice::<Stored>(&b).ok());
    if let Some(mut stored) = stored {
        if stylesheet(cx, &stored.current, family).is_ok() {
            return stored;
        }
        if let Some(previous) = stored.previous.take() {
            if stylesheet(cx, &previous, family).is_ok() {
                return Stored {
                    current: previous,
                    previous: None,
                };
            }
        }
        return Stored::default();
    }
    let selection = read_bounded(&dir.join("appearance.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    Stored {
        current: Preferences {
            selection,
            package: None,
            follow_system: false,
        },
        previous: None,
    }
}
fn read_bounded(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((contract::MAX_BYTES * 3 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > contract::MAX_BYTES * 3 {
        return Err(std::io::Error::other("Theme store exceeds size limit"));
    }
    Ok(bytes)
}
fn atomic_write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let pending = dir.join(format!("{name}.pending"));
    let mut file = std::fs::File::create(&pending).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&pending, dir.join(name)).map_err(|e| e.to_string())?;
    Ok(())
}
fn save(dir: &Path, stored: &Stored) -> Result<(), String> {
    atomic_write(
        dir,
        "theme-state.json",
        &serde_json::to_vec(stored).map_err(|e| e.to_string())?,
    )?;
    // Compatibility with versions predating theme packages.
    let _ = atomic_write(
        dir,
        "appearance.json",
        &serde_json::to_vec(&stored.current.selection).unwrap(),
    );
    Ok(())
}
pub fn current(cx: &mut Cx) -> Result<Preferences, String> {
    cx.with_vm(current_for_vm)
}
pub fn current_for_vm(vm: &mut ScriptVm) -> Result<Preferences, String> {
    let key = vm.bx.heap.heap_key();
    let state = vm
        .cx_mut()
        .global::<Runtime>()
        .heaps
        .get(&key)
        .ok_or("Appearance is unavailable")?;
    if state.hosted {
        return Err("Appearance is managed by OctoSense".into());
    }
    Ok(state.preview.as_ref().unwrap_or(&state.preferences).clone())
}
pub fn is_preview(cx: &mut Cx) -> bool {
    cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        vm.cx_mut()
            .global::<Runtime>()
            .heaps
            .get(&key)
            .is_some_and(|s| s.preview.is_some())
    })
}
pub(crate) fn family(cx: &mut Cx) -> DesktopStyle {
    cx.with_vm(|vm| {
        desktop_style::current(vm)
            .and_then(|s| DesktopStyle::parse(&s.name))
            .filter(|f| f.supports_dark())
            .unwrap_or_else(|| platform_family(vm))
    })
}
pub fn preview(cx: &mut Cx, preferences: Preferences) -> Result<(), String> {
    current(cx)?;
    let family = family(cx);
    let sheet = stylesheet(cx, &preferences, family)?;
    install(cx, sheet, preferences, true);
    Ok(())
}
pub fn cancel(cx: &mut Cx) -> Result<(), String> {
    current(cx)?;
    let preferences = cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        vm.cx_mut().global::<Runtime>().heaps[&key]
            .preferences
            .clone()
    });
    let family = family(cx);
    let sheet = stylesheet(cx, &preferences, family)?;
    install(cx, sheet, preferences, false);
    Ok(())
}
pub fn apply(cx: &mut Cx, preferences: Preferences) -> Result<(), String> {
    current(cx)?;
    let family = family(cx);
    let sheet = stylesheet(cx, &preferences, family)?;
    let old = cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        vm.cx_mut().global::<Runtime>().heaps[&key]
            .preferences
            .clone()
    });
    let known_good = cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        vm.cx_mut().global::<Runtime>().heaps[&key]
            .known_good
            .clone()
    });
    atomic_write(
        &crate::app_data_dir(),
        "theme-rollback.json",
        &serde_json::to_vec(&Stored {
            current: known_good,
            previous: None,
        })
        .unwrap(),
    )?;
    save(
        &crate::app_data_dir(),
        &Stored {
            current: preferences.clone(),
            previous: Some(old.clone()),
        },
    )?;
    cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        vm.cx_mut()
            .global::<Runtime>()
            .heaps
            .get_mut(&key)
            .unwrap()
            .previous = Some(old);
    });
    install(cx, sheet, preferences, false);
    cx.with_vm(|vm| {
        let key = vm.bx.heap.heap_key();
        let s = vm.cx_mut().global::<Runtime>().heaps.get_mut(&key).unwrap();
        s.transaction += 1;
        s.pending_apply = Some(s.transaction);
        s.pending_ack = false;
    });
    Ok(())
}

#[derive(Clone, Debug)]
struct Rendered {
    heap: usize,
    transaction: u64,
}
/// Acknowledge only the current transaction, after reapply and a UI draw.
/// File cleanup happens on Actions, never in drawing.
pub fn after_event(cx: &mut Cx, event: &Event) {
    if matches!(event, Event::Draw(_)) {
        let ack = cx.with_vm(|vm| {
            let key = vm.bx.heap.heap_key();
            let state = vm.cx_mut().global::<Runtime>().heaps.get_mut(&key)?;
            let transaction = state.pending_apply?;
            if state.pending_ack || state.preview.is_some() {
                return None;
            }
            state.pending_ack = true;
            Some(Rendered {
                heap: key,
                transaction,
            })
        });
        if let Some(ack) = ack {
            cx.action(ack);
        }
    }
    if let Event::Actions(actions) = event {
        for action in actions {
            if let Some(ack) = action.downcast_ref::<Rendered>() {
                let commit = cx.with_vm(|vm| {
                    let key = vm.bx.heap.heap_key();
                    if key != ack.heap {
                        return false;
                    }
                    let Some(state) = vm.cx_mut().global::<Runtime>().heaps.get_mut(&key) else {
                        return false;
                    };
                    if state.pending_apply != Some(ack.transaction) {
                        return false;
                    }
                    state.pending_apply = None;
                    state.pending_ack = false;
                    state.known_good = state.preferences.clone();
                    true
                });
                if commit {
                    let _ = std::fs::remove_file(crate::app_data_dir().join("theme-rollback.json"));
                }
            }
        }
    }
}

pub fn export_current(cx: &mut Cx) -> Result<ThemePackage, String> {
    let pref = current(cx)?;
    if let Some(package) = pref.package {
        return Ok(package);
    }
    let family = family(cx);
    let mut package = ThemePackage::blank("Custom theme");
    package.id = format!("theme-{}", uuid::Uuid::new_v4());
    for appearance in [Appearance::Light, Appearance::Dark] {
        let selection = Selection {
            appearance,
            ..pref.selection
        };
        let tokens = resolve(cx, &ThemePackage::blank("Base"), selection, family)?;
        for (name, token) in tokens {
            package.set(Some(appearance == Appearance::Dark), &name, token);
        }
    }
    Ok(package)
}
pub fn undo(cx: &mut Cx) -> Result<(), String> {
    current(cx)?;
    let previous = cx
        .with_vm(|vm| {
            let key = vm.bx.heap.heap_key();
            vm.cx_mut().global::<Runtime>().heaps[&key].previous.clone()
        })
        .ok_or("No previous theme to restore")?;
    apply(cx, previous)
}
pub(super) fn install(cx: &mut Cx, sheet: StyleSheet, preferences: Preferences, preview: bool) {
    cx.with_vm(|vm| {
        desktop_style::install(vm, sheet);
        let key = vm.bx.heap.heap_key();
        let state = vm.cx_mut().global::<Runtime>().heaps.get_mut(&key).unwrap();
        state.selection = preferences.selection;
        if preview {
            state.preview = Some(preferences)
        } else {
            state.preview = None;
            state.preferences = preferences
        }
    });
    cx.request_style_reload();
}

pub fn base_tokens(s: &Snapshot) -> Tokens {
    let mut t = Tokens::new();
    for (name, c) in [
        ("color.surface.page", s.page),
        ("color.surface.panel", s.surface),
        ("color.surface.field", s.field),
        ("color.content.primary", s.ink),
        ("color.content.secondary", s.muted),
        ("color.content.disabled", mix(s.ink, s.page, 0.55)),
        ("color.action.primary", s.accent),
        ("color.action.on_primary", s.on_accent),
        ("color.border.default", s.border),
        ("color.state.hover", s.hover),
        ("color.state.pressed", s.pressed),
        ("color.state.selected", s.selected),
        ("color.chat.incoming", s.surface),
        ("color.chat.outgoing", s.selected),
        ("color.chat.mention", s.selected),
        ("color.code.background", s.field),
        ("color.code.foreground", s.ink),
    ] {
        t.insert(
            name.into(),
            contract::color(&format!("{:06x}", argb(c) & 0xffffff)).unwrap(),
        );
    }
    let dark = luminance(s.page) < 0.5;
    for (state, fg, bg) in if dark {
        [
            ("success", 0x86efac, 0x163522),
            ("warning", 0xfde68a, 0x3a2c12),
            ("danger", 0xfca5a5, 0x401c22),
            ("info", 0x93c5fd, 0x182c46),
        ]
    } else {
        [
            ("success", 0x166534, 0xf0fdf4),
            ("warning", 0x854d0e, 0xfffbeb),
            ("danger", 0x991b1b, 0xfef2f2),
            ("info", 0x1e40af, 0xeff6ff),
        ]
    } {
        t.insert(
            format!("color.status.{state}.foreground"),
            contract::color(&format!("{fg:06x}")).unwrap(),
        );
        t.insert(
            format!("color.status.{state}.background"),
            contract::color(&format!("{bg:06x}")).unwrap(),
        );
    }
    for (key, n) in [
        ("shape.surface.radius", s.radius),
        ("metrics.control.height", 44.),
        ("metrics.reading.width", 760.),
        ("metrics.page.gutter", 16.),
    ] {
        t.insert(key.into(), contract::dimension(n));
    }
    for key in ["typography.scale", "metrics.spacing"] {
        t.insert(key.into(), contract::number(1.));
    }
    t.insert(
        "typography.family".into(),
        Token {
            kind: "fontFamily".into(),
            value: serde_json::json!("system"),
            description: String::new(),
        },
    );
    t.insert(
        "motion.duration".into(),
        Token {
            kind: "duration".into(),
            value: serde_json::json!({"value":150,"unit":"ms"}),
            description: String::new(),
        },
    );
    t
}
/// Evaluate the actual framework base in a disposable VM; never execute imported text.
pub fn resolve(
    cx: &mut Cx,
    package: &ThemePackage,
    selection: Selection,
    family: DesktopStyle,
) -> Result<Tokens, String> {
    // A second Cx replaces Makepad's process-wide action sender, stranding
    // async image decodes (and Matrix actions) when that context is dropped.
    // Evaluate the trusted base in a disposable VM on the existing UI context.
    let icons = cx.with_vm(|vm| desktop_style::current(vm).map(|sheet| sheet.icons));
    let isolate = cx.alloc_splash_vm();
    let result = cx.with_script_vm_id_trusted(isolate, |vm| {
        makepad_widgets::makepad_draw::makepad_platform::script::script_mod(vm);
        let mut sheet = selection.stylesheet(family);
        if let Some(icons) = icons { sheet.icons = icons; }
        desktop_style::install(vm, sheet);
        makepad_widgets::script_mod(vm);
        package.resolve(
            selection.appearance == Appearance::Dark,
            &base_tokens(&snapshot_for_vm(vm)),
        )
    });
    cx.free_splash_vm(isolate);
    result
}
pub fn stylesheet(cx: &mut Cx, preferences: &Preferences, family: DesktopStyle) -> Result<StyleSheet, String> {
    let mut selection = preferences.selection;
    if preferences.follow_system {
        selection.appearance = super::system::appearance();
    }
    let mut sheet = selection.stylesheet(family);
    let Some(package) = &preferences.package else {
        return Ok(sheet);
    };
    package.validate_structure()?;
    // Validate BOTH variants before preview/apply, including currently inactive values.
    let mut active = None;
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tokens = resolve(
            cx,
            package,
            Selection {
                appearance,
                ..selection
            },
            family,
        )?;
        if appearance == selection.appearance {
            active = Some(tokens)
        }
    }
    let tokens = active.unwrap();
    contract::makepad::append_makepad(&mut sheet.theme, &tokens)?;
    Ok(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_apply_recovers_last_rendered_theme() {
        let path =
            std::env::temp_dir().join(format!("rinx-theme-recovery-{}", uuid::Uuid::new_v4()));
        let old = Stored::default();
        let mut changed = Preferences::default();
        changed.selection.appearance = Appearance::Dark;
        save(
            &path,
            &Stored {
                current: changed,
                previous: None,
            },
        )
        .unwrap();
        atomic_write(
            &path,
            "theme-rollback.json",
            &serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        assert_eq!(load(&mut cx, &path, DesktopStyle::Macos).current, old.current);
        assert!(!path.join("theme-rollback.json").exists());
        assert_eq!(load(&mut cx, &path, DesktopStyle::Macos).current, old.current);
        atomic_write(&path, "theme-state.json", b"truncated json").unwrap();
        assert_eq!(
            load(&mut cx, &path, DesktopStyle::Macos).current,
            Preferences::default()
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn package_compiles_against_each_platform_family_and_preserves_typography() {
        let mut package = ThemePackage::parse(include_bytes!(
            "../../examples/themes/ocean-violet.octotheme"
        ))
        .unwrap();
        package.set(
            None,
            "typography.family",
            Token {
                kind: "fontFamily".into(),
                value: serde_json::json!("mono"),
                description: String::new(),
            },
        );
        for family in [
            DesktopStyle::Macos,
            DesktopStyle::Windows,
            DesktopStyle::Android,
            DesktopStyle::Ios,
        ] {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let preferences = Preferences {
                    package: Some(package.clone()),
                    selection: Selection {
                        appearance,
                        ..Default::default()
                    },
                    follow_system: false,
                };
                let mut cx = Cx::new(Box::new(|_, _| {}));
                let sheet = stylesheet(&mut cx, &preferences, family).unwrap();
                cx.with_vm(|vm| {
                    desktop_style::install(vm, sheet);
                    vm.bx.captured_errors = Some(Vec::new());
                    makepad_widgets::script_mod(vm);
                    super::super::script_mod(vm);
                    assert!(vm.take_errors().is_empty());
                    let snapshot = snapshot_for_vm(vm);
                    assert_eq!(snapshot.radius, 10.);
                    assert_eq!(snapshot.text_scale, 1.15);
                    assert_eq!(snapshot.tokens["typography.family"].value, "mono");
                });
            }
        }
    }
    #[test]
    fn custom_variants_validate_without_changing_document_or_persisting_preview() {
        let mut p = ThemePackage::blank("Ocean");
        p.set(
            Some(false),
            "color.action.primary",
            contract::color("4f46a5").unwrap(),
        );
        p.set(
            Some(true),
            "color.action.primary",
            contract::color("c4b5fd").unwrap(),
        );
        let pref = Preferences {
            package: Some(p),
            ..Default::default()
        };
        let mut cx = Cx::new(Box::new(|_, _| {}));
        stylesheet(&mut cx, &pref, DesktopStyle::Macos).unwrap();
        let path = std::env::temp_dir().join(format!("rinx-theme-test-{}", uuid::Uuid::new_v4()));
        save(
            &path,
            &Stored {
                current: pref.clone(),
                previous: Some(Preferences::default()),
            },
        )
        .unwrap();
        assert_eq!(load(&mut cx, &path, DesktopStyle::Macos).current, pref);
        let mut bad = pref.clone();
        bad.package.as_mut().unwrap().schema_version = 99;
        save(
            &path,
            &Stored {
                current: bad,
                previous: Some(pref.clone()),
            },
        )
        .unwrap();
        assert_eq!(load(&mut cx, &path, DesktopStyle::Macos).current, pref);
        std::fs::remove_dir_all(path).unwrap();
    }
}
