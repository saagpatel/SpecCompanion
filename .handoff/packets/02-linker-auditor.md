# Packet 02: Linking agent and auditor agent

Read `.handoff/packets/00-architecture.md` (binding) and `.handoff/stress-test-findings.md` if present. Build on the `src/core/` API from packet 01; do not rewrite it (small additive helpers are fine, say so in the report).

## Goal
Deterministic, offline, explainable agents: the **linker** proposes requirement-to-test links with an explanation each; the **auditor** challenges weak or fake links.

## Scope
1. `src/agents/test_index.rs`: build a `TestIndex` of candidate tests from source files: Rust (`#[test]` fns, id `module::path::name` derived from file path + mod nesting as libtest prints it), Python (`def test_*`, id `path/file.py::Class::test_name`), JS/TS (`it(`/`test(` titles, id `path/file.test.ts::describe > title`). Each entry: id, file, line, source body, fingerprint (core `fingerprint`).
2. `src/agents/linker.rs`: `propose(spec, index) -> Vec<Proposal { requirement, test_id, fingerprint, score, explanation }>`. Scoring from shared normalized terms (stemmed lowercase words minus stopwords) between requirement text and test name + body identifiers + string literals; numbers in the requirement matched in the test count extra. Explanation names the matched terms, e.g. `matched "timestamp","export" in test name; literal 30 in body`. Threshold + top-k (default 2) per requirement; requirements with no candidate above threshold get no proposal (stays gray). Every proposal's test id must resolve in the index.
3. `src/agents/auditor.rs`: `audit(requirement, link, index_entry) -> AuditVerdict { accepted|challenged, notes }`. Challenge when: (a) the body has no assertion (Rust `assert*!`/`panic!`-based checks, `#[should_panic]`; Python `assert`/`pytest.raises`; JS `expect(`/`assert`), (b) placeholder assertions only (`assert!(true)`, `assert_eq!(1, 1)`, same-literal comparisons, `assert True`, `expect(true).toBe(true)`; mirror SpecCompanion's placeholder rules — see `src-tauri/src/services/evidence.rs` and the stress findings), (c) the assertions never touch anything derived from the requirement's terms (no requirement term appears in any asserted expression or in a called function name), (d) the requirement names a number and no assertion or input uses it or a value adjacent to it. Notes explain each challenge in one sentence.
4. CLI `living-spec link <spec-dir> --tests <source-root> [--write]`: prints proposals with explanations, runs the auditor on each, and with `--write` merges accepted AND challenged links into `annotations.json` with `proposed_by: "linker"` and the `audit` block (challenged links are kept, marked, never deleted; human links are never overwritten). CLI `living-spec audit <spec-dir> --tests <source-root>` re-audits existing links and exits 1 if any link is challenged.
5. Seeded fixture project `living-spec/tests/fixtures/agents/seeded/` with `living-spec/spec.md` (≥8 lines) and a small fake Rust or Python source tree of tests (not compiled; text only), including **the cheating case**: a test named after a requirement (e.g. `exports_keep_original_timestamps`) whose body passes but proves nothing (e.g. `let _ = export(); assert!(true);`), and a second subtler cheat that asserts on an unrelated value.

## Out of scope
LLM calls, network, runner/web/boundary/deps/quantifier modules.

## Acceptance
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo run --offline -q -- link tests/fixtures/agents/seeded/living-spec --tests tests/fixtures/agents/seeded
cargo run --offline -q -- audit tests/fixtures/agents/seeded/living-spec --tests tests/fixtures/agents/seeded ; test $? -eq 1
```
Required tests: on the seeded project every proposal has a non-empty explanation and a test id that resolves in the index; at least 5 lines get a correct link; both cheating cases are `challenged` with a note naming why (regression test named `cheating_case_is_challenged`); a genuine test is `accepted`; `--write` keeps human links untouched and is idempotent.

## Contract revisions that apply
R2 and R3 in `00-architecture.md`: proposals and audits record the requirement `revision` and use the scoped `test_id`, `fingerprint` and `rename_hint` (from the core API; packet 03 may extend core concurrently, so use only `core::test_identity` functions and add your own thin helpers if needed). Port the placeholder-assertion regression cases from `src-tauri/src/services/evidence.rs` (`is_placeholder_assertion`, about lines 355-414) and add Rust assertion-macro support. If the auditor can't tell whether an assertion is relevant (unsupported syntax), it challenges the link and says the assertion syntax is unsupported. It never accepts a link it can't analyse.
