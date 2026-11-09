# Living Spec stress test

Repository inspection only; no implementation or runtime acceptance claimed. Root `CLAUDE.md` is absent.

## 1. Standalone crate — mitigated by versioned evidence parity

The location is fine; importing the desktop crate would unnecessarily couple Tauri/SQLite. Mirror `Requirement`'s content fingerprint and source span (`src-tauri/src/models/spec.rs:14`). Existing `parse_spec` derives identity from spec, section, normalized text and duplicate ordinal (`src-tauri/src/services/spec_parser.rs:63-86`); authored IDs instead survive edits. Namespace IDs by spec, retain immutable content revisions, and invalidate links/audit/results after semantic edits. Preserve punctuation/operators in revision hashes: existing `normalize_identity_text` strips them (`spec_parser.rs:152`). Ignore fenced examples, mirroring Markdown-aware `parse_spec` and `validate_fences` (`spec_parser.rs:23-25,108`).

Port regression cases from `analyze_assertions`/`is_placeholder_assertion` (`src-tauri/src/services/evidence.rs:286,355-414`): literal `expect`, self-comparisons, `assert True`, numeric-only assertions. Reuse exact trace and implementation-reference rules (`evidence.rs:319,339`), not semantic inference. These heuristics have false positives/negatives and lack Rust assertion-macro support; an auditor cannot establish arbitrary assertion relevance. Unsupported syntax stays unproven.

Mirror `classify_requirement`'s provenance, missing-runtime, non-probative, and insufficient-enforcement gates (`src-tauri/src/services/alignment.rs:228,338-435,466`). `proven` must satisfy VERIFIED gates; PARTIAL/UNKNOWN become reasoned unproven, FAILED becomes broken. Unresolved-link broken is an additional structural diagnostic; qualified has no existing equivalent. Existing repository links are file-level (`src-tauri/src/services/repository_tests.rs:64-70`), not reusable function identities.

## 2. Badges — scope cut needed

Contract rules 1–4 (`.handoff/packets/00-architecture.md:69-74`) give an accepted, resolved, skipped test with no bounds `proven`: neither failure nor boundary-gap applies. Require nonempty, current, audited passing evidence; missing/skipped outcomes are unproven. Separate challenged-link diagnostics from admissible execution evidence: all-challenged failures currently disappear behind rule 1; mixed challenged/accepted links have undefined participation.

A passing A depending on unproven B also remains green (rule 5). Qualify every non-proven prerequisite; reject cycles/dangling references before scoring, and use deterministic shortest paths. Cover these, diamonds and path ties in the 20 graphs. Preserve actual failures separately. Hooks should reject any current broken line, including first-run red, and report malformed/incomplete input distinctly.

## 3. Test identity — mitigated by scoped IDs and execution binding

Exact-ID-first accepts edited bodies without checking fingerprints. Duplicate bodies make fallback ambiguous; parameterized cases share source; libtest names collide across packages, binaries and integration targets. Collapsing whitespace changes literals (`"a  b"` versus `"a b"`) and indentation semantics; hashing the whole function also prevents rename matching when its name changes.

Minimum: framework/package/target/relative-file/qualified-name/case-key identity; validate every exact match against current content. Use a separate rename hint excluding the declaration name, preserving literal bytes; ambiguous hints require relinking. Bind raw source digest, requirement revision, commit and complete per-case outcomes to each run. Never collapse parameter cases into one pass or treat process success as case success.

## 4. Bounds — scope cut needed

Support documented literal forms: “age is at least 18 years,” “size is less than 10 MB,” “count is between 1 and 5 inclusive,” and explicit enumerations. Require subject, comparator, inclusivity, type, unit and domain step; integers use ±1, decimals an annotated step, enums invalid-member cases. Generated inputs alone are not executed coverage.

The 30 fixtures should include rejection/annotation cases: “fast,” percentiles, “normally,” negation, relational bounds, disjunctions, dates, locale numbers, ambiguous “between,” and compound units. Never silently ignore them. Normalize only declared compatible units with explicit scales (MB ≠ MiB). An implied test unit is unknown, not contradictory: require authored measurement metadata or a typed adapter receipt before comparing dimensions/scales.

## 5. Quantifier execution — mitigated by a closed interpreter

Keep English in Markdown; annotate a versioned JSON predicate AST: typed literals/fields, comparisons, membership, bounded boolean composition. No eval, shell, imports or plugins. For 30 synthetic specs, allowlist compiled pure fixture subjects (e.g. clamp), independently authored expected predicates and deliberately faulty variants; record input/output observations. Checking declared domain membership alone tests the annotation, not behavior.

Cap enumeration and AST depth/work; reject unsupported expressions and invalid/empty domains explicitly. Finite-universal requires complete enumeration; existential records a witness; infinite-universal uses fixed samples/seed and reports counts only. Export witnessed failing inputs/sub-domains without generalizing. Apply the contract's forbidden-language check to rendered user text too; retain originals separately.

## 6. Integration — mitigated by explicit gates

`package.json:38` guards allow the directory. Ignore `target/` (`scripts/git/guard-generated.sh:5`); respect 25 paths/commit and 2 MiB/file (`guard-atomic.sh:15`, `guard-large-files.sh:5`). Husky runs lint-staged/Prettier and guards (`.husky/pre-commit:18-23`); commitlint requires conventional headers ≤72 characters (`commitlint.config.mjs:5-8`). Local secret checks require gitleaks.

Keep both Rust locks; pnpm's sole-JS-lock rule does not prohibit them. Explain changes under `.github/PULL_REQUEST_TEMPLATE.md:36`; external reusable lockfile policy is uninspected. Existing macOS CI checks only `src-tauri` and pushes only main (`.github/workflows/backend-quality.yml:3-34`). Add explicit push triggers, manifest/cache paths, Rust 1.95, fmt/clippy/tests with locked offline resolution. Supply complete dependency sources through a prewarmed CI cache artifact or vendoring; a fresh runner cannot assume this Mac's cache. Online fetching would require an offline-contract exception.

## 7. Top five risks, ranked — scope cut needed

1. False green: replace fallback success with explicit evidence gates (§2).
2. Stale identity: version requirements and bind executed bytes (§§1,3).
3. Auditor overclaim: restrict supported assertions; unknown stays unproven (§1).
4. Invented semantics: require annotations and closed synthetic evaluation (§§4–5).
5. Dependency laundering: validate graphs and propagate uncertainty (§2).

Open questions: approve contract corrections before implementation; external lockfile-policy requirements remain unknown. No portfolio registry was accessed; use a fixture snapshot here, not live portfolio acceptance.
