---
name: Code Review
description: Systematic code review using tools. Read files, grep for patterns, report findings with severity.
type: active
match: code review, review code, audit code, review workspace, 审查代码, 代码审查, 审查项目, 审查工作区
companions: Multi Review
---

# Code Review

Rules for systematic code review. Each rule prevents a specific failure class.

## 0. Parallel review fan-out is mandatory

For any repository or multi-file review, the parent agent must emit at least four
read-only `task` calls in the same assistant turn so they execute concurrently:

1. correctness, state transitions, and lifecycle;
2. security, sandbox boundaries, and tool permissions;
3. concurrency, streaming, performance, and resource ownership;
4. tests, public contracts, UI behavior, and regression gaps.

Give every sub-agent a concrete, non-overlapping scope, expected `file:line`
evidence, and completion criteria. Never send the whole review to one giant
sub-agent. The parent must verify candidate lines, deduplicate overlapping
findings, and synthesize the final severity-sorted report. A direct single-agent
review is allowed only when the target is one small file and the user explicitly
limits the scope to that file.

## 1. Read before judging

Before reporting any issue, use `read_file` to read the target file. Never
review from memory or guess at line numbers.

```
# Bad: "There's a bug around line 50" (guessed from memory)
# Good: read_file the file → "NullPointerException risk at src/main.kt:47 — list access without null check"
```

Failure avoided: wrong line numbers, hallucinated code, missed context.

## 2. Check for OWASP Top 10 and common bugs

Use `grep` to scan for these patterns across the codebase:

| Pattern | What it finds | Severity |
|---------|--------------|----------|
| `catch\s*\(\s*Exception` | Swallowing all exceptions | HIGH |
| `Runtime\.getRuntime\(\)\.exec` | Command injection | CRITICAL |
| `\.print\(|System\.out` | Debug output left in code | LOW |
| `TODO|FIXME|XXX|HACK` | Unfinished work | MEDIUM |
| `\.get\(` (without null check) | Potential NullPointer | MEDIUM |
| `Thread\.sleep\(` | Blocking call on UI thread | HIGH |

Failure avoided: missing security vulnerabilities or common bug patterns.

## 3. Report with file:line references

Every finding must include the exact `file:line` from `grep` results. No vague
descriptions.

```
# Bad: "The error handling is weak in the network module"
# Good: "Uncaught IOException at api/HttpClient.kt:89 — httpClient.send() can throw; add try-catch or propagate"
```

Failure avoided: developer cannot locate or reproduce the reported issue.

## 4. Structured output format

After review, produce a severity-sorted table:

```
## Code Review: <filename>

| Severity | File:Line | Issue | Fix |
|----------|-----------|-------|-----|
| CRITICAL | src/db.kt:42 | Raw SQL concatenation → SQL injection | Use parameterized query with `?` placeholders |
| HIGH | src/api.kt:89 | Uncaught IOException from HTTP call | Add try-catch with retry logic |
| MEDIUM | src/util.kt:156 | O(n²) nested loop — 1000 items → 1M iterations | Precompute HashMap lookup; O(n) |
```

Assign severity: **CRITICAL** (crashes/security), **HIGH** (bugs/data loss),
**MEDIUM** (perf/maintainability), **LOW** (style/nits).

Failure avoided: unstructured feedback that requires follow-up clarification.
