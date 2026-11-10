# Packet 37: everything left in one pass (packets 33, 34, 35, 36 combined)

Read `.handoff/packets/30-mutation-architecture.md` (binding). The model (`src/core/mutation.rs`) and engine
(`src/mutation/`, with `run_link` and `subject_digest_for`) are on your base. **Build all four parts below in order and
do not stop early or report partial: each part has a small acceptance of its own, and the final status is `done` only
when all pass.** Run cargo one job at a time. Run `make verify` once at the very end, not between parts. Keep the
diff under 200 KB; if you pass 150 KB, stop and say so in the report.

Per-part acceptance lines that say "then `make verify`": skip them; run `make verify` only once at the end. Per part, run fmt, clippy and the part's own tests.

Order of work: Part A (scoring), Part B (CLI), Part C (web), Part D (wiring and docs). Commit plan: one commit per part.

The four original packet texts follow. Where they say "on your base branch", the earlier parts of this packet count.

---
## Part A: packet 33

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

---
## Part B: packet 34

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (binding). Packets 31
(model), 32 (engine) and 33 (scoring) are on your base branch. `--cargo <path>` hidden test flag is allowed on `mutate` only.

## Goal
Make the tier usable from the command line, opt-in, with the existing hand-rolled argument style (no clap).

## Scope
1. `src/cli/mutate.rs`, registered with one line in `src/cli/mod.rs` and the usage string:
   `living-spec mutate <spec dir> --tests <root> [--crate <dir>] [--max-mutants N] [--jobs N] [--timeout-secs N]
   [--only REQ-ID]... [--out <file>]`.
   - Load spec, annotations and the test index for `--tests`. For every accepted, currently resolvable link (skip
     challenged, unresolved or stale-revision links with a printed reason) call the engine's `run_link`.
   - `--jobs N` runs links in parallel with `std::thread` workers, each using its own runner instance (own copy and
     target dir); results are merged in a stable order (requirement id, then test id) so output is deterministic.
   - Merge into an existing `mutation.json` (replace records for the same `(requirement, test_id)`, keep others and
     all signoffs); write atomically (temp file in the same directory, then rename); never write absolute paths.
   - Print one line per link (`LS-001 checked 8/8 killed`, or `survived m-… src/x.rs:12 cmp_flip`) and a summary.
     Exit 0 when the run completed (survivors are findings, not failures); exit 1 on invalid input or an engine
     `error`; keep the existing usage exit code 64.
   - `CARGO_TERM_COLOR=never` and `NO_COLOR=1` are already forced by the engine; add a test that the CLI path inherits
     it (use the engine's stub cargo option through a hidden `--cargo <path>` flag used only by tests).
2. `src/cli/signoff.rs`, same registration: `living-spec signoff <spec dir> link <REQ> <test_id> --reason <text>
   --signed-by <handle>` and `living-spec signoff <spec dir> mutant <REQ> <test_id> <mutant_id> --reason <text>
   --signed-by <handle>`. It binds the entry to the current test fingerprint, requirement revision and (for mutant
   waivers) the record's subject digest, refuses a mutant waiver for a mutant that does not exist, refuses a link
   signoff for a link whose record is `survived`, requires non-empty reason and handle, and is idempotent.
3. `score --mutation [<path>]` in `src/pipeline/score.rs` (flag value optional: default `<spec dir>/mutation.json`;
   keep the argument parser strict, no duplicates). Missing file with the flag given is an invalid-input error, not
   silent. Without the flag the command is untouched. Same flag on `rescore` if it shares the code path.
4. Usage text and `living-spec/hooks/README.md` mention the flag (one paragraph).

## Out of scope
Rendering, roll-up, Makefile, docs beyond the usage lines, any change to engine behaviour (report bugs instead).

## Tests
`mutate_writes_deterministic_file_for_sample_crate` (run twice, byte-identical), `mutate_merge_preserves_signoffs`,
`mutate_skips_challenged_links_with_reason`, `survivor_does_not_fail_exit_code`, `score_without_flag_unchanged`,
`score_with_flag_attaches_tier`, `score_flag_missing_file_is_error`, `signoff_link_rejected_for_survived`,
`signoff_mutant_binds_subject_digest`, `signoff_idempotent`, `jobs_result_order_stable`, `no_absolute_paths_in_output`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify`. Diff well under 200 KB.

---
## Part C: packet 35

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (binding). Packets 31
to 33 (model in `src/core/mutation.rs`, `LineStatus.mutation`, web status reader) are on your base branch. `web` may import only `core`.

## Goal
Show the mutation tier as a second badge next to plain `proven`, and have the portfolio roll-up report both numbers.

## Scope
1. `src/web/spec_page.rs` (and `src/web/html.rs` helpers): for a line with `mutation`, render beside the plain badge:
   - `mutation_checked` => `proven (mutation-checked)`; `signed_off` => `proven (mutation sign-off)` with the
     waiver and signoff counts (human judgment, shown distinctly and never counted as mutation-checked);
   - `survived` => a visible `mutation survivor` flag with the survivor list (file, line, operator) in a details
     element; `stale`, `error`, `unsupported` => a muted `mutation: <state>` note; `not_run` => nothing extra.
   - All text HTML-escaped; no new inline scripts; the page stays self-contained and works with scripts disabled.
   - Page header counts show plain `proven` and, when any line has a tier, `of which mutation-checked N`.
   A page rendered from a status with no `mutation` fields must be byte-identical to today (golden test).
2. `src/web/rollup.rs`: per project add `proven_mutation_checked` (lines whose tier is pure `mutation_checked`),
   `proven_signed_off` (tier `signed_off`) and `mutation_survivors` (tier `survived`) next to `proven`; the HTML
   table gets one extra column group only when at least one project has a tier, and shows both percentages (`100%
   proven, 61% mutation-checked`) and the sign-off count. JSON export gains the three fields additively (a project
   with no mutation data reports `0`s); existing keys and order unchanged. The roll-up HTML is byte-identical when no
   project has any tier.
3. The Quantifier Map views must stay free of proof language: do not touch them, and add no mutation text there.

## Out of scope
Scoring, CLI, engine, docs. No JavaScript, no external assets.

## Tests
`page_without_mutation_is_byte_identical`, `checked_line_shows_second_badge`, `survivor_flag_lists_location`,
`waived_count_shown`, `stale_tier_not_shown_as_checked`, `html_escaped_survivor_text`, `rollup_reports_both_counts`,
`rollup_json_additive_fields_only`, `rollup_html_without_mutation_unchanged`, `signed_off_not_counted_as_checked`, `quantifier_views_have_no_mutation_text`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify`. Diff well under 200 KB.

---
## Part D: packet 36

Read `.handoff/packets/00-architecture.md` and `.handoff/packets/30-mutation-architecture.md` (binding). Packets 31 to
35 are on your base branch.

## Goal
Wire the tier into the repo's workflow as strictly opt-in, document it truthfully, and prepare the dogfood run. The
head will run the long dogfood mutation job itself and commit the generated `mutation.json`; you build everything
around it and prove it on a small subset.

## Scope
1. `Makefile`: `living-spec-mutate` (runs `living-spec mutate living-spec/living-spec --tests . --jobs 2` from the
   crate, with `CARGO_TERM_COLOR=never NO_COLOR=1`, writing `living-spec/living-spec/mutation.json`; accepts
   `MUTATE_ARGS` for `--only`/`--max-mutants`) and `living-spec-score-mutation` (scores a scratch copy like
   `living-spec-score` does, with `--mutation`, and prints how many lines are mutation-checked, survived, stale).
   Neither is a dependency of `verify`; `make verify` stays fast and unchanged except that it must pass with a
   committed `mutation.json` present (a stale file is reported, never a failure). Add the new targets to `.PHONY`.
2. `.github/workflows/living-spec.yml`: add an optional job `mutation` triggered only by `workflow_dispatch` (never
   on push/PR) that runs `make living-spec-mutate` then `make living-spec-score-mutation` and uploads
   `mutation.json` as an artifact. `CARGO_TERM_COLOR=never` and `NO_COLOR=1` at job env. Keep the existing job
   untouched; extend `living-spec/scripts/test-ci.rb` if it validates the workflow structure.
3. `living-spec/scripts/refresh-dogfood.sh`: when `mutation.json` exists next to the spec, also score with
   `--mutation` so the committed `status.json`, history and page carry the tier; if absent, behave as today.
4. Docs: `living-spec/README.md` (or the repo README Living Spec section): a "Mutation-checked tier" section with the
   plain-language model (what is mutated, what killed/survived mean, equivalent mutants, signoffs, stale rules), the
   commands, the cost (minutes per link, why it is opt-in), and the honest limit (a test that can fail is not proof the
   requirement is met). Update the "Proof tiers" paragraph from packet 31 to the present tense and say Rust-only in v1
   with human signoff as the fallback. `CHANGELOG.md` entry. Contracts list mentions the new schema.
5. Subset proof: add a test (`dogfood_subset_mutation_round_trip`, may be `#[ignore]` only if it exceeds 60 s;
   prefer a fast sample-crate version) showing `mutate` then `score --mutation` on the sample crate yields one
   `mutation_checked` line and one flagged survivor. Add an integration test that the committed dogfood
   `mutation.json`, if present, parses, validates, and contains no absolute paths.

## Out of scope
Changing engine, scoring or web behaviour (report bugs instead). Do not run the full dogfood mutation job.
Do not commit a `mutation.json` for the dogfood spec; the head does.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, `ruby living-spec/scripts/test-ci.rb` from the repo
root, then `make verify`. Diff well under 200 KB.
