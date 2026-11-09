# Packet 25: fix drill-2 findings (web, deps, quantifier)

Read `.handoff/packets/00-architecture.md` (binding) and `.handoff/reviews/drill2-findings.md` (full text of each finding, with suggested fixes). Fix all six at the root, each with a regression named after it. Do not touch `src/agents/` (another worker owns it). Keep `make verify` and `sh living-spec/scripts/check-layers.sh` green. Conservative answers win: unproven, stale or an input error, never a false green and never silent loss.

1. web `spec_page.rs:749`: check stored boundary reports against the current `boundary_links`. When links have changed or been removed, show the result as stale/unproven, never with positive badges.
2. web `spec_page.rs:877`: match quantifier reports to the current claim, not just the requirement id. Edits to the predicate, domain or text make the report stale.
3. web `status.rs:84`: reject duplicate JSON keys anywhere in `status.json` (parsing through `serde_json::Value` silently keeps only one, so a duplicate `statuses` can replace broken results with proven ones). Reuse the duplicate-key rejection already used for annotations.
4. deps `contracts/living-spec-dependency-report-v1.schema.json:9`: make the schema and `ImpactReport` agree. Every report the code emits, including non-empty `unproven_prerequisites` and `qualified_prerequisites`, must validate; add a test that validates real emitted reports.
5. deps `propagate.rs:29`: replace the per-requirement traversal that clones the path with one linear-time pass (multi-source BFS from the non-proven nodes on the reversed graph, keeping parent pointers so minimal paths still come out with the same lexicographic tie-break). Regression: a 50,000-node all-proven chain finishes quickly, and every existing reference-set path is unchanged.
6. quantifier `annotations.rs:183` (minor): an explicit `sampling: null` must be rejected (or handled exactly like the earlier `quantifier: null` fix), never silently dropped.

## Acceptance
`make verify` plus every new regression (name them in the report).
