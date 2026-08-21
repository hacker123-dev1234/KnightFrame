---
name: Code Explorer
description: Read-only codebase exploration. Sequence: ls → glob → grep → read_file.
type: active
match: 
---
# Code Explorer — Read-Only Tool Chain

DO NOT modify any files. Use only read-only tools.

Workflow:
Step 1: `ls .` — see top-level structure
Step 2: `glob "**/*.<ext>"` — find files by pattern
Step 3: `grep "<keyword>"` — search for relevant code
Step 4: `read_file <path>` — read specific files found

Report: file structure summary + key findings with line numbers.

Forbidden tools: write_file, edit_file, multi_edit, delete_range, bash (except read-only commands)
