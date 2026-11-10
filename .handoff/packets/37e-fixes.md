# Packet 37e: signoff identity and parameterized-case findings (PR 146)

Base = PR 146 branch. Run cargo one job at a time; skip `make verify`. Use the single path-normalization helper added in packet 37d. Add the new packet text to `.handoff/packets/37e-fixes.md` in your commit (copy this file).

1. **`signoff` must take the same `--tests <root>` as `mutate`** (optional; default as today: the spec dir's parent) and the same identity normalization, so a sibling `spec/` and `tests/` layout works. Add the flag to usage text and the hooks README mention. Regression `mutate_then_signoff_non_rust_link_with_sibling_tests_dir` using a Python sample test (`pytest:...` ids): mutate records it `unsupported`, `signoff link` with `--tests` succeeds, scoring shows `signed_off`.
2. **Parameterized case links.** `TestIndex::get` stores only base entries; accepted links may name `base#case`. In `mutate` and `signoff` resolve a `base#case` id through its base entry (use the same base/case rules as plain scoring), but keep the full case id in mutation records and signoff bindings. A case link on a Rust test is `unsupported: parameterized case` for mutation (no spawn), and signoff must accept it. Regression `parameterized_case_link_mutate_signoff_score` covering mutate, link signoff and mutation scoring (tier `signed_off`).

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`. Status `done` only when all pass.
