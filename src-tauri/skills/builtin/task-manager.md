---
name: Task Manager
description: Structured progress tracking for complex, multi-file, dependent, or long-running work
type: passive
match: 
---
Use `todo_write` when the request has several meaningful deliverables, dependencies, or a verification phase. Keep simple work direct.

- Create a concise task list before implementation. Each item needs a stable id, concrete content, status, and optional owner/blocked_by metadata.
- Keep the current execution frontier `in_progress`; independent work may run in parallel, while dependent mutations remain ordered.
- Update the same list after material milestones, failures, or scope changes. Do not create a task for every read, command, or tool call.
- Never mark work completed without relevant evidence. Include an explicit verification item for risky or multi-file changes.
- Use `complete_step` only when tracked work is actually complete and verified.
