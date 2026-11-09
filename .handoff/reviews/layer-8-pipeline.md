CHANGES

## Major: re-auditing transfers a human override to changed evidence

**Location:** `living-spec/src/pipeline/rescore.rs:72-85`, especially line 75.

`SourceReauditor` refreshes the fingerprint and requirement revision but copies the
previous audit's `override` into the new audit. Consequently `Audit::is_accepted()`
accepts the new source even when the fresh auditor explicitly challenges it. A
human decision about the old assertion becomes acceptance of a replacement
placeholder. The same transfer can occur across a requirement revision change.

**Failing input:** Under a disposable spec directory `D`, create:

`spec.md`:
```markdown
- [R-001] Exports retain timestamps.
```

`tests/test_exports.py` (the current, changed source):
```python
def test_exports():
    assert True
```

`annotations.json`:
```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-001": {
      "links": [{
        "test_id": "pytest:tests:tests/test_exports.py:test_exports",
        "fingerprint": "sha256:2ecbc8b45f483e4131e4da68c8ef1575e9bc78e1b6428c7c1968a659148fd6df",
        "revision": "sha256:f1e9e2d829701501c501ed144f16ea95e818182722510c6c47b94cb59776d987",
        "explanation": "Synthetic review fixture",
        "proposed_by": "human",
        "audit": {
          "verdict": "challenged",
          "revision": "sha256:f1e9e2d829701501c501ed144f16ea95e818182722510c6c47b94cb59776d987",
          "notes": [],
          "override": {"by": "human", "reason": "Reviewed the prior timestamp assertion."}
        }
      }]
    }
  }
}
```

The stored fingerprint is for the previous bytes
`def test_exports():\n    assert export_timestamp() == 7\n`.
Supply `results.xml`:
```xml
<testsuite tests="1"><testcase classname="tests.test_exports" name="test_exports"/></testsuite>
```

Run `living-spec score D --results D/results.xml --format junit --tests D --commit probe`.
Observed: exit 0, `proven: 1`, and `status.json` marks the placeholder outcome
`admissible: true`. The persisted audit simultaneously says `challenged`, reports
only placeholder assertions, and retains the old human override.

**Suggested fix:** Preserve foreign audit metadata, but invalidate acceptance
overrides whenever the source fingerprint or requirement/audit revision changes.
Retain the previous decision as diagnostic history rather than active acceptance.
An unchanged binding may retain its override; a newly challenged binding must
remain unproven until a human accepts that specific binding.

## Major: pipeline quantifier loading bypasses duplicate-key rejection

**Location:** `living-spec/src/pipeline/rescore.rs:96-107`.

The pipeline constructs claims from `Annotations`' already-deserialized JSON
values instead of using the quantifier loader's original-byte validation.
Duplicate keys have already been discarded when `from_section` runs. This
reintroduces an ambiguity the standalone Quantifier Map rejects, and can silently
replace a failing subject with a passing one.

**Failing input:** `spec.md`:
```markdown
- [R-001] Synthetic absolute-value claim.
```

`annotations.json` (the duplicate `subject` is intentional):
```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-001": {
      "quantifier": {
        "kind": "finite-universal",
        "domain": {"type": "int-range", "lo": -128, "hi": 127},
        "subject": "abs_wrapping_i8",
        "subject": "abs_i8",
        "predicate": {
          "version": 1,
          "expression": {
            "op": "ge",
            "left": {"kind": "field", "source": "output"},
            "right": {"kind": "literal", "value": {"type": "int", "value": 0}}
          }
        }
      }
    }
  }
}
```

Supply `<testsuite tests="1"><testcase name="fixture"/></testsuite>` as
`results.xml`. `living-spec quantifier check D` correctly exits 1 with
`object keys must be unique`. In contrast,
`living-spec score D --results D/results.xml --format junit --commit probe`
exits 0 and writes a report for `abs_i8` with
`no-counterexample-in-tested-cases`. The discarded `abs_wrapping_i8` subject
would produce a counterexample at -128.

**Suggested fix:** Apply the quantifier duplicate-key validator to original
annotation bytes before ordinary map/value deserialization, including nested and
escaped duplicate keys, and before persistence. Share that validation between
standalone and pipeline loading while preserving the pipeline's orphan reporting.

## Minor: generated HTML omits inferred boundary cases

**Location:** `living-spec/src/pipeline/score.rs:85-94`, especially line 89.

The pipeline computes inferred boundary reports, then renders the directory
without making those effective bounds available to the web view's annotation
gate. `push_boundary` in `web/spec_page.rs` returns immediately when stored
`annotations.bounds` is empty, even when the newly stored report has cases.
The badge stays correctly unproven, but its boundary drilldown disappears.

**Failing input:** `spec.md` contains
`- [R-001] Delay is at most 30 s.`. In `annotations.json`, give `R-001` one accepted
human link with `test_id: "fixture"`, `fingerprint: "sha256:synthetic"`,
`explanation: "Synthetic review fixture"`, `proposed_by: "human"`, and both link
and audit revision set to
`sha256:f9e9f760f3fac3f5d8b3f3644b0b6a8900c63e2003c4cce721f7becb02773dcf`;
the audit has `verdict: "accepted"` and `notes: []`. Omit `bounds` and boundary
links. Use the normal annotation schema and requirements wrapper shown above.
Supply `fingerprints.json` as `{"fixture":"sha256:synthetic"}` and the single
passing `fixture` JUnit receipt from the preceding finding.

Run `living-spec score D --results D/results.xml --format junit --fingerprints D/fingerprints.json --commit probe`.
Observed: exit 0; `status.json` correctly includes `boundary_gap` and three
untested cases at 29999, 30000, and 30001 ms. Generated `spec.html` contains no
`Boundary coverage` table and displays none of those cases. A prior `boundary gen`
should not be required for the integrated score command to explain its results.

**Suggested fix:** Render from the effective boundary reports/resolved bounds,
including requirements without cached `bounds`, or supply a resolved rendering
projection from the pipeline. Preserve inferred provenance rather than converting
computed bounds into authored overrides.
