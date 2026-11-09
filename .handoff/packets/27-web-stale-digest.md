# Packet 27: the web view never re-derives badges; a stale status shows as stale

Read `.handoff/packets/00-architecture.md` (binding) and `living-spec/scripts/check-layers.sh` (`web` may import only `core`). The part-6 review (PR #134) found two more ways the web view keeps an earned-looking green after its inputs changed:
1. `src/web/spec_page.rs:267`: dependency propagation isn't reapplied after evidence is downgraded. A depends on B, both were scored proven, B is edited: the view downgrades B but A stays proven (violates R4).
2. `src/web/spec_page.rs:813`: a stored passing boundary case only needs a non-empty `test_id` and is never matched to the current boundary links, so a receipt with no `boundary_links` and arbitrary `tested_pass` ids keeps a proven badge.

Both come from the same design flaw: the web layer tries to re-validate a stored `status.json` piece by piece, and every piece it misses is a hole. Fix the class:
- When the pipeline scores (`rescore` / `score`), it records an **input digest** in `status.json`: `sha256` over the exact `spec.md` bytes, the canonical `annotations.json` bytes, the test-result file bytes, and the tool version. Record it per requirement too (its line revision, its annotation entry and the outcomes it used) so a single edit marks only the affected lines **and their transitive dependents** as stale. Use the dependency edges stored in the status, not the deps module.
- The web view (`render` and `rollup`) recomputes the digests from the current files. On a whole-file mismatch, the page shows a prominent "Stale: inputs changed since this status was scored; run `living-spec score`" banner, and the roll-up row is `stale`, contributing **zero** proven. On a per-line mismatch, that line and every line that transitively depends on it show a gray `stale` badge with the reason; none counts as proven. The web layer no longer re-derives boundary or quantifier verdicts at all. It shows the stored ones only when the digests match, and otherwise stale.
- Remove the piecemeal re-validation code that this replaces (keep the HTML escaping and display logic). Update the status schema.
- Regressions: `editing_prerequisite_marks_dependent_stale` (render and roll-up), `boundary_receipt_without_current_link_is_stale`, `unchanged_inputs_render_stored_badges` (no false stale), `stale_rollup_counts_zero_proven`. Keep every existing web test passing, adapting expectations where they asserted re-validation behaviour that is now "stale".

## Acceptance
`make verify` and `sh living-spec/scripts/check-layers.sh`; the dogfood stays 33/33 proven (refresh it with `make living-spec-refresh` if the status format changes); the regressions pass.
