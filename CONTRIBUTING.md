# Contributing to KnightFrame

KnightFrame is a Windows-first Rust/Tauri project. Keep changes small, provider-neutral and measurable.

## Before opening a pull request

1. Run `pnpm install`.
2. Run `pnpm check`, `pnpm lint` and `pnpm test`.
3. Add protocol fixtures or focused tests for provider, streaming, tool, cache and plugin changes.
4. Keep UI text in `src/lib/i18n.ts`; do not embed product copy directly in components.
5. Never commit API keys, local settings, diagnostics, model responses, generated binaries or project data.

Provider adapters must preserve text, reasoning summaries, tool calls, stop reasons and usage separately. Unknown capabilities must be shown as untested or user-overridden rather than silently assumed.

By contributing, you agree that your contribution is licensed under Apache-2.0.
