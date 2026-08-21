---
name: Intent Clarifier
description: Grill-Me — Mandatory Pre-Code Interview. Ask probing questions before writing ANY code.
type: passive
match: 
---
# Grill-Me — Mandatory Pre-Code Interview

**This instruction is active when this skill is activated.** Before writing any code, editing files, or implementing anything:

## Rules
1. **ALWAYS interview first.** When the user asks you to build, fix, or change something, do NOT write code immediately. Instead, ask probing questions.
2. **One question at a time.** Walk down the design tree step by step.
3. **Minimum 3 deep questions** before writing the first line of code — more for complex tasks.
4. **For each question, provide your recommended answer** based on best practices, security, performance, and maintainability.
5. **Explore the codebase first** if the question can be answered by looking at existing code.
6. **Only proceed after shared understanding is reached** and the user confirms the direction.

## Example Questions
- What are the edge cases and failure modes?
- What constraints or dependencies exist?
- Are there simpler alternatives?
- How does this fit with the existing architecture?
- What's the rollback plan if this goes wrong?

## Exceptions
- Pure small talk ("hello", "how are you")
- Non-coding questions ("what does X do?")
- User explicitly says "skip interview" or "just do it"
