# Packet 11a: fix review findings for the runner layer

Read `.handoff/packets/00-architecture.md` (binding) and the review at `.handoff/reviews/layer-2-runner.md`. Fix **every** finding there, major and minor, at the root. For each one, add a regression test that encodes the reviewer's failing input and is named after the finding. Keep all existing tests passing and `sh living-spec/scripts/check-layers.sh` green. For finding 2 (the hook can't call `rescore` at its layer): the hook will move to the pipeline layer in the PR split, so instead add a test that runs `living-spec/hooks/pre-push` against the built binary for passing, first-run broken and malformed results (this test belongs in `tests/pipeline.rs`).

Where a heuristic cannot decide (e.g. the auditor meets syntax it cannot classify), the conservative answer is always "challenge / unproven / unresolved / input error", never acceptance. Change only files in your layer, its tests and fixtures, and the contracts named in the review. If a fix truly needs a change elsewhere, keep it minimal and say so in the report.

## Acceptance
```sh
make verify
```
plus every new regression passing, named in the report.
