# Packet 02b: finish the linker and auditor (continuation of packet 02)

Your base branch already contains a partial packet 02 (Python test index, preliminary linker, identity helpers in `src/core/test_identity.rs`, `tests/agents_partial.rs`). Finish **all** of `.handoff/packets/02-linker-auditor.md`, including its "Contract revisions that apply" section, and read `.handoff/packets/00-architecture.md` (binding). Do not stop at partial: the packet is done only when every acceptance command passes. Work through it fully, in order: index (Rust, Python, JS/TS) → linker → auditor → CLI `link`/`audit` → seeded fixture with both cheating cases → required tests.

Answers to the open questions from the first attempt:
- Python target scope: use the first path component under the scanned root that contains the test file (e.g. `tests`), or `root` for files at the root. Rust: the integration-test file stem for `tests/*.rs`, or `lib` for unit tests in `src/`. JS/TS: the first path component.
- Linker matching: restrict scoring to the test's name, the identifiers it calls, and its string and numeric literals, not the raw source or the full test id.
- R2 revision: compute it from the requirement text as written after the `- [ID] ` prefix, trimming only trailing whitespace. If the core parser trims more, add a `raw_text` field to `RequirementLine` (additive) and use that.
- `audit` must exit 1 *because a link was challenged*. A usage error must use a different exit code (64) so the acceptance check can tell them apart.

Acceptance: as in packet 02, plus `cargo run --offline -q -- audit …; test $? -eq 1` must come from a challenge (print the challenged links).
