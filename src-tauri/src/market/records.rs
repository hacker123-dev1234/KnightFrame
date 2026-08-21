//! 记录持久化、增量历史与经验库读取（移植自 records/pending_writer.py、analysis_history.py、experience_reader.py）。
//! 存储位置：app_config_dir/market/ 下 records/ 与 experience/。

use super::types::{AnalysisRecord, FollowupTurn, KlineBar, KlineFrame, MarketSettings};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct MarketPaths {
    pub base: PathBuf,
}

impl MarketPaths {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            base: config_dir.join("market"),
        }
    }
    pub fn records_dir(&self) -> PathBuf {
        self.base.join("records")
    }
    pub fn experience_dir(&self) -> PathBuf {
        self.base.join("experience")
    }
    pub fn settings_path(&self) -> PathBuf {
        self.base.join("settings.json")
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mask_api_key(data: &mut Value, api_key: &str) {
    if api_key.is_empty() {
        return;
    }
    match data {
        Value::String(text) => {
            *text = text.replace(
                api_key,
                &format!(
                    "***{}***",
                    api_key.chars().rev().take(4).collect::<String>()
                ),
            );
        }
        Value::Array(items) => {
            for item in items {
                mask_api_key(item, api_key);
            }
        }
        Value::Object(map) => {
            for (_, value) in map.iter_mut() {
                mask_api_key(value, api_key);
            }
        }
        _ => {}
    }
}

pub struct RecordWriter {
    pub directory: PathBuf,
    pub api_key: String,
}

impl RecordWriter {
    pub fn new(paths: &MarketPaths, api_key: &str) -> Self {
        Self {
            directory: paths.records_dir(),
            api_key: api_key.to_string(),
        }
    }

    pub fn record_id(&self, record: &AnalysisRecord) -> String {
        let symbol = record
            .meta
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("SYM");
        let timeframe = record
            .meta
            .get("timeframe")
            .and_then(Value::as_str)
            .unwrap_or("15m");
        let ts = record
            .meta
            .get("timestamp_local_ms")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms);
        format!("{ts}_{symbol}_{timeframe}")
    }

    fn persist(&self, record: &AnalysisRecord, id: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.directory)?;
        let mut value = serde_json::to_value(record).unwrap_or(Value::Null);
        mask_api_key(&mut value, &self.api_key);
        let path = self.directory.join(format!("{id}.json"));
        let temporary = self.directory.join(format!("{id}.json.tmp"));
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(&value).unwrap_or_default(),
        )?;
        std::fs::rename(&temporary, &path)?;
        Ok(path)
    }

    pub fn save_full(&self, record: &AnalysisRecord) -> std::io::Result<PathBuf> {
        self.persist(record, &self.record_id(record))
    }

    pub fn save_partial(&self, record: &AnalysisRecord, reason: &str) -> std::io::Result<PathBuf> {
        let mut record = record.clone();
        record._partial_reason = Some(reason.to_string());
        self.persist(&record, &self.record_id(&record))
    }

    pub fn append_followup(
        &self,
        record_id: &str,
        turn: &FollowupTurn,
    ) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(format!("{record_id}.followups.jsonl"));
        let line = serde_json::to_string(turn).unwrap_or_default();
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(file, "{line}")?;
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// 增量历史
// ---------------------------------------------------------------------------

pub fn list_records(directory: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.ends_with(".debug.json"))
        })
        .filter_map(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            Some((meta.modified().ok()?, path))
        })
        .collect();
    paths.sort_by_key(|(timestamp, _)| std::cmp::Reverse(*timestamp));
    paths.into_iter().map(|(_, path)| path).collect()
}

pub fn load_record(path: &Path) -> Option<AnalysisRecord> {
    let bytes = std::fs::read(path).ok()?;
    let mut record: AnalysisRecord = serde_json::from_slice(&bytes).ok()?;
    record._partial_reason = None;
    Some(record)
}

/// 列表页轻量解析：只反序列化 meta 与决策标记，跳过大数组分配。
pub struct RecordSummaryLite {
    pub meta: Value,
    pub has_decision: bool,
    pub partial: bool,
}

pub fn load_record_summary(path: &Path) -> Option<RecordSummaryLite> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Lite {
        meta: Value,
        #[serde(default)]
        stage2_decision: Option<Value>,
        #[serde(default)]
        _partial_reason: Option<String>,
    }
    let bytes = std::fs::read(path).ok()?;
    let lite: Lite = serde_json::from_slice(&bytes).ok()?;
    Some(RecordSummaryLite {
        meta: lite.meta,
        has_decision: lite.stage2_decision.is_some(),
        partial: lite._partial_reason.is_some(),
    })
}

pub fn find_latest_successful_record(
    directory: &Path,
    symbol: &str,
    timeframe: &str,
) -> Option<AnalysisRecord> {
    for path in list_records(directory) {
        let Some(record) = load_record(&path) else {
            continue;
        };
        let matches = record.meta.get("symbol").and_then(Value::as_str) == Some(symbol)
            && record.meta.get("timeframe").and_then(Value::as_str) == Some(timeframe);
        if !matches {
            continue;
        }
        let successful = record.exception.is_none()
            && record.stage1_diagnosis.is_some()
            && record.stage2_decision.is_some()
            && !record.kline_data.is_empty();
        if successful {
            return Some(record);
        }
    }
    None
}

