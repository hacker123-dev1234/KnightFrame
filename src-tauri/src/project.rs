use crate::{
    error::{KfResult, LocalizedError},
    state::{AppState, FileRecord, IndexedProject, IndexedTextLine},
    types::{GraphEdge, GraphNode, GraphSnapshot, GraphStats, ProjectSnapshot, RuntimeEvent},
};
use ignore::WalkBuilder;
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{AppHandle, Emitter, Manager};

const MAX_QUERY_MATCHES: usize = 24;
const MAX_QUERY_RELATIONS: usize = 6;
// Initial one-shot projection: slightly wider so real hubs (not test dirs)
// are visible at first glance, cutting blind find/search round-trips.
const MAX_CONTEXT_NODES: usize = 9;
const MAX_CONTEXT_RELATIONS: usize = 3;
const MAX_CONTEXT_BYTES: usize = 2_600;
const MAX_INDEXED_TEXT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRelation {
    pub kind: String,
    pub direction: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMatch {
    pub path: String,
    pub language: String,
    pub size: u64,
    pub relations: Vec<ProjectRelation>,
    pub relation_total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectQueryResult {
    pub root: String,
    pub matches: Vec<ProjectMatch>,
    pub total: usize,
    pub truncated: bool,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

pub fn query_index(
    state: &AppState,
    root: &str,
    query: &str,
    path_prefix: Option<&str>,
    offset: usize,
) -> KfResult<ProjectQueryResult> {
    let canonical = canonical_root(Path::new(root))?;
    let projects = state.projects.read();
    let project = projects
        .get(&canonical)
        .ok_or_else(|| LocalizedError::new("error.project_not_indexed"))?;
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Err(LocalizedError::new("error.search_query"));
    }
    let prefix = path_prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.replace('\\', "/").to_ascii_lowercase());
    let needles: Vec<&str> = query.split_whitespace().collect();
    let mut candidates: Vec<_> = project
        .files
        .iter()
        .filter(|file| {
            let candidate = file.relative.to_ascii_lowercase();
            prefix
                .as_ref()
                .map(|prefix| candidate.starts_with(prefix.as_str()))
                .unwrap_or(true)
                && needles.iter().all(|needle| candidate.contains(needle))
        })
        .map(|file| (match_rank(&file.relative, &query), file))
        .collect();
    candidates.sort_by(|(left_rank, left), (right_rank, right)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left.relative.cmp(&right.relative))
    });
    let total = candidates.len();
    let next_offset =
        (offset + MAX_QUERY_MATCHES < total).then_some(offset.saturating_add(MAX_QUERY_MATCHES));
    let graph = graph_snapshot(project);
    let node_paths: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.path.as_str()))
        .collect();
    let matches = candidates
        .into_iter()
        .skip(offset)
        .take(MAX_QUERY_MATCHES)
        .map(|(_, file)| file)
        .map(|file| {
            let (relations, relation_total) = direct_relations(
                &graph,
                &node_paths,
                &format!("file:{}", file.relative),
                MAX_QUERY_RELATIONS,
            );
            ProjectMatch {
                path: file.relative.clone(),
                language: file.language.clone(),
                size: file.size,
                relations,
                relation_total,
            }
        })
        .collect();
    Ok(ProjectQueryResult {
        root: canonical.display().to_string(),
        matches,
        total,
        truncated: next_offset.is_some(),
        offset,
        next_offset,
    })
}

fn match_rank(path: &str, query: &str) -> u8 {
    let path = path.to_ascii_lowercase();
    let file_name = Path::new(&path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&path);
    let file_stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);
    if path == query {
        0
    } else if file_name == query {
        1
    } else if file_stem == query {
        2
    } else if file_name.starts_with(query) {
        3
    } else if path.starts_with(query) {
        4
    } else {
        5
    }
}

pub fn model_context(state: &AppState, root: &str) -> KfResult<String> {
    let canonical = canonical_root(Path::new(root))?;
    let projects = state.projects.read();
    let project = projects
        .get(&canonical)
        .ok_or_else(|| LocalizedError::new("error.project_not_indexed"))?;
    let graph = graph_snapshot(project);
    // Scope line first: the centrality list below is a ranked excerpt of the
    // WHOLE workspace graph, never a subprojection — models kept reading the
    // top-N list as "the repo is only this subtree".
    let mut context = format!(
        "Workspace graph (whole repo, ranked excerpt — {} of {} nodes shown, tests down-ranked; use find for other subtrees): name={}; links={}; central:",
        MAX_CONTEXT_NODES.min(graph.nodes.len()),
        graph.nodes.len(),
        project.snapshot.name,
        graph.edges.len()
    );
    for line in central_graph_lines(&graph, MAX_CONTEXT_NODES, MAX_CONTEXT_RELATIONS) {
        if context.len() + line.len() + 1 > MAX_CONTEXT_BYTES {
            break;
        }
        context.push('\n');
        context.push_str(&line);
    }
    Ok(context)
}

