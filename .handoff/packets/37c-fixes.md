# Packet 37c: three CLI findings on PR 146

Base = PR 146 branch. Run cargo one job at a time; skip `make verify`. One named regression each.

1. **Overridden links in `mutate` and `signoff`.** Both check `AuditVerdict` directly (`src/cli/mutate.rs` ~162, `src/cli/signoff.rs` ~61) and so skip valid human overrides that plain scoring admits. Use `Link::is_accepted()` in both, keeping the revision and fingerprint checks. End-to-end regression `overridden_link_can_be_mutated_and_signed_off`.
2. **`signoff` with a single-component relative spec path** (`living-spec signoff living-spec ...`): `Path::parent()` returns an empty path and `TestIndex::build("")` fails. Canonicalize the spec dir before taking its parent (or map an empty parent to `.`). Regressions for both `link` and `mutant` signoffs invoked with a relative one-component spec path from the project root.
3. **`score --mutation` with `--tests <crate>/tests`** (`src/pipeline/score.rs` ~141): pass the same normalized identity root that `TestIndex` and `mutate` use to the mutation scoring input so subject resolution does not join `tests/tests/...` and wrongly mark checked evidence stale. Regression `mutate_then_score_with_crate_tests_dir_as_tests_root` (mutate and score both with `--tests <crate>/tests`; the checked link stays checked).

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`. Status `done` only when all pass.
