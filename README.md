# Spec Companion

[![Rust](https://img.shields.io/badge/Rust-dea584?style=flat-square&logo=rust)](#) [![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript)](#) [![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](#)

> Specs drift from code. Spec Companion shows what executable evidence can prove — and what remains unknown.

Spec Companion is a native desktop app for evidence-backed executable requirements. Point it at a codebase and a Markdown spec to get stable, source-linked requirements and an alignment report that separates verified evidence from partial, failed, and unknown states.

A passing process is not automatically proof. Placeholder assertions such as `expect(true).toBe(true)` and `assert True`, tests without a stable requirement link, unsupported languages, missing runtimes, and unavailable scans can never produce `VERIFIED`.

Built with Tauri v2 (Rust backend, React frontend). Runs locally, works offline, keeps your code and specs private.

## Features

- **Stable requirement identities** — parses Markdown with `pulldown-cmark`, records exact source lines, distinguishes duplicates, and invalidates edited requirement identities
- **Deterministic evidence scanning** — matches explicit JavaScript/TypeScript and Python implementation symbols without claiming arbitrary prose-to-code equivalence
- **Existing-test linkage** — discovers contained repository tests and lets the user explicitly link one to a requirement without editing the target project or inferring prose equivalence
- **Honest offline templates** — produces traceable Jest/PyTest scaffolds, while explicitly treating their placeholder assertions as non-probative
- **LLM test generation** — optional Claude API mode generates tests with meaningful assertions, edge cases, and realistic mock data
- **Bounded test execution** — runs contained Jest, Vitest, PyTest, or stdlib `unittest` evidence with canonical paths, stable executed-byte binding, fixed working directories, allowlisted runtimes, a 60-second timeout, process-tree termination, and capped output
- **Evidence-backed alignment** — classifies every requirement as `VERIFIED`, `PARTIAL`, `FAILED`, or `UNKNOWN`, with exact implementation, assertion, execution, and diagnostic evidence
- **Export** — JSON, HTML, or CSV reports

## Quick Start

### Prerequisites

- Node.js 18+
- Rust stable toolchain (`rustup`)
- Tauri system dependencies: [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/)

### Installation

```bash
git clone https://github.com/saagpatel/SpecCompanion
cd SpecCompanion
pnpm install
```

### Usage

```bash
# Start in development mode
pnpm tauri dev
```

1. Select your codebase directory and upload a markdown spec
2. Requirements are extracted automatically
3. Review deterministic implementation candidates
4. Explicitly link a contained existing test, generate scaffolds offline, or optionally use Claude-assisted generation
5. Run selected generated or linked tests in the bounded local runner
6. Review each classification and expand its evidence trail

## Verification

Use the repository root, Node 20.19+ or 22.12+ and pnpm 10 (matching CI), plus Rust and
[Tauri platform prerequisites](https://tauri.app/start/prerequisites/). Install
with `pnpm install --frozen-lockfile --ignore-scripts` for an isolated audit;
normal development installation runs Husky and may configure Git hooks.

The [canonical full gate](.codex/verify.commands) and
[verification contract](DOC-RECONCILIATION.md#verification-authority) are authoritative.
For focused backend workflow changes use `pnpm workflow:smoke`; for the broader
backend use `cargo test --locked --manifest-path src-tauri/Cargo.toml`.
`pnpm ui:gate:static` runs frontend lint/typecheck, while `pnpm build:ui` also
builds the UI. Rust check/format commands must target the nested manifest, for
example `cargo check --locked --manifest-path src-tauri/Cargo.toml` and
`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.

For UI or report changes, install Chromium with `pnpm exec playwright install chromium`
and run `pnpm ui:gate:regression` (visual/accessibility fixtures). Playwright starts
its loopback browser-preview server. For local Lighthouse checks, run:

```bash
pnpm build
pnpm exec lhci collect --config=.lighthouserc.json
pnpm exec lhci assert --config=.lighthouserc.json
```

These collect/assert steps keep reports local. `pnpm ui:test:lighthouse` uses
`lhci autorun`, whose checked-in upload target is temporary public storage; it
uploads reports externally. `pnpm test` includes that upload via the complete UI
gate, so it requires explicit approval for the external operation and is not an
offline smoke or a Rust test command.
Use disposable code/spec fixtures with no API key; do not select personal source
trees or enable Claude generation for an offline smoke. Browser-preview fixtures
are separate from native desktop packaging and human acceptance.

## Tech Stack

| Layer               | Technology                                     |
| ------------------- | ---------------------------------------------- |
| Desktop shell       | Tauri 2                                        |
| Frontend            | React, TypeScript, Tailwind CSS                |
| Requirement parsing | Rust + `pulldown-cmark` (AST-based, not regex) |
| Evidence slice      | JavaScript/TypeScript and Python               |
| Test execution      | Bounded Rust subprocess runner (Jest, PyTest)  |
| LLM integration     | Anthropic Claude API (optional)                |
| Storage             | SQLite (local app data dir)                    |

## Architecture

The Rust backend owns the full pipeline: parse spec → normalize stable requirements → scan candidate evidence → generate or select tests → execute tests → classify alignment → persist the report and evidence ledger. The frontend explains what was checked, skipped, inferred, failed, or left unknown. All data stays local in SQLite. The only network call is optional Claude-assisted generation, triggered explicitly by the user.

`VERIFIED` requires all of the following: deterministic implementation evidence, an exact stable requirement trace, a meaningful non-placeholder assertion, and a passing execution result. Missing any part keeps the requirement partial or unknown; a failure or timeout is reported as failed.

## License

MIT
