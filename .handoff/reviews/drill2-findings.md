# Living Spec drill 2: layers 4–10 on the rebuilt branches

Unsigned Codex Sol (`gpt-6.1-sol`) drill reviews via `two-key-merge.py drill`, started 3 s apart at 12:09 AM PT Nov 9. Each diff is the layer branch against the layer below it (04 against 03-agents). Read-only: no repo edits.

**Result:** 6 findings. Approved: 04, 05 (one minor), 08, 09, 10. Changes requested: 06-web (3 major), 07-deps (2 major). Layer 10 was drilled at the branch as it stood, before packet 24 landed.

## 04-boundary (2ee5496): approve, 0 finding(s)

No actionable correctness, security, data-loss, or contract defects found. Supporting code and tests were inspected; tests were not executed in the read-only environment.


## 05-quantifier (defbc45): approve, 1 finding(s)

No blocking defects found. Reviewed the diff and matching shared dependencies read-only; tests were not run.

- **[minor] `living-spec/src/quantifier/annotations.rs:183`**
  - Issue: Explicit `sampling: null` deserializes as absent, so finite claims accept it and silently omit it from exports. This contradicts the annotation schema and the existing `Annotations::load` validation.
  - Fix: Reject explicitly null sampling before deserialization, and add regression coverage for shared annotations and standalone fixtures.

## 06-web (1c209a9): changes_requested, 3 finding(s)

Cached reports can misrepresent current annotations, and duplicate JSON keys can discard failed evidence. Read-only review; tests were not run.

- **[major] `living-spec/src/web/spec_page.rs:749`**
  - Issue: Stored boundary reports are validated against case shapes but not current boundary links. After scoring passing cases, changing or removing boundary_links without changing the bounds leaves the old report accepted. Both render and rollup can retain positive badges despite missing current boundary evidence.
  - Fix: Bind stored receipts to the current boundary link mapping and invalidate coverage when that mapping changes. Add regressions for changed and removed boundary links with unchanged bounds.
- **[major] `living-spec/src/web/spec_page.rs:877`**
  - Issue: Quantifier reports are selected solely by requirement ID. Editing the current quantifier's predicate or domain leaves old observations and results displayed as its Quantifier Map without a stale warning. Changing requirement text similarly leaves the cached report attached.
  - Fix: Compare the report's quantifier and text with the current claim before presenting its results. Mark mismatches stale or unscored, and test predicate, domain, and text edits without rescoring.
- **[major] `living-spec/src/web/status.rs:84`**
  - Issue: Parsing through serde_json::Value collapses duplicate object keys before typed validation. Duplicate statuses keys can replace broken results with proven results, and duplicate test status fields can replace failed with passed. The new reader can therefore render positive badges from conflicting input that the runner's typed reader rejects.
  - Fix: Reject duplicate keys in the original JSON bytes before constructing Value, covering all status and report loaders. Add regressions for duplicate statuses, badge, and test status fields.

## 07-deps (ca147fc): changes_requested, 2 finding(s)

Two major issues block merging: nonempty JSON reports violate their schema, and propagation has cubic work on long proven chains. Review was read-only; tests were not run.

- **[major] `contracts/living-spec-dependency-report-v1.schema.json:9`**
  - Issue: additionalProperties is false, but ImpactReport always serializes unproven_prerequisites and qualified_prerequisites, neither of which is declared here. Every nonempty dependency report therefore fails its published schema.
  - Fix: Declare both arrays using the prerequisite item definition, and add a test that validates actual serialized reports against this schema.
- **[major] `living-spec/src/deps/propagate.rs:29`**
  - Issue: Each proven requirement starts a fresh traversal, and every traversal step clones the entire accumulated path. An all-proven chain therefore performs cubic string-copying work despite producing no qualifications. At the tested scale of 50,000 nodes, this entails approximately 20.8 trillion string clones. The deep-chain tests never exercise propagation on the acyclic graph.
  - Fix: Compute affected nodes from non-proven and cyclic prerequisites using shared traversal results, storing predecessor links instead of copying paths at every step. Add a large all-proven acyclic propagation regression.

## 08-pipeline (8b21f0c): approve, 0 finding(s)

No blocking correctness, security, data-loss, or contract issues found in the supplied diff. Supporting code was inspected read-only; tests were not run.


## 09-dogfood-ci (593e9fa): approve, 0 finding(s)

No blocking correctness, security, or data-loss findings. Checked the diff against matching local scoring and CI code; formatting checks passed for all three added Rust test files. Runtime tests were not run during this read-only review.


## 10-handoff (5d1a9b7): approve, 0 finding(s)

No actionable findings in this documentation-only diff. Checked command and contract descriptions against available local context; runtime tests were not run.

