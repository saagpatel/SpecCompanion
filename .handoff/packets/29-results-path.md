# Packet 29: freshness re-reads the results path the caller chose

Review finding on `src/core/freshness.rs` around line 423: the results path is canonicalized when stored. Scoring `latest.xml -> run1.xml` saves `run1.xml`; if `latest.xml` is later retargeted to a failing `run2.xml`, `check()` keeps reading the unchanged `run1.xml`, and both the whole-file and per-requirement freshness stay current.

Fix: store the results path exactly as the caller gave it (made relative to the spec directory where possible, never resolved through symlinks), plus the digest of the bytes scored. `check()` re-reads that same path, following symlinks at check time. Any difference in bytes, or a missing file, means stale. Apply the same rule to every input path the receipt stores (spec, annotations). Regression: `retargeted_results_symlink_is_stale` (unix-only with `#[cfg(unix)]`), plus `missing_results_file_is_stale`. Keep `make verify` green and the dogfood at 33/33 proven.
