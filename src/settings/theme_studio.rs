//! Appearance authoring and explicit import/preview/apply. Imported data is never script.
use makepad_widgets::*;
use octosense_theme_contract::{self as contract, ThemePackage};
use crate::theme::packages::{self, Preferences};
use ruma::{OwnedRoomId, OwnedUserId};

#[derive(Clone, Debug)]
pub enum ThemeStudioAction {
    Open,
    Close,
    Imported {
        owner: Option<OwnedUserId>,
        result: Result<ThemePackage, String>,
    },
    Shared {
        owner: OwnedUserId,
        result: Result<(), String>,
    },
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.ThemeStudio = #(ThemeStudio::register_widget(vm)) {
        ..mod.widgets.SolidView
        width: Fill{max: 920} height: Fill flow: Down padding: 20 spacing: 12
        show_bg: true draw_bg.color: RINX_PAGE
        header := View {width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 12
            RinxPageTitle {width: Fill text: #(crate::i18n::tr("Customize appearance")) i18n_text: "Customize appearance"}
            studio_close := RinxButton {text: #(crate::i18n::tr("Close")) i18n_text: "Close"}
        }
        actions := View {width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8
            theme_import := RinxButton {text: #(crate::i18n::tr("Import theme")) i18n_text: "Import theme"}
            theme_export := RinxButton {text: #(crate::i18n::tr("Export theme")) i18n_text: "Export theme"}
            theme_duplicate := RinxButton {text: #(crate::i18n::tr("Duplicate")) i18n_text: "Duplicate"}
            theme_undo := RinxButton {text: #(crate::i18n::tr("Undo theme change")) i18n_text: "Undo theme change"}
            theme_reset := RinxButton {text: #(crate::i18n::tr("Reset appearance")) i18n_text: "Reset appearance"}
        }
        content := ScrollYView {width: Fill height: Fill flow: Down spacing: 12
            RinxLabel {text: #(crate::i18n::tr("Theme name")) i18n_text: "Theme name"}
            theme_name := RinxInput {width: Fill}
            colors := View {width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 12
                View {width: 180 height: Fit flow: Down spacing: 6
                    RinxHint {text: #(crate::i18n::tr("Light accent")) i18n_text: "Light accent"}
                    light_accent := RinxInput {width: Fill}
                }
                View {width: 180 height: Fit flow: Down spacing: 6
                    RinxHint {text: #(crate::i18n::tr("Dark accent")) i18n_text: "Dark accent"}
                    dark_accent := RinxInput {width: Fill}
                }
                View {width: 140 height: Fit flow: Down spacing: 6
                    RinxHint {text: #(crate::i18n::tr("Text scale")) i18n_text: "Text scale"}
                    text_scale := RinxInput {width: Fill}
                }
                View {width: 140 height: Fit flow: Down spacing: 6
                    RinxHint {text: #(crate::i18n::tr("Corner radius")) i18n_text: "Corner radius"}
                    corner_radius := RinxInput {width: Fill}
                }
            }
            preview_modes := View {width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8
                preview_light := RinxButton {text: #(crate::i18n::tr("Light")) i18n_text: "Light"}
                preview_dark := RinxButton {text: #(crate::i18n::tr("Dark")) i18n_text: "Dark"}
            }
            basic_preview := RinxButton {text: #(crate::i18n::tr("Preview changes")) i18n_text: "Preview changes"}
            RinxHint {width: Fill text: #(crate::i18n::tr("Preview changes the open app temporarily. Apply keeps it; Cancel restores your previous appearance.")) i18n_text: "Preview changes the open app temporarily. Apply keeps it; Cancel restores your previous appearance."}
            preview_gallery := RoundedView {width: Fill height: Fit padding: 14 flow: Down spacing: 10 draw_bg +: {color: RINX_SURFACE border_radius: theme.corner_radius}
                RinxLabel {text: #(crate::i18n::tr("Chat, mini app, form and reader")) i18n_text: "Chat, mini app, form and reader"}
                RoundedView {width: Fill height: Fit padding: 10 draw_bg +: {color: RINX_OUTGOING border_radius: theme.corner_radius}
                    RinxLabel {width: Fill text: #(crate::i18n::tr("A message with readable text and balanced margins.")) i18n_text: "A message with readable text and balanced margins."}
                }
                View {width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8
                    RinxPrimaryButton {text: #(crate::i18n::tr("Open mini app")) i18n_text: "Open mini app"}
                    preview_input := RinxInput {width: 220 text: "中文 · Unsaved draft"}
                }
                Markdown {width: Fill height: Fit body: "## Reader\nShared **typography**, links and `code`."}
            }
            advanced_toggle := RinxButton {text: #(crate::i18n::tr("Edit theme JSON")) i18n_text: "Edit theme JSON"}
            advanced := View {visible: false width: Fill height: Fit flow: Down spacing: 8
                theme_json := RinxInput {width: Fill height: 260 is_multiline: true submit_on_enter: false}
                json_preview := RinxButton {text: #(crate::i18n::tr("Preview JSON")) i18n_text: "Preview JSON"}
            }
            share_section := View {width: Fill height: Fit flow: Down spacing: 8
                RinxHint {width: Fill text: #(crate::i18n::tr("Share a theme file in a conversation. Recipients choose whether to apply it.")) i18n_text: "Share a theme file in a conversation. Recipients choose whether to apply it."}
                share_rooms := DropDown {width: Fill height: RINX_CONTROL_HEIGHT labels: [#(crate::i18n::tr("Choose a conversation"))]}
                theme_share := RinxButton {text: #(crate::i18n::tr("Send theme file")) i18n_text: "Send theme file"}
            }
        }
        studio_status := RinxHint {width: Fill}
        footer := View {width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8
            theme_apply := RinxPrimaryButton {text: #(crate::i18n::tr("Apply theme")) i18n_text: "Apply theme"}
            theme_cancel := RinxButton {text: #(crate::i18n::tr("Cancel preview")) i18n_text: "Cancel preview"}
        }
    }
}

#[derive(Script, Widget)]
pub struct ThemeStudio {
    #[deref]
    view: View,
    #[rust]
    draft: Option<ThemePackage>,
    #[rust]
    active: bool,
    #[rust]
    advanced: bool,
    #[rust]
    rooms: Vec<OwnedRoomId>,
    #[rust]
    room_labels: Vec<String>,
    #[rust]
    owner: Option<OwnedUserId>,
    #[rust]
    sharing: bool,
    #[rust]
    status_text: String,
    #[rust]
    restore: bool,
}
impl ScriptHook for ThemeStudio {
    fn on_after_apply(
        &mut self,
        _vm: &mut ScriptVm,
        apply: &Apply,
        _scope: &mut Scope,
        _value: ScriptValue,
    ) {
        self.restore |= apply.is_script_reapply() && self.active;
    }
}
impl ThemeStudio {
    fn status(&mut self, cx: &mut Cx, text: impl Into<String>) {
        self.status_text = text.into();
        self.view
            .label(cx, ids!(studio_status))
            .set_text(cx, &self.status_text);
        self.view.redraw(cx);
    }
    fn populate(&mut self, cx: &mut Cx, package: ThemePackage) {
        self.view
            .text_input(cx, ids!(theme_name))
            .set_text(cx, &package.name);
        let selected = packages::current(cx).unwrap_or_default().selection;
        let family = packages::family(cx);
        let resolved_light = packages::resolve(
            cx,
            &package,
            crate::theme::Selection {
                appearance: crate::theme::Appearance::Light,
                ..selected
            },
            family,
        )
        .unwrap_or_default();
        let resolved_dark = packages::resolve(
            cx,
            &package,
            crate::theme::Selection {
                appearance: crate::theme::Appearance::Dark,
                ..selected
            },
            family,
        )
        .unwrap_or_default();
        for (tokens, id, default) in [
            (&resolved_light, id!(light_accent), "09616f"),
            (&resolved_dark, id!(dark_accent), "72d3df"),
        ] {
            let color = tokens
                .get("color.action.primary")
                .and_then(|t| contract::hex(t).ok())
                .unwrap_or(default.into());
            self.view
                .text_input(cx, &[id])
                .set_text(cx, &format!("#{color}"));
        }
        let tokens = resolved_light;
        let scale = tokens
            .get("typography.scale")
            .and_then(|t| t.value.as_f64())
            .unwrap_or(1.);
        let radius = tokens
            .get("shape.surface.radius")
            .and_then(|t| t.value["value"].as_f64())
            .unwrap_or(6.);
        self.view
            .text_input(cx, ids!(text_scale))
            .set_text(cx, &scale.to_string());
        self.view
            .text_input(cx, ids!(corner_radius))
            .set_text(cx, &radius.to_string());
        self.view.text_input(cx, ids!(theme_json)).set_text(
            cx,
            &String::from_utf8(package.bytes().unwrap_or_default()).unwrap_or_default(),
        );
        self.draft = Some(package);
    }
    fn edited(&self, cx: &mut Cx) -> Result<ThemePackage, String> {
        let mut p = self.draft.clone().ok_or("No theme is open")?;
        p.name = self.view.text_input(cx, ids!(theme_name)).text();
        for (dark, id) in [(false, id!(light_accent)), (true, id!(dark_accent))] {
            p.set(
                Some(dark),
                "color.action.primary",
                contract::color(&self.view.text_input(cx, &[id]).text())?,
            );
        }
        let scale = self
            .view
            .text_input(cx, ids!(text_scale))
            .text()
            .parse::<f64>()
            .map_err(|_| "Enter a text scale between 0.8 and 2")?;
        let radius = self
            .view
            .text_input(cx, ids!(corner_radius))
            .text()
            .parse::<f64>()
            .map_err(|_| "Enter a corner radius between 0 and 24")?;
        for dark in [false, true] {
            p.set(Some(dark), "typography.scale", contract::number(scale));
            p.set(
                Some(dark),
                "shape.surface.radius",
                contract::dimension(radius),
            );
        }
        p.validate_structure()?;
        Ok(p)
    }
    fn edited_package(&self, cx: &mut Cx) -> Result<ThemePackage, String> {
        if self.advanced {
            ThemePackage::parse(self.view.text_input(cx, ids!(theme_json)).text().as_bytes())
        } else {
            self.edited(cx)
        }
    }
    fn preview(&mut self, cx: &mut Cx, p: ThemePackage) -> Result<(), String> {
        let mut current = packages::current(cx)?;
        current.package = Some(p.clone());
        packages::preview(cx, current)?;
        self.draft = Some(p.clone());
        self.view
            .text_input(cx, ids!(theme_json))
            .set_text(cx, &String::from_utf8(p.bytes()?).unwrap());
        self.status(
            cx,
            crate::i18n::tr("Preview active — apply to keep this theme."),
        );
        Ok(())
    }
    fn share(&mut self, cx: &mut Cx) -> Result<(), String> {
        if self.sharing {
            return Err("A theme file is already being sent".into());
        }
        let owner = crate::sliding_sync::current_user_id().ok_or("Sign in to share a theme")?;
        if self.owner.as_ref() != Some(&owner) {
            return Err("The signed-in account changed; reopen appearance".into());
        }
        let index = self.view.drop_down(cx, ids!(share_rooms)).selected_item();
        let room_id = self
            .rooms
            .get(index.checked_sub(1).ok_or("Choose a conversation")?)
            .cloned()
            .ok_or("Choose a conversation")?;
        let p = self.edited_package(cx)?;
        let bytes = p.bytes()?;
        let filename = format!("{}.{}", p.id, contract::EXTENSION);
        let client = crate::sliding_sync::get_client().ok_or("Sign in to share a theme")?;
        let room = client
            .get_room(&room_id)
            .ok_or("Conversation is unavailable")?;
        self.sharing = true;
        self.status(cx, crate::i18n::tr("Sending theme file…"));
        crate::sliding_sync::spawn_async_task(async move {
            if crate::sliding_sync::current_user_id().as_ref() != Some(&owner)
                || crate::logout::logout_state_machine::is_logout_in_progress()
            {
                return;
            }
            let mime = contract::MIME.parse().unwrap();
            let result = room
                .send_attachment(
                    filename,
                    &mime,
                    bytes,
                    matrix_sdk::attachment::AttachmentConfig::new(),
                )
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            Cx::post_action(ThemeStudioAction::Shared { owner, result });
        });
        Ok(())
    }
}
impl Widget for ThemeStudio {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.active {
            return;
        }
        if self.restore {
            self.restore = false;
            self.view
                .view(cx, ids!(advanced))
                .set_visible(cx, self.advanced);
            let rooms = self.view.drop_down(cx, ids!(share_rooms));
            let selected = rooms.selected_item();
            rooms.set_labels(cx, self.room_labels.clone());
            rooms.set_selected_item(cx, selected);
            self.view
                .label(cx, ids!(studio_status))
                .set_text(cx, &self.status_text);
        }
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let result = (|| -> Result<(), String> {
                if self.view.button(cx, ids!(studio_close)).clicked(actions) {
                    cx.action(ThemeStudioAction::Close);
                }
                if self.view.button(cx, ids!(theme_import)).clicked(actions) {
                    import_file()?;
                }
                if self.view.button(cx, ids!(theme_duplicate)).clicked(actions) {
                    let mut p = self.edited(cx)?;
                    p.id = format!("theme-{}", uuid::Uuid::new_v4());
                    p.name = format!("{} copy", p.name);
                    self.populate(cx, p);
                }
                for (id, appearance) in [
                    (id!(preview_light), crate::theme::Appearance::Light),
                    (id!(preview_dark), crate::theme::Appearance::Dark),
                ] {
                    if self.view.button(cx, &[id]).clicked(actions) {
                        let p = self.edited(cx)?;
                        let mut pref = packages::current(cx)?;
                        pref.selection.appearance = appearance;
                        pref.follow_system = false;
                        pref.package = Some(p.clone());
                        packages::preview(cx, pref)?;
                        self.draft = Some(p);
                        self.status(
                            cx,
                            crate::i18n::tr("Preview active — apply to keep this theme."),
                        );
                    }
                }
                if self.view.button(cx, ids!(basic_preview)).clicked(actions) {
                    let p = self.edited(cx)?;
                    self.preview(cx, p)?;
                }
                if self.view.button(cx, ids!(advanced_toggle)).clicked(actions) {
                    if self.advanced {
                        let p = self.edited_package(cx)?;
                        self.populate(cx, p);
                    } else {
                        let p = self.edited(cx)?;
                        self.populate(cx, p);
                    }
                    self.advanced = !self.advanced;
                    self.view
                        .view(cx, ids!(advanced))
                        .set_visible(cx, self.advanced);
                }
                if self.view.button(cx, ids!(json_preview)).clicked(actions) {
                    let p = ThemePackage::parse(
                        self.view.text_input(cx, ids!(theme_json)).text().as_bytes(),
                    )?;
                    self.preview(cx, p.clone())?;
                    self.populate(cx, p);
                }
                if self.view.button(cx, ids!(theme_apply)).clicked(actions) {
                    let p = self.edited_package(cx)?;
                    let mut pref = packages::current(cx)?;
                    pref.package = Some(p);
                    packages::apply(cx, pref)?;
                    self.status(cx, crate::i18n::tr("Theme applied"));
                }
                if self.view.button(cx, ids!(theme_cancel)).clicked(actions) {
                    packages::cancel(cx)?;
                    self.status(cx, crate::i18n::tr("Preview cancelled"));
                }
                if self.view.button(cx, ids!(theme_undo)).clicked(actions) {
                    packages::undo(cx)?;
                    let p = packages::export_current(cx)?;
                    self.populate(cx, p);
                    self.status(cx, crate::i18n::tr("Previous appearance restored"));
                }
                if self.view.button(cx, ids!(theme_reset)).clicked(actions) {
                    packages::apply(cx, Preferences::default())?;
                    let p = packages::export_current(cx)?;
                    self.populate(cx, p);
                    self.status(cx, crate::i18n::tr("Appearance reset"));
                }
                if self.view.button(cx, ids!(theme_export)).clicked(actions) {
                    let p = self.edited_package(cx)?;
                    crate::shared::attachment_download::save_loaded_attachment(
                        format!("{}.{}", p.id, contract::EXTENSION),
                        p.bytes()?.into(),
                    );
                }
                if self.view.button(cx, ids!(theme_share)).clicked(actions) {
                    self.share(cx)?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.status(cx, error)
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
impl ThemeStudioRef {
    pub fn action(&self, cx: &mut Cx, modal: ModalRef, action: &ThemeStudioAction) {
        let Some(mut s) = self.borrow_mut() else {
            return;
        };
        match action {
            ThemeStudioAction::Close => {
                if packages::is_preview(cx) {
                    let _ = packages::cancel(cx);
                }
                s.active = false;
                modal.close(cx);
            }
            ThemeStudioAction::Shared { owner, result } => {
                if s.owner.as_ref() == Some(owner)
                    && crate::sliding_sync::current_user_id().as_ref() == Some(owner)
                {
                    s.sharing = false;
                    s.status(
                        cx,
                        result
                            .as_ref()
                            .map(|_| crate::i18n::tr("Theme file sent").to_owned())
                            .unwrap_or_else(|e| e.clone()),
                    );
                }
            }
            ThemeStudioAction::Open | ThemeStudioAction::Imported { .. } => {
                if let ThemeStudioAction::Imported { owner, .. } = action {
                    if owner != &crate::sliding_sync::current_user_id() {
                        return;
                    }
                }
                let _current = match packages::current(cx) {
                    Ok(p) => p,
                    Err(error) => {
                        crate::shared::popup_list::enqueue_popup_notification(
                            error,
                            crate::shared::popup_list::PopupKind::Info,
                            None,
                        );
                        return;
                    }
                };
                s.active = true;
                s.owner = crate::sliding_sync::current_user_id();
                s.rooms.clear();
                s.room_labels = vec![crate::i18n::tr("Choose a conversation").into()];
                let mut rooms: Vec<_> = crate::sliding_sync::get_client()
                    .map(|c| {
                        c.joined_rooms()
                            .into_iter()
                            .filter(|r| !r.is_space())
                            .map(|r| {
                                (
                                    r.cached_display_name()
                                        .map(|n| n.to_string())
                                        .or_else(|| r.name())
                                        .unwrap_or_else(|| r.room_id().to_string()),
                                    r.room_id().to_owned(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                rooms.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, id) in rooms {
                    s.room_labels.push(name);
                    s.rooms.push(id);
                }
                s.view
                    .drop_down(cx, ids!(share_rooms))
                    .set_labels(cx, s.room_labels.clone());
                s.view
                    .drop_down(cx, ids!(share_rooms))
                    .set_selected_item(cx, 0);
                let package = match action {
                    ThemeStudioAction::Imported { result: Ok(p), .. } => p.clone(),
                    _ => packages::export_current(cx)
                        .unwrap_or_else(|_| ThemePackage::blank("Custom theme")),
                };
                s.populate(cx, package);
                s.status(
                    cx,
                    match action {
                        ThemeStudioAction::Imported { result: Err(e), .. } => e.clone(),
                        ThemeStudioAction::Imported { .. } => {
                            crate::i18n::tr("Theme imported. Preview it before applying.").into()
                        }
                        _ => String::new(),
                    },
                );
                modal.open(cx);
            }
        }
    }
}
pub fn import_file() -> Result<(), String> {
    let owner = crate::sliding_sync::current_user_id();
    robius_file_picker::FileDialog::new()
        .pick_file(move |picked| {
            let file = match picked {
                Ok(Some(file)) => file,
                Ok(None) => return,
                Err(e) => {
                    Cx::post_action(ThemeStudioAction::Imported {
                        owner,
                        result: Err(e.to_string()),
                    });
                    return;
                }
            };
            std::thread::spawn(move || {
                use std::io::Read;
                let result = (|| {
                    let file = file.into_local_file().map_err(|e| e.to_string())?;
                    let mut data = Vec::new();
                    std::fs::File::open(file.path())
                        .map_err(|e| e.to_string())?
                        .take((contract::MAX_BYTES + 1) as u64)
                        .read_to_end(&mut data)
                        .map_err(|e| e.to_string())?;
                    ThemePackage::parse(&data)
                })();
                Cx::post_action(ThemeStudioAction::Imported { owner, result });
            });
        })
        .map_err(|e| e.to_string())
}
