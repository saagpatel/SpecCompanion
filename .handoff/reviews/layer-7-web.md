CHANGES

## 1. Major — edited requirements retain a green badge from an older revision

**Location:** `living-spec/src/web/spec_page.rs:243-251` (also loaded at lines 164-168).

The page combines the current requirement text and annotations with an old status badge, without checking the link/audit revision against `RequirementLine.revision`. Thus an ordinary edit after scoring presents the new requirement as proven, contrary to R2. There is no snapshot or stale-data warning.

**Failing input:** score `- [R-001] Returns the input.`, then change only `spec.md` to `- [R-001] Returns twice the input.`. Retain these files:

```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-001": {
      "links": [{
        "test_id": "libtest:sample:tests/a.rs:case",
        "fingerprint": "sha256:fixture",
        "revision": "sha256:18f0b382ff7684a5303f4c7da20d637efb9ffef40cd02bb899f41aba9b1fdfe1",
        "explanation": "Checks the input.",
        "proposed_by": "human",
        "audit": {
          "verdict": "accepted",
          "revision": "sha256:18f0b382ff7684a5303f4c7da20d637efb9ffef40cd02bb899f41aba9b1fdfe1",
          "notes": []
        }
      }]
    }
  }
}
```

```json
{"statuses":[{"id":"R-001","badge":"proven","tests":[{"test_id":"libtest:sample:tests/a.rs:case","status":"passed"}]}]}
```

`render <spec-dir> --out <output.html>` succeeds and renders `Returns twice the input.` with `data-badge="proven"`, 100.0%, and no `stale_revision` explanation. Reproduced through the CLI.

**Suggested fix:** validate current requirement/link/audit revisions before displaying stored evidence as current. Mark stale evidence unscored/unproven with an explanation until rescoring, or render a clearly identified immutable snapshot with its original requirement text. Do not silently attach an old green badge to new text.

## 2. Major — roll-up omits requirements absent from an old status file

**Location:** `living-spec/src/web/rollup.rs:328-352`.

When status exists, this branch never parses `spec.md`. The denominator is the number of stored status entries, so new, untested requirements disappear entirely. Removed requirement entries also continue contributing evidence, and a status file without any spec is accepted as a scored living spec.

**Failing input:** a registry with `workspace_root: "."` and one project whose `identity.path` is `sample` and `derived.archived` is `false`; `sample/living-spec/spec.md` contains:

```markdown
- [R-001] Returns the input.
- [R-002] Rejects invalid input.
```

Its `status.json` contains only:

```json
{"statuses":[{"id":"R-001","badge":"proven","tests":[{"test_id":"libtest:sample:tests/a.rs:case","status":"passed"}]}]}
```

`rollup --registry <registry.json> --out <output.html> --json <output.json>` succeeds with `total_lines: 1`, `total_proven: 1`, and `total_proven_pct: 100.0`. R-002 is neither counted nor identified as unscored. Reproduced through the CLI.

**Suggested fix:** reconcile status IDs with the current parsed spec before aggregating. Surface missing/extra entries and mark incomplete/stale projects accordingly; never present the partial status array as complete current coverage. Require a spec for a scored living spec.

## 3. Major — conflicting status aliases silently discard failed evidence

**Location:** `living-spec/src/web/status.rs:102-110`.

If both supported status-array keys exist, `lines` wins without checking `statuses`. This resolves contradictory evidence by guessing field precedence and can suppress a failure in both the page and portfolio output.

**Failing input:** a spec containing R-001, a valid empty annotations document, and:

```json
{
  "lines": [{
    "id": "R-001", "badge": "proven",
    "tests": [{"test_id": "libtest:sample:tests/a.rs:case", "status": "passed"}]
  }],
  "statuses": [{
    "id": "R-001", "badge": "broken", "reasons": ["test_failed"],
    "tests": [{"test_id": "libtest:sample:tests/a.rs:case", "status": "failed"}]
  }]
}
```

Rendering succeeds with a green badge and no failed test; the broken entry is discarded. Reproduced through the CLI.

**Suggested fix:** reject documents containing both aliases, or require that they be identical before accepting them. Conflicting input must produce an explicit invalid-input result.

## 4. Major — an incomplete stored boundary report hides every required case

**Location:** `living-spec/src/web/spec_page.rs:631-642`.

