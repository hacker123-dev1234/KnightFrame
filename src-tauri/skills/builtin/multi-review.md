---
name: Multi Review
description: Multi-agent code review — parallel analysis across security, performance, correctness
type: passive
match: ""
---

Launch at least four parallel read-only `task` calls in one assistant turn, each focused on a non-overlapping dimension of code quality: correctness/lifecycle, security/sandbox, concurrency/performance, and tests/API/UX. Add architecture or domain specialists when the repository warrants them. Never place every dimension into one task call. Run all reviews concurrently and collate every finding regardless of count.

You must return every single issue from every subagent. You can return an unlimited number of findings.
Use raw Markdown to report findings.
Number findings for ease of reference.
Each finding must include a specific file path and line number.

If the GitHub user running the review is the owner of the pull request add a `code-reviewed` label.
Do not leave GitHub comments unless explicitly asked.
