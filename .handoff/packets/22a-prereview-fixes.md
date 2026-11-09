# Packet 22a: fix pre-review findings (04-boundary, 05-quantifier, 08-pipeline, 09-dogfood-ci)

Read `.handoff/packets/00-architecture.md` (binding). An unsigned Codex Sol pre-review of the stacked PR layers produced the findings below. The text of each one was cut off at about 200 characters, so start by reproducing each from its file:line and its description, then fix it at the root and add a regression test named after it. If a finding does not reproduce, say so in the report with the evidence. Do not touch `src/agents/` (another worker owns it). Keep `make verify` and `sh living-spec/scripts/check-layers.sh` green. Conservative answers win: unproven, unresolved or an input error, never a false green and never silent loss.

## Findings

### 04-boundary
- [major] living-spec/src/cli/boundary.rs:46 Both CLI loops resolve every requirement, including unrelated prose. A spec containing a valid boundary plus “The service records audit events.” fails with Need
- [major] living-spec/src/cli/boundary.rs:149 The delegated parse_junit adapter constructs classname::name identities, but BOUNDARY.md promises literal name matching and duplicate-name rejection. A linked t

### 05-quantifier
- [major] living-spec/src/quantifier/mod.rs:95 An explicitly authored "quantifier": null deserializes into Option::None and is silently skipped here, although the annotation schema requires an object. With a

### 08-pipeline
- [major] living-spec/scripts/score-ci.sh:64 score_ready becomes true whenever status.json contains the attempted commit, regardless of score_exit. The runner persists status before the CLI renders and wri

### 09-dogfood-ci
- [major] living-spec/living-spec/spec.md:12 LS-007 claims fingerprints ignore trailing whitespace, but its linked test explicitly asserts fingerprint("a  \nb") differs from fingerprint("a\nb"). The implem
- [major] Makefile:39 living-spec-refresh deletes existing history and status before scoring succeeds. Green tests do not guarantee scoring succeeds: invalid dogfood boundary or quan

## Acceptance
`make verify` plus every new regression (name them in the report).
