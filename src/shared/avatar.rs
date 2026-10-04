//! An avatar shows a photo, an initial, or a tiled group of room members.
//!
//! The Avatar view (either text or image) is masked by a circle.
//!
//! By default, an avatar displays the one-character text label.
//! You can use [AvatarRef::set_text] to set the content of that text label,
//! or [AvatarRef::show_image] to display an image instead of the text.

use std::sync::Arc;

use makepad_widgets::*;
use matrix_sdk::{ruma::{EventId, OwnedRoomId, OwnedUserId, UserId}};
use matrix_sdk_ui::timeline::{Profile, TimelineDetails};
use ruma::OwnedMxcUri;

use crate::{
    avatar_cache::{self, AvatarCacheEntry},
    profile::{user_profile::{ShowUserProfileAction, UserProfile, UserProfileAndRoomId}, user_profile_cache},
    room::{FetchedRoomAvatar, RoomAvatarMember},
    sliding_sync::{submit_async_request, MatrixRequest, TimelineKind},
    utils,
};

// Avatar colors identify people, independently of the interface accent/theme.
// All of these backgrounds retain at least 4.5:1 contrast with white initials.
const AVATAR_COLORS: [u32; 12] = [
    0x356cb0, 0x7556a8, 0xad4874, 0x257d75, 0xb55a30, 0xb74747,
    0x4a6b83, 0x515da8, 0x926b28, 0x677c32, 0x287c97, 0x905587,
];

fn avatar_color(identity: &str) -> Vec4 {
    let hash = blake3::hash(identity.as_bytes());
    let index = u16::from_le_bytes([hash.as_bytes()[0], hash.as_bytes()[1]]) as usize;
    let rgb = AVATAR_COLORS[index % AVATAR_COLORS.len()];
    vec4(((rgb >> 16) & 255) as f32 / 255., ((rgb >> 8) & 255) as f32 / 255., (rgb & 255) as f32 / 255., 1.)
}

/// Center short first rows, as in a group-chat avatar, while keeping every tile square.
fn member_tile_rects(count: usize, size: Vec2d) -> Vec<Rect> {
    let count = count.min(9);
    if count == 0 { return Vec::new(); }
    let columns = if count > 4 { 3 } else if count > 1 { 2 } else { 1 };
    let rows = count.div_ceil(columns);
    let side = size.x.min(size.y).max(0.0);
    let padding = side * 0.05;
    let gap = side * 0.025;
    let tile = (side - 2.0 * padding - (columns - 1) as f64 * gap) / columns as f64;
    let first_row = count - (rows - 1) * columns;
    let mut rects = Vec::with_capacity(count);
    for row in 0..rows {
        let row_count = if row == 0 { first_row } else { columns };
        let x = (size.x - row_count as f64 * tile - (row_count - 1) as f64 * gap) / 2.0;
        let y = (size.y - rows as f64 * tile - (rows - 1) as f64 * gap) / 2.0;
        for column in 0..row_count {
            rects.push(Rect {pos: dvec2(x + column as f64 * (tile + gap), y + row as f64 * (tile + gap)), size: dvec2(tile, tile)});
        }
    }
    rects
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*


    // An avatar view holds either an image thumbnail or a single character of text.
    // By default, the text label is visible, but can be replaced by an image
    // once it is available.
    //
    // The Avatar view (either text or image) is masked by a circle.
    mod.widgets.Avatar = #(Avatar::register_widget(vm)) {
        width: 36.0,
        height: 36.0,
        // centered horizontally and vertically.
        align: Align{ x: 0.5, y: 0.5 }
        // the text_view and img_view are overlaid on top of each other.
        flow: Overlay,

        // TODO: use PageFlip to switch between text and image instead of an overlay.

        text_view := CircleView {
            visible: true,
            align: Align { x: 0.5, y: 0.5 }
            show_bg: true,
            draw_bg.color: (COLOR_AVATAR_BG)

            text := Label {
                padding: 0,
                margin: 0,
                width: Fit, height: Fit,
                flow: Flow.Right { wrap: false },
                align: Align{ x: 0.5, y: 0.5 }
                draw_text +: {
                    text_style: TITLE_TEXT { font_size: 15. }
                    color: #f,
                }
                text: "?"
            }
        }

        img_view := CircleView {
            visible: false,
            align: Align { x: 0.5, y: 0.5 }
            img := Image {
                fit: ImageFit.CropToFill,
                width: Fill, height: Fill,
                draw_bg +: {
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size);
                        let c = self.rect_size * 0.5;
                        let r = min(self.rect_size.x, self.rect_size.y) * 0.5;
                        sdf.circle(c.x, c.y, r);
                        sdf.fill_keep(self.get_color());
                        return sdf.result
                    }
                }
            }
        }

        members_view := RoundedView {
            visible: false width: Fill height: Fill flow: Overlay
            draw_bg +: {color: mod.widgets.RINX_BORDER border_radius: 4.0}
        }
        member_template: View {
            flow: Overlay
            tile_text := SolidView {
                width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                draw_bg +: {color: instance(#fff)} // theme-content: replaced with each member's stable identity color before drawing.
                text := Label {
                    width: Fit height: Fit padding: 0
                    draw_text +: {color: #fff text_style: theme.font_bold {font_size: 8.0}} // theme-content: white initials contrast with the fixed identity palette.
                }
            }
            tile_image := Image {
                visible: false width: Fill height: Fill fit: ImageFit.CropToFill
            }
        }
    }
}


/// What an [`Avatar`] is currently set to display.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AvatarDisplayState {
    /// No image was set, so the one-grapheme text label is visible.
    #[default]
    Text,
    /// An image was set but is still being fetched/decoded, so we show text until it's ready.
    ImageLoading,
    /// Showing the fully-decoded avatar image.
    Image,
    Members,
}

