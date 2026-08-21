use crate::{
    error::{KfResult, LocalizedError},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use walkdir::WalkDir;

include!(concat!(env!("OUT_DIR"), "/builtin_skills.rs"));

const MAX_SKILL_BYTES: u64 = 128 * 1024;
const MAX_SELECTED_SKILLS: usize = 3;
const SKILL_TOOL_COMPATIBILITY: &str = "Use only KnightFrame's exposed find/search/read/edit/run/task tools. Translate legacy read_file/glob/grep/ls to find/search/read, bash to run, multi_edit/edit_file to edit, and todo_write/complete_step to task. Do not invent unavailable tools.";

#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub companions: Vec<String>,
    pub body: String,
    pub source: String,
    pub version: String,
    pub compatibility: String,
    pub compatibility_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub source: String,
    pub version: String,
    pub enabled: bool,
    pub compatibility: String,
    pub compatibility_notes: Vec<String>,
    pub routed: u64,
    pub injected_bytes: u64,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUsageStats {
    pub routed: u64,
    pub injected_bytes: u64,
    pub last_generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedSkill {
    pub id: String,
    pub name: String,
    pub source: String,
    pub version: String,
    pub reason: String,
    pub compatibility: String,
    pub compatibility_notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillRoute {
    pub generation: u64,
    pub selected: Vec<SelectedSkill>,
    pub injected_bytes: usize,
}

impl SkillRoute {
    pub fn directory(&self) -> Option<String> {
        render_directory(&self.selected)
    }

    pub fn receipt(&self, turn_id: &str) -> Value {
        serde_json::json!({
            "id": format!("skill:{turn_id}"),
            "turnId": turn_id,
            "generation": self.generation,
            "selected": self.selected,
            "injectedBytes": self.injected_bytes,
            "estimatedTokens": (self.injected_bytes as u64).div_ceil(4),
            "router": "local-keyword",
            "selectorModelUsed": false,
        })
    }
}

fn render_directory(selected: &[SelectedSkill]) -> Option<String> {
    if selected.is_empty() {
        return None;
    }
    let mut output =
        String::from("Relevant skills. Call the skill tool with an id before applying it:\n");
    for skill in selected {
        output.push_str("- ");
        output.push_str(&skill.id);
        output.push_str(": ");
        output.push_str(&skill.name);
        output.push('\n');
    }
    Some(output)
}

pub fn route_turn(state: &AppState, root: Option<&str>, prompt: &str) -> SkillRoute {
    let generation = state.skill_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let catalog = load_catalog(root);
    let states = load_effective_states(root);
    let normalized = prompt.to_lowercase();
    let explicit = explicit_skill_ids(&normalized, &catalog);
    let mut ranked = catalog
        .iter()
        .filter(|skill| !internal_skill(&skill.id))
        .filter(|skill| states.get(&skill.id).copied().unwrap_or(true))
        .filter_map(|skill| {
            let (score, reason) = score(skill, &normalized, &explicit);
            (score >= 10).then_some((score, skill, reason))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });

    let mut selected = Vec::new();
    let mut selected_ids = BTreeSet::new();
    for (_, skill, reason) in ranked {
        if selected.len() >= MAX_SELECTED_SKILLS {
            break;
        }
        push_selected(&mut selected, &mut selected_ids, skill, reason);
    }
    let companion_ids = selected
        .iter()
        .filter_map(|selected| catalog.iter().find(|skill| skill.id == selected.id))
        .flat_map(|skill| skill.companions.iter())
        .map(|name| slug(name))
        .collect::<Vec<_>>();
    for companion_id in companion_ids {
        if selected.len() >= MAX_SELECTED_SKILLS || internal_skill(&companion_id) {
            break;
        }
        if let Some(skill) = catalog.iter().find(|skill| skill.id == companion_id)
            && states.get(&skill.id).copied().unwrap_or(true)
        {
            push_selected(&mut selected, &mut selected_ids, skill, "companion".into());
        }
    }
    let injected_bytes = render_directory(&selected).map_or(0, |directory| directory.len());
    {
        let mut stats = state.skill_usage.write();
        for skill in &selected {
            let entry = stats.entry(skill.id.clone()).or_default();
            entry.routed = entry.routed.saturating_add(1);
            entry.injected_bytes = entry
                .injected_bytes
                .saturating_add(injected_bytes as u64 / selected.len().max(1) as u64);
            entry.last_generation = generation;
        }
    }
    SkillRoute {
        generation,
        selected,
        injected_bytes,
    }
}

fn push_selected(
    selected: &mut Vec<SelectedSkill>,
    selected_ids: &mut BTreeSet<String>,
    skill: &SkillDefinition,
    reason: String,
) {
    if !selected_ids.insert(skill.id.clone()) {
        return;
    }
    selected.push(SelectedSkill {
        id: skill.id.clone(),
        name: skill.name.clone(),
        source: skill.source.clone(),
        version: skill.version.clone(),
        reason,
        compatibility: skill.compatibility.clone(),
        compatibility_notes: skill.compatibility_notes.clone(),
    });
}

fn score(skill: &SkillDefinition, prompt: &str, explicit: &BTreeSet<String>) -> (u32, String) {
    if explicit.contains(&skill.id) {
        return (100, "explicit".into());
    }
    if skill
        .triggers
        .iter()
        .any(|trigger| contains_signal(prompt, trigger))
    {
        return (30, "trigger".into());
    }
    if aliases(&skill.id)
        .iter()
        .filter(|alias| reliable_signal(alias))
        .any(|alias| contains_signal(prompt, alias))
    {
        return (20, "keyword".into());
    }
    if reliable_signal(&skill.name) && contains_signal(prompt, &skill.name) {
        return (18, "name".into());
    }
    (0, "none".into())
}

fn reliable_signal(signal: &str) -> bool {
    let signal = signal.trim();
    !signal.is_ascii() || signal.len() >= 4
}

fn explicit_skill_ids(prompt: &str, catalog: &[SkillDefinition]) -> BTreeSet<String> {
    catalog
        .iter()
        .filter(|skill| {
            [
                format!("${}", skill.id),
                format!("skill:{}", skill.id),
                format!("skill {}", skill.id),
                format!("启用技能{}", skill.name.to_lowercase()),
            ]
            .iter()
            .any(|marker| prompt.contains(marker))
        })
        .map(|skill| skill.id.clone())
        .collect()
}

fn contains_signal(text: &str, signal: &str) -> bool {
    let signal = signal.trim().trim_matches(['\'', '"']).to_lowercase();
    if signal.is_empty() {
        return false;
    }
    if !signal.is_ascii() {
        return text.contains(&signal);
    }
    text.match_indices(&signal).any(|(start, value)| {
        let before = text[..start].chars().next_back();
        let after = text[start + value.len()..].chars().next();
        !before.is_some_and(word_character) && !after.is_some_and(word_character)
    })
}

fn word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

pub fn load_catalog(root: Option<&str>) -> Vec<SkillDefinition> {
    let mut catalog = BTreeMap::new();
    for (filename, text) in BUILTIN_SKILLS {
        if let Some(skill) = parse_skill(filename, text, "builtin") {
            catalog.insert(skill.id.clone(), skill);
        }
    }
    if let Some(home) = home_directory() {
        for directory in [
            home.join(".knightframe/skills"),
            home.join(".agent/skills"),
            home.join(".agents/skills"),
            home.join(".claude/skills"),
            home.join(".codex/skills"),
        ] {
            load_directory(&directory, "user", &mut catalog);
        }
    }
    if let Some(root) = root {
        let root = Path::new(root);
        for directory in [
            root.join(".knightframe/skills"),
            root.join(".agent/skills"),
            root.join(".agents/skills"),
            root.join(".claude/skills"),
            root.join(".codex/skills"),
        ] {
            load_directory(&directory, "project", &mut catalog);
        }
    }
    catalog.into_values().collect()
}

pub fn load_for_agent(root: Option<&str>, name: &str) -> KfResult<String> {
    let id = slug(name);
    let states = load_effective_states(root);
    let skill = load_catalog(root)
        .into_iter()
        .find(|skill| skill.id == id && !internal_skill(&skill.id))
        .ok_or_else(|| LocalizedError::new("error.tool_argument").arg("field", "name"))?;
    if !states.get(&skill.id).copied().unwrap_or(true) {
        return Err(LocalizedError::new("error.tool_argument").arg("field", "disabled skill"));
    }
    Ok(format!(
        "# {}\n\n{}\n\nHost compatibility: {}",
        skill.name, skill.body, SKILL_TOOL_COMPATIBILITY
    ))
}

fn load_directory(directory: &Path, source: &str, catalog: &mut BTreeMap<String, SkillDefinition>) {
    if !directory.is_dir() {
        return;
    }
    let mut entries = WalkDir::new(directory)
        .max_depth(4)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_string_lossy().as_ref(),
                ".git" | "build" | "node_modules" | ".gradle"
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let path = entry.path();
            path.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
                || (entry.depth() == 1
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("md")))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path().to_path_buf());
    for entry in entries {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > MAX_SKILL_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let filename = if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("SKILL.md")
        {
            entry
                .path()
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("skill")
        } else {
            entry.file_name().to_str().unwrap_or("skill.md")
        };
        if let Some(skill) = parse_skill(filename, &text, source) {
            catalog.insert(skill.id.clone(), skill);
        }
    }
}

fn parse_skill(filename: &str, text: &str, source: &str) -> Option<SkillDefinition> {
    let (frontmatter, body) = split_frontmatter(text);
    let name = frontmatter
        .get("name")
        .and_then(|values| values.last())
        .map(|value| unquote(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            filename
                .trim_end_matches(".md")
                .trim_end_matches(".MD")
                .to_owned()
        });
    let id = slug(&name);
    if id.is_empty() || body.trim().is_empty() {
        return None;
    }
    let description = frontmatter
        .get("description")
        .and_then(|values| values.last())
        .map(|value| unquote(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| generated_description(body));
    let mut triggers = list_field(&frontmatter, "match");
    triggers.extend(list_field(&frontmatter, "triggers"));
    triggers.sort();
    triggers.dedup();
    let companions = list_field(&frontmatter, "companions");
    let mut notes = Vec::new();
    if frontmatter
        .get("type")
        .is_some_and(|values| values.iter().any(|value| unquote(value) == "passive"))
    {
        notes.push("legacy passive mode is routed on demand".into());
    }
    let legacy_tools = [
        "read_file",
        "write_file",
        "edit_file",
        "multi_edit",
        "todo_write",
        "complete_step",
        "web_search",
        "web_fetch",
        "ask_user",
        "enable_skill",
    ];
    if legacy_tools.iter().any(|tool| body.contains(tool)) {
        notes.push("legacy tool names use the KnightFrame compatibility mapping".into());
    }
    if body.contains("subagent") || body.contains("sub-agent") {
        notes.push("subagent-specific steps are unavailable in this runtime".into());
    }
    let supported_fields = BTreeSet::from([
        "name",
        "description",
        "type",
        "match",
        "triggers",
        "companions",
        "version",
    ]);
    let unsupported_fields = frontmatter
        .keys()
        .filter(|key| !supported_fields.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_fields.is_empty() {
        notes.push(format!(
            "unsupported metadata is not executed: {}",
            unsupported_fields.join(", ")
        ));
    }
    Some(SkillDefinition {
        id,
        name,
        description,
        triggers,
        companions,
        body: body.trim().to_owned(),
        source: source.into(),
        version: frontmatter
            .get("version")
            .and_then(|values| values.last())
            .map(|value| unquote(value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "legacy".into()),
        compatibility: if notes.is_empty() {
            "supported".into()
        } else {
            "degraded".into()
        },
        compatibility_notes: notes,
    })
}

fn split_frontmatter(text: &str) -> (BTreeMap<String, Vec<String>>, &str) {
    let Some(remainder) = text.strip_prefix("---") else {
        return (BTreeMap::new(), text);
    };
    let remainder = remainder
        .strip_prefix("\r\n")
        .or_else(|| remainder.strip_prefix('\n'));
    let Some(remainder) = remainder else {
        return (BTreeMap::new(), text);
    };
    let mut offset = 0;
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in remainder.split_inclusive('\n') {
        let trimmed = line.trim();
        offset += line.len();
        if trimmed == "---" {
            return (fields, &remainder[offset..]);
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            fields
                .entry(key.trim().to_lowercase())
                .or_default()
                .push(value.trim().to_owned());
        }
    }
    (BTreeMap::new(), text)
}

fn list_field(fields: &BTreeMap<String, Vec<String>>, key: &str) -> Vec<String> {
    fields
        .get(key)
        .into_iter()
        .flatten()
        .flat_map(|value| {
            value
                .trim_matches(['[', ']'])
                .split(',')
                .map(unquote)
                .collect::<Vec<_>>()
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches(['\'', '"']).trim().to_owned()
}

fn generated_description(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("On-demand workflow skill")
        .chars()
        .take(180)
        .collect()
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.to_lowercase().chars() {
        if character.is_alphanumeric() {
            if separator && !output.is_empty() {
                output.push('-');
            }
            separator = false;
            output.push(character);
        } else {
            separator = true;
        }
    }
    output.trim_matches('-').to_owned()
}

fn internal_skill(id: &str) -> bool {
    matches!(id, "caveman" | "skill-router")
}

fn aliases(id: &str) -> &'static [&'static str] {
    match id {
        "code-review" => &["review", "audit", "审查", "复审", "代码检查"],
        "code-review-exhaustive" => &["exhaustive review", "deep review", "全面审查", "彻底审查"],
        "multi-review" => &["parallel review", "multi review", "多路审查"],
        "debug-4-phase" => &[
            "debug", "diagnose", "failure", "调试", "排查", "崩溃", "报错",
        ],
        "test-runner" => &["test", "测试", "回归"],
        "red-green" => &["tdd", "red green", "测试驱动"],
        "code-refactor" => &["refactor", "cleanup code", "重构", "整理代码"],
        "architect" => &["architecture", "design plan", "架构", "方案", "规划"],
        "blueprint" => &["blueprint", "implementation plan", "实施计划", "技术方案"],
        "code-explorer" => &[
            "explore",
            "codebase",
            "understand project",
            "探索",
            "了解项目",
            "阅读工程",
        ],
        "task-manager" => &[
            "multi-step",
            "long task",
            "workflow",
            "多步骤",
            "长任务",
            "工作流",
        ],
        "swarm-dev" => &["parallel", "subagent", "swarm", "并行", "子代理", "多代理"],
        "frontend-design" => &["frontend", "ui", "visual", "前端", "界面", "视觉"],
        "ui-ux-pro-max" => &["ux", "responsive", "layout", "交互", "响应式", "排版"],
        "huashu-design" => &[
            "prototype",
            "slides",
            "animation",
            "原型",
            "幻灯片",
            "动画",
            "海报",
        ],
        "web-research" => &["web research", "online research", "联网调研", "网页搜索"],
        "deep-research" => &["deep research", "sources", "深度研究", "资料研究"],
        "document-generator" => &["document", "report", "docx", "文档", "报告", "办公"],
        "paper-assistant" => &["paper", "citation", "academic", "论文", "引用", "学术"],
        "security-review" => &[
            "security",
            "vulnerability",
            "sandbox",
            "安全",
            "漏洞",
            "沙箱",
        ],
        "intent-clarifier" => &[
            "clarify",
            "ambiguous",
            "grillme",
            "澄清",
            "歧义",
            "需求不清",
        ],
        "goal-corrector" => &["goal drift", "wrong direction", "目标偏离", "方向错误"],
        "idea-forge" => &["brainstorm", "ideas", "创意", "头脑风暴"],
        "configure-environment" => &["configure", "environment", "setup", "配置环境", "安装环境"],
        "project-init" => &["initialize project", "init project", "初始化项目"],
        "change-verifier" => &["verify", "validate", "验收", "验证"],
        "universal-analysis" => &["analyze", "assessment", "分析", "评估"],
        "codebase-audit" => &["lines of code", "language breakdown", "代码量", "项目规模"],
        "issue-tracker" => &["github issue", "gitlab issue", "议题", "工单"],
        "pr-workflow" => &["pull request", "merge request", "pr流程", "合并请求"],
        "pr-guardian" => &["watch pr", "monitor pr", "守护pr", "监控pr"],
        "webhook-manager" => &["webhook", "web hook", "网络钩子"],
        _ => &[],
    }
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn load_state(path: &Path) -> BTreeMap<String, bool> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, bool>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(name, enabled)| (slug(&name), enabled))
        .collect()
}

fn load_effective_states(root: Option<&str>) -> BTreeMap<String, bool> {
    let mut states = home_directory()
        .map(|home| load_state(&home.join(".knightframe/skill-states.json")))
        .unwrap_or_default();
    if let Some(root) = root {
        states.extend(load_state(
            &Path::new(root).join(".knightframe/skill-states.json"),
        ));
    }
    states
}

#[tauri::command]
pub fn kf_skill_catalog(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    root: Option<String>,
) -> Vec<SkillCatalogEntry> {
    let states = load_effective_states(root.as_deref());
    let usage = state.skill_usage.read();
    let settings = state.settings.read().clone();
    load_catalog(root.as_deref())
        .into_iter()
        .map(|skill| {
            let stats = usage.get(&skill.id).cloned().unwrap_or_default();
            let suggestion = if settings.skill_opt {
                if skill.triggers.is_empty() {
                    Some("add-explicit-triggers".to_owned())
                } else if skill.body.len() > 12_000 {
                    Some("move-bulk-details-to-references".to_owned())
                } else {
                    None
                }
            } else {
                None
            };
            let enabled = match skill.id.as_str() {
                "caveman" => settings.caveman_mode == "lite",
                "skill-router" => false,
                _ => states.get(&skill.id).copied().unwrap_or(true),
            };
            SkillCatalogEntry {
                enabled,
                id: skill.id,
                name: skill.name,
                description: skill.description,
                triggers: skill.triggers,
                source: skill.source,
                version: skill.version,
                compatibility: skill.compatibility,
                compatibility_notes: skill.compatibility_notes,
                routed: stats.routed,
                injected_bytes: stats.injected_bytes,
                suggestion,
            }
        })
        .collect()
}

#[tauri::command]
pub fn kf_skill_set_enabled(
    scope: String,
    root: Option<String>,
    name: String,
    enabled: bool,
) -> KfResult<()> {
    let path = match scope.as_str() {
        "user" => home_directory()
            .ok_or_else(|| LocalizedError::new("error.io").arg("detail", "home unavailable"))?
            .join(".knightframe/skill-states.json"),
        "project" => Path::new(
            root.as_deref()
                .ok_or_else(|| LocalizedError::new("error.project_none"))?,
        )
        .join(".knightframe/skill-states.json"),
        _ => return Err(LocalizedError::new("error.tool_argument").arg("field", "scope")),
    };
    persist_state(&path, &name, enabled)
}

fn persist_state(path: &Path, name: &str, enabled: bool) -> KfResult<()> {
    let mut states = load_state(path);
    states.insert(slug(name), enabled);
    let parent = path
        .parent()
        .ok_or_else(|| LocalizedError::new("error.io").arg("detail", "state path"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&states)
            .map_err(|error| LocalizedError::new("error.settings_encode").arg("detail", error))?,
    )?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SettingsSnapshot;

    #[test]
    fn all_legacy_builtins_are_embedded_and_parseable() {
        assert_eq!(BUILTIN_SKILLS.len(), 34);
        let embedded = BUILTIN_SKILLS
            .iter()
            .filter_map(|(filename, text)| parse_skill(filename, text, "builtin"))
            .collect::<Vec<_>>();
        assert_eq!(embedded.len(), 34);
        let catalog = load_catalog(None);
        assert!(catalog.iter().any(|skill| skill.id == "code-review"));
        assert!(catalog.iter().any(|skill| skill.id == "caveman"));
    }

    #[test]
    fn chinese_and_english_keywords_route_without_directory_prompt() {
        let state = AppState::new(SettingsSnapshot::default());
        let review = route_turn(&state, None, "请审查代码并报告问题");
        assert!(
            review
                .selected
                .iter()
                .any(|skill| skill.id == "code-review")
        );
        let directory = review.directory().unwrap();
        assert!(directory.contains("code-review: Code Review"));
        assert!(!directory.contains("Host compatibility"));

        let debug = route_turn(&state, None, "diagnose this failure");
        assert!(
            debug
                .selected
                .iter()
                .any(|skill| skill.id == "debug-4-phase")
        );
    }

    #[test]
    fn unmatched_prompt_has_zero_skill_prompt_and_caveman_is_not_routed() {
        let state = AppState::new(SettingsSnapshot::default());
        let route = route_turn(&state, None, "hello there");
        assert!(route.selected.is_empty());
        assert!(route.directory().is_none());
        assert_eq!(route.injected_bytes, 0);

        let route = route_turn(&state, None, "caveman lite");
        assert!(!route.selected.iter().any(|skill| skill.id == "caveman"));
    }

    #[test]
    fn project_package_overrides_user_style_flat_skill() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join(".knightframe/skills/custom");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("SKILL.md"),
            "---\nname: Custom\ndescription: project version\nmatch: 特制\n---\nProject body",
        )
        .unwrap();

        let catalog = load_catalog(Some(temp.path().to_str().unwrap()));
        let custom = catalog.iter().find(|skill| skill.id == "custom").unwrap();
        assert_eq!(custom.source, "project");
        assert_eq!(custom.description, "project version");
    }

    #[test]
    fn compatibility_is_explicit_for_legacy_tools_and_passive_mode() {
        let skill = parse_skill(
            "legacy.md",
            "---\nname: Legacy\ntype: passive\n---\nUse read_file and bash.",
            "user",
        )
        .unwrap();
        assert_eq!(skill.compatibility, "degraded");
        assert_eq!(skill.compatibility_notes.len(), 2);
    }

    #[test]
    fn project_state_disables_a_locally_routed_skill() {
        let temp = tempfile::tempdir().unwrap();
        let state_dir = temp.path().join(".knightframe");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            state_dir.join("skill-states.json"),
            r#"{"code-review":false}"#,
        )
        .unwrap();
        let state = AppState::new(SettingsSnapshot::default());
        let route = route_turn(&state, Some(temp.path().to_str().unwrap()), "review code");
        assert!(!route.selected.iter().any(|skill| skill.id == "code-review"));
    }

    #[test]
    fn receipt_reports_local_routing_without_a_selector_model() {
        let state = AppState::new(SettingsSnapshot::default());
        let route = route_turn(&state, None, "debug this failure");
        let receipt = route.receipt("turn-1");
        assert_eq!(receipt["id"], "skill:turn-1");
        assert_eq!(receipt["router"], "local-keyword");
        assert_eq!(receipt["selectorModelUsed"], false);
        assert!(receipt["injectedBytes"].as_u64().unwrap() > 0);
        assert_eq!(
            receipt["injectedBytes"].as_u64().unwrap() as usize,
            route.directory().unwrap().len()
        );
    }

    #[test]
    fn ordinary_benchmark_wording_does_not_route_short_or_metadata_skills() {
        let state = AppState::new(SettingsSnapshot::default());
        let route = route_turn(
            &state,
            None,
            "Do not create artifacts. Fix the failing code and run its tests.",
        );
        assert!(!route.selected.iter().any(|skill| skill.id == "do"));
        assert!(
            !route
                .selected
                .iter()
                .any(|skill| skill.id == "project-artifact")
        );
    }

    #[test]
    fn skill_body_is_loaded_only_by_exact_tool_name() {
        let state = AppState::new(SettingsSnapshot::default());
        let route = route_turn(&state, None, "$code-review inspect this patch");
        let directory = route.directory().unwrap();
        let body = load_for_agent(None, "code-review").unwrap();
        assert!(directory.len() < body.len());
        assert!(!directory.contains("Host compatibility"));
        assert!(body.contains("Host compatibility"));
    }

    #[test]
    fn state_store_updates_existing_user_or_project_choice() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("skill-states.json");
        persist_state(&path, "Code Review", false).unwrap();
        persist_state(&path, "Code Review", true).unwrap();
        persist_state(&path, "Debug 4-Phase", false).unwrap();

        let state = load_state(&path);
        assert_eq!(state.get("code-review"), Some(&true));
        assert_eq!(state.get("debug-4-phase"), Some(&false));
        let serialized = fs::read_to_string(path).unwrap();
        assert!(
            serialized.find("code-review").unwrap() < serialized.find("debug-4-phase").unwrap()
        );
    }
}
