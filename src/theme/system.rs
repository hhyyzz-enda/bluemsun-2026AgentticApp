//! System appearance is separate from OctoSense's full stylesheet authority.
use super::*;
use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};
static DARK: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();
#[derive(Clone, Debug)]
pub struct SystemAppearanceChanged(pub Appearance);
pub fn supported() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    ))
}
pub fn appearance() -> Appearance {
    if DARK.load(Ordering::Relaxed) {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}
pub fn start() {
    START.call_once(|| {
        #[cfg(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", not(target_env = "ohos"))
        ))]
        {
            DARK.store(
                matches!(dark_light::detect(), Ok(dark_light::Mode::Dark)),
                Ordering::Relaxed,
            );
            std::thread::spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if let Ok(mode) = dark_light::detect() {
                        if matches!(mode, dark_light::Mode::Unspecified) {
                            continue;
                        }
                        let dark = matches!(mode, dark_light::Mode::Dark);
                        if DARK.swap(dark, Ordering::Relaxed) != dark {
                            Cx::post_action(SystemAppearanceChanged(if dark {
                                Appearance::Dark
                            } else {
                                Appearance::Light
                            }));
                        }
                    }
                }
            });
        }
    });
}
/// Native adapters and hosts can deliver appearance changes without a renderer fork.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    #[cfg(target_os = "android")]
    if matches!(event, Event::Startup | Event::Foreground) {
        cx.android_integration("rinx.appearance", "");
    }
    let mut changed = None;
    if let Event::Actions(actions) = event {
        for action in actions {
            if let Some(value) = action.downcast_ref::<SystemAppearanceChanged>() {
                changed = Some(value.0);
            }
        }
    }
    #[cfg(target_os = "android")]
    if let Event::AndroidIntegration { channel, payload } = event {
        if channel == "rinx.appearance" {
            changed = match payload.as_str() {
                "dark" => Some(Appearance::Dark),
                "light" => Some(Appearance::Light),
                _ => None,
            };
        }
    }
    if let Some(appearance) = changed {
        DARK.store(appearance == Appearance::Dark, Ordering::Relaxed);
        if let Ok(pref) = packages::current(cx) {
            if pref.follow_system {
                let family = packages::family(cx);
                if let Ok(sheet) = packages::stylesheet(cx, &pref, family) {
                    let preview = packages::is_preview(cx);
                    packages::install(cx, sheet, pref, preview);
                }
            }
        }
    }
}
