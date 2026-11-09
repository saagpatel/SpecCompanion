# Packet 30: `deps report --status` must not reuse stale conclusions

Read `.handoff/packets/00-architecture.md` (binding). Review finding on `src/cli/deps.rs` around line 51 (part 7, PR #136): `load_status` discards the saved document's input bindings and checks only requirement ids. Score R-002 as qualified through broken R-001, then remove that dependency and run the report again with the saved status: `apply_qualified` leaves already-qualified lines alone, so the export still claims `R-002 → R-001` for an edge that no longer exists. Edited requirements likewise mix current text with old evidence and show no stale indication.

Fix it the same way as the web layer (packet 27): use `core::freshness` to compare the saved status's input digests (whole-file and per-requirement, with stored dependency edges) against the current files. Then:
- Recompute qualification from scratch over the **current** dependency graph, starting from each line's own pre-propagation badge (store it in the status if it isn't already there). Never carry a previous `qualified` forward.
- Any line whose inputs changed, together with its transitive dependents, is reported as **stale**, with no dependency conclusion and no impact path. A whole-file mismatch makes the report state that the status is stale and exit 1 with an input error unless `--allow-stale` is given, in which case every line shows stale.

Regressions in `tests/deps.rs`: `removed_dependency_is_not_reported`, `edited_requirement_is_stale_in_report`, `unchanged_status_reports_current_paths`. Keep `make verify` green, the layers valid and the dogfood at 33/33.