/// Test fixtures and harnesses accumulate degree without being architecture.
/// Down-rank them in centrality so real hubs (large sources, fan-in cores)
/// surface in the excerpt instead of test scaffolding.
fn is_test_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    let name = lowered.rsplit(['/', '\\']).next().unwrap_or(&lowered);
    lowered.split(['/', '\\']).any(|segment| {
        matches!(
            segment,
            "tests" | "test" | "__tests__" | "spec" | "specs" | "testing"
        )
    }) || name.starts_with("test_")
        || name.ends_with("_test.rs")
        || name.ends_with("_tests.rs")
        || name.ends_with(".test.js")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.js")
        || name.ends_with(".spec.ts")
        || name.ends_with("_test.py")
        || name.ends_with("_test.go")
}

fn direct_relations(
    graph: &GraphSnapshot,
    node_paths: &HashMap<&str, &str>,
    node_id: &str,
    limit: usize,
) -> (Vec<ProjectRelation>, usize) {
    let mut unique = BTreeSet::new();
    for edge in &graph.edges {
        let relation = if edge.source == node_id {
            node_paths
                .get(edge.target.as_str())
                .map(|path| (edge.kind.as_str(), "out", *path))
        } else if edge.target == node_id {
            node_paths
                .get(edge.source.as_str())
                .map(|path| (edge.kind.as_str(), "in", *path))
        } else {
            None
        };
        if let Some((kind, direction, path)) = relation {
            unique.insert((relation_rank(kind), kind, direction, path));
        }
    }
    let total = unique.len();
    let relations = unique
        .into_iter()
        .take(limit)
        .map(|(_, kind, direction, path)| ProjectRelation {
            kind: kind.into(),
            direction: direction.into(),
            path: path.into(),
        })
        .collect();
    (relations, total)
}

fn relation_rank(kind: &str) -> u8 {
    match kind {
        "depends" => 0,
        "contains" => 1,
        _ => 2,
    }
}

fn central_graph_lines(
    graph: &GraphSnapshot,
    node_limit: usize,
    relation_limit: usize,
) -> Vec<String> {
    let mut metrics = HashMap::<&str, (usize, f32)>::new();
    for node in &graph.nodes {
        metrics.insert(node.id.as_str(), (0, 0.0));
    }
    for edge in &graph.edges {
        for node_id in [edge.source.as_str(), edge.target.as_str()] {
            let metric = metrics.entry(node_id).or_default();
            metric.0 += 1;
            metric.1 += edge.weight;
        }
    }
    let mut ranked: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| !(node.kind == "directory" && node.path == "."))
        .filter_map(|node| {
            let (degree, strength) = metrics.get(node.id.as_str()).copied()?;
            let score = node.weight + strength + degree as f32 * 2.0;
            // Tests still rank (they can be genuinely central) but only after
            // real code of comparable weight — see is_test_path.
            let score = if is_test_path(&node.path) {
                score * 0.25
            } else {
                score
            };
            Some((node, degree, score))
        })
        .collect();
    ranked.sort_by(
        |(left, left_degree, left_score), (right, right_degree, right_score)| {
            right_score
                .total_cmp(left_score)
                .then_with(|| right_degree.cmp(left_degree))
                .then_with(|| left.path.cmp(&right.path))
        },
    );
    let node_paths: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.path.as_str()))
        .collect();
    ranked
        .into_iter()
        .take(node_limit)
        .map(|(node, degree, _)| {
            let (relations, total) = direct_relations(graph, &node_paths, &node.id, relation_limit);
            let relations = relations
                .into_iter()
                .map(|relation| {
                    let arrow = if relation.direction == "out" {
                        "->"
                    } else {
                        "<-"
                    };
                    format!(
                        "{} {arrow} {}",
                        relation.kind,
                        compact_path(&relation.path, 72)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let omitted = total.saturating_sub(relation_limit);
            let suffix = if omitted > 0 {
                format!(", +{omitted}")
            } else {
                String::new()
            };
            format!(
                "{} [{}; weight={:.1}; degree={degree}] => {relations}{suffix}",
                compact_path(&node.path, 96),
                node.kind,
                node.weight
            )
        })
        .collect()
}

fn compact_path(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_owned();
    }
    let tail: String = path
        .chars()
        .rev()
        .take(max_chars.saturating_sub(3))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}

pub fn canonical_root(path: &Path) -> KfResult<PathBuf> {
    let root = path.canonicalize().map_err(|e| {
        LocalizedError::new("error.project_root")
            .arg("path", path.display())
            .arg("detail", e)
    })?;
    if !root.is_dir() {
        return Err(LocalizedError::new("error.project_not_directory").arg("path", root.display()));
    }
    Ok(root)
}

pub fn include_project_entry(entry: &ignore::DirEntry) -> bool {
    if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return true;
    }
    !matches!(
        entry.file_name().to_string_lossy().as_ref(),
        ".git"
            | ".gradle"
            | ".idea"
            | ".next"
            | ".venv"
            | "__pycache__"
            | "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "target"
            | "venv"
    )
}

