# Packet 32c: crate-copy runner and `run_link` (engine part 3)

Read `.handoff/packets/30-mutation-architecture.md` and `32-mutation-engine.md` (binding; this packet is items 3 and 4 of 32 plus the remaining tests). Your base already has the model, `generate.rs`, `subject.rs`, `candidates_for`, `subject_digest_for` and the sample crate.

## Scope
1. `src/mutation/runner.rs`: per packet 32 item 3 exactly. One stable crate copy and one `CARGO_TARGET_DIR` per runner instance in a `tempfile` directory (move `tempfile` to `[dependencies]`, state it in your report), file restored between mutants, `std::process::Command` with no shell, args as arguments, **`CARGO_TERM_COLOR=never` and `NO_COLOR=1` on every spawn**, wall-clock timeout that kills the process group, stdout+stderr captured. Classification via the existing `parse_libtest`: exactly one executed matching test required; a completed matching failed case is `killed`; passed is `survived`; timeout is `timeout`; build failure (nonzero, no test result, `error[E` or `could not compile`) is `unviable`; anything else records `infra_failure` on the link (the model field added in PR 141's follow-up) keeping completed outcomes. Baseline runs twice first; record `baseline_passes`.
2. `src/mutation/mod.rs`: `run_link(options, entry, requirement, requirement_revision, tests_root) -> LinkRecord` per packet 32 item 4: options `max_mutants` (8), `timeout` (180 s), `cargo` binary path (injectable stub). Non-Rust tests return `unsupported: only Rust tests are supported in v1` with no spawn. The returned record must pass the model's `validate`.
3. The original tree is never modified.

## Tests
Integration tests on the sample crate with real cargo: `strong_test_kills_every_mutant`, `weak_test_leaves_a_survivor`, `original_tree_unchanged` (hash the source tree before and after). Stub-cargo tests (a script that records env and args and prints canned libtest output): `cargo_env_forced_no_color` (every call, including baseline), `zero_matching_tests_is_error_not_kill`, `build_failure_is_unviable_not_kill`, `baseline_failure_is_error`, `timeout_is_inconclusive_not_a_kill`, `killed_then_spawn_failure_keeps_evidence_as_error`, `non_rust_test_unsupported_without_spawn`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `sh scripts/check-layers.sh`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify` from the repo root (alone, never overlapping another cargo run).
