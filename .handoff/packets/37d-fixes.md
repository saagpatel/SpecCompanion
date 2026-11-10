# Packet 37d: relative path normalization in the mutation CLI (PR 146)

Base = PR 146 branch. Run cargo one job at a time; skip `make verify`.

Finding: from a crate root, `--tests tests` makes `identity_root()` return an empty path, and the downstream `canonicalize()` fails. With `--crate .` every link is skipped; without it existing evidence is replaced with `unsupported` records; mutation scoring marks checked records stale.

Fix in one place, not per call site: add a single helper used by `mutate`, `signoff` and the `--mutation` scoring input that maps every user-supplied filesystem root/path (`--tests`, `--crate`, the spec dir) to a canonical absolute path, mapping an empty path to `.` first, while keeping test identities (relative `tests/x.rs` ids) unchanged. Never let an empty path reach `canonicalize()` or `read_dir`. Failures to canonicalize must be an invalid-input error, never a silent skip or an `unsupported` record replacing evidence.

Regressions (subprocess, current directory = the crate root): `relative_tests_dir_from_crate_root_mutate_with_crate_dot`, `relative_tests_dir_without_crate_flag_keeps_existing_evidence`, `relative_tests_dir_then_score_keeps_checked`, `uncanonicalizable_root_is_error_not_unsupported_record`.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`. Status `done` only when all pass.