pub struct IncrementalDelta {
    pub new_count: usize,
    pub new_bar_ts_opens: Vec<f64>,
}

/// 以上一轮 K1 的 ts_open 为锚，统计新增已收盘棒。
pub fn compute_incremental_delta(
    frame: &KlineFrame,
    previous: &AnalysisRecord,
) -> Option<IncrementalDelta> {
    let anchor = previous.kline_data.first()?.ts_open;
    let new_bars: Vec<&KlineBar> = frame
        .bars
        .iter()
        .filter(|bar| bar.ts_open > anchor + 1.0)
        .collect();
    // 锚必须存在于当前 frame（数据连续性）
    let anchor_exists = frame
        .bars
        .iter()
        .any(|bar| (bar.ts_open - anchor).abs() <= 1.0);
    if !anchor_exists {
        return None;
    }
    Some(IncrementalDelta {
        new_count: new_bars.len(),
        new_bar_ts_opens: new_bars.iter().map(|bar| bar.ts_open).collect(),
    })
}

// ---------------------------------------------------------------------------
// 经验库
// ---------------------------------------------------------------------------

pub fn read_experience(
    paths: &MarketPaths,
    cycle_position: &str,
    direction: &str,
    patterns: &[String],
    settings: &MarketSettings,
) -> Vec<Value> {
    let max_entries = settings.prompt.experience_max_entries as usize;
    if max_entries == 0 {
        return Vec::new();
    }
    let max_chars = settings.prompt.experience_max_chars_per_entry as usize;
    let root = paths.experience_dir().join(cycle_position);
    let mut candidates: Vec<(i32, Value)> = Vec::new();
    for case_type in ["success_cases", "failure_cases"] {
        let directory = root.join(case_type);
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext != "json") {
                continue;
            }
            let Ok(Ok(content)) =
                std::fs::read_to_string(&path).map(|text| serde_json::from_str::<Value>(&text))
            else {
                continue;
            };
            let case_direction = content
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut score = 0;
            if !direction.is_empty() && case_direction == direction {
                score += 2;
            }
            let case_patterns: Vec<&str> = content
                .get("patterns")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            for pattern in patterns {
                if case_patterns.contains(&pattern.as_str()) {
                    score += 1;
                }
            }
            let summary: String = content
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .take(max_chars)
                .collect();
            candidates.push((
                score,
                json!({
                    "filename": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                    "case_type": if case_type == "success_cases" { "success" } else { "failure" },
                    "cycle_position": cycle_position,
                    "summary": summary,
                }),
            ));
        }
    }
    candidates.sort_by_key(|(timestamp, _)| std::cmp::Reverse(*timestamp));
    candidates
        .into_iter()
        .take(max_entries)
        .map(|(_, value)| value)
        .collect()
}

pub fn render_experience(entries: &[Value]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let case_type = entry.get("case_type").and_then(Value::as_str).unwrap_or("");
            let summary = entry.get("summary").and_then(Value::as_str).unwrap_or("");
            let label = if case_type == "success" {
                "成功案例"
            } else {
                "失败案例"
            };
            format!("案例{index}（{label}）：{summary}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 构建记录 meta。
pub fn build_meta(frame: &KlineFrame, settings: &MarketSettings) -> Value {
    json!({
        "timestamp_local_ms": now_ms(),
        "symbol": frame.symbol,
        "timeframe": frame.timeframe,
        "bar_count": frame.bars.len(),
        "ai_provider": {
            "model": settings.provider.model,
            "base_url": settings.provider.base_url,
            "thinking": settings.provider.thinking,
            "reasoning_effort": settings.provider.reasoning_effort,
        },
        "decision_stance": settings.stance(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_delta_counts_new_bars_after_anchor() {
        let bars: Vec<KlineBar> = (0..60)
            .rev()
            .map(|i| KlineBar {
                seq: 0,
                ts_open: 1_700_000_000_000.0 + i as f64 * 900_000.0,
                open: 1.0,
                high: 2.0,
                low: 0.5,
                close: 1.5,
                volume: 1.0,
                closed: true,
            })
            .collect();
        let frame = KlineFrame {
            symbol: "T".into(),
            timeframe: "15m".into(),
            bars,
            indicators: Default::default(),
            snapshot_ts_local_ms: 1,
        };
        // 上一轮 K1 = 当前 frame 的第 3 根
        let previous = AnalysisRecord {
            meta: json!({}),
            kline_data: vec![frame.bars[3]],
            htf_text: String::new(),
            stage1_messages: vec![],
            stage1_response: None,
            stage1_diagnosis: Some(json!({"direction": "bullish"})),
            stage2_messages: vec![],
            stage2_response: None,
            stage2_decision: Some(json!({})),
            strategy_files_used: vec![],
            experience_loaded: vec![],
            exception: None,
            usage_total: json!({}),
            _partial_reason: None,
        };
        let delta = compute_incremental_delta(&frame, &previous).expect("delta");
        assert_eq!(delta.new_count, 3);
    }
}
