# Packet 31b: fix review findings on the mutation model (PR 141)

Read `.handoff/packets/30-mutation-architecture.md`. Your base branch already has packet 31. Two review findings:

1. **Infrastructure failure must be representable next to partial evidence.** Today an `error` record is rejected when
   baseline passes are 2 and a mutant is killed, so the engine (packet 32) could not keep completed outcomes when a
   later spawn failure or garbled result forces the whole link to `error`. Add an optional link field
   `infra_failure: String` (non-empty when present, serde default and skipped when absent) to the model, schema and
   contract (`.handoff/packets/30-mutation-architecture.md` file-shape section and rule list). Status recomputation:
   `infra_failure` present => status must be `error` (precedence over everything, completed mutants kept as evidence,
   never counted toward checked); absent => existing rules unchanged. `reason` stays required on `error`. Regression
   tests: `killed_mutant_then_infra_failure_is_error_and_keeps_evidence`, `infra_failure_blocks_checked_even_if_all_killed`,
   `empty_infra_failure_rejected`, plus a fixture entry in `sample.json`.
2. **Schema/code parity test must assert each validator independently.** In `living-spec/tests/mutation.rs` stop
   asserting a combined conjunction is false. For constraints both can express, assert the schema result and the Rust
   result separately and equal. For Rust-only semantic checks (mutant id hash recomputation, link-key uniqueness,
   status-from-evidence), keep an explicit list and test that the schema accepts the shape while Rust rejects it, with
   a comment saying these are intentionally Rust-only.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify` from the repo root.
