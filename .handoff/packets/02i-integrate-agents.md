# Packet 02i: resolve the linker/auditor merge (packet 02b) into the integrated tree

Your base branch is a merge commit that still contains **conflict markers** (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`) in `living-spec/src/cli/mod.rs`, `src/core/spec.rs`, `src/core/test_identity.rs`, `src/lib.rs` and `src/main.rs`. One side ("HEAD") is the integrated tree with packets 01, 03 (runner and core corrections R1/R2/R3/R5), 05, 06 and 07. The other side is packet 02b (test index, linker, auditor, `link`/`audit` CLI), which made its own additive identity and revision helpers. Read `.handoff/packets/00-architecture.md` first (binding).

## Do
1. Remove every conflict marker. For `src/core/` keep packet 03's R1/R2/R3 implementation as the single source of truth. Port packet 02b's code to call it, and delete 02b's duplicate helpers (revision, fingerprint, rename hint, scoped id) wherever the core already has an equivalent. Keep any additive field 02b needs (e.g. `raw_text`) only if the core lacks it.
2. CLI: keep the central `cli::Exit` type (codes: 1 invalid input, 2 broken, 3 boundary coverage, 4 dependency cycles, 64 usage). Add `link` and `audit`. `audit` exits 1 *only because a link was challenged*; give that its own `Exit` variant mapped to 1 and keep invalid input distinct in the message. Register every subcommand: check, rescore, boundary, deps, quantifier, link, audit.
3. Test-id scoping must agree between the test index (packet 02b) and the libtest/JUnit adapters (packet 03i): a test the index finds and the same test in `cargo test` output must yield the same `test_id`. Add `index_and_adapter_ids_agree`, which indexes this crate's own `tests/` directory and checks the ids against real-shaped libtest output for one of those tests.
4. The rescorer re-audits a `Changed` link automatically when sources are available (`--fingerprints-from <source-root>` / `--tests`), using the auditor. Add `changed_test_is_reaudited`.

## Out of scope
New features and the web view.

## Acceptance
```sh
cd living-spec && ! grep -rnE '^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)' src tests
cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo test --offline cheating_case_is_challenged index_and_adapter_ids_agree changed_test_is_reaudited
cargo test --offline boundary_reference_set deps_reference_set quantifier_specs_count simulated_push_flips_line_red
cargo run --offline -q -- audit tests/fixtures/agents/seeded/living-spec --tests tests/fixtures/agents/seeded ; test $? -eq 1
```
(Run multi-name test filters as separate `cargo test` calls if needed.)
