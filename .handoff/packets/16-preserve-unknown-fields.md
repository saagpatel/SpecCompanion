# Packet 16: the annotation layer never drops fields it does not understand

Review finding (`src/core/annotations.rs` around line 128): `Dependency` has no flattened extra map. The schema allows additional dependency fields, but deserialization discards them, so loading annotations and then saving an unrelated link edit silently deletes dependency metadata (for example review receipts).

Fix the class: every struct serialized into `annotations.json` (requirement annotations, `Link`, `Audit`, audit override, `Dependency`, and the typed bound and quantifier views where they round-trip through save) either preserves unknown fields with `#[serde(flatten)] extra: BTreeMap<String, Value>`, or its schema `$def` sets `additionalProperties: false` and loading rejects unknown fields with an input error. Prefer preservation. Make the schema and the code agree for every `$def`.

Regressions: `dependency_extension_fields_survive_link_edit`, plus one generic test that adds an `x_extension` field at every object level of a fixture annotations file, runs a link write and a boundary gen write, and asserts each field is still present after both saves.

## Acceptance
`make verify` and both regressions.