pub fn resolve_inside(root: &Path, input: &Path, must_exist: bool) -> KfResult<PathBuf> {
    let candidate = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let resolved = if must_exist {
        candidate.canonicalize().map_err(|e| {
            LocalizedError::new("error.path_missing")
                .arg("path", candidate.display())
                .arg("detail", e)
        })?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| LocalizedError::new("error.path_parent"))?;
        let parent = parent
            .canonicalize()
            .map_err(|e| LocalizedError::new("error.path_parent").arg("detail", e))?;
        parent.join(
            candidate
                .file_name()
                .ok_or_else(|| LocalizedError::new("error.path_name"))?,
        )
    };
    Ok(resolved)
}

pub(crate) fn resolve_indexed_tool_path(
    state: &AppState,
    root: &Path,
    input: &str,
    must_exist: bool,
) -> KfResult<PathBuf> {
    let trimmed = input.trim();
    let graph_path = trimmed
        .strip_prefix("file:")
        .or_else(|| trimmed.strip_prefix("directory:"))
        .unwrap_or(trimmed);
    let normalized = graph_path.replace('\\', "/");
    let expanded = if let Some(suffix) = normalized.strip_prefix("...") {
        let suffix = suffix.trim_start_matches('/').to_ascii_lowercase();
        let canonical = canonical_root(root)?;
        let projects = state.projects.read();
        let matches = projects
            .get(&canonical)
            .into_iter()
            .flat_map(|project| project.files.iter())
            .filter(|file| file.relative.to_ascii_lowercase().ends_with(&suffix))
            .map(|file| file.relative.as_str())
            .take(2)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [unique] => (*unique).to_owned(),
            [] => normalized,
            _ => {
                return Err(LocalizedError::new("error.tool_argument")
                    .arg("field", "path")
                    .arg("detail", "abbreviated graph path is ambiguous"));
            }
        }
    } else {
        normalized
    };
    resolve_inside(root, Path::new(&expanded), must_exist)
}

fn language(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" | "mjs" => "JavaScript",
        "kt" | "kts" => "Kotlin",
        "java" => "Java",
        "py" => "Python",
        "json" => "JSON",
        "md" => "Markdown",
        "toml" => "TOML",
        "html" => "HTML",
        "css" => "CSS",
        _ => "Other",
    }
    .into()
}

