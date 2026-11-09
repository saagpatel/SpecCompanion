# Packet 06: Dependency Proofs — dependency model, cycle detection, qualified propagation, reports

Read `.handoff/packets/00-architecture.md` (binding) and `.handoff/stress-test-findings.md`. Build on `src/core/`; additive helpers only.

## Goal
A feature cannot look green while its foundation is red. Requirements carry explicit, authored (or evidence-backed) dependencies; when a prerequisite is broken, every transitively dependent `proven` requirement becomes `qualified` with the exact minimal dependency path, without fabricating any test failure.

## Scope
1. `src/deps/model.rs`: `DepGraph` built from the annotation layer's `depends_on` (`{ id, evidence }`; `evidence` must be `"authored"` or a non-empty reviewable string; an empty/missing evidence is a load error — dependencies are never guessed). Errors for unknown ids and self-loops. `detect_cycles() -> Vec<Vec<String>>` returning every elementary cycle found via Tarjan SCC (report each non-trivial SCC with one concrete cycle path, deterministic order). A graph with a cycle still loads; propagation treats a cycle as reported, not crashing.
2. `src/deps/propagate.rs`: input = core statuses after rules 1–4; output = statuses after rule 5. Only `proven` lines can become `qualified`; `broken`/`unproven` stay as they are; the dependent's own test evidence and outcomes are byte-for-byte unchanged (assert this). Minimal path = shortest path (BFS, ties broken lexicographically) from the dependent to the nearest `broken` prerequisite. Implement the core's qualified provider trait if packet 03 introduced one; otherwise expose `apply_qualified(&mut [LineStatus], &DepGraph)`.
3. `src/deps/report.rs`: `ImpactReport` per affected claim: dependent id + text, the broken prerequisite id + text + its failing test, the minimal path rendered `R-010 → R-004 → R-001 (broken: test x::y failed)`, and every other broken prerequisite reachable (listed, not hidden). Markdown + JSON export (`contracts/living-spec-dependency-report-v1.schema.json`).
4. **Reference set:** `living-spec/tests/fixtures/deps/graphs/` with exactly **20** original synthetic feature graphs (JSON: requirements, depends_on with evidence, a status vector, and the expected qualified set + minimal paths + cycles). Cover: chains, diamonds (two equal-length paths → lexicographic tie-break), deep chains (≥8), disconnected components, multiple broken prerequisites, a dependent that is itself broken (stays broken), an unproven prerequisite (does NOT qualify dependents — only broken does; document this choice in the report), ≥3 graphs containing cycles (one through a broken node).
5. CLI `living-spec deps check <spec-dir>` (exit 4 on cycles, listing them) and `living-spec deps report <spec-dir> [--status status.json] [--md out.md] [--json out.json]`.

## Out of scope
Inferring dependencies from prose; task scheduling; changing any test outcome.

## Acceptance
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo test --offline deps_reference_set -- --nocapture   # reports 20/20
```
Required tests: `deps_reference_set` (all 20 load and match expected qualified sets, paths and cycles); `cycle_fixture_reported`; `prerequisite_red_marks_dependents_qualified` with the report printing the minimal path; `dependent_tests_untouched` (outcomes/evidence for dependents identical before and after propagation; no dependent becomes `broken`); missing evidence → load error.

## Contract revisions that apply
R4 overrides scope item 4's 'unproven prerequisite does NOT qualify': any non-proven prerequisite (broken, unproven, qualified) qualifies a proven dependent, with reason `prerequisite_broken`/`prerequisite_unproven`/`prerequisite_qualified` and the minimal path to the nearest non-proven prerequisite. Proven cycle members become `qualified` with reason `dependency_cycle`. Dangling ids fail loading. Never alter outcomes.