A stored report selected solely by requirement ID bypasses computation and validation against current bounds. A structurally valid but incomplete report can therefore produce an empty boundary table without an unavailable/untested warning. R1 requires every case to remain visible unless it has executed passing evidence.

**Failing input:** `spec.md` contains `- [R-001] Delay is at most 30 ms.`. Its annotations are:

```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-001": {
      "bounds": [{
        "quantity": "Delay",
        "kind": {
          "type": "numeric", "value": "30", "unit": "ms",
          "op": {"type": "le"},
          "step": {"value": "1", "unit": "ms"}, "integer": false
        }
      }]
    }
  }
}
```

Use this status file:

```json
{
  "statuses": [{"id":"R-001","badge":"proven","tests":[{"test_id":"libtest:sample:tests/a.rs:case","status":"passed"}]}],
  "boundary_reports": [{"requirement":"R-001","bounds":[]}]
}
```

Rendering succeeds with a green badge and a boundary table containing no rows. The required 29/30/31 ms cases, all lacking boundary links, are absent, with no `untested` or unavailable message. Reproduced through the CLI.

**Suggested fix:** validate stored reports against the current bounds and generated case set before rendering. Reject incomplete, duplicate, or mismatched coverage reports, or explicitly render missing cases as untested. Never let the mere presence of a matching report ID suppress current boundaries.

## 5. Minor — repeated failures replace the actual breaking commit

**Location:** `living-spec/src/web/spec_page.rs:470-478`.

The runner appends same-state rescoring events as well as badge transitions. Searching backward for any event whose destination is broken selects a later `broken → broken` event, rather than the commit that introduced the current failure.

**Failing input:** R-001 has a current broken status and this `history.jsonl`:

```jsonl
{"at":"2026-01-01T00:00:00Z","commit":"commit-break","requirement":"R-001","from":"proven","to":"broken","test_id":"case","note":"First failure"}
{"at":"2026-01-02T00:00:00Z","commit":"commit-still-broken","requirement":"R-001","from":"broken","to":"broken","test_id":"case","note":"Still failing"}
```

The page reports `breaking commit commit-still-broken` and omits `commit-break`. These are ordinary runner-produced event shapes. Reproduced through the CLI.

**Suggested fix:** select the latest actual transition into broken (`from != broken && to == broken`), preserving the first-run transition from unproven. Treat same-state events as later observations, not the breaking commit.

## 6. Minor — orphan annotations disappear from the web report

**Location:** `living-spec/src/web/spec_page.rs:164-165` and `:125`.

The annotations are loaded without reporting `annotations.orphans(&spec)`. The extra-status warning examines only status IDs, so it does not cover annotation-only orphans. This violates the shared annotation contract's explicit orphan-reporting requirement.

**Failing input:** `spec.md` contains only `- [R-001] Returns the input.`; `status.json` contains only R-001; `annotations.json` contains a valid R-999 entry with a link (the link from finding 1 suffices). Render the directory.

The output contains no `R-999` and no orphan warning. Its link and audit notes are silently absent from the report. Reproduced through the CLI.

**Suggested fix:** include a clearly identified orphan-annotation section using the existing `Annotations::orphans` API, preserving orphan IDs and relevant diagnostics without counting them as current requirements.

## 7. Minor — unknown archival state is guessed to mean kept

**Location:** `living-spec/src/web/rollup.rs:436-448`.

The binding registry contract selects projects with `derived.archived == false`. Missing `derived`, null `derived`, and missing/null `archived` are instead all treated as an explicit false. An incomplete registry entry can thus contribute green evidence despite its inclusion state being unknown.

**Failing input:** a registry containing:

```json
{"workspace_root":".","projects":[{"identity":{"project_key":"sample","path":"sample"}}]}
```

Give `sample/living-spec/` a one-line spec and the one-line proven status from finding 2. Roll-up succeeds and counts the project as scored, adding one proven line, although the snapshot never says it is kept. Reproduced through the CLI. The same default applies to `"derived":null` and `"derived":{"archived":null}`.

**Suggested fix:** require an explicit boolean archival value to decide kept/archived membership. Reject incomplete selection metadata or surface an unknown-membership diagnostic; continue tolerating genuinely optional display metadata. This finding conservatively treats archival state as required because it controls the binding kept-project predicate.
