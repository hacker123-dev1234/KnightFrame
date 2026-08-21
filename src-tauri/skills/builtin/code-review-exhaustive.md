---
name: Code Review (Exhaustive)
description: Multi-pass exhaustive code review with 9 finder angles, verification, and gap sweep. Use when auditing new or changed code at max effort.
type: active
match: 
---

# Code Review — Exhaustive Multi-Pass Protocol

This is a multi-pass review protocol. In field use it found 18 confirmed bugs,
6 plausible, and 2 cleanup items across multiple review rounds.

## Core Rules

1. **Read every line before reporting.** Never review from memory. Use `read_file`
   to read the target file. Quote exact line numbers.
2. **A missed bug ships.** Err on the side of surfacing — a false positive wastes
   reviewer time; a missed bug wastes user time.
3. **Dedup before verify.** Two angles flagging the same line → merge into one
   candidate with the most concrete failure scenario.
4. **Verify survives uncertainty.** CONFIRMED or PLAUSIBLE carries. Only REFUTED
   drops. A single non-REFUTED vote keeps the finding.
5. **Always re-review after fixes.** Every code change batch triggers a fresh pass.
   Fixes can introduce new bugs.

---

## Phase 0 — Gather the Diff

Run `git diff HEAD` (or the appropriate range) to get the full diff under review.
Include uncommitted changes. READ EVERY FILE in the diff — do not skip unchanged
lines in touched functions (the PR re-exposes them).

If the diff is empty, the review is meaningless — error out.

---

## Phase 1 — Find Candidates (9 Angles × up to 8 each)

Run all 9 angles. Do NOT let one angle's conclusions suppress another's. Each
surfaces up to 8 candidate findings.

### Angle A — Line-by-Line Diff Scan

Read every hunk, line by line. Then read the enclosing function for each hunk.
For every line ask: what input, state, timing, or platform makes this line wrong?

Checklist:
- [ ] Inverted/wrong conditions (`==` vs `!=`, `<` vs `>`)
- [ ] Off-by-one errors (index, count, boundary)
- [ ] Null/undefined dereference (`!!`, unchecked nullable)
- [ ] Missing `await` / wrong coroutine scope
- [ ] Falsy-zero checks (`if (value)` when `value == 0` is valid)
- [ ] Wrong-variable copy-paste (`event.id` used where `event.name` intended)
- [ ] Error swallowed in empty catch block
- [ ] Unescaped regex metacharacters or JSON string injection
- [ ] Resource leak (unclosed stream, reader, connection)
- [ ] Race condition (shared mutable state without `@Volatile` or `@Synchronized`)

Example finding:
```
file: FreeSession.kt, line: 45
summary: flow{} collected inside channelFlow{} — Flow invariant violation
failure_scenario: Adapter collects session flow inside channelFlow → "Emission from another coroutine" at runtime
```

### Angle B — Removed-Behavior Auditor

For every line the diff DELETES or replaces:
1. Name the invariant or behavior it enforced
2. Search the new code for where that invariant is re-established
3. If you can't find it → candidate: removed guard, dropped error path, narrowed validation

For ports/transplants: compare the original source language (Rust, Go, etc.)
against the Kotlin/Java port. Check:
- [ ] Missing error paths (Rust `?` → Kotlin should `throw` or `return`)
- [ ] Dropped state transitions
- [ ] Simplified guards (Rust `match` exhaustive → Kotlin `when` may miss branches)
- [ ] Changed ordering (Rust checks X before Y, Kotlin checks Y before X)

Example finding:
```
file: FreeSSEParser.kt, line: 84
summary: Initial snapshot check moved after p/o application, unlike Rust which checks first
failure_scenario: First SSE frame with both 'p' and 'response' fields → snapshot path missed entirely
```

### Angle C — Cross-File Tracer

For each function the diff changes:
1. Find callers (grep for the symbol) — does the change break any call site?
2. Find callees — does a parallel change make the call unsafe?
3. Check return shapes, new exceptions, timing/ordering dependencies

Key chains to trace:
- Constructor → field init → method usage
- HTTP client → auth → request → response parsing
- SSE parser → event emit → collector → tool execution

Example finding:
```
file: ApiAdapter.kt, line: 60
summary: Password read from wrong config field — AdapterFactory doesn't pass password
failure_scenario: config.endpoint contains password but code reads from constructor param which is always ""
```

### Angle D — Language-Pitfall Specialist

Scan for classic Kotlin/JVM footguns:

