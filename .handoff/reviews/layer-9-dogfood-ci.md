CHANGES

## Major: failed scoring uploads the committed all-green status as CI evidence

**Location:** `.github/workflows/living-spec.yml:37-44` (also the unconditional scoring attempt at lines 33-36).

The artifact step runs after failures because its condition is `!cancelled()`, and its inputs include the tracked `living-spec/living-spec/status.json` and `history.jsonl`. If tests cannot compile, a harness aborts with incomplete output, or an earlier gate skips the test step, scoring cannot produce a new report. The tracked files survive and are uploaded under `living-spec-score`. They currently report all 33 lines as `proven`, with `audit: fresh`, for an older commit. The workflow itself fails, but its downloadable evidence still presents a successful historical score without recording the failed scoring attempt; the requested HTML is absent. This is misleading report output, not a false successful workflow conclusion.

**Failing input:** a `test-output.txt` containing a compilation failure instead of a completed libtest suite:

```text
error[E0425]: cannot find function `missing` in this scope
error: could not compile `living-spec`
```

Reproduced locally by copying the committed dogfood directory to `living-spec/target/layer9-probe/dogfood`, saving that input under the same scratch directory, and invoking the built binary from `living-spec/`:

```sh
target/debug/living-spec score target/layer9-probe/dogfood \
  --results target/layer9-probe/test-output.txt --format libtest --tests . \
  --commit synthetic-failed-ci --html target/layer9-probe/spec.html
```

Observed: exit 1 with `incomplete libtest output: missing suite completion`; status and history remain byte-identical to the committed copies; status still contains 33 proven lines and commit `2c12b687b8983b3bbd54f20c9c53f39d5241b0c9`; no requested HTML is created. The workflow's artifact condition nevertheless permits uploading those surviving files.

**Suggested fix:** score in a clean generated directory containing the spec and annotations but no inherited status or HTML. Upload score artifacts only when a new status for the attempted commit was actually emitted, including when scoring returns 2 for broken requirements. On build/input failure, publish the failed step's diagnostics and captured test output explicitly instead of uploading a historical status as the current score. Keep the workflow failure intact.
