# AGENTS.md

<!-- comm-contract:start -->

## Communication Contract

- Inherit global Codex communication and reporting rules from `~/.codex/AGENTS.override.md` and `~/.codex/policies/communication/BigPictureReportingV1.md`.
- Repo-specific instructions below add project constraints only; do not restate global voice or status-reporting rules here.
<!-- comm-contract:end -->

## Inherited Operating Rules

- Inherit global git, review/fix, testing, docs, UI, security, skill-use, and reporting gates from `~/.codex/AGENTS.md` and active session instructions.
- Use `.codex/verify.commands` and `.codex/scripts/run_verify_commands.sh` as this repo-local verification authority when present.

<!-- portfolio-context:start -->

# Portfolio Context

## What This Project Is

SpecCompanion is an active local project in the ~/Projects portfolio.

## Current State

Portfolio truth currently marks this project as `active` with `boilerplate` context. Phase 104 recovered minimum-viable context so future sessions can resume without rediscovery.

## Stack

| Layer               | Technology                                     |
| ------------------- | ---------------------------------------------- |
| Desktop shell       | Tauri 2                                        |
| Frontend            | React, TypeScript, Tailwind CSS                |
| Requirement parsing | Rust + `pulldown-cmark` (AST-based, not regex) |
| Evidence slice      | JavaScript/TypeScript and Python               |
| Test generation     | Jest and PyTest (offline templates or Claude)  |
| Test execution      | Bounded Rust subprocess runner (Jest, Vitest, PyTest, `unittest`) |
| LLM integration     | Anthropic Claude API (optional)                |
| Storage             | SQLite (local app data dir)                    |

## How To Run

```bash
pnpm install --frozen-lockfile   # pnpm-lock.yaml is the only lockfile; CI uses pnpm 10
pnpm tauri dev
```

Node 20.19+, 22.13+ (not 23), or 24+ (CI uses Node 20). For Claude-assisted test generation, enter an API key on the in-app Settings page; the app does not read an `ANTHROPIC_API_KEY` environment variable. Everything else works offline without a key.

Verification: see the README Verification section and `.codex/verify.commands`. `pnpm test` runs the full UI gate, including `lhci autorun`, which uploads Lighthouse reports externally; use `pnpm ui:gate:static` and `cargo test --locked --manifest-path src-tauri/Cargo.toml` for offline checks.

## Known Risks

- This repo only has minimum-viable recovery context today; deeper handoff details may still live in the README and supporting docs.

## Next Recommended Move

Use this context plus the README and supporting docs to resume the next active task, then promote the repo beyond minimum-viable by capturing a dedicated handoff, roadmap, or discovery artifact.

<!-- portfolio-context:end -->
