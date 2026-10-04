//! Account-scoped agent identities and Matrix AppService administration.
pub mod model;
pub mod commands;
pub mod discovery;
pub mod routing;
pub mod ui;
pub use ui::{AgentAccessAction, AgentAccessPanelWidgetRefExt, script_mod};

use std::sync::{LazyLock, RwLock};
use matrix_sdk::{
    Client, Room,
    ruma::{OwnedUserId, UserId},
};
use model::{AgentAccessSettings, AgentFramework};

static SETTINGS: LazyLock<RwLock<(Option<OwnedUserId>, AgentAccessSettings)>> =
    LazyLock::new(|| RwLock::new((None, AgentAccessSettings::default())));

pub fn publish(owner: Option<OwnedUserId>, settings: &AgentAccessSettings) {
    let mut state = SETTINGS.write().unwrap();
    if state.0 != owner || &state.1 != settings {
        *state = (owner, settings.clone());
    }
}

pub fn current() -> AgentAccessSettings {
    let state = SETTINGS.read().unwrap();
    if state.0.is_some() && state.0 == crate::sliding_sync::current_user_id() {
        state.1.clone()
    } else {
        AgentAccessSettings::default()
    }
}

pub fn framework_label(user: &UserId) -> Option<&'static str> {
    let settings = current();
    settings
        .agent_registry
        .get(user)
        .map(|entry| entry.framework.label())
        .or_else(|| {
            settings
                .bot_settings
                .is_identified_bot(user, crate::sliding_sync::current_user_id().as_deref())
                .then_some("AppService")
        })
}

impl AgentFramework {
    pub const ALL: [Self; 5] = [
        Self::Unknown,
        Self::Octos,
        Self::OctosDirect,
        Self::Hermes,
        Self::OpenClaw,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Agent",
            Self::Octos => "Octos AppService",
            Self::OctosDirect => "Octos Direct",
            Self::Hermes => "Hermes",
            Self::OpenClaw => "OpenClaw",
        }
    }
}

/// The DM room to open with `user`. The SDK's lookup goes by `m.direct` only,
/// so it can return a room the other person declined or left; messages sent
/// there reach no one. Prefer a room where they are joined, then one they are
/// still invited to, and skip rooms they left or were banned from. A room whose
/// membership cannot be read stays a candidate rather than being lost.
pub async fn find_dm(client: &Client, user: &UserId) -> Option<Room> {
    use matrix_sdk::ruma::events::room::member::MembershipState;
    let mut invited = None;
    let mut unknown = None;
    for room in client.get_dm_rooms(user) {
        match room.get_member(user).await {
            Ok(Some(member)) => match member.membership() {
                MembershipState::Join => return Some(room),
                MembershipState::Invite => { invited.get_or_insert(room); }
                _ => {}
            },
            Ok(None) => {}
            Err(_) => { unknown.get_or_insert(room); }
        }
    }
    invited.or(unknown)
}

/// Keep the SDK's encrypted default for people. Explicitly registered bots can
/// use a new unencrypted DM; existing rooms and their encryption never change.
pub async fn create_dm(client: &Client, user: &UserId) -> matrix_sdk::Result<Room> {
    let settings = current();
    if settings.should_create_encrypted_dm(user, client.user_id()) {
        return client.create_dm(user).await;
    }
    create_bot_dm(client, user).await
}

async fn create_bot_dm(client: &Client, user: &UserId) -> matrix_sdk::Result<Room> {
    use matrix_sdk::ruma::api::client::room::create_room::v3::{Request, RoomPreset};
    let mut request = Request::new();
    request.invite = vec![user.to_owned()];
    request.is_direct = true;
    request.preset = Some(RoomPreset::TrustedPrivateChat);
    // create_room maintains m.direct when is_direct is set (SDK contract).
    client.create_room(request).await
}

fn create_command(username: &str, name: &str, prompt: &str) -> Result<String, String> {
    let username = username.trim();
    let name = name.trim();
    if username.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Use letters, numbers, underscores or hyphens for the bot username.".into());
    }
    if name.is_empty() || name.contains(['\n', '\r']) || name.contains("--") {
        return Err("Enter a display name without command options or newlines.".into());
    }
    let mut command = format!("/createbot {username} {name}");
    if !prompt.trim().is_empty() {
        command.push_str(&format!(
            " --prompt \"{}\"",
            prompt
                .trim()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace(['\n', '\r'], " ")
        ));
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_arguments_cannot_inject_options() {
        assert!(create_command("name --admin", "Name", "").is_err());
        assert!(create_command("name", "Name --admin", "").is_err());
        assert_eq!(
            create_command("helper", "My Helper", "say \"hi\"").unwrap(),
            "/createbot helper My Helper --prompt \"say \\\"hi\\\"\""
        );
    }
    #[test]
    fn only_explicit_bot_identities_change_new_dm_encryption() {
        let owner = matrix_sdk::ruma::user_id!("@me:example.org");
        let bot = matrix_sdk::ruma::user_id!("@bot:example.org");
        let mut settings = AgentAccessSettings::default();
        assert!(settings.should_create_encrypted_dm(bot, Some(owner)));
        settings
            .agent_registry
            .register(bot.to_owned(), Default::default());
        settings.bot_settings.set_room_bound(
            matrix_sdk::ruma::room_id!("!r:example.org").to_owned(),
            Some(bot.to_owned()),
            true,
        );
        assert!(!settings.should_create_encrypted_dm(bot, Some(owner)));
        settings.unregister_agent_and_clear_bot_identity(bot, Some(owner));
        assert!(settings.should_create_encrypted_dm(bot, Some(owner)));
        let restored: AgentAccessSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(restored, AgentAccessSettings::default());
    }
}
