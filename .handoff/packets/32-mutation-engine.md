# Packet 32: mutation engine (subject resolution, mutant generation, crate-copy runner)

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (binding). Packet 31
(`src/core/mutation.rs`) is already on your base branch: reuse its types, `mutant_id` and `subject_digest`; do not
redefine them.

## Goal
`living_spec::mutation::engine`: given a Rust link, find the code the test calls, build a deterministic list of
mutants, run each in a throwaway copy of the owning crate, and return a `LinkRecord` (model type).

## Scope (new files under `living-spec/src/mutation/`)
1. `subject.rs`: from a `TestEntry` (see `src/agents/test_index.rs`) get the directly called free-function names of
   the test body (`name(...)` and `path::name(...)` only; reuse the lexer in `src/agents/syntax.rs`; add small public
   helpers there only if unavoidable and say so in your report). Resolve each name to a free `fn` definition in the
   owning package's non-test code (`src/**/*.rs`, skipping `#[cfg(test)]` modules and items, `tests/`, `benches/`,
   `examples/`). One hop only. Only an **unambiguous** name (exactly one definition) joins the subject; method calls,
   macro-generated functions, names with several definitions, names that resolve to nothing, and tests that only spawn
   a binary or call fixtures are not subjects. No subject => `unsupported: no resolvable subject`; more than 6 subject
   functions => `unsupported: subject too large`. Also derive the cargo target from the test id (`libtest:<target>:
   <file>:<name>`; `tests/<stem>.rs` => `--test <stem>`, `src/lib.rs` => `--lib`, anything else unsupported) and the
   owning package (single nearest `Cargo.toml` above the file under the tests root), and reject crates whose path
   dependencies or workspace inheritance reach outside the crate directory (`unsupported: dependencies outside
   crate`). Expose a pure `subject_digest_for(entry, tests_root)` (no cargo) so scoring can recompute bindings.
2. `generate.rs`: token-level mutant generation for the four operators in the contract, using the lexer so string/char
   literals, comments, attributes, `const`/`static` items, macro bodies and `#[cfg(test)]` code are never touched.
   `int_bump` only for plain decimal literals (no suffix, underscore or prefix). If a function's extent or attributes
   are uncertain (any `cfg` other than exactly `cfg(test)`), skip it. Record `function` and `ordinal` per mutant. Output `Mutant`
   (model type) plus the exact byte span in the file. Deterministic order; deterministic selection under the cap
   (round-robin across operators, then source order); the ordinal feeds `mutant_id`. Byte-exact application:
   `apply(source, span, replacement)`; a test proves applying then reverting yields the original bytes.
3. `runner.rs`: the crate-copy runner. Copy the owning crate directory (target and package as in `subject.rs`) into a
   `tempfile` directory excluding `target/` and `.git/`, once per runner instance, restoring the file between mutants;
   one shared `CARGO_TARGET_DIR` per runner instance inside that temp tree (move `tempfile` to `[dependencies]`;
   state this in your report); run
   `cargo test --offline --locked <--lib | --test <target>> <name> -- --exact` via `std::process::Command` (no shell)
   with `CARGO_TERM_COLOR=never` and `NO_COLOR=1` set on **every** spawn, a wall-clock timeout (kill the process group),
   and stdout+stderr captured. Classify with the existing `parse_libtest`: the run must show **exactly one executed
   test whose id matches** (a filter matching zero tests, or garbled output, is never a pass or a kill):
   - mutant: executed test passed => `survived`; a completed matching **failed** case in parsed output => `killed`;
     timeout => `timeout` (inconclusive, never a kill); cargo build failure (nonzero exit, no test result, and a
     `error[E` / `could not compile` marker) => `unviable`; anything else (spawn failure, garbled output, missing
     toolchain or dependency) => the whole link is `error` with a reason.
   - baseline (unmutated copy) runs twice first; both must pass or the link is `error: baseline failed|flaky`.
   Zero viable mutants => `unsupported: no viable mutants`. A timeout with no survivor => link `error: timeout`.
   Record `baseline_passes` (0, 1 or 2) and the `target` in the record.
   Never modify the original tree (a test hashes the whole source tree before and after).
4. `mod.rs`: `run_link(options, entry, requirement, requirement_revision) -> LinkRecord` filling `test_fingerprint`,
   `requirement_revision`, `subject_digest` (via the model's function over the sorted subject functions), `status`,
   `mutants`, and `reason`. Options: `max_mutants` (8), `timeout` (180 s), `cargo` binary path (so tests can inject a
   stub), `jobs` is handled by the CLI, not here. Non-Rust tests return `unsupported: only Rust tests are supported in
   v1` with no cargo spawn.
5. `living-spec/scripts/check-layers.sh`: add the `mutation` layer (`allowed='core adapters agents mutation'`) and let
   `pipeline` and `cli` import it; `core`, `web`, `runner` must not. Keep the check passing.
6. Fixtures: a tiny sample crate under `living-spec/tests/fixtures/mutation/sample-crate/` (own `Cargo.toml`, a
   `src/lib.rs` with a few functions, `tests/strong.rs` whose test kills every mutant, `tests/weak.rs` whose test
   calls the code but only asserts something that lets at least one mutant survive, an inline `#[cfg(test)]` unit test,
   and a file with strings/comments containing operator text that must not be mutated). It must be excluded from any
   workspace of the main crate and from the repo's own lint/test discovery (check `Cargo.toml` `exclude` and
   `scripts/check-layers.sh`; do not break them). Keep the Cargo.lock offline-resolvable (no dependencies).

## Out of scope
Scoring, CLI, web, docs. No parallel `--jobs` here. No network. No new crate dependencies.

## Tests (integration tests may run real cargo on the sample crate; unit tests need no cargo)
`strong_test_kills_every_mutant`, `weak_test_leaves_a_survivor`, `string_and_comment_operators_not_mutated`,
`cfg_test_code_not_mutated`, `zero_matching_tests_is_error_not_kill`, `build_failure_is_unviable_not_kill`,
`baseline_failure_is_error`, `timeout_is_inconclusive_not_a_kill`, `ambiguous_name_not_a_subject`, `method_call_not_a_subject`, `path_dependency_outside_crate_unsupported`, `bin_target_and_doctest_unsupported`, `original_tree_unchanged`,
`cargo_env_forced_no_color` (inject a stub cargo that records its env and args, assert `CARGO_TERM_COLOR=never` and
`NO_COLOR=1` on every call), `non_rust_test_unsupported_without_spawn`, `unresolvable_subject_unsupported`,
`mutant_cap_deterministic`, `subject_digest_changes_when_subject_edited`, `apply_then_revert_round_trips`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, and from the repo root `make verify`. Diff well
under 200 KB. Public repo: no absolute or real paths in code, fixtures or output.
