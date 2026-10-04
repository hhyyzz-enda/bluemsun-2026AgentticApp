//! Reopen targets for the reader, stored separately for each signed-in account.
//! Documents are fetched again through Matrix; their plaintext is not saved here.
use std::{
    io::{Read, Write},
    path::Path,
};
use ruma::{OwnedEventId, OwnedRoomId, UserId};
use serde::{Deserialize, Serialize};
use super::attachment_download::DownloadableAttachment;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReaderTab {
    Web {
        url: String,
    },
    Article {
        room: OwnedRoomId,
        event: OwnedEventId,
    },
    Markdown {
        attachment: DownloadableAttachment,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReaderSession {
    pub tabs: Vec<ReaderTab>,
    pub active: usize,
}

impl ReaderSession {
    pub fn load(user: &UserId) -> anyhow::Result<Self> {
        Self::load_from(&crate::persistence::persistent_state_dir(user).join("reader_tabs.json"))
    }

    pub fn save(&self, user: &UserId) -> anyhow::Result<()> {
        self.save_to(&crate::persistence::persistent_state_dir(user).join("reader_tabs.json"))
    }

    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "Reader session exceeds its size limit"
        );
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        serde_json::to_writer(&mut file, self)?;
        file.flush()?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::attachment_download::DownloadKind;

    #[test]
    fn round_trip_keeps_mixed_tab_order_and_selection_without_document_body() {
        let session = ReaderSession {
            active: 1,
            tabs: vec![
                ReaderTab::Web {
                    url: "https://example.org/article".into(),
                },
                ReaderTab::Markdown {
                    attachment: DownloadableAttachment {
                        media_source: ruma::events::room::MediaSource::Plain(
                            "mxc://example.org/notes".into(),
                        ),
                        filename: "notes.md".into(),
                        size: Some(123),
                        kind: DownloadKind::Markdown,
                    },
                },
                ReaderTab::Article {
                    room: "!room:example.org".try_into().unwrap(),
                    event: "$article".try_into().unwrap(),
                },
            ],
        };
        let path = std::env::temp_dir().join(format!(
            "rinx-reader-test-{}-{}.json",
            std::process::id(),
            makepad_widgets::LiveId::unique().0
        ));
        session.save_to(&path).unwrap();
        let restored = ReaderSession::load_from(&path).unwrap();
        assert_eq!(restored.active, 1);
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&session).unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // Explicitly closing the reader replaces, rather than merges, its saved tabs.
        ReaderSession::default().save_to(&path).unwrap();
        assert!(ReaderSession::load_from(&path).unwrap().tabs.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
