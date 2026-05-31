## UI Hard Gates (Required for frontend/UI changes)

<!-- comm-contract:start -->

## Communication Contract (Global)

- Follow `/Users/d/.codex/policies/communication/BigPictureReportingV1.md` for all user-facing updates.
- Use exact section labels from `BigPictureReportingV1.md` for formal delivery, blocker, waiting, risk, decision, or explicit status/report requests.
- Keep ordinary in-flight updates conversational, warm, PM-readable, operator-grade, and low-noise.
- Keep technical details in internal artifacts unless explicitly requested by the user or required by failure, risk, or verification.
- Honor toggles literally: `simple mode`, `show receipts`, `tech mode`, `debug mode`.
<!-- comm-contract:end -->

1. Read-only reviewer agent must output `UIFindingV1[]`.
2. Fixer agent must apply findings in severity order: `P0 -> P1 -> P2 -> P3`.
3. Required states per changed UI surface: loading, empty, error, success, disabled, focus-visible.
4. Required pre-done gates:
   - eslint + typecheck + stylelint
   - visual regression (Playwright snapshots)
   - accessibility regression (axe)
   - responsive parity checks (mobile + desktop)
   - Lighthouse CI thresholds
5. Done-state is blocked if any required gate is `fail` or `not-run`.

<!-- portfolio-context:start -->

# Portfolio Context

## What This Project Is

SpecCompanion is an active local project in the /Users/d/Projects portfolio.

## Current State

Portfolio truth currently marks this project as `active` with `boilerplate` context. Phase 104 recovered minimum-viable context so future sessions can resume without rediscovery.

## Stack

| Layer               | Technology                                     |
| ------------------- | ---------------------------------------------- |
| Desktop shell       | Tauri 2                                        |
| Frontend            | React, TypeScript, Tailwind CSS                |
| Requirement parsing | Rust + `pulldown-cmark` (AST-based, not regex) |
| Test execution      | Rust subprocess runner (Jest, PyTest)          |
| LLM integration     | Anthropic Claude API (optional)                |
| Storage             | SQLite (local app data dir)                    |

## How To Run

```
npm install
npm run tauri dev
```

Optionally set ANTHROPIC_API_KEY for Claude-assisted spec suggestions. App works fully offline without it.

## Known Risks

- This repo only has minimum-viable recovery context today; deeper handoff details may still live in the README and supporting docs.

## Next Recommended Move

Use this context plus the README and supporting docs to resume the next active task, then promote the repo beyond minimum-viable by capturing a dedicated handoff, roadmap, or discovery artifact.

<!-- portfolio-context:end -->
