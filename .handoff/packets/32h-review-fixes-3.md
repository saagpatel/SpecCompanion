# Packet 32h: two more review findings on the mutation engine (PR 142)

Read `.handoff/packets/30-mutation-architecture.md`. Base = the PR 142 branch. Fail closed, simplest possible rules, one named regression each. File: `living-spec/src/mutation/subject.rs` and tests. Do not redesign anything.

1. **A same-named cfg(test) helper hides the real binding.** Production `fn value()` plus a `#[cfg(test)]` module with `fn value() { super::value() }` and a test calling `value()` selects the production function. Rule: every free `fn <name>` defined anywhere in cfg(test) code of the package (inline test modules, `#[cfg(test)] mod x;` files, `tests/*.rs`, `benches/`, `examples/`) and inside the linked test's own file counts as a competing definition, so the name is ambiguous and not a subject. Regression `cfg_test_helper_with_same_name_makes_name_ambiguous`; keep the positive `plain_call_still_resolves`.
2. **`include!` and other opaque item-producing macros.** Rule: if any file under the package's `src/` contains `include!(` or `include!{` (also `include!` with spaces), or an invocation of a non-std macro at item position that is not defined by a visible `macro_rules!` whose body has no `fn` (i.e. any item-position macro call we cannot see through), return `unsupported: opaque item macros` for the whole package. Regression `include_generated_wrapper_stays_unsupported` with a `generated::value()` wrapper delegating to a production `value()`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `sh scripts/check-layers.sh`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`. Run cargo one job at a time. Skip `make verify` (the head runs it once at the end). Status `done` only when all pass.
