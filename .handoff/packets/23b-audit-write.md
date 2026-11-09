# Packet 23b: `living-spec audit --write` re-binds existing links; use it to repair the dogfood

Your base contains packet 23 (`tests/cli.rs` test rewritten into an allowlisted shape). Two problems remain:
1. **Product gap:** after a spec line or a test changes, nothing re-audits a project's *existing* links and saves the result. `link --write` also merges every new linker proposal, which floods lines with extra links, and the dogfood was last refreshed with a throwaway helper.
2. **Dogfood:** LS-007's stored link and audit revision no longer match its current spec text (`sha256:2f2f8bd1…` vs `sha256:53249a02…`). LS-021's stored fingerprint is stale after packet 23's test rewrite. So `dogfood_verdicts_match_the_auditor` and `make verify` fail.

## Do
- Add `--write` to `audit` (`src/cli/audit.rs`): re-resolve and re-audit **only existing links** against the current spec revision and current source. Persist each link's current `test_id` (on a rename), `fingerprint`, `rename_hint`, `revision`, and the fresh `audit` (verdict, notes, revision). Never add, remove or reorder links, and never change `proposed_by`, explanations, human overrides, or any other annotation section. Challenged links are saved as challenged. Exit codes are unchanged (1 if any link is challenged after writing). Document it in the CLI usage text and in the README's Living Spec section.
- Regressions: `audit_write_rebinds_existing_links_only` (an edited line and an edited test are re-bound, no link is added, other sections are untouched, a second run makes no changes) and `audit_write_keeps_challenged_links`.
- Repair the dogfood by running `living-spec audit living-spec --tests . --write` and then `make living-spec-refresh`. Every dogfood link must be accepted by the real auditor. If one is honestly challenged, fix the *test* (supported shape, same assertions) or the *spec line*; never use an override.

## Acceptance
```sh
make verify
cd living-spec && ./target/debug/living-spec audit living-spec --tests . ; test $? -eq 0
```
The dogfood shows 33 proven, each line with exactly the links it had before.
