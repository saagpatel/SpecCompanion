# Packet 26b: allow plain #[cfg(...)] on tests

Packet 26 now challenges any attribute other than `#[test]`, `#[ignore]` and plain `#[should_panic]`. That is too strict for `#[cfg(<predicate>)]`, which only decides whether a test is compiled at all. A test that isn't compiled produces no outcome, so its link is already `unproven not_executed` and can never be a false green. As a result, the dogfood link LS-018 (`tests/pipeline.rs: pre_push_hook_blocks_a_broken_scored_line`, marked `#[cfg(unix)]`) is challenged and `living-spec audit living-spec --tests .` exits 1.

Allow plain `#[cfg(...)]` with any predicate on a test, in `src/agents/test_index.rs` and/or the auditor. Keep `#[cfg_attr(...)]` and every other attribute challenged, exactly as packet 26 made them. Regressions in `tests/agents_review.rs`: `cfg_gate_is_allowed` (an otherwise genuine test with `#[cfg(unix)]` is accepted) and `cfg_attr_should_panic_is_challenged` (must still pass). Then `living-spec audit living-spec --tests . --write` and `make living-spec-refresh` if any stored audit changes.

## Acceptance
`make verify`; `cd living-spec && ./target/debug/living-spec audit living-spec --tests .` exits 0 with 33 accepted.
