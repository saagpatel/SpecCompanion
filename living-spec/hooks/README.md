# Optional pre-push hook

Build the binary with `cargo build --offline --manifest-path living-spec/Cargo.toml`.
Copy `pre-push` into your repository's existing hooks directory and make it executable,
or explicitly choose a hooks directory yourself. No tool changes `core.hooksPath`.

The hook runs `LIVING_SPEC_TEST_CMD` (default `cargo test`) from the push working
directory, captures stdout/stderr, prints it, and rescores even if tests fail.
It keeps the capture at `<spec-dir>/.results/latest.txt` and adds `/.results/`
to the spec directory's `.gitignore` so saved scores can be rendered later.
It blocks on any currently broken requirement (exit 2), malformed results (exit 1),
or a failed project test command (exit 1). Previously broken lines remain blocking.

Environment:
- `LIVING_SPEC_BIN`: executable path (default `living-spec`, on PATH).
- `LIVING_SPEC_DIR`: spec directory (default `living-spec`).
- `LIVING_SPEC_TEST_CMD`: trusted, user-authored shell command.
- `LIVING_SPEC_FORMAT`: `libtest` (default) or `junit`.
- `LIVING_SPEC_FINGERPRINTS`: current identity JSON file.
- `LIVING_SPEC_TESTS`: source root for resolution and automatic re-audit
  (default `.`, used when no fingerprint file is supplied).

For JUnit, the captured stdout/stderr must contain only XML. Redirect tool
diagnostics to a separate file inside the command so they do not mix into XML.
The libtest adapter requires complete running headers, case rows and summaries;
zero-case reports, truncated suites and duplicate identities are errors.

The hook uses the same pipeline as `score`: source resolution and re-audit,
boundary coverage, core badges, dependency propagation, quantified coverage,
history and HTML rendering. To use externally supplied current identities instead,
set `LIVING_SPEC_FINGERPRINTS`:
`{"libtest:exports:tests/exports.rs:timestamps":{"fingerprint":"sha256:...","rename_hint":"sha256:..."}}`.
String digest values support exact resolution only.
Missing digests cannot establish passing evidence; changed tests require re-audit.
Fingerprint files must be regenerated for the tested source, not copied from
annotations. Scoped IDs should include framework, target, file and case; adapters
preserve reported names by default and reject collisions rather than invent scope.
JUnit callers can supply an ID function through the library's configurable parser.

Legacy annotations without requirement/audit revisions stay unproven until reviewed.
Bounds stay unproven until their linked cases execute and pass. Dependencies
qualify otherwise proven lines when a prerequisite is not proven.

For this standalone crate, run pushes from the repository root with
`LIVING_SPEC_DIR=living-spec/living-spec`, `LIVING_SPEC_TESTS=living-spec`, and
`LIVING_SPEC_TEST_CMD='cd living-spec && cargo test --locked'`. Hook installation
is opt-in; CI scores independently on every push and pull request.

Mutation checks are opt-in. `living-spec mutate <spec-dir> --tests <source-root>` runs bounded Rust mutants for currently accepted links and writes or merges `<spec-dir>/mutation.json`. `living-spec score ... --mutation [<path>]` (or `rescore`) attaches current tiers; the default path is `<spec-dir>/mutation.json`, and a missing file is an input error. Scoring without `--mutation` does not read mutation evidence.
