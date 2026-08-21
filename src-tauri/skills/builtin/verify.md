---
name: Change Verifier
description: Build → Run → Test → Verify. Use bash for build commands, read_file for output.
type: active
match: 
---
# Verify Changes — Tool Workflow

Step 1: `bash gradle build` or `bash <project build command>` — ensure it compiles
Step 2: If tests exist, `bash <test command>` — run test suite
Step 3: `read_file <changed file>` — visually verify the edit looks correct
Step 4: Check for regressions: `grep <function_name>` to find all callers are updated

Report: ✓ Build passed / ✗ Build failed with error
       ✓ Tests passed (N/N) / ✗ N tests failed
       ✓ No regressions detected / ⚠ caller at file:line not updated

Use actual bash commands with the project's build system.
