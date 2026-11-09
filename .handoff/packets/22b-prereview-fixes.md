# Packet 22b: fix pre-review findings (06-web, 07-deps)

Read `.handoff/packets/00-architecture.md` (binding). An unsigned Codex Sol pre-review of the stacked PR layers produced the findings below. The text of each one was cut off at about 200 characters, so start by reproducing each from its file:line and its description, then fix it at the root and add a regression test named after it. If a finding does not reproduce, say so in the report with the evidence. Do not touch `src/agents/` (another worker owns it). Keep `make verify` and `sh living-spec/scripts/check-layers.sh` green. Conservative answers win: unproven, unresolved or an input error, never a false green and never silent loss.

## Findings

### 06-web
- [major] living-spec/src/web/rollup.rs:352 Rollup counts stored badges after validating only requirement IDs. Editing requirement text while retaining its ID leaves an old proven result contributing to p
- [major] living-spec/src/web/spec_page.rs:200 The any-passing-link check preserves proven status when another current accepted link has never executed. For example, add a second reviewed link after scoring:
- [major] living-spec/src/web/spec_page.rs:701 Boundary projection treats cached annotation bounds as current and ignores bounds_source revision handling. After an inferred limit changes from 30 ms to 20 ms,

### 07-deps
- [major] living-spec/src/deps/report.rs:52 Unselected unproven prerequisites disappear from both exports. For example, if proven R-003 depends on broken R-001 and unproven R-002 with BoundaryGap, BFS sel
- [major] living-spec/src/deps/model.rs:121 Removing vertices from on_path without retaining a visited set makes cycle-witness construction enumerate simple paths. A valid SCC with root→entry, entry→dense
- [major] living-spec/src/deps/model.rs:150 cycle_members clones the entire cycle for every member and performs repeated reachability searches. For the added 50,000-node cycle, qualification attempts to r

## Acceptance
`make verify` plus every new regression (name them in the report).
