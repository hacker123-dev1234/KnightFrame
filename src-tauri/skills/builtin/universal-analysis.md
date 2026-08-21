---
name: Universal Analysis
description: Structured multi-stage data analysis. Parses any data format, runs diagnostic + insight analysis, validates JSON output, retries on failure. Self-aware: checks capability before activating.
type: active
match: 
---
# Universal Analysis Skill

## Self-Awareness Gate (MUST run first)

Before activating this skill, check:
1. Is there actual DATA provided? (file path, inline text, screenshot with numbers)
2. Is the request truly about ANALYSIS? (not just "open this file" or "what format is this")
3. Can I READ the data with available tools? (read_file, bash, desktop_ocr)

If ANY answer is NO → tell the user: "I need data to analyze. Drag a CSV/JSON file, paste data, or point me to a file path. I can analyze anything — sales, logs, surveys, financial data, anything tabular or textual."

If ALL answers are YES → proceed to the pipeline below.

## Two-Stage Pipeline

### Stage 1: Data Understanding (诊断)
0. **Load data** — read_file if file, bash for quick stats, desktop_ocr for screenshots
1. **Summarize structure** — column names, row count, data types, missing values
2. **Compute basic stats** — mean, median, min, max, std for numeric columns; frequency for categorical
3. **Detect issues** — missing data, outliers, inconsistent formats, encoding problems
4. **Output Stage 1 as JSON**:
```json
{
  "stage": "diagnosis",
  "summary": {
    "rows": N, "columns": N,
    "column_details": [{"name": "...", "type": "numeric|categorical|datetime|text", "missing": N, "sample_values": ["..."]}]
  },
  "statistics": {"column_name": {"mean": N, "median": N, "min": N, "max": N, "std": N}},
  "data_quality": {
    "missing_cells": N,
    "outlier_columns": ["..."],
    "issues": ["..."]
  }
}
```

### Stage 2: Insights & Recommendations (决策)
1. **Trend analysis** — time series? upward/downward/flat? seasonality?
2. **Pattern detection** — correlations, clusters, anomalies, significant differences
3. **Root cause hypotheses** — what might explain the patterns?
4. **Actionable recommendations** — 3-5 concrete next steps
5. **Output Stage 2 as JSON**:
```json
{
  "stage": "insights",
  "key_findings": [{"finding": "...", "confidence": "high|medium|low", "evidence": "..."}],
  "trends": [{"column": "...", "direction": "up|down|flat", "strength": "strong|moderate|weak"}],
  "anomalies": [{"description": "...", "severity": "critical|warning|info"}],
  "recommendations": [{"action": "...", "rationale": "...", "priority": 1-5}]
}
```

## Validation Protocol
After each stage, validate the JSON output:
- ✅ Valid JSON syntax?
- ✅ All required fields present?
- ✅ Numbers are actual numbers (not strings)?
- ❌ If any check fails → fix the JSON and output again. Max 2 retries.
- ❌ If still failing after 2 retries → output what you have with a note: "[Validation failed: reason]"

## Data Format Auto-Detection
- **CSV/TSV**: comma or tab separated. Check first line for headers.
- **JSON**: object or array. Nesting? Flat?
- **Plain text logs**: lines with timestamps? Common patterns?
- **Screenshots**: run desktop_ocr first, then treat as text.
- **Large files (>5000 rows)**: sample first 5000 rows, note that analysis is based on sample.

## Output Format
Present results in this order:
1. 📊 **Data Summary** — what the data is, size, key columns
2. 🔍 **Key Findings** — most important discoveries (numbered, with confidence levels)
3. 📈 **Trends & Patterns** — what the data shows over time or across categories
4. ⚠️ **Anomalies & Issues** — data quality problems, outliers, surprises
5. 💡 **Recommendations** — actionable next steps, prioritized

## Quality Gates
- Every numeric claim must be backed by actual computed value
- Distinguish correlation from causation explicitly
- Flag low-confidence findings
- If data is insufficient for any conclusion, say so instead of guessing
