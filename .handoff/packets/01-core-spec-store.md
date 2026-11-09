# Packet 01: Living Spec crate scaffold, spec store, shared annotation layer, badge model

Read `.handoff/packets/00-architecture.md` first; it is binding. This packet creates the foundation every later packet builds on, so keep the public API small, documented, and stable.

## Goal
Create the `living-spec/` Rust crate (lib `living_spec` + bin `living-spec`) with the core requirement model, the shared annotation layer, the badge model and the history store.

## Scope
1. `living-spec/Cargo.toml` (edition 2021, `rust-version = "1.95"`, deps per the contract), `Cargo.lock` generated offline, `.gitignore` for `target/`.
2. `src/core/spec.rs`: parse `spec.md` into `Spec { lines: Vec<RequirementLine { id, text, heading_path: Vec<String>, line_no }> }`. Errors (typed, with line numbers) for duplicate ids and malformed bracket ids. Prose and non-id list items are ignored.
3. `src/core/annotations.rs`: `Annotations` with `requirements: BTreeMap<String, RequirementAnnotations>`; `RequirementAnnotations { links: Vec<Link>, depends_on: Vec<Dependency>, bounds: Vec<serde_json::Value>, quantifier: Option<serde_json::Value>, extra: BTreeMap<String, Value> (#[serde(flatten)]) }`. `bounds` and `quantifier` are raw `Value` here; packets 05/07 own typed views over them via their own modules. Load/save with stable key ordering and a trailing newline; round-trip must be byte-identical for a canonical file. `orphans(&Spec) -> Vec<String>`.
4. `contracts/living-spec-annotations-v1.schema.json` (draft 2020-12) describing the file; `bounds` items and `quantifier` are open objects at this stage (later packets may tighten their sub-schemas in `$defs`).
5. `src/core/badge.rs`: `Badge` (`proven|broken|unproven|qualified`), `Reason` (`no_link`, `link_challenged`, `test_failed`, `link_unresolved`, `boundary_gap`, `prerequisite_broken { path: Vec<String> }`), `LineStatus { id, badge, reasons, tests: Vec<TestEvidence> }`. A pure function `score_line(annotations, outcomes, boundary_gaps: &BTreeSet<String>) -> LineStatus` implementing rules 1–4 of the contract (rule 5 belongs to packet 06; expose a hook point, e.g. `fn apply_qualified(statuses, qualified_map)` is NOT in scope — leave a doc comment).
6. `src/core/test_identity.rs`: `TestOutcome { test_id, status, duration_ms }`, `fingerprint(source: &str) -> String` (`sha256:` + hex of whitespace-collapsed source), and `resolve_link(link, outcomes, fingerprints_by_id) -> Resolution { Exact | Renamed { new_id } | Unresolved }`.
7. `src/core/history.rs`: append/read `history.jsonl`; `last_transition(id)` and `history_for(id)`.
8. `src/cli/mod.rs` dispatch table + `src/main.rs`; one subcommand `living-spec check <spec-dir>` that parses spec + annotations, prints line count, orphans, and exits 1 on parse errors or orphans.
9. Fixtures: `living-spec/tests/fixtures/core/basic/{spec.md,annotations.json}` (≥6 lines, some linked, one orphan variant in a sibling dir).

## Out of scope
Linker/auditor, runner adapters, web, boundary/deps/quantifier logic, CI wiring, any change outside `living-spec/` and `contracts/`.

## Acceptance (must pass, offline)
```sh
cd living-spec
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
cargo run --offline -q -- check tests/fixtures/core/basic   # exit 0
```
Tests required: spec round-trip (parse → annotations save → load → byte-identical), duplicate-id and malformed-id errors with line numbers, orphan detection, every badge rule 1–4 with a dedicated test, rename resolution by fingerprint, unresolved link → broken, history append/read test (`history_for` returns transitions in order), unknown annotation section preserved on round-trip.
