# Packet 09: make the module graph acyclic so the crate can land as layered PRs

Read `.handoff/packets/00-architecture.md` first (binding). Behaviour must not change. This is a pure restructuring: every existing test keeps passing, with the same names and assertions. You may move a test to another file.

## Why
The code has to land as a series of pull requests, each under 200 KB of diff, and each must compile and pass its own tests on top of the previous one. Today `runner` imports `boundary`, `deps` and `agents`, and `boundary` and `deps` import `runner`, so none of these can land on its own.

## Target layering (a module may import only modules listed before it)
1. `core` (spec, annotations, badge, identity, history, plus the provider **traits** `BoundaryGapProvider`, `QualifiedProvider`, `Reauditor`, and the shared `TestOutcome`)
2. `adapters` (libtest and JUnit parsing; move it out of `runner` into its own top-level module `src/adapters/`)
3. `agents` (imports core and adapters only)
4. `runner` (rescore, history writing and hook logic; core and adapters only, and it takes the providers as trait objects)
5. `boundary` (core and adapters; implements `BoundaryGapProvider`)
6. `deps` (core only; implements `QualifiedProvider`)
7. `quantifier` (core only)
8. `web` (core; it may read boundary/deps/quantifier data only through serialized `status.json` fields or through core types, not by importing those modules. If the spec page needs boundary or quantifier panels, those modules put plain view-model structs in `core` or in the status, and `web` renders them)
9. `pipeline` (the only module that wires everything together: the `score` command, the providers passed to `rescore`, and re-auditing through agents)
10. `cli` (one file per subcommand. `cli/mod.rs` holds the dispatch and `Exit`, and each subcommand's dispatch line sits alone on one line, in layer order)

Tests: integration-test files may import only their own layer and the layers before it. Move cross-layer tests (e.g. `rescore_applies_boundary_and_dependency_rules`, `changed_test_is_reaudited`, the dogfood and `score` tests) into `tests/pipeline.rs`. Each `tests/<layer>.rs` uses fixtures under `tests/fixtures/<layer>/` only.

Add `scripts/check-layers.sh` (POSIX sh, no deps) that greps `use crate::` and `crate::` paths in each `src/<module>` and fails if any import breaks the order above. Run it from `make verify` and from the CI workflow.

## Acceptance
```sh
sh living-spec/scripts/check-layers.sh
make verify
cd living-spec && cargo test --offline 2>&1 | grep -c '^test .* ok$'   # count must be >= the count before this packet; print both numbers in the report
```