#[derive(Script, Widget)]
pub struct Avatar {
    #[source] source: ScriptObjectRef,
    #[deref] view: View,

    /// Information about the user profile being shown in this Avatar.
    /// If `Some`, this Avatar will respond to clicks/taps.
    #[rust] info: Option<UserProfileAndRoomId>,
    #[rust] display_state: AvatarDisplayState,
    #[rust] text_bg_color: Option<Vec4>,
    #[rust] text_label: String,
    #[live] member_template: Option<crate::LivePtr>,
    #[rust] members: Vec<RoomAvatarMember>,
    #[rust] member_tiles: Vec<ViewRef>,
    #[rust] member_images: Vec<Option<OwnedMxcUri>>,
}

impl ScriptHook for Avatar {
    fn on_after_apply(&mut self, vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        // A reapply (like rotation on mobile) resets the default visibility, so we need to set it again.
        if !apply.is_script_reapply() {
            return;
        }
        vm.with_cx_mut(|cx| {
            self.sync_visibility(cx);
            if let Some(color) = self.text_bg_color.take() {
                self.set_background_color(cx, color);
            }
            if !self.text_label.is_empty() {
                self.label(cx, ids!(text_view.text)).set_text(cx, &self.text_label);
            }
        });
    }
}

impl Widget for Avatar {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.display_state == AvatarDisplayState::Members && matches!(event, Event::Signal) {
            avatar_cache::process_avatar_updates(cx);
        }
        self.view.handle_event(cx, event, scope);

        // A loading tile's Image is hidden and has no draw area to invalidate yet.
        // Redraw the enclosing mosaic when its asynchronous decode completes.
        if self.display_state == AvatarDisplayState::Members {
            if let Event::Actions(actions) = event {
                if actions.iter().any(|action| action.downcast_ref::<AsyncImageLoad>().is_some()) {
                    self.view.redraw(cx);
                }
            }
        }

        // Check to see if this image has been loaded/decoded.
        if self.display_state == AvatarDisplayState::ImageLoading {
            if let Event::Actions(actions) = event {
                if actions.iter().any(|a| a.downcast_ref::<AsyncImageLoad>().is_some())
                    && self.image(cx, ids!(img_view.img)).has_content()
                {
                    self.display_state = AvatarDisplayState::Image;
                    self.view(cx, ids!(img_view)).set_visible(cx, true);
                    self.view(cx, ids!(text_view)).set_visible(cx, false);
                    self.view.redraw(cx);
                }
            }
        }

