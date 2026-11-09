CHANGES

Reviewed only HEAD `cee996d6393537ac7cfc676b8c91264625f26c5a` against its parent, using the architecture contract and its revisions. No existing tracked files were changed. `CLAUDE.md` is absent. Verification in `living-spec/`: `cargo test --offline` passed (65 tests, including all 20 dependency fixtures); `cargo fmt -- --check` and `cargo clippy --offline --all-targets -- -D warnings` passed. Throwaway probes under ignored `living-spec/target/` reproduced the findings below.

## 1. Major — diagnostic failures are presented as the cause of a broken prerequisite

**Location:** `living-spec/src/deps/report.rs:51-55`, rendered at `:77`.

**Failing input:** Two requirements, with an authored dependency `R-002 -> R-001`. `R-002` has passing admissible evidence. `R-001` has two links against its current revision:

- `rust:synthetic:src/lib.rs:diagnostic`: challenged audit, matching current fingerprint, executed with status `failed`.
- `rust:synthetic:src/lib.rs:missing`: accepted audit, but absent from both the source identities and outcomes, with no rename match.

The existing core scorer correctly produces this prerequisite status:

```json
{
  "id": "R-001",
  "badge": "broken",
  "reasons": ["link_challenged", "link_unresolved"],
  "tests": [{
    "test_id": "rust:synthetic:src/lib.rs:diagnostic",
    "status": "failed",
    "duration_ms": 1
  }]
}
```

There is no `test_failed` reason: the challenged failure is diagnostic only under R1. After qualification, `impacts()` nevertheless selects that diagnostic as `failing_test`; Markdown says:

```text
R-002 → R-001 (broken: test rust:synthetic:src/lib.rs:diagnostic failed)
```

The actual unresolved link disappears from the report. The same selection also chooses a challenged failure over an admissible failure when both exist and the challenged one occurs first. This guesses causal evidence from `tests`, which intentionally includes inadmissible diagnostics.

**Suggested fix:** Preserve per-link failure provenance/admissibility in the status or supply it explicitly to reporting. Select only a failure that actually contributed to the badge, and report unresolved links as unresolved. The presence of a failed diagnostic alone must never populate a causal `failing_test`; when provenance is unavailable, leave it unknown rather than guessing. Add the challenged-failure-plus-unresolved-link regression.

## 2. Major — valid deep dependency input aborts cycle detection with stack overflow

**Location:** `living-spec/src/deps/model.rs:54-64` (also recursive cycle-path search at `:121`).

**Failing input:** The following defines an exact, valid acyclic spec and annotation graph; all dependency evidence is authored:

```text
For each integer i in 0..50000:
  spec line = format!("- [R-{i:05}] Synthetic item.\n")
For each integer i in 0..49999:
  annotations.requirements[format!("R-{i:05}")].depends_on =
    [{"id": format!("R-{:05}", i + 1), "evidence": "authored"}]
annotations.schema = "living-spec.annotations/v1"
```

`Spec::parse` and `DepGraph::new` succeed. Calling `graph.detect_cycles()` from an ordinary debug executable then prints:

```text
Loaded 50000 valid requirements; detecting cycles
thread 'main' (...) has overflowed its stack
fatal runtime error: stack overflow, aborting
```

The observed process exit was 134. `apply_qualified()` reaches the same traversal through `cycle_members()`, so this aborts scoring even with no cycles or failed tests. Graph depth is controlled by input and no depth/size gate protects the recursive DFS. A cycle of similar depth also reaches recursive path construction.

**Suggested fix:** Use iterative SCC traversal and iterative cycle-path reconstruction with explicit work stacks, or reject excessive graph depth/size with a normal load error before recursive processing. Add a deep-graph regression that returns a result or a diagnostic rather than aborting the process.

## 3. Minor — dependency exports discard the untested-boundary reason

**Location:** `living-spec/src/deps/report.rs:46-56` and `:64-82`.

**Failing input:** Spec lines `R-001` (a bounded requirement) and `R-002` (its dependent), with authored `R-002 -> R-001`. `R-001` has an accepted current passing link but an unexecuted boundary case, so core status is `unproven` with `reasons: ["boundary_gap"]`. `R-002` starts `proven`. The following is the relevant status input to `apply_qualified()`:

```json
[
  {
    "id": "R-001", "badge": "unproven", "reasons": ["boundary_gap"],
    "tests": [{"test_id": "rust:synthetic:src/lib.rs:happy_path", "status": "passed", "duration_ms": 1}]
  },
  {
    "id": "R-002", "badge": "proven", "reasons": [],
    "tests": [{"test_id": "rust:synthetic:src/lib.rs:dependent", "status": "passed", "duration_ms": 1}]
  }
]
```

Qualification correctly produces `prerequisite_unproven`. However, the report exports only:

```json
{
  "dependent_id": "R-002",
  "dependent_text": "Synthetic dependent.",
  "path": ["R-002", "R-001"],
  "explanation": "R-002 → R-001",
  "broken_prerequisites": []
}
```

Markdown likewise gives only the path. Neither export retains the prerequisite's badge, text, or `boundary_gap` reason. Changing the prerequisite reason to `no_link` or `not_executed` produces the same report, so readers cannot discover the untested boundary from this artifact. This violates the honesty rule against hiding untested boundaries, although the badge itself correctly remains qualified.

**Suggested fix:** Export the selected non-proven prerequisite's badge and actual reasons, and render them in the explanation (for example, `unproven: boundary_gap`). Preserve available boundary details or an explicit reference to them. Update the JSON schema accordingly and test both JSON and Markdown with an unproven boundary prerequisite.
