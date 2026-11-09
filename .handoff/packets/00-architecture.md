# Living Spec architecture contract (binding for every packet)

Living Spec adds four capabilities to SpecCompanion on one shared annotation layer:
**Living Spec core** (requirement lines linked to tests, badges, history, runner hook, web view, portfolio roll-up),
**Boundary Cases** (typed below/at/above bounds), **Dependency Proofs** (requirement dependency graph, qualified status),
and the **Quantifier Map** (domain + quantifier annotations, finite-exhaustive vs sampled checking).

## Where the code lives

A standalone Rust crate at `living-spec/` (its own `Cargo.toml` and `Cargo.lock`, NOT a member of `src-tauri`,
no Tauri or network dependencies). Library crate `living_spec` plus one binary `living-spec`.
Allowed dependencies (all already in the local cargo cache; build with `--offline`): `serde` (derive), `serde_json`,
`sha2`, `regex`, `quick-xml`, `tempfile` (dev only). No others without a stated reason in the report.
MSRV: Rust 1.95 (match `src-tauri`). `cargo fmt` clean, `cargo clippy --all-targets -- -D warnings` clean.

Module map (each packet owns its modules; do not rewrite another packet's module, extend through its public API):

| Module | Owner packet |
|---|---|
| `src/core/` (spec parser, requirement model, annotation layer, badge model, history store) | 01 |
| `src/agents/linker.rs`, `src/agents/auditor.rs` | 02 |
| `src/runner/` (test-result adapters, rescorer, hook) | 03 |
| `src/web/` (static HTML view + portfolio roll-up) | 04 |
| `src/boundary/` (`model.rs`, `gen.rs`, `coverage.rs`) | 05 |
| `src/deps/` (`model.rs`, `propagate.rs`, `report.rs`) | 06 |
| `src/quantifier/` (`annotations.rs`, `check/finite.rs`, `check/sampled.rs`, `view.rs`, `export/edition.rs`, `export/counterexample.rs`) | 07 |
| `src/main.rs` CLI subcommand wiring, `Makefile` targets, CI, dogfood spec | 08 (integration) |

Each packet adds its own CLI subcommand in its own file under `src/cli/<name>.rs` and registers it with ONE line in
`src/cli/mod.rs` (the dispatch table). Hand-rolled arg parsing (no clap) keeps deps small.

## The spec format (plain English, no DSL)

A living spec is a directory, by convention `<project>/living-spec/`, containing:

- `spec.md` — plain-English Markdown. A requirement line is a list item that starts with a bracketed stable id:
  `- [R-007] Exports keep the original timestamps.` Ids match `^[A-Z][A-Z0-9]*-[0-9]{3,}$`, are unique per spec, and
  never get reused. Headings group lines. Everything else is prose and is ignored. Duplicate or malformed ids are a
  parse error naming the line number.
- `annotations.json` — THE shared annotation layer (schema `contracts/living-spec-annotations-v1.schema.json`,
  JSON Schema draft 2020-12). Keyed by requirement id; every capability reads and writes its own section only:

```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-007": {
      "links":      [ { "test_id": "suite::module::test_name", "fingerprint": "sha256:…", "explanation": "…",
                        "proposed_by": "linker|human", "audit": { "verdict": "accepted|challenged", "notes": ["…"] } } ],
      "bounds":     [ /* Boundary Cases section, packet 05 */ ],
      "depends_on": [ { "id": "R-003", "evidence": "authored|<reviewable evidence text>" } ],
      "quantifier": { /* Quantifier Map section, packet 07 */ }
    }
  }
}
```

  Unknown sections are preserved on round-trip (store them as `serde_json::Value` in a flattened `extra` map).
  A requirement present in `annotations.json` but absent from `spec.md` is reported as an orphan, never silently dropped.
- `history.jsonl` — append-only, one JSON object per rescoring event per line:
  `{ "at": "<RFC3339>", "commit": "<sha or label>", "requirement": "R-007", "from": "proven", "to": "broken",
     "test_id": "…", "note": "…" }`.

## Badge model (the single source of truth for state)

`Badge` is a string-literal enum serialized lower-case: `proven` (green), `broken` (red), `unproven` (gray),
`qualified` (amber; Dependency Proofs only). Computation, in order:

1. No link, or every link was challenged by the auditor → `unproven` (reason `no_link` / `link_challenged`).
2. Any linked test failed, errored, or its link no longer resolves (test id missing AND no fingerprint match) → `broken`
   (reason `test_failed` / `link_unresolved`). Unresolved links are never silently dropped.
3. All linked tests passed, but the requirement has `bounds` with any boundary case lacking an executed passing test →
   `unproven` with reason `boundary_gap` (Boundary Cases: a happy-path-only pass is not proof).
4. Otherwise `proven`.
5. After 1–4, Dependency Proofs runs: a `proven` requirement with any transitive prerequisite that is `broken`
   becomes `qualified` (reason `prerequisite_broken`, with the minimal dependency path). Its own test result is untouched.

Each badge carries `reasons: Vec<Reason>` so the view and reports can explain themselves.

## Stable test identity

A test is identified by `test_id` (framework-qualified name) plus `fingerprint` = `sha256:` of the test function's
source with whitespace collapsed. Re-resolution on each run: exact id → else unique fingerprint match (a rename; the
link is updated and a history note says so) → else `link_unresolved` (broken).

## Test results input

Pluggable adapters producing `Vec<TestOutcome { test_id, status: passed|failed|errored|skipped, duration_ms? }>`:
libtest plain text (`cargo test` output, `test path::name ... ok|FAILED|ignored`) and JUnit XML (pytest/jest/vitest).

## Portfolio roll-up input

The kept-project list comes from the portfolio-truth registry snapshot, never a hand-written list:
the snapshot file given by `--registry PATH` or the `LIVING_SPEC_REGISTRY` environment variable (no hard-coded default path; the real list is read at runtime and is never committed). Shape (subset):
`{ "workspace_root": "…", "projects": [ { "identity": { "project_key", "display_name", "path" }, "derived": { "archived": bool } } ] }`.
A kept project is one with `derived.archived == false`. For each kept project, its living spec is
`<workspace_root>/<identity.path>/living-spec/`; a kept project without one is listed as `no living spec` (counted, not hidden).
Extra roots can be added with `--include PATH` (for a project newly revived but not yet re-registered).

## Honesty rules (acceptance-relevant)

- Never fabricate a test failure (qualified ≠ broken). Never hide an untested boundary or an unresolved link.
- The Quantifier Map coverage view and edition must never use proof language. Forbidden words (case-insensitive,
  whole word) in any rendered coverage text: `proven`, `proof`, `proves`, `guaranteed`, `guarantee`, `always`,
  `verified`, `correct for all`, `holds for all`. (The core badge label `proven` is allowed only in the core spec view,
  not in Quantifier Map output.)
- Offline only. No network, no LLM calls. "Agents" (linker, auditor) are deterministic, explainable, local.

## Verification commands (every packet must keep these green)

```sh
cd living-spec
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
```

## Public-repo hygiene

This repository is public. No real filesystem paths, user names, emails, hostnames or secrets in code, fixtures, docs or commit messages. All fixtures are synthetic (invented project names). Never commit a real portfolio-truth snapshot.

## Contract revisions after the stress test (binding; supersede anything above)

Findings: `.handoff/stress-test-findings.md`.

**R1 Badge gates (replaces rules 1–4).** Only *admissible* links count: link `audit.verdict == accepted`, resolved, and its `revision` equals the line's current revision. Then:
1. No admissible link → `unproven` (reasons explain why: `no_link`, `link_challenged`, `stale_revision`, `test_changed`, `not_executed`).
2. Any admissible link whose test failed or errored, or any link (admissible or not) that is unresolved → `broken` (`test_failed`, `link_unresolved`). A challenged link's failure is a diagnostic in `tests`, not a badge change.
3. Any admissible link skipped or missing from the outcomes → `unproven` (`not_executed`). Process success is never case success: every case of a parametrized test must appear and pass.
4. Bounds present and any boundary case not `tested_pass` → `unproven` (`boundary_gap`).
5. Otherwise `proven`.
Then dependency propagation (R4).

**R2 Requirement revisions.** `revision` = `sha256:` of the requirement line text exactly as written (punctuation and operators kept; only the leading `- [ID] ` and trailing whitespace stripped). Every link and audit stores the revision it was made against. Editing a line makes its links `stale_revision` until relinked or re-audited. Fenced code blocks in `spec.md` are ignored by the parser.

**R3 Test identity.** `test_id` is scoped: `<framework>:<package-or-target>:<relative-file>:<qualified-name>[#case]`. Two digests per test: `fingerprint` = `sha256:` of the raw source bytes with only trailing whitespace per line trimmed (literals untouched); `rename_hint` = same over the body excluding the declaration line. Resolution: exact id AND matching fingerprint → `Exact`; exact id with a different fingerprint → `Changed` (link needs re-audit; `unproven test_changed` until the auditor accepts it again, and the rescorer re-audits automatically when sources are available); id missing → unique `rename_hint` match → `Renamed`; zero or several matches → `Unresolved` (broken; ambiguity is never guessed).

**R4 Dependencies.** A `proven` requirement with any transitive prerequisite that is not `proven` becomes `qualified`. The reason is `prerequisite_broken` or `prerequisite_unproven` (and `prerequisite_qualified` passes through), with the minimal path to the nearest non-proven prerequisite (BFS, lexicographic tie-break). Dangling references and missing evidence fail loading. Members of a cycle that would be `proven` become `qualified` with reason `dependency_cycle` and the cycle path. Actual test outcomes are never altered.

**R5 Hook exit.** `rescore` exits 2 when ANY line is currently `broken`, including on the first run, and lists new transitions separately from lines that were already broken. Malformed or incomplete input exits 1.

**R6 Units.** A test's unit is known only from authored link metadata (`unit`) or a typed constructor in the test source (`Duration::from_millis/from_secs/...`, `timedelta(seconds=...)`). A bare number has an unknown unit: that case stays `untested` with reason `unit_unknown`, never inferred, never counted as a contradiction. A contradiction is a known unit whose dimension or scale disagrees with the requirement (30 s vs 30 ms; MB vs MiB is a scale difference).

**R7 Quantifier execution.** No eval, no shell, no plugins. Claims run as a closed, versioned JSON predicate AST (typed literals, input/output fields, comparisons, membership, and/or/not with depth and work caps) evaluated over observations from an allowlisted catalog of compiled, pure fixture *subjects* (e.g. `clamp`, `abs_i8`, with deliberately faulty variants). Checking domain membership alone is not a behaviour check.

**R8 CI.** The tool makes no network calls. Building it in CI may fetch crates normally (`cargo test --locked`); `--offline` is for local runs.

**R9 Repo gates.** `living-spec/target/` ignored; ≤25 paths per commit and ≤2 MiB per file (repo git guards); conventional commit headers ≤72 chars.
