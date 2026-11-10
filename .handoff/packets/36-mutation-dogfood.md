# Packet 36: Makefile, optional CI job, docs, dogfood wiring

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
