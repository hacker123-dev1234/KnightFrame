---
name: Skill Router
description: Analyze user request and determine which skills to activate. Only this skill body is injected; it tells the model which other skills to enable.
type: passive
match: 
---

# Skill Router

Before doing anything, determine which skills are needed for this task:

## Available Skills
- **Code Review**: review, audit, check code → use grep + read_file for systematic audit
- **Security Review**: security, vulnerability → scan for OWASP Top 10
- **Code Explorer**: explore, find, investigate → read-only: ls → glob → grep → read_file
- **Code Refactor**: refactor, simplify → read → identify → multi_edit → verify
- **Test Runner**: test, run tests → detect test command, run, diagnose, fix
- **Change Verifier**: verify, check, confirm → build → run → test
- **Deep Research**: research, investigate, analyze → multi-source fact-checked research
- **Frontend Design**: frontend, web, UI, design → polished HTML/CSS/JS components
- **Web Research**: search, find information → web_search + web_fetch with citation
- **Document Generator**: document, report, article → outline → draft → format → export
- **Project Init**: init, setup → analyze codebase, generate docs
- **Configure Environment**: configure, env, setup → detect project environment, install deps
- **caveman**: concise mode → drop pleasantries, be direct
- **html-ppt**: presentation, PPT, slides → HTML presentation deck
- **UI/UX Design**: ui-ux-pro-max, design, brand, banner, styling → design intelligence
- **Universal Analysis**: analyze data, 分析数据, data analysis, insight, trend, pattern → structured two-stage data analysis

## Decision Logic
1. Read the user's request carefully
2. If it involves coding → enable Code Review OR Code Explorer
3. If it involves UI/frontend → enable Frontend Design, UI/UX, ui-styling
4. If it involves presentations → enable html-ppt, slides
5. If it involves research → enable Web Research, Deep Research
6. If it involves testing → enable Test Runner, Change Verifier
7. If it involves documents → enable Document Generator
8. If it involves data analysis → enable Universal Analysis
9. If it's a new project → enable Project Init, Configure Environment

## Output Format
After analyzing, state briefly: "Skills needed: [list]" then proceed with the task using ONLY those skills' workflows. Do NOT inject all skill bodies — use only the relevant ones.
