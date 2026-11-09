# Packet 08: Integration and release — one live project, CI hook, `make verify`

Read `.handoff/packets/00-architecture.md` (binding, including the "Contract revisions" and "Public-repo hygiene" sections). Packets 01–07 are merged on your base branch. Integrate them; fix integration bugs where needed (say which in the report). Do not weaken any existing test.

## Goal
Wire the store, agents, hook, Boundary Cases, Dependency Proofs, Quantifier Map and web view into one pipeline, and make SpecCompanion itself the first fully live project: a living spec whose every line links to a test, rescored on every push.

## Step 0 (do first): finish merging packet 04
Your base commit merges packet 04 (web view and roll-up, built by another worker against the *original* packet-01 core). It still contains **conflict markers** in `living-spec/src/cli/mod.rs` and `src/lib.rs`, and its code predates the core changes from packets 03/03i/02i (R1–R5, `cli::Exit`, scoped test ids, `Link.revision`, `TestOutcome.failure_message`, new `Reason` variants such as `boundary_gap`, `prerequisite_*`, `dependency_cycle`, `stale_revision`, `test_changed`, `not_executed`). Remove every marker, register `render` and `rollup` in the central dispatch using `cli::Exit`, port `src/web/` and `tests/web.rs` to the current core, and make every reason render a readable explanation. Keep all of packet 04's tests (see `.handoff/packets/04-web-rollup.md`).

Also fix: the Quantifier Map Markdown export escapes characters inside backtick code spans, so the output shows stray backslashes (`\{`, `\-`). Escape only outside code spans and add a test.

## Scope
1. **One pipeline command.** `living-spec score <spec-dir> --results <file> --format libtest|junit [--tests <source-root>] [--commit SHA] [--html out.html]`: resolve links (re-auditing changed tests with the auditor), then run boundary gaps (packet 05 provider), core badges, dependency propagation (packet 06), and quantifier coverage for annotated lines (packet 07). It writes `status.json` and history, renders the spec page, and exits per R5 (2 if any line is broken). `rescore` keeps working and uses the same providers. Remove any duplicated scoring path.
2. **Spec page shows the shared layer.** For a line with bounds, the drill-down shows its boundary coverage table (below/at/above, tested or untested). For a quantified line, it shows the Quantifier Map's "Tested here" and "Claimed everywhere" panels, proof-free. For a qualified line, it shows the minimal dependency path.
3. **Dogfood spec.** `living-spec/living-spec/spec.md`: 25 to 40 plain-English requirement lines describing Living Spec itself (spec store, linker, auditor and the cheating case, runner hook, web view, roll-up, Boundary Cases, Dependency Proofs, Quantifier Map), under headings. Write `annotations.json` by running `living-spec link … --write` over the crate's own `tests/` and `src/`, then review and correct links by hand (`proposed_by: human` for hand-fixed ones). **Every line must have at least one accepted link** to a real test of this crate. Include ≥2 authored `depends_on` edges with evidence, ≥1 line with bounds whose cases link to real tests, and ≥1 quantified line. Commit the generated `status.json` and `spec.html` from a green run.
4. **CI hook (runs on every push).** `.github/workflows/living-spec.yml`: `on: push` (all branches) and `pull_request`, `ubuntu-latest`, `dtolnay/rust-toolchain@1.95.0` with rustfmt and clippy, `Swatinem/rust-cache@v2` with `workspaces: living-spec`, `permissions: contents: read`. Steps: fmt check, clippy `-D warnings`, `cargo test --locked` with output tee'd to a file, `cargo run --locked -- score living-spec --results test-output.txt --format libtest --tests . --commit "$GITHUB_SHA" --html spec.html`, and upload `status.json`, `spec.html` and `history.jsonl` as an artifact. The job fails when `score` exits non-zero. Pin action versions to the majors already used in the repo's other workflows.
5. **`make verify`.** Add `verify`, `living-spec-test` and `living-spec-score` targets to the root `Makefile` without changing the existing targets. `make verify` runs the living-spec fmt, clippy and tests (including `boundary_reference_set`, `deps_reference_set`, `quantifier_specs_count`, `cheating_case_is_challenged`, `happy_path_only_is_not_proven`, `simulated_push_flips_line_red`), then the dogfood `score`, and exits 0 only if everything is green.
6. **Roll-up.** Make `living-spec rollup --registry "$LIVING_SPEC_REGISTRY" --include . --out rollup.html --json rollup.json` work against the real-shaped registry fixture in tests. Add a test showing the roll-up includes this repo through `--include` with a non-zero proven %.
7. **Docs.** Add a "Living Spec" section to `README.md`: what the four badges mean, the spec format, the annotation layer, the commands, the git pre-push hook (opt-in), CI, roll-up usage with `LIVING_SPEC_REGISTRY`, the honesty rules, and the known limits. No real paths, names or emails. Add a CHANGELOG entry.

## Out of scope
Tauri/React app changes, publishing, releases, network access at tool runtime.

## Acceptance
```sh
make verify
cd living-spec && cargo test --offline 2>&1 | tee target/test-output.txt && cargo run --offline -q -- score living-spec --results target/test-output.txt --format libtest --tests .
```
More precisely: `make verify` exits 0. The dogfood `status.json` shows every line with ≥1 admissible link and a live proven/broken/unproven/qualified count. A deliberately broken copy of one linked test (in a temp copy of the crate, used inside a test) makes `score` exit 2. `actionlint`, if installed, passes on the new workflow (otherwise run `python3 -c 'import yaml'`-free structural sanity checks). The repo's git guards still pass: `pnpm git:guard:all` (if it can run in the sandbox; otherwise say so).
