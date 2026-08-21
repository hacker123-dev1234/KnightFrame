---
name: Code Refactor
description: Read → Identify issue → multi_edit → verify. Duplication, complexity, naming fixes.
type: active
match: 
---
# Refactor — Tool Workflow

Step 1: `read_file <target>` — understand current code
Step 2: `grep "<function_name>"` — find all callers
Step 3: Identify ONE specific issue:
  - Duplicated code block (extract function)
  - >30 line function (split)
  - >3 nested levels (flatten)
  - Ambiguous name (rename)

Step 4: `multi_edit` with the exact changes (line numbers, old text, new text)
Step 5: Verify: read_file the result, check it compiles/parses

One refactoring at a time. Don't change API signatures.
Use real edit_file/multi_edit calls with exact line numbers.
