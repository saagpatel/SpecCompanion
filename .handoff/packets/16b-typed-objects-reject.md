# Packet 16b: finish packet 16 (typed objects reject, everything else preserves)

Your base already contains packet 16's partial work (`.handoff/packets/16-preserve-unknown-fields.md`). Decision for the open question: **typed boundary and quantifier objects** (bounds and their kinds, ops, steps, cases, boundary links, the quantifier section and its domains, predicates and sampling) **reject** unknown fields with a clear input error that names the field and path. Their schema `$def`s set `additionalProperties: false`, and loading never drops such a field silently. **Untyped containers** (requirement annotations, links, audits, the audit override, dependencies) **preserve** unknown fields.

Adjust the generic regression: an `x_extension` placed on an untyped container survives a link write and a boundary-gen write, and one placed inside a typed object makes the load fail with an input error naming it. Add `schema_and_loader_agree_on_additional_properties`, which walks every `$def` in `contracts/living-spec-annotations-v1.schema.json` and checks its `additionalProperties` setting against a fixture loaded with an extension at that level.

## Acceptance
`make verify` passes, plus the regressions.