        let Some(info) = self.info.as_ref() else { return };
        let area = self.view.area();
        match event.hits(cx, area) {
            Hit::FingerDown(_fde) => {
                cx.set_key_focus(area);
            }
            Hit::FingerUp(fue) if fue.is_over && fue.is_primary_hit() && fue.was_tap() => {
                cx.widget_action(
                    self.widget_uid(),
                    ShowUserProfileAction::ShowUserProfile(info.clone()),
                );
            }
            _ =>()
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.display_state == AvatarDisplayState::Members {
            let size = cx.peek_walk_turtle(walk).size;
            self.draw_members(cx, size);
        }
        self.view.draw_walk(cx, scope, walk)
    }

    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        self.display_state = AvatarDisplayState::Text;
        self.set_text_label(cx, v);
        self.view(cx, ids!(img_view)).set_visible(cx, false);
        self.view(cx, ids!(text_view)).set_visible(cx, true);
        self.view(cx, ids!(members_view)).set_visible(cx, false);
    }
}

impl Avatar {
    fn sync_visibility(&self, cx: &mut Cx) {
        let members = self.display_state == AvatarDisplayState::Members;
        let image = matches!(self.display_state, AvatarDisplayState::Image | AvatarDisplayState::ImageLoading)
            && self.image(cx, ids!(img_view.img)).has_content();
        self.view(cx, ids!(members_view)).set_visible(cx, members);
        self.view(cx, ids!(img_view)).set_visible(cx, image);
        self.view(cx, ids!(text_view)).set_visible(cx, !members && !image);
    }

