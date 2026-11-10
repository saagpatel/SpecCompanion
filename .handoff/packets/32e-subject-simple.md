# Packet 32e: finish subject resolution with a simpler, fail-closed rule

Read `.handoff/packets/30-mutation-architecture.md`, `32b-mutation-subject.md`, `32d-subject-fix.md`. Two earlier attempts stopped because full `mod` reachability and the TOML reader looked too big. **Scope cut: do the simple version below and finish it; do not report partial.** The current resolver rejects any package with `mod x;` file modules, which makes the dogfood crate (`living-spec/`) unsupported. That is the one thing to fix.

## Rules (replace the old external-module rejection)
1. Candidate files = every `*.rs` file under the package's `src/` (recursive). No `mod` reachability walk. Skip files whose path ends in `/tests.rs`, `/test.rs`, or whose parent module is declared `#[cfg(test)] mod x;` in a sibling `mod.rs`/`lib.rs`/`<parent>.rs` (scan for the literal attribute line followed by `mod <name>;`). A `#[path = ...]` attribute anywhere in `src/` => `unsupported: path attribute module`.
2. Inside a candidate file, skip items under `#[cfg(test)]` (inline modules and fns) and any item with another `cfg`/`cfg_attr` (fail closed). Free `fn` items only, at module level or inside inline `mod` blocks (not inside `impl`, `trait`, fn bodies or macro bodies).
3. A called name is a subject only if exactly one such free `fn` exists across all candidate files. If two files define the same name, or the package contains `macro_rules!` whose body contains `fn`, the name is ambiguous (not a subject).
4. Function extent: keep the improved scanner from 32d; uncertain extents are skipped.
5. Manifest check, conservative string rule: read `Cargo.toml` text; if it contains `[workspace]`, `workspace = true`, `git =`, or any `path =` whose value escapes the crate dir (resolve and compare), return `unsupported: dependencies outside crate`. No TOML crate; document the rule.
6. Tests: `external_module_function_resolves` (sample crate gets `mod a;` file and `b/mod.rs`), `cfg_test_module_file_excluded`, `path_attribute_module_unsupported`, `duplicate_name_across_files_ambiguous`, `macro_with_fn_makes_names_ambiguous`, `workspace_inherit_unsupported`, and `dogfood_crate_functions_resolve`: run the resolver on the real `living-spec/` crate for a link whose test calls a plain free function (pick one from `living-spec/tests/`, e.g. a test calling a function in `src/core/`) and assert at least one subject resolves with a non-empty digest.

## Acceptance (from `living-spec/`)
`cargo fmt -- --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `sh scripts/check-layers.sh`,
`CARGO_TERM_COLOR=never NO_COLOR=1 cargo test --offline --locked`, then `make verify` from the repo root (alone). Status must be `done` only if the dogfood test passes.
