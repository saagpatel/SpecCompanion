# Packet 13: core fixes from review round 3 of part 1

Read `.handoff/packets/00-architecture.md` (binding). Fix each item at the root and add the named regression. Keep `make verify` green.

1. `src/core/boundary/gen.rs` (around line 82): integer-step validation compares the normalized delta against one *base* unit instead of one *declared* unit. An integer bound of 10 KB with a 1 KB step is rejected, but a 0.001 KB step is accepted. Validate integer steps in the declared unit, accounting for conversion factors. Regressions: `integer_step_in_declared_unit_kb_accepted`, `fractional_step_in_declared_unit_rejected` (both should also cover ms/s).
2. `src/core/annotations.rs`: loading never checks the `schema` discriminator, so `living-spec.annotations/v2` is accepted. Reject any value other than `living-spec.annotations/v1` with an input error. Regressions: `unsupported_annotation_schema_rejected` (library) and a check-command test that it exits 1 with an invalid-input message.
3. `src/core/badge.rs` (around line 112): renamed links produce `TestEvidence` with `admissible: true`, while scoring excludes them until re-audit. Use one admissibility predicate for both evidence and scoring. Extend `renamed_link_requires_reaudit` to assert the evidence is marked inadmissible.