    fn show_members(&mut self, cx: &mut Cx, members: &[RoomAvatarMember]) {
        if members.is_empty() {
            self.show_text(cx, None, None, "?");
            return;
        }
        let members = &members[..members.len().min(9)];
        if self.members != members {
            self.members = members.to_vec();
            self.member_tiles.clear();
            self.member_images = vec![None; members.len()];
            for member in members {
                let tile = crate::widget_ref_from_live_ptr(cx, self.member_template).as_view();
                let mut background = tile.child(id!(tile_text)).as_view();
                let color = avatar_color(member.user_id.as_str());
                script_apply_eval!(cx, background, {draw_bg.color: #(color)});
                tile.child(id!(tile_text)).child(id!(text)).as_label().set_text(cx, &utils::user_name_first_letter(
                    member.display_name.as_deref().unwrap_or(member.user_id.as_str())
                ).unwrap_or("?").to_uppercase());
                self.member_tiles.push(tile);
            }
            if let Some(mut view) = self.view(cx, ids!(members_view)).borrow_mut() {
                view.children = self.member_tiles.iter().enumerate().map(|(index, tile)| {
                    (LiveId::from_num(id!(member_tile).0, index as u64), WidgetRef::clone(tile))
                }).collect();
                cx.widget_tree_mark_dirty(view.widget_uid());
            }
        }
        self.display_state = AvatarDisplayState::Members;
        self.info = None;
        self.view.cursor = Some(MouseCursor::Default);
        self.sync_visibility(cx);
        self.view.redraw(cx);
    }

    fn draw_members(&mut self, cx: &mut Cx, size: Vec2d) {
        let rects = member_tile_rects(self.members.len(), size);
        for (index, ((member, tile), rect)) in self.members.iter().zip(&self.member_tiles).zip(rects).enumerate() {
            if let Some(mut view) = tile.borrow_mut() {
                view.walk = Walk {
                    width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y),
                    margin: Inset {left: rect.pos.x, top: rect.pos.y, ..Default::default()},
                    ..Default::default()
                };
            }
            let background = tile.child(id!(tile_text)).as_view();
            let label = tile.child(id!(tile_text)).child(id!(text)).as_label();
            if let Some(mut label) = label.borrow_mut() {
                label.draw_text.text_style.font_size = (rect.size.y * 0.46) as f32;
            }
            let image = tile.child(id!(tile_image)).as_image();
            if let Some(uri) = &member.avatar_url {
                if self.member_images[index].as_ref() != Some(uri) {
                    if let AvatarCacheEntry::Loaded(data) = avatar_cache::get_or_fetch_avatar(cx, uri) {
                        let avatar = AvatarImage::from((uri.clone(), data));
                        // An immutable MXC image that cannot decode should keep
                        // its initials, without retrying the bad bytes every frame.
                        self.member_images[index] = Some(uri.clone());
                        let _ = utils::load_avatar_image(&image, cx, &avatar);
                    }
                }
            }
            let loaded = self.member_images[index].is_some() && image.has_content();
            image.set_visible(cx, loaded);
            background.set_visible(cx, !loaded);
        }
    }

    fn set_background_color(&mut self, cx: &mut Cx, color: Vec4) {
        if self.text_bg_color.replace(color) == Some(color) {
            return;
        }
        let mut text_view = self.view(cx, ids!(text_view));
        script_apply_eval!(cx, text_view, {draw_bg.color: #(color)});
    }

    /// Internal function to sets the text label to the first grapheme of `v`.
    ///
    /// Specifically does NOT change the avatar's display state or image/text view visibility.
    fn set_text_label(&mut self, cx: &mut Cx, v: &str) {
        let f = utils::user_name_first_letter(v)
            .unwrap_or("?").to_uppercase();
        self.label(cx, ids!(text_view.text)).set_text(cx, &f);
        self.text_label = f;
    }

    /// Sets the text content of this avatar, making the user name visible
    /// and the image invisible.
    ///
    /// ## Arguments
    /// * `info`: information about the user represented by this avatar, including a tuple of
    ///   the user ID, displayable user name, and room ID.
    ///   * Set this to `Some` to enable a user to click/tap on the Avatar itself.
    ///   * Set this to `None` to disable the click/tap action.
    /// * `username`: the displayable text for this avatar, either a user name or user ID.
    ///   Only the first non-`@` letter (Unicode grapheme) is displayed.
    pub fn show_text<T: AsRef<str>>(
        &mut self,
        cx: &mut Cx,
        bg_color: Option<Vec4>,
        info: Option<AvatarTextInfo>,
        username: T,
    ) {
        let bg_color = bg_color.unwrap_or_else(|| avatar_color(
            info.as_ref().map(|info| info.user_id.as_str()).unwrap_or(username.as_ref())
        ));
        if let Some(AvatarTextInfo { user_id, username, room_id }) = info {
            self.info = Some(UserProfileAndRoomId {
                user_profile: UserProfile {
                    user_id,
                    username,
                    avatar_state: AvatarState::Unknown,
                },
                room_id,
            });
            self.view.cursor = Some(MouseCursor::Hand);
        } else {
            self.info = None;
            self.view.cursor = Some(MouseCursor::Default);
        }
        self.set_text(cx, username.as_ref());

        self.set_background_color(cx, bg_color);
    }

    /// Sets the image content of this avatar, making the image visible
    /// and the user name text invisible.
    ///
    /// If the image is still being loaded/decoded asynchronously and isn't ready yet,
    /// the text label will still be shown until the image is ready.
    ///
    /// ## Arguments
    /// * `info`: information about the user represented by this avatar:
    ///   the user name, user ID, room ID, and avatar image data.
    ///   * Set this to `Some` to enable a user to click/tap on the Avatar itself.
    ///   * Set this to `None` to disable the click/tap action.
    /// * `image_set_function`: - a function that is passed in the `&mut Cx`
    ///   and an [ImageRef] that refers to the image that will be displayed in this avatar.
    ///   This allows the caller to set the image contents in any way they want.
    ///   If `image_set_function` returns an error, no change is made to the avatar.
    pub fn show_image<F, E>(
        &mut self,
        cx: &mut Cx,
        info: Option<AvatarImageInfo>,
        image_set_function: F,
    ) -> Result<(), E>
        where F: FnOnce(&mut Cx, ImageRef) -> Result<(), E>
    {
        let img_ref = self.image(cx, ids!(img_view.img));
        let res = image_set_function(cx, img_ref.clone());
        if res.is_ok() {
            // Don't show the avatar image until it's been decoded in full (which is async).
            let has_content = img_ref.has_content();
            self.display_state = if has_content { AvatarDisplayState::Image } else { AvatarDisplayState::ImageLoading };
            self.view(cx, ids!(img_view)).set_visible(cx, has_content);
            self.view(cx, ids!(text_view)).set_visible(cx, !has_content);
            self.view(cx, ids!(members_view)).set_visible(cx, false);

            if let Some(AvatarImageInfo { user_id, username, room_id, img_data }) = info {
                self.set_background_color(cx, avatar_color(user_id.as_str()));
                self.set_text_label(cx, username.as_deref().unwrap_or(user_id.as_str()));
                self.info = Some(UserProfileAndRoomId {
                    user_profile: UserProfile {
                        user_id,
                        username,
                        avatar_state: AvatarState::Loaded(img_data),
                    },
                    room_id,
                });
                self.view.cursor = Some(MouseCursor::Hand);
            } else {
                self.info = None;
                self.view.cursor = Some(MouseCursor::Default);
            }
        }
        res
    }

    /// Sets the given avatar and returns a displayable username based on the
    /// given profile and user ID of the sender of the event with the given event ID.
    ///
    /// If the user profile is not ready, this function will submit an async request
    /// to fetch the user profile from the server, but only if the event ID is `Some`.
    /// For Read Receipt cases, there is no user profile. The Avatar cache is taken from the sender's profile
    ///
    /// This function will always choose a nice, displayable username and avatar.
    ///
    /// The specific behavior is as follows:
    /// * If the timeline event's sender profile *is* ready, then the `username` and `avatar`
    ///   will be the user's display name and avatar image, if available.
    ///   * If it's not ready, we attempt to fetch the user info from the user profile cache.
    /// * If no avatar image is available, then the `avatar` will be set to the first character
    ///   of the user's display name, if available.
    /// * If the user's display name is not available or has not been set, the user ID
    ///   will be used for the `username`, and the first character of the user ID for the `avatar`.
    /// * If the timeline event's sender profile isn't ready and the user ID isn't found in
    ///   our user profile cache , then the `username` and `avatar`  will be the user ID
    ///   and the first character of that user ID, respectively.
    ///
    /// If `is_clickable` is `true`, this Avatar will respond to clicks.
    ///
    /// ## Return
    /// Returns a tuple of:
    /// 1. The displayable username that should be used to populate the username field.
    /// 2. A boolean indicating whether the user's profile info has been completely drawn
    ///    (for purposes of caching it to avoid future redraws).
    pub fn set_avatar_and_get_username(
        &mut self,
        cx: &mut Cx,
        timeline_kind: &TimelineKind,
        avatar_user_id: &UserId,
        avatar_profile_opt: Option<&TimelineDetails<Profile>>,
        event_id: Option<&EventId>,
        is_clickable: bool,
    ) -> (String, bool) {
        // A closure to get the user's displayable name and avatar from the cache.
        // This is only used if those timeline details are not `Ready`.
        let try_get_cached_username_avatar = || {
            user_profile_cache::with_user_profile(
                cx,
                avatar_user_id.to_owned(),
                Some(timeline_kind.room_id()),
                true,
                |profile, rooms| {
                    rooms.get(timeline_kind.room_id()).and_then(|entry| entry.loaded()).map(|rm| {
                        (
                            rm.display_name().map(|n| n.to_owned()),
                            AvatarState::Known(rm.avatar_url().map(|u| u.to_owned())),
                        )
                    })
                    .unwrap_or_else(|| (profile.username.clone(), profile.avatar_state.clone()))
                }
            )
        };

        // Get the display name and avatar URL from the user's profile, if available,
        // or if the profile isn't ready, fall back to querying our user profile cache.
        let timeline_details = match avatar_profile_opt {
            Some(TimelineDetails::Ready(profile)) => Some((
                profile.display_name.clone(),
                AvatarState::Known(profile.avatar_url.clone()),
            )),
            Some(TimelineDetails::Unavailable) => {
                if let Some(event_id) = event_id {
                    submit_async_request(MatrixRequest::FetchDetailsForEvent {
                        timeline_kind: timeline_kind.clone(),
                        event_id: event_id.to_owned(),
                    });
                }
                None
            }
            _ => None,
        };
        let (username_opt, avatar_state) = timeline_details
            .or_else(try_get_cached_username_avatar)
            .unwrap_or((None, AvatarState::Unknown));

        let (avatar_img_opt, profile_drawn) = match avatar_state {
            AvatarState::Loaded(image) => (Some(image), true),
            AvatarState::Known(Some(uri)) => match avatar_cache::get_or_fetch_avatar(cx, &uri) {
                AvatarCacheEntry::Loaded(data) => (Some((uri, data).into()), true),
                AvatarCacheEntry::Failed => (None, true),
                AvatarCacheEntry::Requested => (None, false),
            },
            AvatarState::Known(None) | AvatarState::Failed => (None, true),
            AvatarState::Unknown => (None, false),
        };

        // Set sender to the display name if available, otherwise the user id.
        let username = username_opt
            .clone()
            .unwrap_or_else(|| avatar_user_id.to_string());
        let bg_color = avatar_color(avatar_user_id.as_str());
        self.set_background_color(cx, bg_color);

        // Set the sender's avatar image, or use the username if no image is available.
        avatar_img_opt.and_then(|image| {
            self.show_image(
                cx,
                is_clickable.then(|| AvatarImageInfo::from((
                    avatar_user_id.to_owned(),
                    username_opt.clone(),
                    timeline_kind.room_id().to_owned(),
                    image.clone()
                ))),
                |cx, img| utils::load_avatar_image(&img, cx, &image),
            )
            .ok()
        }).inspect(|_| {
            // While the image is being decoded, show the text avatar as the placeholder.
            if self.display_state == AvatarDisplayState::ImageLoading {
                self.set_text_label(cx, &username);
            }
        }).unwrap_or_else(|| {
            self.show_text(
                cx,
                Some(bg_color),
                is_clickable.then(|| AvatarTextInfo::from((
                    avatar_user_id.to_owned(),
                    username_opt,
                    timeline_kind.room_id().to_owned(),
                ))),
                &username,
            )
        });
        (username, profile_drawn)
    }
}

impl AvatarRef {
    /// Displays a room photo, a member mosaic, or the room's initial.
    pub fn show_room_avatar(&self, cx: &mut Cx, avatar: &FetchedRoomAvatar) {
        match avatar {
            FetchedRoomAvatar::Text(text) => self.show_text(cx, None, None, text),
            FetchedRoomAvatar::Image(image) => {
                let _ = self.show_image(cx, None, |cx, img| utils::load_avatar_image(&img, cx, image));
            }
            FetchedRoomAvatar::Members(members) => {
                if let Some(mut inner) = self.borrow_mut() { inner.show_members(cx, members); }
            }
        }
    }

    /// Shows a non-clickable user's initials with the same color as their chat avatar.
    pub fn show_user_text(&self, cx: &mut Cx, user_id: &UserId, username: &str) {
        self.show_text(cx, Some(avatar_color(user_id.as_str())), None, username);
    }

    /// See [`Avatar::show_text()`].
    pub fn show_text<T: AsRef<str>>(
        &self,
        cx: &mut Cx,
        bg_color: Option<Vec4>,
        info: Option<AvatarTextInfo>,
        username: T,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show_text(cx, bg_color, info, username);
        }
    }

    /// See [`Avatar::show_image()`].
    pub fn show_image<F, E>(
        &self,
        cx: &mut Cx,
        info: Option<AvatarImageInfo>,
        image_set_function: F,
    ) -> Result<(), E>
        where F: FnOnce(&mut Cx, ImageRef) -> Result<(), E>
    {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show_image(cx, info, image_set_function)
        } else {
            Ok(())
        }
    }

    /// See [`Avatar::set_avatar_and_get_username()`].
    pub fn set_avatar_and_get_username(
        &self,
        cx: &mut Cx,
        timeline_kind: &TimelineKind,
        avatar_user_id: &UserId,
        avatar_profile_opt: Option<&TimelineDetails<Profile>>,
        event_id: Option<&EventId>,
        is_clickable: bool,
    ) -> (String, bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_avatar_and_get_username(
                cx,
                timeline_kind,
                avatar_user_id,
                avatar_profile_opt,
                event_id,
                is_clickable,
            )
        } else {
            (avatar_user_id.to_string(), false)
        }
    }
}

