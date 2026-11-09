# Packet 03: Runner hook and rescorer

Read `.handoff/packets/00-architecture.md` (binding) and `.handoff/stress-test-findings.md` if present. Build on `src/core/` from packet 01; additive helpers only.

## Goal
On each push, re-resolve every link by stable test identity, recompute every badge, append history, and fail loudly when a line goes red.

## Scope
1. `src/runner/adapters.rs`: parse libtest plain output (`test a::b ... ok|FAILED|ignored`, including the `failures:` section; tolerate interleaved stdout) and JUnit XML (`testsuite/testcase` with `failure|error|skipped`; id = `classname::name`, configurable). Output `Vec<TestOutcome>`. Unknown/garbled input is an error, not an empty list.
2. `src/runner/rescore.rs`: `rescore(spec_dir, outcomes, fingerprints, commit, note_source) -> RescoreReport { statuses, transitions, renamed_links, unresolved }`. Uses core `resolve_link` and `score_line`. Boundary gaps come from a pluggable `BoundaryGapProvider` trait (default: none) so packet 05 can plug in; qualified status from a `QualifiedProvider` trait (default: none) for packet 06. Renamed links are rewritten in `annotations.json` with a history note. Each transition appended to `history.jsonl` with commit, test id and a note (for `broken`: which test failed and, if available, the first failure line from the adapter output).
3. Breaking-commit attribution: a transition to `broken` records the commit passed in (`--commit`, default `git rev-parse HEAD` of the spec's repo, or the label `uncommitted`).
4. CLI `living-spec rescore <spec-dir> --results <file> --format libtest|junit [--commit SHA] [--fingerprints-from <source-root>]` prints a summary (counts per badge, then each transition `R-007 proven -> broken (test x::y failed at <commit>)`), writes `status.json` (current statuses, schema `contracts/living-spec-status-v1.schema.json`), and exits **2** when any line transitioned to `broken`, 0 otherwise (1 for usage/parse errors).
5. `living-spec/hooks/pre-push` (POSIX sh) and `living-spec/hooks/README.md`: runs the project's test command (from `living-spec/runner.toml`-free env var `LIVING_SPEC_TEST_CMD`, default `cargo test`) capturing output, then `rescore`; blocks the push on exit 2 with the summary. Install is opt-in (`git config core.hooksPath` is NOT changed by any code).
6. Fixtures `tests/fixtures/runner/` : a spec dir plus two libtest outputs (before: all pass; after: one linked test fails) and a JUnit sample with a renamed test.

## Out of scope
Fingerprint extraction beyond calling packet 02's `TestIndex` if present (if absent, accept `--fingerprints <json>` mapping id→fingerprint), web view, CI workflow (packet 08).

## Acceptance
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
```
Required tests: `simulated_push_flips_line_red` (copy fixture spec to a tempdir, rescore with "before" → exit 0 and all linked lines proven; rescore with "after" → exit 2, the line is `broken`, history has a `proven -> broken` entry carrying the commit and failing test id); rename resolved via fingerprint and annotations updated; unresolved link → broken (never dropped); garbled results → error exit 1; JUnit and libtest adapters each tested on failures, errors and skips.

## Core corrections owned by this packet (from the stress test)
You own R1, R2, R3 and R5 in `00-architecture.md`. You may modify `src/core/badge.rs`, `src/core/test_identity.rs`, `src/core/annotations.rs` (add `revision` on links/audits, `rename_hint`) and `src/core/spec.rs` (revision function, ignore fenced code) to implement them, keeping existing public names where possible. Update packet 01's tests to the new rules. Additional required tests: `skipped_test_is_not_proven`, `edited_line_makes_links_stale`, `edited_test_needs_reaudit`, `ambiguous_rename_is_unresolved`, `first_run_broken_exits_2`, `parametrized_case_missing_is_not_proven`, `challenged_failure_is_diagnostic_only`.
