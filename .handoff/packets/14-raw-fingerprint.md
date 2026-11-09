# Packet 14: fingerprints hash raw source bytes

Review finding (`src/core/test_identity.rs` around line 39): trimming each source line also removes significant whitespace inside multiline literals. Deleting spaces before a newline in a Python triple-quoted expected string changes the test's behaviour but leaves the fingerprint unchanged, so `Exact` keeps the old audit.

Fix conservatively: `fingerprint` = `sha256:` over the raw source bytes, normalizing only CRLF/CR line endings to LF. Apply the same rule to the rename hint's body portion. Update contract R3's wording in `.handoff/packets/00-architecture.md` to match ("raw bytes; only line endings normalized"). Re-audit on any change is automatic in the pipeline, so a whitespace-only edit is safe: it just triggers a re-audit.

Regressions: `trailing_space_inside_python_multiline_literal_changes_fingerprint`, `trailing_space_inside_rust_raw_string_changes_fingerprint`, `crlf_and_lf_sources_share_a_fingerprint`. Then update any fixture or dogfood fingerprints that change (re-run `make living-spec-refresh` for the dogfood, then confirm the auditor still accepts every dogfood link).

## Acceptance
`make verify`, plus the three regressions.
