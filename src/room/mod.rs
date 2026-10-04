//! Widgets, types, and functions related to a Matrix room.

use std::sync::Arc;
use makepad_widgets::ScriptVm;
use matrix_sdk::{RoomDisplayName, RoomHeroWithProfile, RoomState, SuccessorRoom, room_preview::RoomPreview};
use ruma::{OwnedMxcUri, OwnedRoomAliasId, OwnedRoomId, OwnedUserId, room::{JoinRuleSummary, RoomType}};

use crate::shared::avatar::AvatarImage;
use crate::utils::RoomNameId;

pub mod reply_preview;
pub mod room_input_bar;
mod speech_input;
pub mod room_display_filter;
pub mod typing_notice;

pub fn script_mod(vm: &mut ScriptVm) {
    reply_preview::script_mod(vm);
    room_input_bar::script_mod(vm);
    typing_notice::script_mod(vm);
}

/// Info about a room, either partially or completely known.
///
/// This is useful to represent a room that you may want to know more about,
/// but haven't yet fetched the full room preview,
/// such as a room to navigate to, or a room to join.
#[derive(Clone, Debug)]
pub enum BasicRoomDetails {
    /// We only know the room's ID, so we'll need to fetch the room preview
    /// to get more info about the room.
    // Implementation Note: instead of just a room ID, this variant contains
    // a `RoomNameId` with an `Empty` name so that we can borrow it
    // in the `room_name_id()` method below.
    RoomId(RoomNameId),
    /// We know the room's displayable name, but nothing else.
    Name(RoomNameId),
    /// We know the room's name and have fetched its full avatar,
    /// but not any auxiliary info like topic, aliases, etc.
    NameAndAvatar {
        room_name_id: RoomNameId,
        room_avatar: FetchedRoomAvatar,
    },
    /// We have fetched the full preview for this room,
    /// including its avatar and all other possible info about it.
    FetchedRoomPreview(FetchedRoomPreview),
}
impl From<&SuccessorRoom> for BasicRoomDetails {
    fn from(sr: &SuccessorRoom) -> Self {
        BasicRoomDetails::RoomId(RoomNameId::empty(sr.room_id.clone()))
    }
}
impl From<FetchedRoomPreview> for BasicRoomDetails {
    fn from(frp: FetchedRoomPreview) -> Self {
       BasicRoomDetails::FetchedRoomPreview(frp)
    }
}
impl BasicRoomDetails {
    pub fn room_id(&self) -> &OwnedRoomId {
        match self {
            Self::RoomId(room_name_id)
            | Self::Name(room_name_id)
            | Self::NameAndAvatar { room_name_id, ..} => room_name_id.room_id(),
            Self::FetchedRoomPreview(frp) => frp.room_name_id.room_id(),
        }
    }

    /// Returns the displayable name of this room.
    ///
    /// If this is the `RoomId` variant, the name will be `Empty`.
    pub fn room_name_id(&self) -> &RoomNameId {
        match self {
            Self::RoomId(room_name_id)
            | Self::Name(room_name_id)
            | Self::NameAndAvatar { room_name_id, .. } => room_name_id,
            Self::FetchedRoomPreview(frp) => &frp.room_name_id,
        }
    }

    /// Returns the fetched avatar of this room.
    ///
    /// If this is the `RoomId` or `Name` variants, the avatar will be empty.
    pub fn room_avatar(&self) -> &FetchedRoomAvatar {
        match self {
            Self::RoomId(_)
            | Self::Name(_) => &EMPTY_AVATAR,
            Self::NameAndAvatar { room_avatar, ..} => room_avatar,
            Self::FetchedRoomPreview(frp) => &frp.room_avatar,
        }
    }
}


/// Actions related to room previews being fetched.
#[derive(Debug)]
pub enum RoomPreviewAction {
    Fetched(Result<FetchedRoomPreview, matrix_sdk::Error>),
}

/// A modified [`RoomPreview`], augmented with the room's fetched avatar.
#[derive(Clone, Debug)]
pub struct FetchedRoomPreview {
    /// The room's ID and displayable name.
    pub room_name_id: RoomNameId,
    /// The room's fetched avatar, ready to be displayed.
    pub room_avatar: FetchedRoomAvatar,

    // Below: copied from the `RoomPreview` struct.

