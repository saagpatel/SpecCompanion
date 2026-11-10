# Packet 35: badges on the spec page, columns in the roll-up

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
