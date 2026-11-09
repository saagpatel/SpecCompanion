## Store

- [LS-001] The auditor re-audits changed test source before it keeps a requirement link admissible.
- [LS-002] The parser ignores requirement-looking examples inside fenced code blocks.
- [LS-003] Link writes preserve human metadata, other annotation sections, and repeatable output.
- [LS-004] The core reports orphan annotations without discarding their stored data.
- [LS-005] Renaming a linked test updates its identity and records the change in history.

## Linking and auditing

- [LS-006] The source index assigns scoped test names and records source spans across supported languages.
- [LS-007] Test fingerprints preserve literal contents and trailing whitespace.
- [LS-008] The resolver leaves ambiguous rename matches unresolved.
- [LS-009] The auditor marks irrelevant and unsupported Rust assertions as challenged.
- [LS-010] The command reports challenged links separately from usage and input errors.
- [LS-011] The auditor leaves a Rust test with unrelated or placeholder assertions challenged.

## Scoring and runner

- [LS-012] The libtest adapter scopes a repeated test name to its binary and documentation test target.
- [LS-013] The JUnit adapter preserves failed and skipped testcase outcomes.
- [LS-014] The scorer requires both the link and audit revisions to match the current line.
- [LS-015] A changed test is re-audited against its current source before scoring.
- [LS-016] An unresolved link remains broken even when its audit was challenged.
- [LS-017] Malformed test output exits with an input error before writing status.
- [LS-018] The opt-in pre-push hook blocks a push when scoring finds a broken line.

## Web and portfolio

- [LS-019] The linker proposes a resolvable session expiry test with a non-empty explanation.
- [LS-020] The web page escapes markup found in requirement text.
- [LS-021] The roll-up requires an explicit registry and accepts extra included roots.
- [LS-022] The roll-up lists a kept project that has no living spec.

## Boundary Cases

- [LS-023] The boundary generator emits adjacent millisecond cases around a 30 s delay limit.
- [LS-024] Rescore applies boundary evidence before assigning a requirement badge.

## Dependency Proofs

- [LS-025] Dependency loading rejects edges that lack authored evidence.
- [LS-026] The spec page shows current badge counts and each broken line test in its drill-down.

## Quantifier Map

- [LS-027] The Quantifier CLI checks finite claims against compiled fixture subjects and writes coverage exports.
- [LS-028] Sampled quantifier coverage records its seed and untested remainder.

## Integration

- [LS-029] The score command uses the same boundary, dependency, and quantifier providers as rescore.
- [LS-030] A malformed saved status leaves existing history and status untouched.
- [LS-031] The score command writes the current status and renders the spec page.
- [LS-032] The compatibility check command reports orphan annotations.
- [LS-033] The roll-up excludes archived projects and counts projects without a spec.