| Pitfall | Pattern | Why it breaks |
|---------|---------|---------------|
| data class with ByteArray | `data class Foo(val data: ByteArray)` | `equals()`/`hashCode()` use reference equality on arrays |
| flow{} in channelFlow{} | `flow{}` collected inside `channelFlow{}` with `withContext(Dispatchers.IO)` | Flow invariant — emission from wrong coroutine |
| !! on nullable | `val x = nullable!!` without prior null check | NPE at runtime |
| channelFlow emit after close | `send()` in `catch` block after `return@channelFlow` | Silent drop or exception |
| Gson TypeToken + inline reified | `object : TypeToken<T>() {}.type` in inline function | Type erasure mismatch |
| readLine() blocking in coroutine | `BufferedReader.readLine()` without `withContext(Dispatchers.IO)` | Blocks thread, coroutine can't cancel |
| Array<String> for key-value pairs | `arrayOf("key", "value", "key2", "value2")` | Index parity bugs, step-by-2 loops fragile |
| `bytes[i].toInt() and 0xFF` signed byte | `answerBytes[i].toInt() shl (i*8)` without `and 0xFF` | Negative bytes corrupt bitwise assembly |
| `return` in non-inline lambda | Bare `return` inside `withFileLock { }` lambda | "return is prohibited here" |
| joinToString misuse | `delimiter.joinToString("", "", "", list)` treating String as Iterable<Char> | Joins chars of delimiter, not list entries |

### Angle E — Wrapper/Proxy Correctness

When the diff adds a type that wraps another (adapter, proxy, decorator):
- [ ] Every method routes to the wrapped instance, not back through a registry
- [ ] The wrapper forwards ALL methods callers actually use
- [ ] Session/cache reuse logic is correct (not re-initializing every call)
- [ ] Cleanup/close propagates to wrapped instances
- [ ] The adapter is isolated — errors in it can't break other providers

Example finding:
```
file: ApiAdapter.kt, line: 73
summary: Session reuse condition always evaluates to 'session == null'
failure_scenario: session?.let{false}==true is always false → every turn creates new login+session
```

### Angle F — Reuse Detection

Flag new code that re-implements something the codebase already has:
- [ ] String escaping — does `ApiConfig.escapeJson()` or similar exist?
- [ ] SSE parsing — does `StreamParser` already handle this event format?
- [ ] HTTP header building — does the existing adapter have a shared header helper?
- [ ] Tool execution — does `ToolRegistry.executeBatch()` already partition tools?
- [ ] JSON extraction — does `JsonArg.extract()` already exist?

### Angle G — Simplification

Flag unnecessary complexity:
- [ ] Redundant or derivable state (field = computed from another field)
- [ ] Copy-paste with slight variation (should be parameterized)
- [ ] Deep nesting (>4 levels of `{}` or `when`)
- [ ] Dead code left behind (unused imports, unreachable branches, discarded return values)
- [ ] Unused imports (`import kotlinx.coroutines.Job` — used only in FreeSession, not here)

### Angle H — Efficiency

Flag wasted work:
- [ ] Repeated I/O that could be cached (WASM downloaded every call)
- [ ] Sequential operations that could be parallel (login + create_session could run together)
- [ ] Blocking work on hot paths (WASM download on every turn instead of once)
- [ ] Repeated string/byte operations (header array allocated 3× in the same method)
- [ ] Objects created unnecessarily (new Gson() per call instead of shared instance)

### Angle I — Altitude

Check if changes are at the right architectural level:
- [ ] Should this be a plugin/configuration instead of hardcoded routing?
- [ ] Should shared infrastructure be generalized instead of duplicated?
- [ ] Is the fix a bandaid on the symptom instead of the root cause?
- [ ] Could this be a configuration value instead of a code constant?

---

## Phase 2 — Dedup + Verify (1-Vote, 3-State)

### Step 1: Dedup

Remove candidates pointing at the same file:line:mechanism. Keep the one with
the most concrete failure scenario.

### Step 2: Verify Each

For each remaining candidate:
1. Read the file at the claimed line
2. Read surrounding context (±30 lines)
3. Check if the bug is guarded elsewhere (trace callers, check preconditions)
4. Vote exactly one of:

