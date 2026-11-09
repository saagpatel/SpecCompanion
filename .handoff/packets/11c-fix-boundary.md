# Packet 11c: fix review findings for the boundary layer

Read `.handoff/packets/00-architecture.md` (binding) and the review at `.handoff/reviews/layer-4-boundary.md`. Fix **every** finding there, major and minor, at the root. For each one, add a regression test that encodes the reviewer's failing input and is named after the finding. Keep all existing tests passing and `sh living-spec/scripts/check-layers.sh` green. Distinguish inferred from authored bounds (e.g. `origin: inferred|authored` plus the requirement `revision` on inferred bounds). The provider must resolve bounds from the current spec text, the same way the CLI does.

Where a heuristic cannot decide (e.g. the auditor meets syntax it cannot classify), the conservative answer is always "challenge / unproven / unresolved / input error", never acceptance. Change only files in your layer, its tests and fixtures, and the contracts named in the review. If a fix truly needs a change elsewhere, keep it minimal and say so in the report.

## Acceptance
```sh
make verify
```
plus every new regression passing, named in the report.
