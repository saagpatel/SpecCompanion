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

- Node.js 20.19+, 22.13+ (not 23), or 24+ (CI uses Node 20)
- pnpm 10 (`pnpm-lock.yaml` is the only lockfile)
- Rust stable toolchain (`rustup`)
- Tauri system dependencies: [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/)

### Installation

```bash
git clone https://github.com/saagpatel/SpecCompanion
cd SpecCompanion
pnpm install --frozen-lockfile
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

Use the repository root, Node 20.x (at least 20.19), 22.13+ or 24+ and pnpm 10 (matching CI), plus Rust and
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
| Test execution      | Bounded Rust subprocess runner (Jest, Vitest, PyTest, `unittest`) |
| LLM integration     | Anthropic Claude API (optional)                |
| Storage             | SQLite (local app data dir)                    |

## Architecture

The Rust backend owns the full pipeline: parse spec → normalize stable requirements → scan candidate evidence → generate or select tests → execute tests → classify alignment → persist the report and evidence ledger. The frontend explains what was checked, skipped, inferred, failed, or left unknown. All data stays local in SQLite. The only network call is optional Claude-assisted generation, triggered explicitly by the user.

`VERIFIED` requires all of the following: deterministic implementation evidence, an exact stable requirement trace, a meaningful non-placeholder assertion, and a passing execution result. Missing any part keeps the requirement partial or unknown; a failure or timeout is reported as failed.

## Living Spec

The standalone Rust crate in `living-spec/` tracks whether the requirements for the Living Spec tools themselves have executable test evidence. It works offline and does not use an LLM.

The auditor is a deterministic, best-effort static check aimed at lazy or accidental non-tests: placeholders, irrelevant or tautological assertions, and unexecuted or unsupported shapes. Unsupported shapes are always challenged. It is not a sandbox or a proof that a test is adversarially sound; a deliberately obfuscated test can still mislead it. A green badge means “a passing test the auditor could relate to this line”, never “proven correct”.

### Proof tiers

Plain `proven` rests on the static allowlist auditor, the documented best-effort baseline; it is no longer being hardened. Its known limits are listed below.

The opt-in mutation-checked tier tests whether a linked test fails when its Rust subject is deliberately changed. In v1, mutation subjects are Rust free functions; when mutation is unsupported or a surviving mutant is judged equivalent, a human signoff is the visible fallback. A killed mutant shows test sensitivity, not that the requirement is met. See the [mutation architecture](.handoff/packets/30-mutation-architecture.md).

### Mutation-checked tier

Mutation testing copies a supported Rust crate, changes one small operator or literal at a time, and reruns the linked test. A mutant is **killed** when the selected test fails on that change; it **survives** when the test still passes. A surviving mutant may be equivalent to the original behavior, but only a bound, visible human waiver can record that judgment. Unsupported links, execution errors, changed tests, changed requirements, or changed subjects remain unchecked or stale. The tier supplements the plain badge: even a test that can fail on a mutation is not proof that the requirement is met.

Run `make living-spec-mutate` to build or merge `living-spec/living-spec/mutation.json`; pass `MUTATE_ARGS='--only LS-001 --max-mutants 2'` to select a subset. Then run `make living-spec-score-mutation` to score a scratch copy with the evidence. The lower-level commands are `living-spec mutate <spec-dir> --tests <root> [--max-mutants N] [--jobs N]` and `living-spec score <spec-dir> ... --mutation [<path>]`. Mutation execution is opt-in because it performs two baseline runs and rebuilds/runs the crate for each selected mutant; budget minutes per link, depending on the crate and build cache. Rust-only v1 cannot resolve every subject, and manual signoff remains human judgment rather than a mutation check.

Mutation subject resolution scans Rust files under the package's `src/`, without following module reachability, and requires a unique free-function name. Test-only code, conditional items, uncertain extents and function-producing macros are excluded conservatively. The manifest check is a conservative text scan: workspace declarations or inheritance, Git dependencies, and paths resolving outside the crate are unsupported; it is not a TOML parser and can reject harmless text.

Each requirement line has a stable identifier, such as `- [LS-001] The store preserves unknown annotation sections.` The four badges describe the current evidence: **proven** means accepted, current links passed and required boundary cases passed; **broken** means a linked test failed or a link cannot be resolved; **unproven** means evidence is absent, stale, challenged, or not executed, or a required boundary case is missing; **qualified** means the line's own evidence passed but a prerequisite is not proven. Qualification preserves the line's own test results.

The shared `annotations.json` file stores accepted test links and their audits alongside optional typed bounds, dependency edges with evidence, and quantifier domains and claims. The mutation records use `contracts/living-spec-mutation-v1.schema.json`; status output extends `contracts/living-spec-status-v1.schema.json` with the optional tier and mutation input digest. The Markdown spec remains plain English; annotations add structured evidence without changing its requirement text. `status.json` is the current score and `history.jsonl` records score transitions. Saved scores bind exact spec and result-file bytes, canonical annotation JSON, and the tool version with SHA-256 digests. When mutation scoring is enabled, the mutation file digest is tracked separately from plain-score freshness. `render` and `rollup` check those bindings rather than recomputing evidence verdicts. Changed plain inputs show a stale banner and a stale roll-up row with zero proven; affected requirements and their transitive dependents get gray **stale** badges until `living-spec score` runs again. Changed mutation input marks only the mutation tier stale. Older scores without digests also require rescoring. Result-file paths are relative to the spec directory; unavailable or malformed results make every line stale.

From the repository root, `make living-spec-test` runs formatting, lint and tests; `make living-spec-score` scores the dogfood spec using those test results; `make verify` runs both. Mutation execution is separate: `make living-spec-mutate` creates evidence, and `make living-spec-score-mutation` scores a scratch copy and reports current or stale tiers. If a committed `mutation.json` is present, `verify` prints whether its bytes match the digest recorded in the committed status or whether that binding is stale or missing. This is a byte-binding notice only: it does not parse the file, run mutants, or fail the gate. The optional mutation CI job runs only when manually dispatched. These targets also require Ruby 3+ for the CI workflow and scoring-script regressions (standard library only). The equivalent direct commands are:

```sh
cd living-spec
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline --locked 2>&1 | tee target/test-output.txt
cargo run --offline --locked -q -- score living-spec --results target/test-output.txt --format libtest --tests .
```

From the crate directory, `living-spec link living-spec --tests . --write` proposes links for review; `living-spec audit living-spec --tests .` reports its conservative assertion audit. Add `--write` to audit to rebind and save only existing links against current requirements and source, including challenged verdicts; it does not add linker proposals. Hand-reviewed links use `proposed_by: human` and bind both the test fingerprint and the current requirement revision. `score` accepts libtest or JUnit results, optional `--commit SHA`, and `--html out.html`; `rescore` uses the same pipeline. `living-spec render living-spec --out spec.html` renders saved evidence. The [Boundary Cases](living-spec/BOUNDARY.md) and [Quantifier Map](living-spec/QUANTIFIER.md) guides describe their annotations and standalone exports. `living-spec deps report living-spec --status living-spec/status.json` explains dependency impacts.

With `--tests` (or `--fingerprints-from`), scoring re-audits every resolved link against current sources, including human-proposed links with unchanged fingerprints. A stored `accepted` verdict cannot bypass this audit. To accept a challenge explicitly, add `"override": { "by": "human", "reason": "Explain the reviewed limitation", "test_id": "<reviewed test_id>", "fingerprint": "<reviewed fingerprint>", "revision": "<reviewed requirement revision>" }` inside that link's `audit` object. The reason must contain non-whitespace text, and all three binding fields must match the current link. An override with a missing or changed binding is inactive; write commands retain it in `audit.invalidated_overrides` as history. The auditor's challenged verdict is retained; scoring records `human_override`, the spec page displays “accepted by human override” with the reason, and `status.json` counts these links separately as `human_overrides`. Overrides do not bypass source identity, requirement revisions, execution results, or boundary gates. Without sources, scoring uses saved verdicts and labels `status.json` with `"audit": "stored"`; source-backed scoring records `"audit": "fresh"`. The dogfood spec uses no overrides.

The optional [Git pre-push hook](living-spec/hooks/README.md) is installed manually in an existing hooks directory or an explicitly chosen hooks directory. For this repository, configure its spec directory as `living-spec/living-spec`, source root as `living-spec`, and test command as `cd living-spec && cargo test --locked`. It runs a local rescore before pushing. CI runs the test and score gates on every push and pull request and saves the HTML and JSON evidence as an artifact.

Portfolio roll-up reads the kept-project registry at runtime. Set `LIVING_SPEC_REGISTRY` to a registry snapshot, then run `living-spec rollup --include . --out rollup.html --json rollup.json`; `--registry PATH` can supply the snapshot explicitly. Projects without a scored living spec remain visible as unscored or missing.

The honesty rules keep failed links visible, require every typed boundary case to be tested, and keep quantifier coverage descriptive rather than proof language. Known limits: the deterministic auditor checks syntax and relevance, rather than semantic equivalence; source discovery supports a bounded set of languages and syntax; current source fingerprints are needed for admissible links; and a passing process does not establish that every parameterized case ran. Quantifier checks cover declared observations of allowlisted compiled fixture subjects, with finite or sampled domains; they do not execute arbitrary application code. Dependency qualification records prerequisite status; it does not claim the dependent behavior failed. Exit 2 means at least one requirement is broken; malformed or incomplete results exit 1, while CLI usage errors exit 64.

## License

MIT
