CHANGES

## 1. Major — namespaced XML attributes can give an unexecuted test a green badge

**Location:** `living-spec/src/adapters/mod.rs:742-747`.

**Failing input:**

```xml
<testsuite xmlns:q="urn:extension" tests="2">
  <testcase classname="rust:fixture:tests/export.rs:exports"
            name="other" q:name="timestamps"/>
  <testcase classname="rust:fixture:tests/export.rs:exports" name="order"/>
</testsuite>
```

Parse this with `parse_junit` and rescore a temporary copy of `tests/fixtures/runner/spec` with `tests/fixtures/runner/fingerprints.json`, `NoBoundaryEvidence`, and `NoQualification`. The first case's JUnit name is `other`; `q:name` is a distinct extension attribute. However, `attributes()` strips the namespace prefix and inserts both under the same `name` key. The later value silently wins, so the adapter reports a passing `rust:fixture:tests/export.rs:exports::timestamps` outcome. The rescorer then marks `R-001` as `proven`, although that linked test never executed. This was reproduced through parsing and persisted rescoring, not only by inspecting the attribute map. Reversing attribute order changes the result, so the ambiguity is being guessed.

**Suggested fix:** Interpret JUnit identity/count/time attributes only under their exact supported qualified names. Reject ambiguous aliases or ignore extension attributes without letting them overwrite unqualified attributes; detect normalized-key collisions instead of replacing values. Add a parser-to-rescorer regression ensuring the example cannot produce a passing `timestamps` outcome or green `R-001`.

## 2. Major — the documented hook cannot invoke the new runner

**Location:** `living-spec/hooks/pre-push:12-19`.

**Failing input:** Run the hook against the binary built from this HEAD, with the existing passing runner fixture:

```sh
TMPDIR="$PWD/living-spec/target/review-probes" \
LIVING_SPEC_TEST_CMD='cat living-spec/tests/fixtures/runner/before.txt' \
LIVING_SPEC_BIN="$PWD/living-spec/target/debug/living-spec" \
LIVING_SPEC_DIR='living-spec/target/review-probes/spec' \
LIVING_SPEC_FINGERPRINTS='living-spec/tests/fixtures/runner/fingerprints.json' \
sh living-spec/hooks/pre-push
```

Create the temporary spec by copying the fixture's `spec.md` and `annotations.json` into the indicated directory. The test command succeeds, but the binary prints usage and the hook exits 1 with `Push blocked: living spec results could not be rescored.` At this HEAD, `src/cli/mod.rs` dispatches only `check`; `rescore` is never registered. Consequently the newly documented hook cannot rescore, record history, or allow even an all-passing push.

**Suggested fix:** Add and register the runner CLI entry point, wire its results/identity inputs to the rescorer, and propagate its 0/1/2 exit behavior. Exercise the hook with this binary for passing, first-run broken, already-broken, and malformed results. The architecture assigns integration work to a later packet but also requires each packet to register its own subcommand; this finding conservatively evaluates the hook as shipped in this commit.

## 3. Minor — every persisted status document violates its new schema

**Location:** `contracts/living-spec-status-v1.schema.json:35-36` (also the reason alternatives at lines 79-103); emitted fields at `living-spec/src/runner/rescore.rs:351-357`.

**Failing input:** Call `rescore_with_providers` using a temporary copy of `tests/fixtures/runner/spec`, outcomes parsed from `tests/fixtures/runner/before.txt`, identities loaded from `tests/fixtures/runner/fingerprints.json`, commit `fixture-commit`, `NoBoundaryEvidence`, and `NoQualification`. It succeeds with exit code 0 and writes `status.json` containing `"quantifier_reports": []` and `"boundary_reports": []`. Neither property is declared in the schema, whose root sets `additionalProperties: false`; even this ordinary passing document is invalid. A consumer enforcing the advertised contract must reject every runner-generated report. The schema also excludes the existing R4 reason variants `prerequisite_unproven`, `prerequisite_qualified`, and `dependency_cycle`, so merely removing the two extra fields does not make all supported status documents valid.

**Suggested fix:** Bring the schema into agreement with `StatusDocument`, including typed boundary/quantifier report properties and every supported reason variant. Validate actual serialized passing, boundary, quantifier, and qualified documents against the schema.

## 4. Minor — a truncated next target is accepted as a complete passing capture

**Location:** `living-spec/src/adapters/mod.rs:40-47` and `147-151`.

**Failing input:**

```text
Running tests/a.rs (target/debug/deps/a-123abc)
running 1 test
test pass ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Running tests/b.rs (target/debug/deps/b-456def)
```

`parse_libtest` returns `Ok` containing only the passing `libtest:a:tests/a.rs:pass` outcome. Cargo announced another test binary, but its entire suite is missing. The adapter only marks a suite pending when it sees `running N tests`; a target header alone leaves no pending state for the EOF check. This silently accepts incomplete results instead of rejecting them under R5, and also permits consecutive target headers to erase an announced, unreported target.

**Suggested fix:** Track an announced target as pending until its running header and summary complete, including legitimate zero-case suites. Reject EOF or another target header while an announced target has no completed suite. Keep support for standalone harness captures without Cargo headers.
