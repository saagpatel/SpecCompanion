# Packet 31b: finish the linear-size dependency report

Your base has packet 31's partial work (streaming exports). The open question is answered as follows. Listing every other reachable prerequisite id per claim is quadratic, so don't do that. Use shared groups:

- Compute the condensation of the dependency graph (Tarjan SCCs, which already exist in `deps/model.rs`). For each claim, the report gives:
  - its own **minimal path** to the nearest non-proven prerequisite, as today;
  - `other_nonproven_count`;
  - `prerequisite_group`: the id of the SCC (or chain) whose non-proven members it reaches.
- A top-level `groups` section lists each group's non-proven members **once**, with badges and no paths. Store reachability between groups as edges between group ids, not as expanded member lists.
- A top-level `cycles` section emits each cycle witness **once**; claims reference cycles by index. No per-claim `reasons` payload may embed another claim's path.
- Markdown renders the same structure. Update `contracts/living-spec-dependency-report-v1.schema.json`. Rendered impact text must still state every affected claim and its minimal path, and nothing that was reported before may be hidden; only duplication is removed.
- Regression `report_size_is_linear_for_large_cycle`: a 1,000-node cycle, plus a 1,000-node chain, must each produce JSON and Markdown under 300 bytes × N. Keep `deps_reference_set` (20/20) and all report tests passing.

## Acceptance
`make verify` plus the regression.