| Verdict | Criteria |
|---------|----------|
| CONFIRMED | Can name exact inputs/state → wrong output/crash. Quote the line. |
| PLAUSIBLE | Mechanism is real, trigger is uncertain (timing, env, config). State what would confirm. |
| REFUTED | Factually wrong (code doesn't say that) or guarded elsewhere. Quote the proof. |

**A single non-REFUTED vote carries the finding.** Do NOT drop on uncertainty.

### Step 3: Rank

Sort by severity:
1. CONFIRMED crash/fail bugs first
2. CONFIRMED logic errors
3. PLAUSIBLE high-risk
4. PLAUSIBLE low-risk
5. Efficiency/simplification/altitude

---

## Phase 3 — Sweep for Gaps

Re-read the full diff AS A FRESH REVIEWER. The job is to find defects NOT
already on the verified list. Do NOT re-derive or re-confirm anything.

Focus on what first-pass reviews tend to miss:
- [ ] Moved/extracted code that dropped a guard or anchor
- [ ] Second-tier footguns (companion object duplicated, `hashCode()` non-deterministic, lock scope shrunk, predicate methods with side effects)
- [ ] Setup/teardown asymmetry
- [ ] Config defaults flipped
- [ ] Transitive dependency issues (new JAR added to build.gradle.kts but not to runtime classpath)
- [ ] String escaping issues (JSON built with string templates)
- [ ] Numerical overflow/truncation (Long → Int conversion without bounds check)

Return up to 8 additional candidates or an empty list. Do not pad.

---

## Phase 4 — Output

Return findings as JSON with 15 max, ranked most-severe first:

```json
[
  {
    "file": "path/to/file.kt",
    "line": 123,
    "summary": "one-sentence bug description",
    "failure_scenario": "concrete inputs → wrong output or crash",
    "severity": "CRITICAL|HIGH|MEDIUM|LOW"
  }
]
```

If nothing survives, return `[]`. Never return more than 15.

---

## Protocol Port / API Transplant Checklist

When reviewing code that ports an external API or protocol, ADDITIONALLY check:

### Configuration Match
- [ ] API base URL matches the original project's default config
- [ ] WASM/binary URLs match (`config.example.toml` → code constant)
- [ ] User-Agent string matches (browser vs. app client)
- [ ] Client-Version / Client-Platform / Client-Locale match
- [ ] Model type values match (`model_types` array → code mapping)

### Wire Format Match  
- [ ] Prompt tag format matches (case, whitespace, special characters like `｜` U+FF5C vs `|`)
- [ ] HTTP headers match (auth, PoW, client identity)
- [ ] Envelope structure matches (code, msg, biz_code, biz_data nesting)
- [ ] Endpoint paths match (no trailing slash issues)
- [ ] Payload field names match (snake_case in JSON, camelCase in code)

### Protocol Flow Match
- [ ] Login → Session → PoW → Completion order unchanged
- [ ] PoW target_path matches the endpoint being called
- [ ] SSE event types match (Meta, ThinkStart, ThinkDelta, ContentStart, ContentDelta, Done)
- [ ] State machine transitions match (p/o/v patch: path persistence, op default, BATCH recursion)
- [ ] Error hint parsing matches (rate_limit, input_exceeds_limit, hint→error mapping)

### Lifecycle Match
- [ ] Session cleanup (stop_stream + delete_session) matches
- [ ] File upload → fetch_files poll matches
- [ ] WASM caching/download-once matches
- [ ] Login reuse (token/session reuse across turns) matches

---

## Kotlin-Specific Gotchas (from field review experience)

These are the exact bugs we found and fixed. Scan for them in EVERY review:

1. **`flow{}` collected inside `channelFlow{}` with `withContext(Dispatchers.IO)`** → Flow invariant. Use `channelFlow{}` everywhere.
2. **`readLine()` blocks without `Dispatchers.IO` + `invokeOnCompletion` body close** → Coroutine can't cancel. Must close body on cancel.
3. **Gson `TypeToken` with Kotlin `inline reified`** → Type erasure mismatch. Works with Kotlin reified but verify.
4. **`data class` with `ByteArray` field** → `equals()`/`hashCode()` broken. Override or avoid.
5. **String template JSON without escaping** → `"challenge":"$challenge"` if challenge contains `"`. Always escape.
6. **`ExportFunction.apply(long...)` — all args are `Long`** → Chicory 1.0.0 uses `long[]`, not `Value` objects.
7. **`WasmModule` not `Module`** → Chicory 1.0.0: `Parser.parse(bytes)` not `Module.builder()`.
8. **`return` in non-inline lambda** → "return is prohibited here". Use `return@label`.
9. **`String.joinToString()` vs `Iterable.joinToString()`** → Different APIs. Be explicit.
10. **Duplicate `companion object`** → Kotlin allows only one. Merge or restructure.
11. **`config.field` that doesn't exist in the data class** → Check ApiConfig/ModelConfig field names.
12. **When block on sealed class missing branches** → Compiler warns. Don't ignore; add all branches or `else → {}`.
13. **`send()` after `return@channelFlow` in catch block** → May silently drop. Use `doneSent` flag pattern.

---

## Example: 3-Round Review of an API Port

**Round 1** — Found 12 bugs:
- 6 CONFIRMED (API URL wrong, Flow invariant, password never read, session reuse dead, header array 3× alloc, dead frag.copy())
- 6 PLAUSIBLE (ByteArray equals, TypeToken, first() on empty, readLine blocking, snapshot order, JSON unescaped)

**Round 2** — After fixes, found 4 more:
- 2 CONFIRMED (config.model missing, injectMemoryContext missing, registerDynamic missing)
- Several provider syntax errors (companion object duplicate, joinToString misuse, return in lambda)

**Round 3** — After all fixes, found 2 remaining:
- 1 CONFIRMED (duplicate Done event after return@use)
- 4 minor (unused imports)

**Config Cross-Check** (post-round-3) — Found 4 more:
- Asset CDN URL completely wrong (wrong host)
- Prompt tags wrong case (lowercase vs Title Case)
- Client identity wrong (Chrome/web/web vs native app)
- Locale format wrong (zh-CN vs zh_CN)
