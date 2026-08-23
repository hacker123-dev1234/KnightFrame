use crate::{
    skill::SkillUsageStats,
    types::{
        HistoryItem, MessageAttachment, ProjectSnapshot, SessionSnapshot, SettingsSnapshot,
        TaskSnapshot,
    },
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
pub struct IndexedTextLine {
    pub number: usize,
    pub text: String,
    pub folded: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub relative: String,
    pub language: String,
    pub size: u64,
    pub search_lines: Vec<IndexedTextLine>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IndexedProject {
    pub snapshot: ProjectSnapshot,
    pub files: Vec<FileRecord>,
}

#[derive(Clone)]
pub struct ToolObservation {
    pub reference: String,
    pub epoch: u64,
    pub tool: String,
    pub projection: serde_json::Value,
    pub artifact_id: String,
    pub successful: bool,
}

#[derive(Default)]
pub struct ToolObservationIndex {
    pub epoch: u64,
    pub next_reference: u64,
    pub by_key: HashMap<String, String>,
    pub by_reference: HashMap<String, ToolObservation>,
}

pub struct AppState {
    /// Set by the desktop bootstrap. Headless/test states intentionally keep
    /// this empty so they never write into a user's profile.
    pub storage_dir: RwLock<Option<PathBuf>>,
    pub settings: RwLock<SettingsSnapshot>,
    pub sessions: RwLock<HashMap<String, SessionSnapshot>>,
    pub tasks: RwLock<HashMap<String, TaskSnapshot>>,
    pub projects: RwLock<HashMap<PathBuf, IndexedProject>>,
    pub active_project: RwLock<Option<PathBuf>>,
    pub active_turns: RwLock<HashMap<String, ActiveTurn>>,
    pub histories: RwLock<HashMap<String, Vec<HistoryItem>>>,
    pub accepted_requirement_briefs: RwLock<HashMap<String, String>>,
    pub artifacts: RwLock<HashMap<String, serde_json::Value>>,
    pub tool_observations: RwLock<HashMap<String, ToolObservationIndex>>,
    pub available_models: RwLock<HashSet<String>>,
    pub skill_generation: AtomicU64,
    pub skill_usage: RwLock<HashMap<String, SkillUsageStats>>,
    pub memory: RwLock<crate::memory::MemoryArchive>,
    pub client: reqwest::Client,
    pub auxiliary_client: reqwest::Client,
}

impl AppState {
    pub fn new(settings: SettingsSnapshot) -> Arc<Self> {
        Arc::new(Self {
            storage_dir: RwLock::new(None),
            settings: RwLock::new(settings),
            sessions: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            projects: RwLock::new(HashMap::new()),
            active_project: RwLock::new(None),
            active_turns: RwLock::new(HashMap::new()),
            histories: RwLock::new(HashMap::new()),
            accepted_requirement_briefs: RwLock::new(HashMap::new()),
            artifacts: RwLock::new(HashMap::new()),
            tool_observations: RwLock::new(HashMap::new()),
            available_models: RwLock::new(HashSet::new()),
            skill_generation: AtomicU64::new(0),
            skill_usage: RwLock::new(HashMap::new()),
            memory: RwLock::new(crate::memory::MemoryArchive::default()),
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(8))
                .user_agent("KnightFrame/0.1")
                .build()
                .expect("valid HTTP client"),
            auxiliary_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(8))
                .user_agent("KnightFrame-Auxiliary/0.1")
                .build()
                .expect("valid auxiliary HTTP client"),
        })
    }

    pub fn set_storage_dir(&self, directory: PathBuf) {
        *self.storage_dir.write() = Some(directory);
    }
}

#[derive(Clone)]
pub struct ActiveTurn {
    pub turn_id: String,
    pub cancellation: CancellationToken,
    pub guidance: Arc<parking_lot::Mutex<VecDeque<QueuedGuidance>>>,
    pub accepting_guidance: Arc<AtomicBool>,
    pub guidance_signal: Arc<Notify>,
}

#[derive(Clone)]
pub struct QueuedGuidance {
    pub content: String,
    pub clarify: bool,
    pub attachments: Vec<MessageAttachment>,
}

impl ActiveTurn {
    pub fn guide(&self, guidance: QueuedGuidance) -> bool {
        if !self.accepting_guidance.load(Ordering::Acquire) {
            return false;
        }
        let mut queue = self.guidance.lock();
        if !self.accepting_guidance.load(Ordering::Acquire) {
            return false;
        }
        queue.push_back(guidance);
        drop(queue);
        self.guidance_signal.notify_one();
        true
    }

    pub fn drain_guidance(&self) -> Vec<QueuedGuidance> {
        self.guidance.lock().drain(..).collect()
    }

    pub fn close_or_drain_guidance(&self) -> Vec<QueuedGuidance> {
        let mut queue = self.guidance.lock();
        if queue.is_empty() {
            self.accepting_guidance.store(false, Ordering::Release);
            Vec::new()
        } else {
            queue.drain(..).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_turn() -> ActiveTurn {
        ActiveTurn {
            turn_id: "turn".into(),
            cancellation: CancellationToken::new(),
            guidance: Arc::new(parking_lot::Mutex::new(VecDeque::new())),
            accepting_guidance: Arc::new(AtomicBool::new(true)),
            guidance_signal: Arc::new(Notify::new()),
        }
    }

    #[test]
    fn guidance_is_drained_and_closes_without_losing_a_message() {
        let turn = active_turn();
        assert!(turn.guide(QueuedGuidance {
            content: "continue with tests".into(),
            clarify: false,
            attachments: Vec::new(),
        }));
        assert_eq!(turn.close_or_drain_guidance().len(), 1);
        assert!(turn.close_or_drain_guidance().is_empty());
        assert!(!turn.guide(QueuedGuidance {
            content: "too late".into(),
            clarify: false,
            attachments: Vec::new(),
        }));
    }

    #[tokio::test]
    async fn guidance_wakes_the_active_provider_round() {
        let turn = active_turn();
        let signal = turn.guidance_signal.clone();
        assert!(turn.guide(QueuedGuidance {
            content: "steer now".into(),
            clarify: false,
            attachments: Vec::new(),
        }));
        tokio::time::timeout(std::time::Duration::from_millis(50), signal.notified())
            .await
            .expect("guidance signal must wake immediately");
    }
}
