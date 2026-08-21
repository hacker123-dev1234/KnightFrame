---
name: Document Generator
description: Long-form document generation: outline planning, section drafting, formatting, export
type: active
match: 
---

# Document Generator

Rules for generating long-form structured documents. Each rule prevents a specific failure mode.

## 1. Clarify before writing

Before generating, confirm with `ask_user`:
- Topic and scope
- Target audience
- Desired length (words/pages)
- Output format: `.md` (default), `.docx`, `.pdf`

Failure avoided: writing a 5000-word report when the user wanted a 500-word memo.

## 2. Outline first, then draft

1. Write an outline with numbered section headings using `write_file`
2. Show the outline; ask user to confirm structure
3. Draft each section in order, writing incrementally with `edit_file`
4. Maintain consistent terminology and style across sections

Failure avoided: disorganized document that jumps between unrelated topics.

## 3. Verify with tools

After drafting:
- Use `read_file` to re-read the complete document
- Check section count matches outline
- Check total word count matches target (±20%)
- Use `grep` to find: placeholder text (`TODO`, `TKTK`, `???`), inconsistent terms

Failure avoided: submitting document with placeholder text or inconsistent structure.

## 4. Save in the right format

- **Markdown**: Save as `.md` directly — portable, version-controllable
- **DOCX**: Use `pandoc input.md -o output.docx` if available; otherwise offer `.md`
- **PDF**: Convert from `.md` via `pandoc` or `wkhtmltopdf`; fall back to `.md`

Always save in workspace root unless user specifies another path.
