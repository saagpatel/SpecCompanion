# Packet 11f: fix review findings for the web layer

Read `.handoff/packets/00-architecture.md` (binding) and the review at `.handoff/reviews/layer-7-web.md`. Fix **every** finding there, major and minor, at the root. For each one, add a regression test that encodes the reviewer's failing input and is named after the finding. Keep all existing tests passing and `sh living-spec/scripts/check-layers.sh` green. Wherever a decision can't be made, the conservative answer is "unproven / input error", never acceptance. Change only the web layer, its tests, fixtures and contract, unless that's truly impossible; if so, say so in the report.

## Acceptance
`make verify`, plus every new regression passing, named in the report.