fn indexed_text_lines(path: &Path, declared_size: u64) -> Vec<IndexedTextLine> {
    if declared_size > MAX_INDEXED_TEXT_BYTES {
        return Vec::new();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut bytes = Vec::with_capacity(declared_size as usize);
    if file
        .take(MAX_INDEXED_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_INDEXED_TEXT_BYTES
        || bytes.contains(&0)
    {
        return Vec::new();
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return Vec::new();
    };
    content
        .lines()
        .enumerate()
        .map(|(index, text)| IndexedTextLine {
            number: index + 1,
            folded: text.to_lowercase(),
            text: text.to_owned(),
        })
        .collect()
}

/// Reopening an unchanged project must not pay a full re-index. The manifest
/// archive stores the last built `IndexedProject` (including text search
/// lines) keyed by canonical root; a lightweight walk comparing
/// (relative, size) pairs decides whether the archive is still fresh.
/// Only a fingerprint mismatch pays the full `build_manifest` cost.
///
/// Metadata-only walk: same traversal rules as `build_manifest` without
/// reading file contents. Returns sorted (relative, size) pairs.
fn quick_scan(root: &Path) -> KfResult<Vec<(String, u64)>> {
    let root = canonical_root(root)?;
    let mut files = Vec::new();
    for entry in WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(false)
        .filter_entry(include_project_entry)
        .build()
    {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let path = entry.into_path();
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        files.push((
            path.strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
            metadata.len(),
        ));
    }
    files.sort();
    Ok(files)
}

/// FNV-1a over the lowercased canonical root: a stable, dependency-free
/// archive key (self-written archive, read back by the same version).
fn archive_key(root: &Path) -> String {
    let text = root.to_string_lossy().to_lowercase();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.json")
}

fn archive_path(app: &AppHandle, root: &Path) -> Option<std::path::PathBuf> {
    let directory = app.path().app_config_dir().ok()?.join("index");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory.join(archive_key(root)))
}

fn store_manifest(app: &AppHandle, root: &Path, indexed: &IndexedProject) {
    let Some(path) = archive_path(app, root) else {
        return;
    };
    if let Ok(bytes) = serde_json::to_vec(indexed) {
        let _ = std::fs::write(path, bytes);
    }
}

/// Load the archived manifest when the workspace fingerprint still matches;
/// any path/size drift (external edits, git checkout, branch switch) falls
/// through to a full rebuild.
fn load_fresh_archive(root: &Path, archive: &std::path::Path) -> Option<IndexedProject> {
    let bytes = std::fs::read(archive).ok()?;
    let cached: IndexedProject = serde_json::from_slice(&bytes).ok()?;
    let scan = quick_scan(root).ok()?;
    let archived: Vec<(String, u64)> = cached
        .files
        .iter()
        .map(|file| (file.relative.clone(), file.size))
        .collect();
    (scan == archived).then_some(cached)
}

pub(crate) fn refresh_indexed_file(state: &AppState, root: &Path, path: &Path) {
    let Ok(relative) = path.strip_prefix(root) else {
        return;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let mut projects = state.projects.write();
    let Some(project) = projects.get_mut(root) else {
        return;
    };
    let Some(file) = project
        .files
        .iter_mut()
        .find(|file| file.relative == relative)
    else {
        return;
    };
    file.size = metadata.len();
    file.search_lines = indexed_text_lines(path, file.size);
}

pub fn build_manifest(root: &Path) -> KfResult<IndexedProject> {
    let root = canonical_root(root)?;
    let mut files = Vec::new();
    let mut languages = BTreeSet::new();
    let mut failures = 0usize;
    for entry in WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(false)
        .filter_entry(include_project_entry)
        .build()
    {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => {
                failures += 1;
                continue;
            }
        };
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let path = entry.into_path();
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let size = match std::fs::metadata(&path) {
            Ok(value) => value.len(),
            Err(_) => {
                failures += 1;
                continue;
            }
        };
        let file_language = language(&path);
        languages.insert(file_language.clone());
        files.push(FileRecord {
            relative,
            language: file_language,
            size,
            search_lines: indexed_text_lines(&path, size),
        });
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_owned();
    let count = files.len();
    Ok(IndexedProject {
        snapshot: ProjectSnapshot {
            root: root.display().to_string(),
            name,
            status: "ready".into(),
            stage: "manifest".into(),
            completed: count,
            total: count,
            files: count,
            languages: languages.into_iter().collect(),
            failures,
        },
        files,
    })
}

pub fn graph_snapshot(project: &IndexedProject) -> GraphSnapshot {
    let mut directories: BTreeMap<String, usize> = BTreeMap::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut file_ids = HashMap::new();

    for file in &project.files {
        let id = format!("file:{}", file.relative);
        file_ids.insert(file.relative.to_ascii_lowercase(), id.clone());
        let label = Path::new(&file.relative)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&file.relative)
            .to_owned();
        nodes.push(GraphNode {
            id: id.clone(),
            label,
            kind: "file".into(),
            path: file.relative.clone(),
            line: None,
            weight: (file.size.max(1) as f32).log10().clamp(1.0, 6.0),
        });
        let mut parent = Path::new(&file.relative).parent();
        while let Some(path) = parent {
            let relative = path.to_string_lossy().replace('\\', "/");
            if relative.is_empty() {
                break;
            }
            *directories.entry(relative).or_default() += 1;
            parent = path.parent();
        }
        let parent = Path::new(&file.relative)
            .parent()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".into());
        edges.push(GraphEdge {
            source: format!("dir:{parent}"),
            target: id,
            kind: "contains".into(),
            weight: 1.0,
        });
    }

    directories.entry(".".into()).or_insert(project.files.len());
    for (directory, count) in &directories {
        let label = if directory == "." {
            project.snapshot.name.clone()
        } else {
            Path::new(directory)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(directory)
                .to_owned()
        };
        nodes.push(GraphNode {
            id: format!("dir:{directory}"),
            label,
            kind: "directory".into(),
            path: directory.clone(),
            line: None,
            weight: ((*count + 1) as f32).log2().clamp(1.6, 8.0),
        });
        if directory != "." {
            let parent = Path::new(directory)
                .parent()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| ".".into());
            edges.push(GraphEdge {
                source: format!("dir:{parent}"),
                target: format!("dir:{directory}"),
                kind: "contains".into(),
                weight: 1.4,
            });
        }
    }

    let mut dependency_keys = BTreeSet::new();
    for file in &project.files {
        if file.size > 1_048_576
            || !matches!(
                file.language.as_str(),
                "Rust" | "TypeScript" | "JavaScript" | "Python" | "Kotlin" | "Java"
            )
        {
            continue;
        }
        for candidate in dependency_candidates(
            &file.relative,
            &file.language,
            file.search_lines.iter().map(|line| line.text.as_str()),
        ) {
            if let Some(target) = resolve_dependency(&candidate, &file_ids) {
                let source_id = format!("file:{}", file.relative);
                if source_id != target
                    && dependency_keys.insert((source_id.clone(), target.clone()))
                {
                    edges.push(GraphEdge {
                        source: source_id,
                        target,
                        kind: "depends".into(),
                        weight: 2.0,
                    });
                }
            }
        }
    }

    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    edges.sort_by(|left, right| {
        (&left.source, &left.target, &left.kind).cmp(&(&right.source, &right.target, &right.kind))
    });
    GraphSnapshot {
        root: project.snapshot.root.clone(),
        stats: GraphStats {
            files: project.files.len(),
            directories: directories.len(),
            dependencies: dependency_keys.len(),
        },
        nodes,
        edges,
    }
}

