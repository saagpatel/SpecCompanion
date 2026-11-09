# Packet 10: fixes from the Codex review of PR part 1 (core)

Read `.handoff/packets/00-architecture.md` (binding) and `living-spec/scripts/check-layers.sh` (the layer order must hold). Fix the root cause each time and add the named regressions. Keep every existing test.

1. **Rename hint drops executable code** (`src/core/test_identity.rs`, `rename_hint`). Removing the whole first line throws away code that sits on the declaration line. `fn old() { assert_eq!(f(), 1);\n}` and `fn new() { assert_eq!(f(), 2);\n}` get the same hint, so a changed assertion inherits the old accepted audit. Fix: strip only the declaration syntax (attributes, `fn name(...)`/`def name(...):`/`it("title",` header up to the body opener) and hash everything after it, including the rest of that line. When extraction is uncertain, return no hint (rename matching is refused, so the link is unresolved). Regressions: `rename_hint_keeps_code_on_declaration_line`, `uncertain_extraction_refuses_rename`.
2. **Duplicate outcomes can hide a failure** (`src/core/badge.rs` around the outcome lookup, and the adapters). Scoring uses only the first outcome for a test id, so `Passed` then `Failed` for the same id comes out proven. Fix: the adapters reject duplicate ids within one results file with an input error, unless the duplicates are distinct parametrized cases (`#case`). Scoring is also conservative on its own: when several outcomes share an id, any failed or errored one makes the line broken, and all of them appear in the test evidence. Regressions: `duplicate_outcome_pass_then_fail_is_broken`, `duplicate_outcome_fail_then_pass_is_broken`, `adapter_rejects_duplicate_ids`.
3. **Roll-up label for `--include`** (`src/web/rollup.rs`). An included root is labeled with the path as given (e.g. `..`). Use the canonicalized directory's final component, and keep `path` as given. Regression: `include_row_uses_directory_name`.

## Acceptance
```sh
make verify
cd living-spec && cargo test --offline rename_hint_keeps_code_on_declaration_line && cargo test --offline duplicate_outcome && cargo test --offline adapter_rejects_duplicate_ids && cargo test --offline include_row_uses_directory_name
```
