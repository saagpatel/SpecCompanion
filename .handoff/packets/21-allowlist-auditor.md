# Packet 21: allowlist auditor, plus Cargo binary targets in the index

Read `.handoff/packets/00-architecture.md` (binding). The review of part 3 (PR #131) found four more ways past the auditor, on top of the earlier rounds:
- `stringify!(assert_eq!(export_timestamp(30), 30));` is accepted (assertions scanned inside arbitrary macro token trees).
- `let (export_timestamp, _) = (true, ()); assert!(export_timestamp);` and Python `export_timestamp: bool = True` are accepted (unsupported binding forms are silently ignored, so constants with requirement-shaped names count as evidence).
- Python `assert (export_timestamp(30), False)` and JS `expect([export_timestamp(30)]).toBeTruthy()` are accepted (always-truthy containers in truthiness assertions).
- Index: `src/bin/tool.rs` with `#[test] fn smoke()` is indexed as `libtest:sample:src/lib.rs:bin::tool::smoke`, while the adapter says `libtest:tool:src/bin/tool.rs:smoke`.

## Change the auditor's design from denylist to allowlist
The auditor **accepts a link only when every statement in the test body is a supported shape, and at least one supported assertion connects a value computed by calling code under test to an independent expected value.** Everything else is challenged with the reason `unsupported test shape: <what>`. Supported shapes, per language, are kept deliberately small:
- **Rust:** simple `let name = <expr>;` / `let mut name = <expr>;` (single identifier pattern), expression statements that are calls, and `assert!(<cmp>)`, `assert_eq!(a, b)`, `assert_ne!(a, b)`, `assert!(x.is_ok()/is_err()/is_some()/is_none())`, and `assert!(matches!(x, <refutable pattern with a literal, enum path or range>))`. `#[should_panic]` only when the body is exactly one call into code under test, optionally preceded by simple lets. Any other macro invocation (including `stringify!`, `format!` used as a statement, `dbg!`) is challenged.
- **Python:** simple `name = <expr>` (single target, no annotation), call statements, `assert <comparison>` where neither side is a literal container or tuple, and `with pytest.raises(E):` whose suite is exactly one call. Decorators other than `pytest.mark.parametrize` are challenged.
- **JS/TS:** `const/let name = <expr>`, call statements, and `expect(<expr>).<matcher>(<expected>)` for `toBe`, `toEqual`, `toStrictEqual`, `toThrow`, `toBeGreaterThan(OrEqual)`, `toBeLessThan(OrEqual)`, where `<expr>` is not an array or object literal. `toBeTruthy`/`toBeFalsy` only on a direct call result or a bound identifier.
- **Every language:** an asserted value is "computed" only if it comes from a call (directly or through simple lets) whose callee is not a literal constructor, and it is compared against something not derived from that same value. Tautologies (`x == x`) are challenged.

Keep all existing auditor regressions (every earlier cheating case must stay challenged) and add one per bullet above, named after the bypass. Genuine fixtures and the dogfood must still be accepted. If a dogfood test falls outside the allowlist, rewrite the *test* into a supported shape that still asserts the same behaviour (never weaken what it checks), or rewrite the spec line to match what is genuinely tested. The real auditor must accept all dogfood links (`living-spec audit living-spec --tests .` exits 0) and `make verify` must pass.

## Index: Cargo binary targets
Resolve standard binary targets (`src/main.rs` → package-named bin, `src/bin/<name>.rs`, `src/bin/<name>/main.rs` and its modules), and honour explicit `[[bin]]`, `[lib]` and `[[test]]` `name`/`path` in Cargo.toml where present. Unknown layouts produce no id rather than a guessed one. Regression: `index_and_adapter_ids_agree_for_bin_targets`.

## Acceptance
`make verify`, `cd living-spec && ./target/debug/living-spec audit living-spec --tests .` exits 0, and the new regressions pass.