/// Information about a text-based Avatar.
pub struct AvatarTextInfo {
    pub user_id: OwnedUserId,
    pub username: Option<String>,
    pub room_id: OwnedRoomId,
}
impl From<(OwnedUserId, Option<String>, OwnedRoomId)> for AvatarTextInfo {
    fn from((user_id, username, room_id): (OwnedUserId, Option<String>, OwnedRoomId)) -> Self {
        Self { user_id, username, room_id }
    }
}

/// Information about an image-based avatar.
pub struct AvatarImageInfo {
    pub user_id: OwnedUserId,
    pub username: Option<String>,
    pub room_id: OwnedRoomId,
    pub img_data: AvatarImage,
}
impl From<(OwnedUserId, Option<String>, OwnedRoomId, AvatarImage)> for AvatarImageInfo {
    fn from((user_id, username, room_id, img_data): (OwnedUserId, Option<String>, OwnedRoomId, AvatarImage)) -> Self {
        Self { user_id, username, room_id, img_data }
    }
}


/// A fetched avatar: its image data and its MxcUri.
#[derive(Clone)]
pub struct AvatarImage {
    pub uri: OwnedMxcUri,
    pub data: Arc<[u8]>,
}
impl<U, D> From<(U, D)> for AvatarImage
where
    U: Into<OwnedMxcUri>,
    D: Into<Arc<[u8]>>,
{
    fn from((uri, data): (U, D)) -> Self {
        Self { uri: uri.into(), data: data.into() }
    }
}
impl From<AvatarImage> for AvatarState {
    fn from(image: AvatarImage) -> Self {
        Self::Loaded(image)
    }
}

