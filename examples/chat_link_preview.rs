//! Offline link-card fixture. All metadata is local; no Matrix login or HTTP requests.
use makepad_widgets::*;
use rinx::home::link_preview::{LinkPreviewCache, LinkPreviewData, LinkPreviewWidgetRefExt};
use rinx::media_cache::{MediaCache, MediaCacheEntry};
use rinx::mini_app::{MiniAppCardWidgetRefExt, SharedMiniApp, WebMiniApp};
use rinx::sliding_sync::TimelineKind;
use rinx::shared::html_or_plaintext::HtmlOrPlaintextWidgetRefExt;
use rinx::shared::avatar::AvatarWidgetRefExt;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: vec2(1200, 900)
                window.title: "Rinx · offline link preview"
                body +: {
                    flow: Down padding: 24 spacing: 12
                    Label {text: "Link preview fixture"}
                    avatar_samples := View {
                        width: Fill height: Fit flow: Right spacing: 12
                        alice := Avatar {width: 40 height: 40}
                        bob := Avatar {width: 40 height: 40}
                        carol := MobileAvatar {width: 40 height: 40}
                        dave := MobileAvatar {width: 40 height: 40}
                        erin := Avatar {width: 40 height: 40}
                        frank := Avatar {width: 40 height: 40}
                        alice_profile := MobileAvatar {width: 40 height: 40}
                    }
                    group_samples := View {
                        visible: false width: Fill height: Fit flow: Right spacing: 18
                        group_two := Avatar {width: 36 height: 36}
                        group_three := Avatar {width: 36 height: 36}
                        group_four := MobileAvatar {width: 48 height: 48}
                        group_nine := Avatar {width: 96 height: 96}
                        recycled_group := MobileAvatar {width: 48 height: 48}
                    }
                    card := LinkPreview {width: 320}
                    after := Label {text: "After the cards"}
                    controls := View {
                        width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 8
                        replace := Button {text: "Replace links"}
                        narrow := Button {text: "Narrow card"}
                        empty := Button {text: "Empty preview"}
                        clear := Button {text: "Clear links"}
                        layouts := Button {text: "Message layouts"}
                        inspect := Button {text: "Inspect layout"}
                        fonts := Button {text: "Preview fonts"}
                        scale := Button {text: "Larger theme"}
                        article := Button {text: "Article card"}
                        recycle := Button {text: "Recycle article"}
                        groups := Button {text: "Group avatars"}
                        group_update := Button {text: "Update members"}
                        group_single := Button {text: "Reuse for a user"}
                        group_photo := Button {text: "Reuse for a room photo"}
                    }
                    result := Label {width: Fill height: Fit text: "Ready"}
                    font_cases := View {
                        visible: false width: Fill height: Fit flow: Right spacing: 20
                        font_desktop := Message {width: 500}
                        font_mobile := MobileMessage {width: 360}
                    }
                    article_card := MiniAppCard {width: 320}
                    layout_cases := View {
                        visible: false width: Fill height: Fit flow: Down spacing: 12
                        desktop := Message {}
                        compact := CondensedMessage {}
                        own := MobileOwnMessage {}
                        narrow_row := View {
                            width: 360 height: Fit
                            mobile := MobileMessage {}
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live] ui: WidgetRef,
}

impl App {
    fn groups(&self, cx: &mut Cx, update: bool) {
        use rinx::room::{FetchedRoomAvatar, RoomAvatarMember};
        rinx::avatar_cache::enqueue_avatar_update(rinx::avatar_cache::AvatarUpdate {
            mxc_uri: "mxc://example.org/member-photo".into(),
            avatar_data: Ok(std::sync::Arc::from(include_bytes!("../resources/icon_64.png").as_slice())),
        });
        rinx::avatar_cache::enqueue_avatar_update(rinx::avatar_cache::AvatarUpdate {
            mxc_uri: "mxc://example.org/broken-photo".into(),
            avatar_data: Ok(std::sync::Arc::from(b"invalid image".as_slice())),
        });
        let members: Vec<_> = ["Alice", "Bob", "Carol", "Dave", "Erin", "Frank", "Grace", "Helen", "Ian"]
            .into_iter().enumerate().map(|(index, name)| RoomAvatarMember {
                user_id: format!("@{}:example.org", name.to_lowercase()).try_into().unwrap(),
                display_name: Some(name.into()),
                avatar_url: match index {
                    0 => Some("mxc://example.org/member-photo".into()),
                    1 => Some("mxc://example.org/broken-photo".into()),
                    _ => None,
                },
            }).collect();
        self.ui.view(cx, ids!(group_samples)).set_visible(cx, true);
        self.ui.view(cx, ids!(font_cases)).set_visible(cx, false);
        self.ui.view(cx, ids!(layout_cases)).set_visible(cx, false);
        if update {
            self.ui.avatar(cx, ids!(recycled_group)).show_room_avatar(cx, &FetchedRoomAvatar::Members(members[7..].to_vec()));
        } else {
            for (path, count) in [(ids!(group_two), 2), (ids!(group_three), 3), (ids!(group_four), 4), (ids!(group_nine), 9), (ids!(recycled_group), 9)] {
                self.ui.avatar(cx, path).show_room_avatar(cx, &FetchedRoomAvatar::Members(members[..count].to_vec()));
            }
        }
        self.ui.redraw(cx);
    }

    fn article(&self, cx: &mut Cx, recycle: bool) {
        let timeline = TimelineKind::MainRoom {room_id: "!fixture:example.org".try_into().unwrap()};
        let app = if recycle {
            SharedMiniApp::Web(WebMiniApp::new("Replacement mini app", "https://example.org/app").unwrap())
        } else {
            SharedMiniApp::PublishedArticle {
                title: "Blog with a cover 中文".into(),
                summary: "A brief introduction saved by the author, shown before opening the full article.".into(),
                cover: Some((article_core::document::Cover {
                    asset: "fixture".into(), focal_x: 500, focal_y: 500, show_in_article: false,
                }, ruma::events::room::MediaSource::Plain("mxc://example.org/cover".into()))),
                room: timeline.room_id().clone(), event: "$article:example.org".try_into().unwrap(),
            }
        };
        self.ui.mini_app_card(cx, ids!(article_card)).set_app(cx, Some(app), &timeline, |_| {
            MediaCacheEntry::Loaded(std::sync::Arc::from(include_bytes!("../resources/icon_64.png").as_slice()))
        });
        self.ui.redraw(cx);
    }

    fn populate_fonts(&self, cx: &mut Cx) {
        self.ui.link_preview(cx, ids!(card)).clear(cx);
        self.ui.view(cx, ids!(layout_cases)).set_visible(cx, false);
        self.ui.view(cx, ids!(font_cases)).set_visible(cx, true);
        let link: url::Url = "https://example.org/fonts".parse().unwrap();
        let mut cache = LinkPreviewCache::new(None);
        cache.insert(&link, LinkPreviewData {
            title: Some("Preview title 中文".into()),
            description: Some("Preview body 中文".into()),
            ..Default::default()
        });
        for path in [ids!(font_desktop), ids!(font_mobile)] {
            let row = self.ui.widget(cx, path);
            row.html_or_plaintext(cx, ids!(content.message)).show_plaintext(cx, "Chat body 中文");
            row.label(cx, ids!(username)).set_text(cx, "Sender");
            row.view(cx, ids!(replied_to_message)).set_visible(cx, false);
            row.link_preview(cx, ids!(content.link_preview_view)).populate_below_message(
                cx, std::slice::from_ref(&link), &mut MediaCache::new(None), &mut cache, &|_, _, _, _, _, _| true,
            );
        }
        self.ui.redraw(cx);
    }

    fn populate(&self, cx: &mut Cx, replacement: bool) {
        let mut cache = LinkPreviewCache::new(None);
        let mut media = MediaCache::new(None);
        let links: Vec<url::Url> = if replacement {
            vec!["https://example.org/replacement".parse().unwrap()]
        } else {
            (1..=3).map(|i| format!("https://example.org/{i}").parse().unwrap()).collect()
        };
        for (i, url) in links.iter().enumerate() {
            cache.insert(url, LinkPreviewData {
                title: Some(if replacement { "Replacement card".into() } else { format!("Preview title {}", i + 1) }),
                site_name: Some("Example site".into()),
                description: Some("A readable description that wraps inside the card, including 中文 and a long line of text.".into()),
                image: (!replacement && i == 0).then(|| "mxc://example.org/fixture".into()),
                url: Some("https://example.org/metadata-canonical-url".into()),
                ..Default::default()
            });
        }
        self.ui.link_preview(cx, ids!(card)).populate_below_message(
            cx, &links, &mut media, &mut cache, &|cx, image, _, source, _, _| {
                image.show_image(cx, Some(source), |cx, image| {
                    image.load_png_from_data(cx, include_bytes!("../resources/icon_64.png"))?;
                    Ok::<_, makepad_widgets::image_cache::ImageError>((64, 64))
                }).unwrap();
                true
            },
        );
        self.ui.redraw(cx);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        for (path, user, name) in [
            (ids!(alice), "alice", "Alice"), (ids!(bob), "bob", "Bob"),
            (ids!(carol), "carol", "Carol"), (ids!(dave), "dave", "Dave"),
            (ids!(erin), "erin", "Erin"), (ids!(frank), "frank", "Frank"),
            (ids!(alice_profile), "alice", "Renamed Alice"),
        ] {
            let id: ruma::OwnedUserId = format!("@{user}:example.org").try_into().unwrap();
            self.ui.avatar(cx, path).show_user_text(cx, &id, name);
        }
        self.populate(cx, false);
        let text = "Long chat messages should wrap comfortably and leave space beside the text. ".repeat(3);
        for path in [ids!(desktop), ids!(compact), ids!(own), ids!(mobile)] {
            let row = self.ui.widget(cx, path);
            row.html_or_plaintext(cx, ids!(content.message)).show_plaintext(cx, &text);
            row.label(cx, ids!(username)).set_text(cx, "Sender");
            row.view(cx, ids!(replied_to_message)).set_visible(cx, false);
            row.view(cx, ids!(download_section)).set_visible(cx, false);
            row.view(cx, ids!(thread_root_summary)).set_visible(cx, false);
        }
    }
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.ui.button(cx, ids!(groups)).clicked(actions) { self.groups(cx, false); }
        if self.ui.button(cx, ids!(group_update)).clicked(actions) { self.groups(cx, true); }
        if self.ui.button(cx, ids!(group_single)).clicked(actions) {
            self.ui.avatar(cx, ids!(recycled_group)).show_user_text(cx, ruma::user_id!("@jane:example.org"), "Jane");
            self.ui.redraw(cx);
        }
        if self.ui.button(cx, ids!(group_photo)).clicked(actions) {
            self.ui.avatar(cx, ids!(recycled_group)).show_image(cx, None, |cx, image| {
                image.load_png_from_data(cx, include_bytes!("../resources/icon_64.png"))
            }).unwrap();
            self.ui.redraw(cx);
        }
        if self.ui.button(cx, ids!(fonts)).clicked(actions) { self.populate_fonts(cx); }
        if self.ui.button(cx, ids!(scale)).clicked(actions) {
            rinx::theme::packages::preview(cx, rinx::theme::packages::Preferences {
                package: Some(serde_json::from_str(include_str!("themes/ocean-violet.octotheme")).unwrap()),
                ..Default::default()
            }).unwrap();
        }
        if self.ui.button(cx, ids!(article)).clicked(actions) {
            self.ui.view(cx, ids!(font_cases)).set_visible(cx, false);
            self.ui.view(cx, ids!(layout_cases)).set_visible(cx, false);
            self.article(cx, false);
        }
        if self.ui.button(cx, ids!(recycle)).clicked(actions) { self.article(cx, true); }
        if self.ui.button(cx, ids!(replace)).clicked(actions) { self.populate(cx, true); }
        if self.ui.button(cx, ids!(clear)).clicked(actions) {
            self.ui.link_preview(cx, ids!(card)).clear(cx);
        }
        if self.ui.button(cx, ids!(narrow)).clicked(actions) {
            let mut card = self.ui.link_preview(cx, ids!(card));
            script_apply_eval!(cx, card, {width: 240});
            self.ui.redraw(cx);
        }
        if self.ui.button(cx, ids!(empty)).clicked(actions) {
            let link = "https://example.org/empty".parse().unwrap();
            let mut cache = LinkPreviewCache::new(None);
            cache.insert(&link, LinkPreviewData {title: Some("  \n ".into()), ..Default::default()});
            self.ui.link_preview(cx, ids!(card)).populate_below_message(
                cx, &[link], &mut MediaCache::new(None), &mut cache, &|_, _, _, _, _, _| true,
            );
        }
        if self.ui.button(cx, ids!(layouts)).clicked(actions) {
            self.ui.link_preview(cx, ids!(card)).clear(cx);
            self.ui.view(cx, ids!(layout_cases)).set_visible(cx, true);
        }
        if self.ui.button(cx, ids!(inspect)).clicked(actions) {
            let mut rects = serde_json::Map::new();
            if self.ui.view(cx, ids!(group_samples)).visible() {
                for (name, path) in [("group_two", ids!(group_two)), ("group_three", ids!(group_three)), ("group_four", ids!(group_four)), ("group_nine", ids!(group_nine)), ("recycled_group", ids!(recycled_group))] {
                    let avatar = self.ui.widget(cx, path);
                    let group = avatar.view(cx, ids!(members_view));
                    let children = group.borrow().unwrap().children.clone();
                    let tiles: Vec<_> = children.iter().map(|(_, tile)| {
                        let rect = tile.area().rect(cx);
                        let background = tile.child(id!(tile_text)).as_view();
                        let mut color = [0.; 4];
                        background.borrow().unwrap().draw_bg.get_instance(cx, id!(color), &mut color);
                        if color[3] == 0. { background.borrow().unwrap().draw_bg.get_uniform(cx, id!(color), &mut color); }
                        serde_json::json!({
                            "rect": [rect.pos.x, rect.pos.y, rect.size.x, rect.size.y],
                            "text": tile.child(id!(tile_text)).child(id!(text)).as_label().text(),
                            "color": rinx::theme::argb(vec4(color[0], color[1], color[2], color[3])),
                            "photo": tile.child(id!(tile_image)).as_image().visible(),
                        })
                    }).collect();
                    rects.insert(name.into(), serde_json::json!({
                        "tiles": tiles, "mosaic": group.visible(),
                        "single_text": avatar.view(cx, ids!(text_view)).visible(),
                        "single_image": avatar.view(cx, ids!(img_view)).visible(),
                    }));
                }
            }
            for (name, path) in [("desktop", ids!(desktop)), ("compact", ids!(compact)), ("own", ids!(own)), ("mobile", ids!(mobile))] {
                let row = self.ui.widget(cx, path);
                let rect = row.area().rect(cx);
                let body = row.widget(cx, ids!(content.message)).area().rect(cx);
                rects.insert(name.into(), serde_json::json!({"row": [rect.pos.x, rect.size.x], "text": [body.pos.x, body.size.x]}));
            }
            for (name, path) in [("font_desktop", ids!(font_desktop)), ("font_mobile", ids!(font_mobile))] {
                let row = self.ui.widget(cx, path);
                let paths: [&[LiveId]; 3] = [
                    ids!(content.message.plaintext_view.pt_label),
                    ids!(content.link_preview_view.title_label),
                    ids!(content.link_preview_view.description_label),
                ];
                let sizes: Vec<_> = paths
                    .into_iter().map(|path| row.label(cx, path).borrow().map(|label| label.draw_text.text_style.font_size)).collect();
                rects.insert(name.into(), serde_json::json!(sizes));
            }
            let colors: Vec<_> = [ids!(alice), ids!(bob), ids!(carol), ids!(dave), ids!(erin), ids!(frank), ids!(alice_profile)]
                .into_iter().map(|path| {
                    let avatar = self.ui.widget(cx, path);
                    avatar.view(cx, ids!(text_view)).borrow().map(|view| {
                        let mut color = [0.; 4];
                        view.draw_bg.get_instance(cx, id!(color), &mut color);
                        rinx::theme::argb(vec4(color[0], color[1], color[2], color[3]))
                    })
                }).collect();
            rects.insert("avatar_colors".into(), serde_json::json!(colors));
            self.ui.label(cx, ids!(result)).set_text(cx, &serde_json::Value::Object(rects).to_string());
        }
        for action in actions {
            if let HtmlLinkAction::Clicked {url, ..} = action.as_widget_action().cast() {
                self.ui.label(cx, ids!(result)).set_text(cx, &url);
            }
            if let Some(rinx::article_app::ArticleAction::Read {event, ..}) = action.downcast_ref() {
                self.ui.label(cx, ids!(result)).set_text(cx, &format!("Read {event}"));
            }
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        rinx::theme::init_standalone(vm);
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        article_makepad::apple_fonts::install(vm);
        makepad_widgets::widgets_mod(vm);
        desktop_style::apply_widgets(vm);
        rinx::app::register_widgets(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        rinx::theme::packages::after_event(cx, event);
    }
}