fn dependency_candidates<'a>(
    relative: &str,
    language: &str,
    lines: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let parent = Path::new(relative)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut result = Vec::new();
    for line in lines.into_iter().take(20_000) {
        let trimmed = line.trim();
        if matches!(language, "TypeScript" | "JavaScript")
            && (trimmed.starts_with("import ")
                || trimmed.starts_with("export ")
                || trimmed.contains("require("))
        {
            if let Some(value) = quoted_value(trimmed).filter(|value| value.starts_with('.')) {
                result.push(normalize_join(&parent, value));
            }
        } else if language == "Rust" {
            if let Some(name) = trimmed
                .strip_prefix("mod ")
                .and_then(|value| value.strip_suffix(';'))
            {
                result.push(normalize_join(&parent, name.trim()));
            } else if let Some(path) = trimmed.strip_prefix("use crate::") {
                result.push(
                    path.split([':', ';', '{'])
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        } else if language == "Python" {
            if let Some(path) = trimmed
                .strip_prefix("from ")
                .and_then(|value| value.split_whitespace().next())
            {
                result.push(path.trim_start_matches('.').replace('.', "/"));
            } else if let Some(path) = trimmed
                .strip_prefix("import ")
                .and_then(|value| value.split([',', ' ']).next())
            {
                result.push(path.replace('.', "/"));
            }
        } else if matches!(language, "Kotlin" | "Java")
            && let Some(path) = trimmed.strip_prefix("import ")
        {
            result.push(path.trim_end_matches(';').replace('.', "/"));
        }
    }
    result
}

fn quoted_value(line: &str) -> Option<&str> {
    let (start, quote) = line
        .char_indices()
        .find(|(_, value)| *value == '\'' || *value == '"')?;
    let remainder = &line[start + quote.len_utf8()..];
    let end = remainder.find(quote)?;
    Some(&remainder[..end])
}

fn normalize_join(parent: &str, child: &str) -> String {
    let joined = format!("{parent}/{child}").replace('\\', "/");
    let mut parts = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    parts.join("/")
}

fn resolve_dependency(candidate: &str, files: &HashMap<String, String>) -> Option<String> {
    let candidate = candidate.trim_matches('/').to_ascii_lowercase();
    let extensions = [
        "",
        ".rs",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".py",
        ".kt",
        ".java",
        "/mod.rs",
        "/index.ts",
        "/index.tsx",
        "/index.js",
        "/__init__.py",
    ];
    extensions
        .iter()
        .find_map(|extension| files.get(&format!("{candidate}{extension}")).cloned())
        .or_else(|| {
            files
                .iter()
                .find(|(path, _)| {
                    path.ends_with(&format!("/{candidate}.py"))
                        || path.ends_with(&format!("/{candidate}.kt"))
                        || path.ends_with(&format!("/{candidate}.java"))
                })
                .map(|(_, id)| id.clone())
        })
}

#[tauri::command]
pub async fn kf_project_open(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> KfResult<ProjectSnapshot> {
    let root = canonical_root(Path::new(&path))?;
    let indexing = ProjectSnapshot {
        root: root.display().to_string(),
        name: root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("project")
            .into(),
        status: "indexing".into(),
        stage: "manifest".into(),
        completed: 0,
        total: 0,
        files: 0,
        languages: vec![],
        failures: 0,
    };
    let _ = app.emit(
        "kf://runtime",
        RuntimeEvent::new("project.index_progress", json!(indexing)),
    );
    // Reuse path: in-memory index (kept fresh by refresh_indexed_file after
    // agent edits) or the on-disk manifest archive when a metadata-only walk
    // proves the workspace unchanged. Anything else pays the full rebuild.
    let memory_snapshot = {
        let projects = state.projects.read();
        projects.get(&root).cloned()
    };
    let archive = archive_path(&app, &root);
    let scan_root = root.clone();
    let reuse = tokio::task::spawn_blocking(move || {
        if let Some(project) = memory_snapshot {
            let archived: Vec<(String, u64)> = project
                .files
                .iter()
                .map(|file| (file.relative.clone(), file.size))
                .collect();
            if quick_scan(&scan_root)
                .map(|scan| scan == archived)
                .unwrap_or(false)
            {
                return Some(project);
            }
        }
        archive.and_then(|archive| load_fresh_archive(&scan_root, &archive))
    })
    .await
    .unwrap_or(None);
    let indexed = if let Some(indexed) = reuse {
        indexed
    } else {
        let build_root = root.clone();
        match tokio::task::spawn_blocking(move || build_manifest(&build_root)).await {
            Ok(Ok(indexed)) => {
                store_manifest(&app, &root, &indexed);
                indexed
            }
            result => {
                let error = match result {
                    Ok(Err(error)) => error,
                    Err(error) => {
                        LocalizedError::new("error.project_index_task").arg("detail", error)
                    }
                    Ok(Ok(_)) => unreachable!(),
                };
                let mut failed = indexing;
                failed.status = "failed".into();
                failed.stage = "failed".into();
                failed.failures = 1;
                let _ = app.emit(
                    "kf://runtime",
                    RuntimeEvent::new("project.failed", json!({"project":failed,"error":&error})),
                );
                return Err(error);
            }
        }
    };
    let snapshot = indexed.snapshot.clone();
    state.projects.write().insert(root.clone(), indexed);
    *state.active_project.write() = Some(root);
    let _ = app.emit(
        "kf://runtime",
        RuntimeEvent::new("project.ready", json!(snapshot)),
    );
    Ok(snapshot)
}

#[tauri::command]
pub fn kf_project_query(
    state: tauri::State<'_, Arc<AppState>>,
    root: String,
    query: String,
) -> KfResult<ProjectQueryResult> {
    query_index(&state, &root, &query, None, 0)
}

#[tauri::command]
pub fn kf_project_graph(
    state: tauri::State<'_, Arc<AppState>>,
    root: String,
) -> KfResult<GraphSnapshot> {
    let canonical = canonical_root(Path::new(&root))?;
    let projects = state.projects.read();
    let project = projects
        .get(&canonical)
        .ok_or_else(|| LocalizedError::new("error.project_not_indexed"))?;
    Ok(graph_snapshot(project))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_access_allows_parent_paths() {
        let temp = tempfile::tempdir().unwrap();
        let resolved = resolve_inside(temp.path(), Path::new("../escape"), false).unwrap();
        assert!(!resolved.starts_with(temp.path()));
    }

    #[test]
    fn graph_is_stable_and_resolves_local_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/main.ts"),
            "import { x } from './util';\n",
        )
        .unwrap();
        std::fs::write(temp.path().join("src/util.ts"), "export const x = 1;\n").unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        let first = graph_snapshot(&indexed);
        std::fs::remove_file(temp.path().join("src/main.ts")).unwrap();
        std::fs::remove_file(temp.path().join("src/util.ts")).unwrap();
        let second = graph_snapshot(&indexed);
        assert_eq!(
            first.nodes.iter().map(|node| &node.id).collect::<Vec<_>>(),
            second.nodes.iter().map(|node| &node.id).collect::<Vec<_>>()
        );
        assert!(first.edges.iter().any(|edge| edge.kind == "depends"
            && edge.source.ends_with("src/main.ts")
            && edge.target.ends_with("src/util.ts")));
        let ids = first
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(
            first.edges.iter().all(
                |edge| ids.contains(edge.source.as_str()) && ids.contains(edge.target.as_str())
            )
        );
    }

    #[test]
    fn manifest_excludes_generated_and_dependency_directories() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::create_dir_all(temp.path().join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(temp.path().join("target/debug")).unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "pub fn ready() {}\n").unwrap();
        std::fs::write(temp.path().join("node_modules/pkg/index.js"), "vendor\n").unwrap();
        std::fs::write(temp.path().join("target/debug/build.log"), "generated\n").unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        assert_eq!(indexed.files.len(), 1);
        assert_eq!(indexed.files[0].relative, "src/lib.rs");
    }

    #[test]
    fn indexed_query_rejects_enumeration_and_ranks_exact_names() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("docs")).unwrap();
        std::fs::write(temp.path().join("README.md"), "root\n").unwrap();
        std::fs::write(temp.path().join("docs/readme-notes.md"), "notes\n").unwrap();
        for index in 0..30 {
            std::fs::write(
                temp.path().join(format!("docs/readme-{index}.txt")),
                "fixture\n",
            )
            .unwrap();
        }
        let indexed = build_manifest(temp.path()).unwrap();
        let root = canonical_root(temp.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        let result = query_index(&state, root.to_str().unwrap(), "readme", None, 0).unwrap();
        assert_eq!(result.matches[0].path, "README.md");
        assert_eq!(result.matches.len(), MAX_QUERY_MATCHES);
        assert_eq!(result.total, 32);
        assert!(result.truncated);
        assert_eq!(result.next_offset, Some(MAX_QUERY_MATCHES));
        // offset paging: the second page continues past the first 24 matches
        let page2 = query_index(
            &state,
            root.to_str().unwrap(),
            "readme",
            None,
            result.next_offset.unwrap(),
        )
        .unwrap();
        assert_eq!(page2.matches.len(), 32 - MAX_QUERY_MATCHES);
        assert!(!page2.truncated);
        assert_eq!(page2.next_offset, None);
        // path prefix filter narrows to the docs subtree only
        let docs = query_index(&state, root.to_str().unwrap(), "readme", Some("docs"), 0).unwrap();
        assert_eq!(docs.total, 31);
        assert!(docs.matches.iter().all(|m| m.path.starts_with("docs/")));
        assert_eq!(
            query_index(&state, root.to_str().unwrap(), "   ", None, 0)
                .unwrap_err()
                .key,
            "error.search_query"
        );
    }

    #[test]
    fn indexed_query_includes_direct_dependency_relations() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::create_dir_all(temp.path().join("private")).unwrap();
        std::fs::write(
            temp.path().join("src/main.ts"),
            "import { x } from './util';\nconsole.log(x);\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("src/util.ts"),
            "import { deep } from './deep';\nexport const x = deep;\n",
        )
        .unwrap();
        std::fs::write(temp.path().join("src/deep.ts"), "export const deep = 1;\n").unwrap();
        std::fs::write(
            temp.path().join("private/credentials.txt"),
            "unrelated fixture\n",
        )
        .unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        let root = canonical_root(temp.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        let result = query_index(&state, root.to_str().unwrap(), "src/main.ts", None, 0).unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.total, 1);
        assert!(!result.truncated);
        assert_eq!(result.matches[0].path, "src/main.ts");
        assert!(result.matches[0].relation_total >= 2);
        assert!(result.matches[0].relations.iter().any(|relation| {
            relation.kind == "depends"
                && relation.direction == "out"
                && relation.path == "src/util.ts"
        }));
        assert!(
            result.matches[0]
                .relations
                .iter()
                .all(|relation| relation.path != "src/deep.ts")
        );
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("credentials.txt")
        );
    }

    #[test]
    fn manifest_archive_reuses_unchanged_workspaces_and_rejects_drift() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(temp.path().join("README.md"), "# demo\n").unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        // The archive must live outside the project root: a JSON inside the
        // tree would surface in quick_scan as workspace drift.
        let outside = tempfile::tempdir().unwrap();
        let archive = outside.path().join(archive_key(temp.path()));
        std::fs::write(&archive, serde_json::to_vec(&indexed).unwrap()).unwrap();

        // Unchanged workspace: the archive round-trips with search lines intact.
        let cached = load_fresh_archive(temp.path(), &archive).expect("fresh archive reused");
        assert_eq!(cached.files.len(), indexed.files.len());
        assert!(
            cached
                .files
                .iter()
                .any(|file| file.relative == "src/main.rs" && !file.search_lines.is_empty())
        );

        // Any drift (size change, added or removed file) invalidates reuse.
        std::fs::write(temp.path().join("src/main.rs"), "fn main() { extra() }\n").unwrap();
        assert!(load_fresh_archive(temp.path(), &archive).is_none());
        std::fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(temp.path().join("notes.txt"), "drift\n").unwrap();
        assert!(load_fresh_archive(temp.path(), &archive).is_none());
        std::fs::remove_file(temp.path().join("notes.txt")).unwrap();
        assert!(load_fresh_archive(temp.path(), &archive).is_some());

        // quick_scan must match build_manifest's (relative, size) fingerprint.
        let scan = quick_scan(temp.path()).unwrap();
        let fingerprint: Vec<(String, u64)> = indexed
            .files
            .iter()
            .map(|file| (file.relative.clone(), file.size))
            .collect();
        assert_eq!(scan, fingerprint);
    }

    #[test]
    fn model_context_is_compact_factual_and_path_relative() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        let mut hub = String::new();
        for index in 0..8 {
            hub.push_str(&format!("import {{ leaf{index} }} from './leaf{index}';\n"));
            std::fs::write(
                temp.path().join(format!("src/leaf{index}.ts")),
                format!("export const leaf{index} = {index};\n"),
            )
            .unwrap();
        }
        std::fs::write(temp.path().join("src/hub.ts"), hub).unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        let root = canonical_root(temp.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        let context = model_context(&state, root.to_str().unwrap()).unwrap();
        assert!(context.starts_with("Workspace graph (whole repo"));
        // boundary declaration: excerpt size vs full node count
        assert!(context.contains("of") && context.contains("nodes shown"));
        assert!(context.contains("links="));
        assert!(context.contains("weight="));
        assert!(context.contains("degree="));
        assert!(!context.contains("landmarks="));
        assert!(!context.contains(root.to_str().unwrap()));
        assert!(context.len() <= MAX_CONTEXT_BYTES);
        assert!(context.lines().skip(1).count() <= MAX_CONTEXT_NODES);
        let hub = context
            .lines()
            .find(|line| line.starts_with("src/hub.ts "))
            .expect("high-connectivity hub must be selected");
        assert_eq!(hub.matches("depends ->").count(), MAX_CONTEXT_RELATIONS);
        assert!(hub.contains("+6"));
        assert!(!context.contains("src/leaf7.ts"));
    }

    #[test]
    fn centrality_excerpt_downranks_test_scaffolding() {
        // tests/unit accumulates degree by importing every leaf; the real hub
        // must outrank it in the excerpt, not lose to test scaffolding.
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::create_dir_all(temp.path().join("tests/unit")).unwrap();
        let mut hub = String::new();
        for index in 0..10 {
            hub.push_str(&format!("import {{ leaf{index} }} from './leaf{index}';\n"));
            std::fs::write(
                temp.path().join(format!("src/leaf{index}.ts")),
                format!("export const leaf{index} = {index};\n"),
            )
            .unwrap();
            std::fs::write(
                temp.path().join(format!("tests/unit/case{index}.ts")),
                format!("import {{ leaf{index} }} from '../../src/leaf{index}';\n"),
            )
            .unwrap();
        }
        std::fs::write(temp.path().join("src/hub.ts"), hub).unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        let root = canonical_root(temp.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);
        let context = model_context(&state, root.to_str().unwrap()).unwrap();
        let hub_line = context
            .lines()
            .position(|line| line.starts_with("src/hub.ts "))
            .expect("real hub must appear in the excerpt");
        let test_line = context
            .lines()
            .position(|line| line.starts_with("tests/unit"));
        if let Some(test_line) = test_line {
            assert!(
                hub_line < test_line,
                "test scaffolding must rank below real hubs"
            );
        }
    }

    #[test]
    fn indexed_tool_paths_expand_graph_ids_and_unique_abbreviations() {
        let temp = tempfile::tempdir().unwrap();
        let relative = "a/very/long/project/path/whose/tail/remains/visible/source.rs";
        let file = temp.path().join(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let indexed = build_manifest(temp.path()).unwrap();
        let root = canonical_root(temp.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        let graph_id =
            resolve_indexed_tool_path(&state, &root, &format!("file:{relative}"), true).unwrap();
        let abbreviated =
            resolve_indexed_tool_path(&state, &root, "...tail/remains/visible/source.rs", true)
                .unwrap();

        assert_eq!(graph_id, file.canonicalize().unwrap());
        assert_eq!(abbreviated, file.canonicalize().unwrap());
    }
}
