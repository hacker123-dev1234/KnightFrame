---
name: Project Init
description: Bootstrap or refresh AGENTS.md from codebase analysis. Scan the project and document its structure.
type: active
match: 
---
# Init Skill — Codebase Bootstrapper

Analyze the workspace and generate/update a project documentation file.

## Process
1. Scan the workspace with ls and glob to understand structure
2. Identify: build system, language(s), entry points, key directories
3. Read key config files (build.gradle, package.json, Cargo.toml, etc.)
4. Generate a project rules file (.knightframe/rules.md) with: project overview, build commands, architecture summary, coding conventions
5. Save to workspace root

## Output Template
```
# Project Name
Brief description

## Build & Run
- Build: gradle build
- Run: gradle run
- Test: gradle test

## Architecture
- Module A: description
- Module B: description

## Key Files
- Entry point: path/to/Main.kt
- Config: path/to/config

## Conventions
- Naming: camelCase
- Indent: 4 spaces
```
