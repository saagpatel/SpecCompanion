# Packet 04: Static web view and portfolio roll-up

Read `.handoff/packets/00-architecture.md` (binding). Build on `src/core/`; additive helpers only.

## Goal
A local, offline, self-contained HTML view of one living spec, and a portfolio roll-up of proven percentages across kept projects read from the portfolio-truth registry.

## Scope
1. `src/web/spec_page.rs`: `render_spec(spec, annotations, status, history) -> String` producing one self-contained HTML file (inline CSS, no JS required; `<details>` for drill-down, no external URLs). Header shows live counts: proven / broken / unproven / qualified and proven %. Lines grouped under their headings; red (broken) lines first within the page summary, gray (unproven) lines listed last as "promises nobody has proven yet". Each line: badge (color + text label, accessible, not color-only), text, id. Drill-down: linked tests (id, last status), for broken lines the breaking commit and the history note, the auditor's notes, reasons (boundary_gap, prerequisite path for qualified). Escape all text (HTML injection test).
2. `src/web/rollup.rs`: read the registry snapshot (shape in the contract; parse only needed fields, ignore the rest, tolerate missing optional fields), select kept projects (`derived.archived == false`), plus `--include` paths. For each: if `<root>/living-spec/status.json` exists use it (else if spec exists, compute statuses with no outcomes → all unproven/proven per annotations? NO: without a status.json report "not yet scored"), produce `RollupRow { project, path, lines, proven, broken, unproven, qualified, proven_pct, state: scored|not_scored|no_living_spec }`. Portfolio total proven % = sum proven / sum lines over scored projects. Rows sorted by proven % ascending, no-spec projects counted in a separate tally (shown, not hidden).
3. `render_rollup(rows) -> String` self-contained HTML table + JSON export.
4. CLI `living-spec render <spec-dir> --out <file.html>` (reads status.json if present, else treats lines as unscored with gray badges and says so) and `living-spec rollup [--registry PATH] [--include DIR]... --out <file.html> [--json <file.json>]`. Registry path: `--registry`, else env `LIVING_SPEC_REGISTRY`, else a usage error (no hard-coded default). Fixtures use invented project names only.
5. Fixtures `tests/fixtures/web/`: a seeded spec dir with status.json + history.jsonl, and `tests/fixtures/web/registry/portfolio-truth-mini.json` in the real shape with ≥4 projects (2 kept with living specs and status, 1 kept without a spec, 1 archived that must be excluded) and workspace dirs beside it.

## Out of scope
React/Tauri UI changes, JS, servers, network fonts.

## Acceptance
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo run --offline -q -- render tests/fixtures/web/seeded/living-spec --out /dev/stdout | grep -q 'proven'
```
Required tests: view renders the seeded project with correct counts and a broken line's drill-down containing its test id, breaking commit and note; gray lines appear after all others; HTML escaping of `<script>` in a requirement; roll-up aggregates ≥2 projects with correct per-project and total percentages, excludes the archived project, and lists the no-spec project; the rendered HTML contains no `http://` or `https://` references.
