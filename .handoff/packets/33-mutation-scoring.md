# Packet 33: mutation tier in scoring and status

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (binding). Packet 31
(model in `src/core/mutation.rs`, including `evaluate_link` and `line_tier`) and 32 (engine; use its pure `subject_digest_for`) are already on your base branch.

## Goal
Carry the optional tier through scoring, `status.json` and saved-score freshness without changing anything when the
mutation file is not supplied.

## Scope
1. `src/core/badge.rs`: add `LineStatus.mutation: Option<crate::core::mutation::Tier>` (`#[serde(default,
   skip_serializing_if = "Option::is_none")]`). `score_line_at_revision` keeps its plain-badge logic untouched.
2. `src/pipeline/` (and `src/runner/` only where needed): an optional `MutationEvidence` input
   (`Option<&MutationFile>`) passed through the existing rescore/score entry points via an extension of the existing
   context struct or provider pattern (follow how `QualifiedProvider` and `BoundaryGapProvider` are wired; do not
   add parameters to every function). After plain scoring and qualification, for each line whose plain badge is
   `Proven`, compute per-link states with `evaluate_link` using the current bindings (test fingerprint from the test
   index, requirement revision from the spec, and the subject digest recomputed from current source when a test index
   with Rust source is available; if the subject digest cannot be recomputed the link is `Stale`, never checked) and
   set `mutation` with `line_tier`. Lines that are not plain `Proven` get `None`. Only admissible accepted links
   participate.
3. `src/core/freshness.rs`: `ScoreInputs.mutation_digest: Option<String>` (serde default, skipped when none) holding
   the sha256 of the mutation file bytes at scoring time. It is **not** part of the plain score `digest`: a saved
   score whose mutation file changed or vanished keeps its plain badges fresh and renders the mutation tier as
   `stale` (implement the check where status freshness is evaluated for rendering). Everything is byte-identical
   when no mutation file was supplied.
4. `contracts/living-spec-status-v1.schema.json`: add the optional line `mutation` object (state enum, waived,
   signed_off, survivors with file/function/line/operator/original/replacement) **and** the optional `inputs.
   mutation_digest`, without changing required fields (the schema currently rejects unknown properties on both
   line and inputs); keep the schema's existing test passing and extend it. Update the web status reader
   (`src/web/status.rs`, `RawLine`) to carry the new line field through instead of discarding it.
5. History (`history.jsonl`) is unchanged: the tier is not a badge transition.

## Out of scope
CLI flags (packet 34), rendering (packet 35), engine. Do not alter plain-badge semantics or dependency
qualification. The tier never propagates through `depends_on` in v1.

## Tests
`status_json_byte_identical_without_mutation_file` (score a fixture twice, with and without the feature compiled in
use, compare to the committed expected bytes), `proven_line_gets_checked_tier`, `survivor_flagged_but_badge_stays_proven`,
`non_proven_line_has_no_tier`, `stale_when_subject_digest_differs`, `mutation_file_edit_stales_only_the_tier`, `web_reader_keeps_mutation_field`,
`tier_ignores_challenged_and_stale_links`, `status_schema_accepts_mutation_field`, `old_status_without_field_loads`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify` from the repo root. Diff well
under 200 KB.
