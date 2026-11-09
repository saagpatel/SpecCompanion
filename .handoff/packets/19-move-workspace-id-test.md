# Packet 19: move one cross-layer test out of tests/runner.rs

`tests/runner.rs` belongs to the runner layer, which may import only `core`, `adapters` and `runner` (see `living-spec/scripts/check-layers.sh` and the layer order in `.handoff/packets/09-layering.md`). The test `workspace_crates_with_same_test_name_get_distinct_ids` also uses `living_spec::agents::test_index`. Move that one test function, unchanged, into a new file `tests/runner_ids.rs`, together with any helpers it needs. Duplicate small helpers rather than sharing them across files. Leave every other test in `tests/runner.rs` exactly as it is. No behaviour changes.

## Acceptance
```sh
make verify
cd living-spec && ! grep -n "agents" tests/runner.rs && cargo test --offline --test runner_ids
```
