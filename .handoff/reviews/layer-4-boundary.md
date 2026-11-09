CHANGES

## Major: Generated bounds become permanent authored overrides after a requirement edit

Location: `living-spec/src/cli/boundary.rs:59-65`, `living-spec/src/cli/boundary.rs:84` (also consumed at lines 130-138).

Failing input / reproduction:

1. Start with `spec.md` containing `- [R-001] Delay is at most 10 ms.` and annotations `{"schema":"living-spec.annotations/v1","requirements":{}}`.
2. Run `living-spec boundary gen <spec-dir>`. It writes the inferred 10 ms bound into `requirements.R-001.bounds` without its origin or requirement revision.
3. Add these `boundary_links` to that requirement:

   ```json
   [
     {"quantity":"Delay","position":"below","value":"9","unit":"ms","test_id":"case_below"},
     {"quantity":"Delay","position":"at","value":"10","unit":"ms","test_id":"case_at"},
     {"quantity":"Delay","position":"above","value":"11","unit":"ms","test_id":"case_above"}
   ]
   ```

4. Edit the requirement to `- [R-001] Delay is at most 5 ms.` and run `boundary gen` again. Supply a JUnit receipt for those three tests:

   ```xml
   <testsuite tests="3"><testcase name="case_below"/><testcase name="case_at"/><testcase name="case_above"/></testsuite>
   ```

5. Run `boundary coverage <spec-dir> --results <receipt.xml> --format junit`.

Observed: both generation runs and coverage exit 0. Coverage reports 9/10/11 ms as `tested_pass`, including 9 and 10 ms as expected accepted. The current 5 ms limit's 4/5/6 ms cases never appear. `has_bounds` mistakes the previous generator output for an authored override, so even rerunning generation cannot refresh it. This hides an untested boundary and reports incorrect acceptance expectations.

Suggested fix: distinguish inferred bounds from authored overrides and bind inferred bounds to the requirement revision. Regenerate or reject stale inferred bounds after edits, and invalidate/revalidate their case links when values or expected outcomes change. Preserve intentional authored overrides, including an explicitly empty list.

## Major: The runner provider silently omits boundaries that the CLI discovers

Location: `living-spec/src/boundary/coverage.rs:79-85`, `living-spec/src/boundary/coverage.rs:49-56`.

Failing input: parse `- [R-001] Delay is at most 30 s.`; load an annotation entry for `R-001` with no `bounds` field and one accepted core link to `libtest:sample:tests/cases.rs:happy`. Set both the link and audit revision to the parsed line's current revision, its fingerprint to `sha256:synthetic`, and provide that same current fingerprint in the identity map. The only outcome is:

```json
{"test_id":"libtest:sample:tests/cases.rs:happy","status":"passed","duration_ms":null}
```

The linked test exercises only a 15-second happy path; no below/at/above boundary tests exist. There is no explicitly authored empty bounds override.

Observed through the public provider API: `BoundaryEvidence.gaps(&spec, &annotations, &outcomes)` returns an empty set, `reports(...)` returns an empty list, and passing that gap set into `score_line_at_revision` produces `proven`. In contrast, the new CLI's `boundary coverage` resolves this prose into the 29999/30000/30001 ms cases and reports gaps without requiring a prior `gen`. The provider ignores its `Spec` argument and only visits stored annotation bounds, so skipping generation silently removes the entire boundary from scoring and reports.

Suggested fix: apply a shared bounds-resolution/preparation policy before provider scoring and reporting. Either materialize inferred bounds for current spec lines before the core scorer runs, or fail explicitly when required generation/annotation is missing. Do not turn an absent bounds field into an implicit empty override; preserve the distinction from authored `bounds: []`.
