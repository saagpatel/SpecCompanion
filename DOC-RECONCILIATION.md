# Documentation Reconciliation

This reconciliation reflects the executable-evidence engine plus the real-repository dogfood slice. Code, schema migrations, fixtures, command boundaries, and UI types were read as the authority.

## Current product contract

The product is an evidence-backed executable-requirements tool. It does not equate generated tests, test-process success, symbol-name similarity, or model output with verified requirement coverage.

The supported high-confidence evidence slice is JavaScript/TypeScript and Python. The older README claim of broad semantic coverage across Rust, Go, Java, Ruby, and C# described a lightweight symbol extractor, not a trustworthy alignment engine, and has been removed. Unsupported implementation languages are surfaced as `UNKNOWN`.

## Pipeline truth

1. Markdown is parsed with `pulldown-cmark` into stable requirements with source lines and content fingerprints.
2. Duplicate requirements receive distinct deterministic identities. Editing requirement meaning changes the identity and invalidates attached evidence.
3. Source scanning is deterministic, path-contained, symlink-aware, size-bounded, and limited to JavaScript/TypeScript and Python evidence.
4. Candidate implementation matches require explicit identifier-token overlap. The scanner does not claim arbitrary semantic equivalence between prose and code.
5. Generated tests carry a stable requirement marker. A contained existing repository test can instead be explicitly linked by the user; that link is stored as a user decision and is never presented as inferred semantic equivalence.
6. Offline templates intentionally remain scaffolds with placeholder assertions. Assertion analysis rejects tautologies and missing assertions before execution results can contribute to verification.
7. Test execution supports contained Jest, Vitest, PyTest, and stdlib `unittest` files using direct command arguments, canonical paths, explicit working directories, local/allowlisted runtimes, timeouts, process-tree termination, and output caps. Stored assertion evidence is refreshed from the exact stable file bytes executed; oversized or during-run changes are blocked.
8. Alignment assigns `VERIFIED`, `PARTIAL`, `FAILED`, or `UNKNOWN` and stores exact requirement, implementation, test, assertion, execution, and diagnostic evidence.
9. Reports persist deterministic evidence ordering and a digest. JSON, CSV, and HTML exports preserve the evidence taxonomy.
10. The UI explains checked languages, unsupported or skipped evidence, explicit repository links, failure states, and why a passing placeholder remains unknown.

## Verification authority

`.codex/verify.commands` remains the repository-owned full gate. Focused Rust fixtures additionally cover assertion quality, existing-test discovery/linking, Jest/Vitest/PyTest/unittest execution, missing implementation/test evidence, failures, timeouts, stable identities, path containment, command-like filenames, unsupported language/runtime behavior, partial evidence, and deterministic output. An ignored opt-in dogfood test runs the full link → execute → report path against disposable copies of a real Vitest TypeScript repository and a real stdlib-unittest Python repository. Playwright covers browser-preview workflows, keyboard focus restoration, evidence expansion, responsive layouts, and serious/critical accessibility violations.

## Remaining limits

- Claude-assisted generation is optional and is judged by the same deterministic evidence rules; no API key is required for the offline workflow.
- A candidate symbol match is evidence of a possible implementation location, not proof of semantic equivalence.
- Existing external tests remain unlinked until the user explicitly associates a contained path with a requirement. That association replaces only the trace marker; implementation matching, assertion quality, and execution proof remain mandatory.
- Windows uses child-process termination fallback; Unix process groups provide full descendant termination in the current verified slice.
