# Packet 17: parametrized sibling cases, and quantifier validation at load

Read `.handoff/packets/00-architecture.md` (binding). Fix both findings at the root and keep `make verify` green.

1. **Sibling parametrized cases** (`src/core/badge.rs` around line 106). Only the explicitly linked case counts. With one current accepted link to `test#one`, identities for `test#one` and `test#two`, and `test#one` passing, the line is proven even if `test#two` is missing, skipped or failing. R1 requires every case of a parametrized test to appear and pass. Fix: a link to any case of a parametrized test, or to the base id, covers the **complete** case set known from the identities and the outcomes for that base id. Every case must be present and passing, or the badge is `unproven not_executed` (missing or skipped) or `broken test_failed` (any failing case). Regressions: `sibling_case_missing_is_not_proven`, `sibling_case_skipped_is_not_proven`, `sibling_case_failing_is_broken`, `all_sibling_cases_passing_is_proven`.
2. **Quantifier validation at load** (`src/core/annotations.rs` around line 258). The typed quantifier view does not enforce the published schema: `sampling.samples = 0`, an unsupported subject, or an incompatible kind/domain/sampling combination all pass `living-spec check`. Fix: validate during loading, with errors that name the path (samples ≥ 1, subject in the allowlisted catalog, `finite-universal` requires a finite domain, `infinite-universal` requires sampling, `existential` per contract). Regressions: `quantifier_zero_samples_rejected_on_load`, `quantifier_unknown_subject_rejected_on_load`, `quantifier_kind_domain_mismatch_rejected_on_load`, each also checked through `living-spec check` (exit 1).

## Acceptance
`make verify` plus the regressions.
