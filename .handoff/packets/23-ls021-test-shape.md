# Packet 23: bring one dogfood test inside the auditor's allowlist

After packets 21 (allowlist auditor) and 22b merged, the real auditor challenges one dogfood link:

```
LS-021 -> libtest:cli:tests/cli.rs:include_adds_a_root_and_registry_must_be_explicit: challenged
  unsupported test shape: binding (requires one unannotated identifier and a supported expression)
```

Rewrite only that test in `living-spec/tests/cli.rs` into shapes the auditor supports (see `.handoff/packets/21-allowlist-auditor.md` and `src/agents/auditor.rs`). It must still assert everything it asserts today: never weaken it, never drop an assertion. Do not change the auditor. Then run `make living-spec-refresh` so the dogfood annotations, status and page match.

## Acceptance
```sh
make verify
cd living-spec && ./target/debug/living-spec audit living-spec --tests . ; test $? -eq 0
```
The dogfood status shows 33 proven.