    /// The canonical alias for the room.
    pub canonical_alias: Option<OwnedRoomAliasId>,
    /// The room's topic, if set.
    pub topic: Option<String>,
    /// The number of joined members.
    pub num_joined_members: u64,
    /// The number of active members, if known (joined + invited).
    pub num_active_members: Option<u64>,
    /// The room type (space, custom) or nothing, if it's a regular room.
    pub room_type: Option<RoomType>,
    /// What's the join rule for this room?
    pub join_rule: Option<JoinRuleSummary>,
    /// Is the room world-readable (i.e. is its history_visibility set to
    /// world_readable)?
    pub is_world_readable: Option<bool>,
    /// Has the current user been invited/joined/left this room?
    ///
    /// Set to `None` if the room is unknown to the user.
    pub state: Option<RoomState>,
    /// The `m.room.direct` state of the room, if known.
    pub is_direct: Option<bool>,
    /// Room heroes.
    pub heroes: Option<Vec<RoomHeroWithProfile>>,
}
impl FetchedRoomPreview {
    pub fn from(room_preview: RoomPreview, room_avatar: FetchedRoomAvatar) -> Self {
        let display_name = room_preview.name.map_or(
            RoomDisplayName::Empty,
            RoomDisplayName::Named,
        );
        Self {
            room_name_id: RoomNameId::new(display_name, room_preview.room_id),
            room_avatar,
            canonical_alias: room_preview.canonical_alias,
            topic: room_preview.topic,
            num_joined_members: room_preview.num_joined_members,
            num_active_members: room_preview.num_active_members,
            room_type: room_preview.room_type,
            join_rule: room_preview.join_rule,
            is_world_readable: room_preview.is_world_readable,
            state: room_preview.state,
            is_direct: room_preview.is_direct,
            heroes: room_preview.heroes,
        }
    }
}


static EMPTY_AVATAR: FetchedRoomAvatar = FetchedRoomAvatar::Text(String::new());

/// A member used in a room's avatar mosaic. Photos load through the shared avatar cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomAvatarMember {
    pub user_id: OwnedUserId,
    pub display_name: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
}

/// Keep the same members in the same positions across syncs and display-name edits.
pub(crate) fn room_avatar_members(mut members: Vec<RoomAvatarMember>) -> Vec<RoomAvatarMember> {
    members.sort_by(|a, b| a.user_id.cmp(&b.user_id));
    members.dedup_by(|a, b| a.user_id == b.user_id);
    members.truncate(9);
    members
}

/// A room avatar ready to display; mosaic photos load through the shared cache.
#[derive(Clone)]
pub enum FetchedRoomAvatar {
    Text(String),
    Image(AvatarImage),
    Members(Vec<RoomAvatarMember>),
}
impl Default for FetchedRoomAvatar {
    fn default() -> Self {
        FetchedRoomAvatar::Text(String::from("?"))
    }
}
impl From<AvatarImage> for FetchedRoomAvatar {
    fn from(image: AvatarImage) -> Self {
        FetchedRoomAvatar::Image(image)
    }
}
impl std::fmt::Debug for FetchedRoomAvatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchedRoomAvatar::Text(text) => f.debug_tuple("Text").field(text).finish(),
            FetchedRoomAvatar::Image(_) => f.debug_tuple("Image").finish(),
            FetchedRoomAvatar::Members(members) => f.debug_tuple("Members").field(members).finish(),
        }
    }
}
impl PartialEq for FetchedRoomAvatar {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FetchedRoomAvatar::Text(t1), FetchedRoomAvatar::Text(t2)) => t1 == t2,
            (FetchedRoomAvatar::Members(m1), FetchedRoomAvatar::Members(m2)) => m1 == m2,
            (FetchedRoomAvatar::Image(i1), FetchedRoomAvatar::Image(i2)) => {
                Arc::ptr_eq(&i1.data, &i2.data)
            }
            _ => false,
        }
    }
}
impl Eq for FetchedRoomAvatar { }

#[cfg(test)]
mod avatar_tests {
    use super::*;

    #[test]
    fn room_avatar_members_are_stable_unique_and_limited() {
        let members: Vec<_> = (0..12).map(|index| RoomAvatarMember {
            user_id: format!("@member{index:02}:example.org").try_into().unwrap(),
            display_name: Some(format!("Member {index}")),
            avatar_url: None,
        }).collect();
        let expected = room_avatar_members(members.clone());
        assert_eq!(expected.len(), 9);
        let mut reordered = members.clone();
        reordered.reverse();
        reordered.push(members[0].clone());
        assert_eq!(room_avatar_members(reordered), expected);
        let mut renamed = members;
        renamed[0].display_name = Some("Zoe".into());
        renamed[0].avatar_url = Some("mxc://example.org/new-photo".into());
        let updated = room_avatar_members(renamed);
        assert_eq!(updated[0].user_id, expected[0].user_id);
        assert_ne!(FetchedRoomAvatar::Members(updated), FetchedRoomAvatar::Members(expected));
    }
}
