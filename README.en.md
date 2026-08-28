# KnightFrame

> Beta · Local coding agent for Windows

[中文](README.md)

KnightFrame targets three sources of waste: repeated repository exploration, full tool output copied into context, and unstable request prefixes that reduce cache reuse.

## How it works

- **Project index:** stores files, symbols, source lines, and references. The agent queries the index before reading exact lines.
- **Precise tools:** full results stay local; the model receives only the needed fragments. Read, write, search, and run results can be reused.
- **Stable context:** system instructions, tool definitions, and history keep a fixed append order.
- **Visible usage:** input, output, cache hits, and estimated cost are shown in the UI.

Models, auxiliary models, skills, and memory are user-configured. Memory is off by default.

## One measured run

Same model, ten repair tasks, one run per task:

| Metric | KnightFrame | Baseline |
| --- | ---: | ---: |
| Completed | 9 / 10 | 8 / 10 |
| Total tokens | 426,941 | 1,610,622 |
| Requests | 65 | 101 |
| Elapsed | 25:15 | 31:19 |

This is one measurement, not a stable ranking. See the [test record](docs/benchmark-2026-08-18.md) for aggregates and scoring.

## Current status

Project indexing, chat, model adapters, precise read/write tools, command execution, web search, the embedded browser, and Plugin Studio are available. Windows is the current test target. Provider changes and long tasks can still expose beta defects.

## Build

Requires Rust stable, Node.js 20+, pnpm 9+, WebView2, and Visual Studio C++ Build Tools.

```powershell
pnpm install
pnpm check
pnpm test
pnpm build:test-exe
```

Release build:

```powershell
pnpm build:release
```

## Plugin Studio

The KnightFrame preview is embedded and needs no second source checkout. External host preview is optional: build the host source, point `KF_DSH_ROOT` to the root containing `apps/cli/lib/bin.js`, then restart KnightFrame.

```powershell
[Environment]::SetEnvironmentVariable("KF_DSH_ROOT", "D:\Projects\host", "User")
```

Apache-2.0. OpenAI Codex is used for development and review.
