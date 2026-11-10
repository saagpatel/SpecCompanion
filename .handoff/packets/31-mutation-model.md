# Packet 31: mutation model (pure types, no cargo) plus best-effort baseline docs

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (both binding).

## Goal
Add the data model for the mutation tier as pure, offline, deterministic Rust, plus the documentation that makes the
static auditor the explicit best-effort baseline.

## Scope
1. New file `living-spec/src/core/mutation.rs` (register in `src/core/mod.rs`; core imports nothing outside core):
   - Types per the contract: `MutationFile`, `LinkRecord`, `Mutant`, `Signoff` (`link` and `mutant` kinds), `LinkStatus`
     (`checked|survived|unsupported|error`), `MutantOutcome` (`killed|survived|timeout|unviable`), `Operator`
     (`cmp_flip|arith_swap|bool_swap|int_bump`), `Target { kind: lib|test, name }`. Link records carry `target`,
     `baseline_passes`, and each mutant carries `function` and `ordinal` (see the contract's file shape).
     Strict serde: unknown fields rejected, `schema` must be `living-spec.mutation/v1`, `engine` must be `rust-cargo-v1`.
   - `MutationFile::from_bytes`, `to_pretty_bytes` (stable key order, trailing newline), and a `validate` that
     **recomputes each link's status from its evidence** and rejects: a stored status that disagrees (`checked` needs
     `baseline_passes == 2`, at least 1 viable mutant, all viable mutants `killed`, no `timeout`; any `survived` mutant
     means `survived`; a `timeout` with no survivor means `error`), duplicate `(requirement, test_id)` records, a mutant
     id that does not match its recomputed id, duplicate mutant ids inside a link, absolute paths or `..` in `file`,
     empty `reason` on `unsupported`/`error`, and signoffs missing a binding field.
   - `mutant_id(file, function, operator, original, replacement, ordinal) -> String` and `subject_digest(parts) -> String`
     exactly as the contract states (sha256, `sha256:` prefix for digests, `m-` plus 12 hex for ids). Document the
     byte layout in a comment so the engine packet reuses these functions.
   - `LinkState` and `evaluate_link(record, bindings, signoffs) -> LinkState` where `bindings` carries the current
     `test_fingerprint`, `requirement_revision` and `subject_digest` (the last `Option`, because non-Rust links have
     none). Result states: `Checked`, `Waived { waived }`, `SignedOff`, `Survived { survivors }`, `Stale`,
     `Error`, `Unsupported`, `NotRun` (no record). Implement contract rules 1 to 3 exactly, failing closed.
   - `line_tier(link_states, plain_badge_is_proven) -> Option<Tier>` implementing contract rule 4 exactly
     (`mutation_checked` only when every link is a pure `Checked`; `signed_off` when the rest are checked but at
     least one is `Waived` or `SignedOff`; otherwise the worst state `survived > stale > error > unsupported >
     not_run`). `Tier` serializes as the contract's line field: `{ "state", "waived", "signed_off", "survivors" }`.
2. `contracts/living-spec-mutation-v1.schema.json` (JSON Schema, `additionalProperties: false` everywhere) and a test
   that validates the sample file below against the same rules the Rust validator enforces (keep schema and code in
   step; look at how `living-spec-status-v1.schema.json` is tested and mirror it).
3. A sample fixture `living-spec/tests/fixtures/mutation/sample.json` (checked, survived, unsupported, waived survivor,
   signoff) used by the tests.
4. Docs, the baseline statement the task requires: in `living-spec/README.md` (or the repo `README.md` Living Spec
   section, wherever the existing "best-effort" paragraph lives) and in `.handoff/packets/00-architecture.md` honesty
   rules, add a short "Proof tiers" section: plain `proven` rests on the static allowlist auditor, which is the
   **documented best-effort baseline and is no longer being hardened**; its known limits stay listed; the opt-in
   mutation-checked tier is the stronger evidence and is described in one paragraph with a pointer to
   `.handoff/packets/30-mutation-architecture.md`. Do not claim anything the code does not yet do: say the tier "is
   being added in stacked changes" until packet 36 updates it.

## Out of scope
Running cargo, parsing Rust source, mutant generation, CLI, scoring changes, web. Do not touch `src/agents/auditor.rs`.

## Tests (name them so a failure reads as the rule)
`tier_requires_every_admissible_link_checked`, `waiver_makes_signed_off_not_checked`, `timeout_is_error_not_kill`, `stored_status_must_match_evidence`, `stale_binding_is_never_checked`, `waiver_bound_to_mutant_and_subject`,
`link_signoff_never_applies_to_survived`, `signoff_bound_to_fingerprint_and_revision`, `unknown_field_rejected`,
`mutant_id_is_stable`, `validate_rejects_checked_with_survivor`, `validate_rejects_absolute_path`,
`non_proven_line_has_no_tier`, `sample_fixture_round_trips_byte_stable`, and the schema/code parity test.

## Acceptance (run from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then from the repo root `make verify`.
No new dependencies. Diff must stay well under 200 KB.