/// The currently-known state of an avatar for a user, room, or space.
#[derive(Clone, Default)]
pub enum AvatarState {
    /// It isn't yet known if this user/room/space has an avatar.
    #[default] Unknown,
    /// It is known that this user/room/space does or does not have an avatar.
    Known(Option<OwnedMxcUri>),
    /// The avatar is known to exist and has been fetched successfully.
    Loaded(AvatarImage),
    /// The avatar is known to exist but could not be fetched.
    Failed,
}
impl std::fmt::Debug for AvatarState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AvatarState::Unknown        => write!(f, "Unknown"),
            AvatarState::Known(Some(_)) => write!(f, "Known(Some)"),
            AvatarState::Known(None)    => write!(f, "Known(None)"),
            AvatarState::Loaded(image)  => write!(f, "Loaded({} bytes)", image.data.len()),
            AvatarState::Failed         => write!(f, "Failed"),
        }
    }
}
impl AvatarState {
    /// Tries to update this `AvatarState` if it has a known avatar URI
    /// by loading the avatar from the cache.
    ///
    /// Returns the fetched avatar if this `AvatarState` is in the `Loaded` state.
    pub fn update_from_cache(&mut self, cx: &mut Cx) -> Option<&AvatarImage> {
        if let Self::Known(Some(uri)) = self {
            if let AvatarCacheEntry::Loaded(data) = avatar_cache::get_or_fetch_avatar(cx, uri) {
                *self = Self::Loaded((uri.clone(), data).into());
            }
        }
        self.image()
    }

