//! What the store tells the person an app may do, in plain words, derived
//! from the manifest alone. The words are App Hub's, so an app reads the
//! same in Rinx as in App Hub's store. Rinx runs no app agents and no
//! research services; for those it says what is asked without detail (such
//! an app is shown as unavailable in Rinx anyway).
use octosense_app_contract::AppManifest;

pub fn permissions(manifest: &AppManifest) -> Vec<String> {
    let mut lines = Vec::new();
    for capability in &manifest.capabilities {
        lines.push(match capability.as_str() {
            "storage" => "Keep its own data on this device".to_string(),
            "net" if manifest.network.hosts.is_empty() => "Reach the network: nothing listed".to_string(),
            "net" => format!("Reach only: {}", manifest.network.hosts.join(", ")),
            "prompt" => "Ask you questions".to_string(),
            "ledger.read" => "Read your shared data".to_string(),
            "location" => "Use your location".to_string(),
            "camera" => "Use the camera".to_string(),
            "clipboard" => "Use the clipboard".to_string(),
            "images" => "Show pictures from any website".to_string(),
            "web" => "Open web pages in a browser view".to_string(),
            "microphone" => "Use the microphone".to_string(),
            "library" => "Save to your photo library, where other apps can see it".to_string(),
            "mail" => "Read and send mail from accounts you sign in to on the device".to_string(),
            "llm" => "Manage the assistant's AI providers, whose keys stay with the device".to_string(),
            "news" => "Read news the device collects from its feeds and topics".to_string(),
            "glance" => "Show cards on your glance screen".to_string(),
            "model" => "Send what you give it to the AI provider you configured, within a daily budget".to_string(),
            "research" => "Search the web".to_string(),
            "crawl" => "Crawl websites, which reaches more than searching".to_string(),
            other => match service_words(other) {
                Some(words) => words.to_string(),
                None => format!("Use {other}"),
            },
        });
    }
    if let Some(agent) = &manifest.agent {
        let host_tools: Vec<&str> = agent
            .tools
            .iter()
            .map(String::as_str)
            .filter(|t| kernel_tool_words(t).is_none())
            .collect();
        let tools = if host_tools.is_empty() { "no tools".to_string() } else { host_tools.join(", ") };
        lines.push(format!("Run an assistant for this app ({tools}), inside this app's own data only"));
        for words in agent.tools.iter().filter_map(|t| kernel_tool_words(t)) {
            if !lines.iter().any(|l| l == words) {
                lines.push(words.to_string());
            }
        }
    }
    if lines.is_empty() {
        lines.push("Draw its screens, and nothing else".to_string());
    }
    lines
}

fn kernel_tool_words(tool: &str) -> Option<&'static str> {
    match tool {
        "ask_user_question" => Some("Ask you questions"),
        _ => None,
    }
}

/// The Matrix and assistant services, by exact name.
fn service_words(capability: &str) -> Option<&'static str> {
    if let Some(words) = octosense_app_contract::palpo::words(capability) { return Some(words); }
    Some(match capability {
        "matrix.account_info" => "See which Matrix account you are using",
        "matrix.device" => "See this device's Matrix session details",
        "matrix.dm_find" => "Find your direct chats with a person",
        "matrix.dm_open" => "Start a direct chat on your Matrix account",
        "matrix.event" => "Read a single message in rooms you allow",
        "matrix.favorite" => "Mark rooms as favourites",
        "matrix.ignored_users" => "See whom you have ignored",
        "matrix.invite" => "Invite people to rooms you allow",
        "matrix.invite_respond" => "Accept or decline room invitations",
        "matrix.invites" => "See your room invitations",
        "matrix.join" => "Join rooms on your Matrix account",
        "matrix.low_priority" => "Mark rooms as low priority",
        "matrix.mark_unread" => "Mark rooms as read or unread",
        "matrix.older_messages" => "Read earlier messages in rooms you allow",
        "matrix.permalink" => "Make links to messages in rooms you allow",
        "matrix.pin" => "Pin messages in rooms you allow",
        "matrix.pinned_events" => "Read pinned messages in rooms you allow",
        "matrix.power_levels" => "See who may do what in rooms you allow",
        "matrix.profile" => "See your Matrix name and picture",
        "matrix.react" => "React to messages in rooms you allow",
        "matrix.read_messages" => "Read messages in rooms you allow",
        "matrix.read_receipt" => "Mark messages as read in rooms you allow",
        "matrix.read_receipts" => "See who has read messages in rooms you allow",
        "matrix.reply" => "Reply to messages in rooms you allow",
        "matrix.room_info" => "See details of rooms you allow",
        "matrix.room_members" => "See the members of rooms you allow",
        "matrix.room_preview" => "Preview rooms before joining",
        "matrix.room_threads" => "List threads in rooms you allow",
        "matrix.rooms_info" => "See details of your rooms",
        "matrix.rooms_list" => "List your rooms",
        "matrix.rooms_messages" => "Read recent messages across rooms you allow",
        "matrix.rooms_search" => "Search your rooms",
        "matrix.rooms_send" => "Send messages to rooms you allow",
        "matrix.search_room" => "Search messages in rooms you allow",
        "matrix.search_rooms" => "Search public rooms",
        "matrix.send_message" => "Send messages in rooms you allow",
        "matrix.space_info" => "See details of your spaces",
        "matrix.space_rooms" => "List the rooms in your spaces",
        "matrix.spaces" => "List your spaces",
        "matrix.successor" => "Follow upgraded rooms to their new room",
        "matrix.thread_replies" => "Read thread replies in rooms you allow",
        "matrix.thread_reply" => "Reply in threads in rooms you allow",
        "matrix.typing" => "Show that you are typing in rooms you allow",
        "matrix.unread" => "See unread counts for your rooms",
        "matrix.user_profile" => "See other people's Matrix names and pictures",
        "octos.session.open" => "Open its own conversation with the assistant",
        "octos.session.history" => "Read its own conversations with the assistant",
        "octos.turn.start" => {
            "Ask the assistant to work for it, using the device's AI settings"
        }
        "octos.turn.interrupt" => "Stop assistant work it started",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_contract_capability_is_told_in_plain_words() {
        for capability in octosense_app_contract::KNOWN_CAPABILITIES {
            let manifest = octosense_app_contract::parse(
                &serde_json::json!({
                    "schema": 1, "id": "dev.example.app", "version": "1", "name": "App",
                    "integrity": {"bundle_blake3": ""}, "capabilities": [capability]
                })
                .to_string(),
            )
            .unwrap();
            let lines = super::permissions(&manifest);
            assert_ne!(lines, [format!("Use {capability}")], "{capability} has no words");
        }
    }
}
