# Packet 15: fix review findings for the pipeline and dogfood/CI layers

Read `.handoff/packets/00-architecture.md` (binding) and the reviews `.handoff/reviews/layer-8-pipeline.md` and `.handoff/reviews/layer-9-dogfood-ci.md`. Fix **every** finding at the root, and add a regression named after each finding (for workflow findings, a test that parses `.github/workflows/living-spec.yml` and asserts the property, plus a script-level test of the CI scoring steps if you move logic into a script). Keep `make verify` and `sh living-spec/scripts/check-layers.sh` green.

For the CI artifact finding: score in a clean scratch directory (spec, annotations and committed history copied in; no inherited `status.json` or HTML). Upload score artifacts only when a fresh status for this commit was emitted, including the exit-2 broken case. Otherwise upload only the captured test output and diagnostics, under a clearly different artifact name. The job must still fail.

Conservative answers win wherever a decision is unclear.

## Acceptance
`make verify`, `actionlint .github/workflows/living-spec.yml` if available, and every new regression passing (name them in the report).
