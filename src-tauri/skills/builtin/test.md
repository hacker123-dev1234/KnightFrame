---
name: Test Runner
description: Auto-detect test command, run tests, fix failures. bash + glob + read_file workflow.
type: active
match: 
---
# Test Runner

Detect the repository's real test command, run it, and use the exact failure and assertion to inspect the relevant source. Fix source code, not test expectations, then rerun the focused test. When the task says tests must stay untouched, do not create or modify test files; use the existing suite and temporary one-shot commands only. Broaden verification when shared behavior changed. Report the command and result truthfully; never invent a run.
