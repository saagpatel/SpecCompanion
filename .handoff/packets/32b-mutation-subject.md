# Packet 32b: subject resolution, function extents, sample crate (engine part 2)

Read `.handoff/packets/30-mutation-architecture.md` and `32-mutation-engine.md` (binding; this packet is items 1, 2 integration and 6 of 32). Your base already has the preliminary `src/mutation/generate.rs` (candidate generation that needs a caller-verified non-test function extent), the layer-check update and `lib.rs` wiring. Do not rewrite it unless you find a bug; fix bugs with a regression test.

## Scope
1. `src/mutation/subject.rs`: per 32 item 1: directly called free-function names from a `TestEntry` body, resolution to exactly-one `fn` definitions in the owning package's non-test code, **with a verified function extent** (byte span of the whole item, attribute list, and whether it is inside `#[cfg(test)]`), skipping ambiguous, method, macro-generated, cfg-uncertain items. Cargo target derivation from the test id (`tests/<stem>.rs` => test, `src/lib.rs` => lib, else unsupported), owning package (single nearest `Cargo.toml` under the tests root), dependencies-outside-crate rejection. Output a `Subject { package_dir, target, functions: Vec<{file, name, span, source}> }` or an `Unsupported(reason)` using the exact reason strings in packet 32.
2. Connect it to `generate.rs`: a function `candidates_for(subject, max_mutants)` returning the deterministic, round-robin-capped `Mutant` list (model type) with byte spans, using extents from the subject.
3. `pub fn subject_digest_for(entry, tests_root) -> Result<Option<String>, Unsupported>` (pure, no cargo) using the model's digest function over the sorted subject functions.
4. Sample crate `living-spec/tests/fixtures/mutation/sample-crate/` per packet 32 item 6 (strong test, weak test, inline cfg(test) unit test, strings/comments containing operator text, an ambiguous-name function, a method, a bin target). Excluded from workspace and lint discovery; no dependencies; its own Cargo.lock.

## Out of scope
Spawning cargo, the crate-copy runner, `run_link` (packet 32c).

## Tests (no cargo spawn)
`ambiguous_name_not_a_subject`, `method_call_not_a_subject`, `cfg_test_code_not_mutated`, `string_and_comment_operators_not_mutated`, `path_dependency_outside_crate_unsupported`, `bin_target_and_doctest_unsupported`, `unresolvable_subject_unsupported`, `subject_too_large_unsupported`, `subject_digest_changes_when_subject_edited`, `mutant_cap_deterministic`, `apply_then_revert_round_trips`, `target_derivation_lib_and_test`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `sh scripts/check-layers.sh`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify` from the repo root (run it alone, never overlapping another cargo run).
