use crate::{
    error::{KfResult, LocalizedError},
    state::AppState,
    types::{HistoryItem, SessionSnapshot},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

const ARCHIVE_VERSION: u32 = 1;
const MAX_ARCHIVE_BYTES: u64 = 96 * 1024 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationArchive {
    version: u32,
    sessions: Vec<SessionSnapshot>,
    histories: BTreeMap<String, Vec<HistoryItem>>,
}

fn archive_path(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .storage_dir
        .read()
        .as_ref()
        .map(|directory| directory.join("conversations.json"))
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("json.bak")
}

fn message_clock(session: &SessionSnapshot) -> &str {
    session
        .messages
        .last()
        .map(|message| message.created_at.as_str())
        .unwrap_or("")
}

fn snapshot(state: &AppState) -> ConversationArchive {
    let mut sessions = state.sessions.read().values().cloned().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        message_clock(right)
            .cmp(message_clock(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    let histories = state
        .histories
        .read()
        .iter()
        .map(|(id, history)| (id.clone(), history.clone()))
        .collect();
    ConversationArchive {
        version: ARCHIVE_VERSION,
        sessions,
        histories,
    }
}

/// Persist all visible conversations and the canonical provider history. The
/// backup makes an interrupted Windows replace recoverable on next launch.
pub fn save(state: &AppState) -> KfResult<()> {
    let Some(path) = archive_path(state) else {
        return Ok(());
    };
    let directory = path
        .parent()
        .ok_or_else(|| LocalizedError::new("error.session_store_path"))?;
    fs::create_dir_all(directory)?;
    let bytes = serde_json::to_vec(&snapshot(state))
        .map_err(|error| LocalizedError::new("error.session_store_encode").arg("detail", error))?;
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(LocalizedError::new("error.session_store_large"));
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    if path.exists() {
        let backup = backup_path(&path);
        fs::copy(&path, &backup)?;
        fs::remove_file(&path)?;
    }
    fs::rename(&temporary, &path)?;
    Ok(())
}

fn read_archive(path: &Path) -> Option<ConversationArchive> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let archive: ConversationArchive = serde_json::from_slice(&bytes).ok()?;
    (archive.version == ARCHIVE_VERSION).then_some(archive)
}

/// Restore saved sessions without reviving interrupted generations. Corrupt
/// primary data falls back to the last complete archive and never blocks boot.
pub fn load(state: &AppState) {
    let Some(path) = archive_path(state) else {
        return;
    };
    let Some(mut archive) = read_archive(&path).or_else(|| read_archive(&backup_path(&path)))
    else {
        return;
    };
    let mut sessions = BTreeMap::new();
    let mut tasks = BTreeMap::new();
    for session in &mut archive.sessions {
        if session.status == "streaming" {
            session.status = "idle".into();
            session.last_error = None;
        }
        if let Some(task) = &session.task {
            tasks.insert(session.id.clone(), task.clone());
        }
        sessions.insert(session.id.clone(), session.clone());
    }
    archive
        .histories
        .retain(|session_id, _| sessions.contains_key(session_id));
    *state.sessions.write() = sessions.into_iter().collect();
    *state.tasks.write() = tasks.into_iter().collect();
    *state.histories.write() = archive.histories.into_iter().collect();
}

pub fn sorted_sessions(state: &AppState) -> Vec<SessionSnapshot> {
    let mut sessions = state.sessions.read().values().cloned().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        message_clock(right)
            .cmp(message_clock(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageSnapshot, SettingsSnapshot, UsageSnapshot};

    fn session(status: &str) -> SessionSnapshot {
        SessionSnapshot {
            id: "saved-session".into(),
            title: "Saved".into(),
            provider_id: "provider".into(),
            model_id: "model".into(),
            project_root: Some("D:/project".into()),
            status: status.into(),
            last_error: None,
            messages: vec![MessageSnapshot {
                id: "message".into(),
                role: "user".into(),
                content: "remember this conversation".into(),
                created_at: "42".into(),
                attachments: Vec::new(),
            }],
            task: None,
            usage: UsageSnapshot::default(),
        }
    }

    #[test]
    fn conversations_round_trip_and_interrupted_turns_recover_idle() {
        let directory = tempfile::tempdir().unwrap();
        let first = AppState::new(SettingsSnapshot::default());
        first.set_storage_dir(directory.path().to_path_buf());
        first
            .sessions
            .write()
            .insert("saved-session".into(), session("streaming"));
        first.histories.write().insert(
            "saved-session".into(),
            vec![HistoryItem::User {
                turn_id: "turn".into(),
                content: "hello".into(),
                attachments: Vec::new(),
            }],
        );
        save(&first).unwrap();

        let second = AppState::new(SettingsSnapshot::default());
        second.set_storage_dir(directory.path().to_path_buf());
        load(&second);

        let restored = second.sessions.read()["saved-session"].clone();
        assert_eq!(restored.status, "idle");
        assert_eq!(restored.messages[0].content, "remember this conversation");
        assert_eq!(second.histories.read()["saved-session"].len(), 1);
    }

    #[test]
    fn corrupt_archive_does_not_block_startup() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("conversations.json"), b"not json").unwrap();
        let state = AppState::new(SettingsSnapshot::default());
        state.set_storage_dir(directory.path().to_path_buf());
        load(&state);
        assert!(state.sessions.read().is_empty());
    }
}