    /// Returns the fetched avatar, if in the `Loaded` state.
    pub fn image(&self) -> Option<&AvatarImage> {
        if let Self::Loaded(image) = self {
            Some(image)
        } else {
            None
        }
    }

    /// Returns the avatar URI, if in the `Known` state and it exists.
    pub fn uri(&self) -> Option<&OwnedMxcUri> {
        if let Self::Known(Some(uri)) = self {
            Some(uri)
        } else {
            None
        }
    }

    /// Returns true if this `AvatarState` indicates that the user/room/space has an avatar,
    /// i.e. it is `Known(Some)` or `Loaded`.
    pub fn has_avatar(&self) -> bool {
        matches!(self, Self::Known(Some(_)) | Self::Loaded(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_tiles_are_square_bounded_and_separated() {
        for side in [18., 36., 48., 96.] {
            for count in 1..=9 {
                let rects = member_tile_rects(count, dvec2(side, side));
                assert_eq!(rects.len(), count);
                for (index, rect) in rects.iter().enumerate() {
                    assert!(rect.size.x > 0. && rect.size.x == rect.size.y);
                    assert!(rect.pos.x >= 0. && rect.pos.y >= 0.);
                    assert!(rect.pos.x + rect.size.x <= side && rect.pos.y + rect.size.y <= side);
                    for other in &rects[..index] {
                        assert!(rect.pos.x >= other.pos.x + other.size.x
                            || other.pos.x >= rect.pos.x + rect.size.x
                            || rect.pos.y >= other.pos.y + other.size.y
                            || other.pos.y >= rect.pos.y + rect.size.y);
                    }
                }
            }
        }
        assert!(member_tile_rects(0, dvec2(36., 36.)).is_empty());
        assert_eq!(member_tile_rects(100, dvec2(36., 36.)).len(), 9);
    }
}
