# Packet 34: `mutate`, `signoff` and `score --mutation`

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
