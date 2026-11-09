# Packet 08b: no hand-set audit verdicts. The dogfood must pass the real auditor.

Read `.handoff/packets/00-architecture.md` first (binding). Your base is packet 08. It ships a dogfood spec (`living-spec/living-spec/`) that reports 33/33 proven. But every link has `proposed_by: human` with a hand-written `audit.verdict: accepted`, and the real auditor disagrees:

```
cd living-spec && ./target/debug/living-spec audit living-spec --tests .   # exits 1: 32 of 33 links challenged
```

Most challenges say "Assertion syntax is unsupported; relevance cannot be analysed conservatively". Others say "Assertions check unrelated values". A green badge that rests on a verdict the auditor never gave is the false green this project exists to prevent. Fix it at the root.

## Do
1. **The auditor understands ordinary Rust tests.** Support multi-line `assert!`, `assert_eq!`, `assert_ne!`, `assert!(matches!(…))`, `assert!(x.is_err())`/`is_ok()`/`is_some()`/`is_none()`/`contains(…)`, `.unwrap_err()` followed by assertions on it, `#[should_panic]`, and comparisons of values bound by `let` earlier in the same test (follow simple `let` bindings to see which identifiers an asserted expression derives from). Relevance is satisfied when an asserted expression, or a `let` binding it derives from, calls a function or method, or mentions an identifier or string literal, that shares a normalized term with the requirement. Keep the cheating cases challenged: `cheating_case_is_challenged` and the placeholder regressions must still pass unchanged. Add Rust-shaped regressions for each supported form, one genuine and one cheating each.
2. **Scoring never trusts a stored verdict it can recompute.** When sources are available (`--tests`), `score` and `rescore` re-audit **every** link and use the fresh verdict. A human may override a challenge only through an explicit `audit.override = { "by": "human", "reason": "<non-empty>" }` field. An override counts as accepted, but the badge reasons record `human_override`, the spec page shows it visibly (e.g. "accepted by human override: <reason>"), and `status.json` counts overrides separately. Without sources, stored verdicts are used and the status says `audit: stored`.
3. **The dogfood is honest.** Regenerate the dogfood `annotations.json` by running `living-spec link living-spec --tests . --write` and then the auditor. Where a link is still challenged, either improve the test so it genuinely asserts on the requirement (tests are legitimately part of this crate) or rewrite the requirement line so it says what the test actually proves. Do not use overrides in the dogfood. Use `proposed_by: linker` for links the linker proposed, and `human` only where you chose a different test than the linker, with the explanation saying why. **Every line must still have an admissible link**, and the final counts are whatever the real pipeline says. Report the counts honestly, even if some lines end up unproven.
4. **Regression:** add `dogfood_verdicts_match_the_auditor` (in the pipeline tests). It re-runs the auditor over every dogfood link and asserts each stored verdict equals the fresh one, and that no dogfood link uses an override.
5. Update `README.md` (Living Spec section) to describe the override rule.

## Acceptance
```sh
make verify
cd living-spec && ./target/debug/living-spec audit living-spec --tests . ; test $? -eq 0
cargo test --offline cheating_case_is_challenged
cargo test --offline dogfood_verdicts_match_the_auditor
```
