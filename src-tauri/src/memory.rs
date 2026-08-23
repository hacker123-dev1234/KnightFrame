use crate::{
    error::{KfResult, LocalizedError},
    state::AppState,
    types::HistoryItem,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path, time::SystemTime};

const MEMORY_VERSION: u32 = 1;
const MAX_MEMORY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRY_CHARS: usize = 420;
const MAX_NEW_PER_COMPACTION: usize = 8;
const MAX_CONTEXT_ENTRIES: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub kind: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    pub content: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryArchive {
    pub version: u32,
    pub next_id: u64,
    pub entries: Vec<MemoryEntry>,
}

impl Default for MemoryArchive {
    fn default() -> Self {
        Self {
            version: MEMORY_VERSION,
            next_id: 1,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MemoryCuration {
    pub discarded: usize,
    pub saved: usize,
    pub axioms: usize,
}

fn archive_path(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .storage_dir
        .read()
        .as_ref()
        .map(|directory| directory.join("memory.json"))
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("json.bak")
}

fn read_archive(path: &Path) -> Option<MemoryArchive> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_MEMORY_BYTES {
        return None;
    }
    let mut archive: MemoryArchive = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if archive.version != MEMORY_VERSION {
        return None;
    }
    archive.entries.retain(|entry| {
        matches!(entry.kind.as_str(), "memory" | "axiom")
            && matches!(entry.scope.as_str(), "global" | "project")
            && !entry.content.trim().is_empty()
            && entry.content.chars().count() <= MAX_ENTRY_CHARS
    });
    Some(archive)
}

pub fn load(state: &AppState) {
    let Some(path) = archive_path(state) else {
        return;
    };
    if let Some(archive) = read_archive(&path).or_else(|| read_archive(&backup_path(&path))) {
        *state.memory.write() = archive;
    }
}

fn save(state: &AppState) -> KfResult<()> {
    let Some(path) = archive_path(state) else {
        return Ok(());
    };
    let directory = path
        .parent()
        .ok_or_else(|| LocalizedError::new("error.memory_store_path"))?;
    fs::create_dir_all(directory)?;
    let bytes = serde_json::to_vec(&*state.memory.read())
        .map_err(|error| LocalizedError::new("error.memory_store_encode").arg("detail", error))?;
    if bytes.len() as u64 > MAX_MEMORY_BYTES {
        return Err(LocalizedError::new("error.memory_store_large"));
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    if path.exists() {
        fs::copy(&path, backup_path(&path))?;
        fs::remove_file(&path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn clean_content(content: &str) -> String {
    let filtered = content.chars().filter(|character| {
        !matches!(
            character,
            '\u{0000}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
        )
    });
    let compact = filtered
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    compact.chars().take(MAX_ENTRY_CHARS).collect()
}

fn contains_any(text: &str, signals: &[&str]) -> bool {
    signals.iter().any(|signal| text.contains(signal))
}

fn classify(content: &str) -> Option<&'static str> {
    let normalized = content.to_lowercase();
    let absolute = contains_any(
        &normalized,
        &["绝不", "永远不要", "never ", "always ", "do not ever"],
    );
    let hard_rule = contains_any(
        &normalized,
        &["必须", "禁止", "务必", "不能再", "must ", "must not"],
    ) && contains_any(
        &normalized,
        &[
            "以后",
            "默认",
            "每次",
            "始终",
            "一律",
            "原则",
            "规则",
            "这个项目",
            "本项目",
            "所有项目",
            "from now on",
            "by default",
            "every ",
            "project",
            " rule",
        ],
    );
    let axiom = absolute || hard_rule;
    if axiom {
        return Some("axiom");
    }
    let durable = contains_any(
        &normalized,
        &[
            "记住",
            "以后",
            "默认",
            "长期",
            "偏好",
            "我喜欢",
            "我不喜欢",
            "从现在开始",
            "remember",
            "from now on",
            "my preference",
            "i prefer",
            "by default",
        ],
    );
    durable.then_some("memory")
}

fn global_scope(content: &str) -> bool {
    let normalized = content.to_lowercase();
    contains_any(
        &normalized,
        &[
            "所有项目",
            "任何项目",
            "以后都",
            "全局",
            "always",
            "all projects",
            "every project",
            "my preference",
            "i prefer",
        ],
    )
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Classify only user-authored messages removed by real history compaction.
/// Assistant guesses and ordinary turns can never create durable memory.
pub fn curate_evicted(
    state: &AppState,
    project_root: Option<&str>,
    evicted: &[HistoryItem],
) -> KfResult<MemoryCuration> {
    if !state.settings.read().memory_enabled {
        return Ok(MemoryCuration::default());
    }
    let mut report = MemoryCuration::default();
    let mut candidates = Vec::new();
    for item in evicted {
        let HistoryItem::User { content, .. } = item else {
            continue;
        };
        let cleaned = clean_content(content);
        let Some(kind) = classify(&cleaned) else {
            report.discarded += 1;
            continue;
        };
        if cleaned.chars().count() < 6 {
            report.discarded += 1;
            continue;
        }
        let global = project_root.is_none() || global_scope(&cleaned);
        candidates.push((kind, global, cleaned));
        if candidates.len() >= MAX_NEW_PER_COMPACTION {
            break;
        }
    }

    let mut archive = state.memory.write();
    let mut known = archive
        .entries
        .iter()
        .map(|entry| entry.content.to_lowercase())
        .collect::<BTreeSet<_>>();
    for (kind, global, content) in candidates {
        if !known.insert(content.to_lowercase()) {
            continue;
        }
        let id = format!("m{}", archive.next_id);
        archive.next_id = archive.next_id.saturating_add(1);
        archive.entries.push(MemoryEntry {
            id,
            kind: kind.into(),
            scope: if global { "global" } else { "project" }.into(),
            project_root: (!global).then(|| project_root.unwrap_or_default().to_owned()),
            content,
            created_at: now_seconds(),
        });
        if kind == "axiom" {
            report.axioms += 1;
        } else {
            report.saved += 1;
        }
    }
    drop(archive);
    if report.saved > 0 || report.axioms > 0 {
        save(state)?;
    }
    Ok(report)
}

fn signals(text: &str) -> BTreeSet<String> {
    let folded = text.to_lowercase();
    let mut result = folded
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| word.len() >= 3)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let chinese = folded
        .chars()
        .filter(|character| ('\u{4e00}'..='\u{9fff}').contains(character))
        .collect::<Vec<_>>();
    for pair in chinese.windows(2) {
        result.insert(pair.iter().collect());
    }
    result
}

/// Return a small, deterministic memory snapshot only when the current request
/// shares a real signal with stored content. Project axioms are always active.
pub fn relevant_context(
    state: &AppState,
    project_root: Option<&str>,
    prompt: &str,
) -> Option<String> {
    if !state.settings.read().memory_enabled {
        return None;
    }
    let prompt_signals = signals(prompt);
    let archive = state.memory.read();
    let mut ranked = archive
        .entries
        .iter()
        .filter(|entry| {
            entry.scope == "global"
                || project_root.is_some_and(|root| entry.project_root.as_deref() == Some(root))
        })
        .filter_map(|entry| {
            let overlap = signals(&entry.content)
                .intersection(&prompt_signals)
                .count();
            let project_axiom = entry.kind == "axiom"
                && entry.scope == "project"
                && project_root.is_some_and(|root| entry.project_root.as_deref() == Some(root));
            (project_axiom || overlap > 0).then_some((project_axiom, overlap, entry))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.created_at.cmp(&left.2.created_at))
    });
    ranked.truncate(MAX_CONTEXT_ENTRIES);
    if ranked.is_empty() {
        return None;
    }
    let mut output =
        String::from("Relevant durable memory (user-authored; apply only when relevant):\n");
    for (_, _, entry) in ranked {
        output.push_str(if entry.kind == "axiom" {
            "- axiom: "
        } else {
            "- memory: "
        });
        output.push_str(&entry.content);
        output.push('\n');
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SettingsSnapshot;

    fn user(content: &str) -> HistoryItem {
        HistoryItem::User {
            turn_id: content.into(),
            content: content.into(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn memory_is_off_by_default_and_ordinary_text_is_discarded() {
        let state = AppState::new(SettingsSnapshot::default());
        let report = curate_evicted(&state, Some("project"), &[user("必须保持测试通过")]).unwrap();
        assert_eq!(report, MemoryCuration::default());
        assert!(state.memory.read().entries.is_empty());
    }

    #[test]
    fn compaction_saves_explicit_preferences_and_axioms_only() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(SettingsSnapshot {
            memory_enabled: true,
            ..Default::default()
        });
        state.set_storage_dir(directory.path().to_path_buf());
        let report = curate_evicted(
            &state,
            Some("D:/project"),
            &[
                user("今天天气怎么样"),
                user("以后默认使用中文回答"),
                user("这个项目禁止直接列目录"),
            ],
        )
        .unwrap();
        assert_eq!(report.discarded, 1);
        assert_eq!(report.saved, 1);
        assert_eq!(report.axioms, 1);
        assert_eq!(state.memory.read().entries.len(), 2);
    }

    #[test]
    fn one_off_must_word_is_not_promoted_to_an_axiom() {
        assert_eq!(classify("本次必须修复这个按钮"), None);
        assert_eq!(classify("这个项目以后必须先用索引"), Some("axiom"));
    }

    #[test]
    fn retrieval_requires_overlap_except_for_project_axioms() {
        let state = AppState::new(SettingsSnapshot {
            memory_enabled: true,
            ..Default::default()
        });
        state.memory.write().entries = vec![MemoryEntry {
            id: "m1".into(),
            kind: "memory".into(),
            scope: "global".into(),
            project_root: None,
            content: "默认使用 Rust 开发工具".into(),
            created_at: 1,
        }];
        assert!(relevant_context(&state, None, "今天天气如何").is_none());
        assert!(relevant_context(&state, None, "继续 Rust 工具开发").is_some());
    }
}
