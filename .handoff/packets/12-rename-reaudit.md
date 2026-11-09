# Packet 12: a renamed link is never admissible on its old audit

Read `.handoff/packets/00-architecture.md` (binding). Review finding on `src/core/test_identity.rs`: the rename hint strips semantic attributes, so renaming a test **and** adding `#[should_panic]` resolves as `Renamed`, and scoring keeps the old accepted audit. A changed test then stays proven without a re-audit.

Fix the class, not the instance:
1. In core scoring (`src/core/badge.rs` and the resolution types), a `Renamed` resolution makes the link non-admissible until it is re-audited, exactly like `Changed`: the badge is `unproven` with reason `test_changed` (or a new `test_renamed` reason; update the status schema and the web rendering of reasons). The rename is still recorded, and the link's `test_id` is still rewritten in history and annotations.
2. In the pipeline, re-auditing covers `Renamed` too, so with sources available a renamed test is re-audited automatically and becomes admissible only if the auditor accepts the new source.
3. Also keep attributes in the rename hint (normalize only the test name), so a rename that adds or removes `#[should_panic]`, `#[ignore]` or a decorator no longer matches as a rename at all.
4. Regressions: `renamed_link_requires_reaudit` (core, no sources → unproven), `rename_with_should_panic_does_not_match` (identity), `renamed_link_reaudited_in_pipeline` (pipeline, sources present → re-audited, proven only if accepted).
Update any existing test that assumed a rename keeps the old audit. Keep `check-layers.sh` green.

## Acceptance
```sh
make verify
cd living-spec && cargo test --offline renamed_link && cargo test --offline rename_with_should_panic_does_not_match
```
