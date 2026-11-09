# Packet 03i: make the merged tree compile and pass (runner + boundary + deps)

Your base branch merges packet 03 (runner hook and core corrections R1/R2/R3/R5) into a tree that already had packets 05 (Boundary Cases) and 06 (Dependency Proofs). The text conflicts are resolved, but the tree does not compile. Read `.handoff/packets/00-architecture.md` first. It is binding, especially the "Contract revisions" section.

## Fix, in this order
1. **One results-adapter layer.** `src/runner/adapters.rs` (packet 03) is the only place that parses libtest and JUnit output. Delete any duplicate parser in `src/boundary/` and make the boundary code call the runner adapters. Use one quick-xml version: the newest `0.42.x` in the offline cache that compiles. Port any deprecated or removed API calls.
2. **Core type changes from packet 03.** Update packet 05/06 code and tests to match the new `TestOutcome` (e.g. `failure_message`), `Link` (`revision`, `rename_hint`) and `Audit` (`revision`) fields and the new badge rules. Update the fixtures in `tests/fixtures/boundary` and `tests/fixtures/deps` (don't delete them) so the reference sets still report 30/30 and 20/20.
3. **Provider wiring.** Plug packet 05's boundary-gap provider and packet 06's dependency propagation into packet 03's `BoundaryGapProvider` / `QualifiedProvider` traits (or equivalent), so `rescore` applies boundary gaps and qualified status. Add a test, `rescore_applies_boundary_and_dependency_rules`, that runs one boundary-gap line and one qualified line through `rescore`.
4. **Exit codes.** Keep these distinct: `rescore` returns 2 on any broken line, `boundary coverage` returns 3, `deps check` returns 4, and usage errors have their own code. Centralize them in one `cli::Exit` error type that `main.rs` maps to codes.
5. **libtest scoping.** The libtest adapter must track `Running <target> (...)` headers and `Doc-tests` sections, so that `test_id` scoping (R3) can tell integration-test binaries apart. Add a test using real-shaped `cargo test` output with two test binaries that both contain `fn same_name`.

## Out of scope
New features, the packet 02 or 07 areas, and the web view.

## Acceptance
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo test --offline boundary_reference_set -- --nocapture   # 30/30
cargo test --offline deps_reference_set -- --nocapture       # 20/20
cargo test --offline rescore_applies_boundary_and_dependency_rules
cargo test --offline simulated_push_flips_line_red
```
