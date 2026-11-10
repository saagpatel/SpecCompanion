# Mutation-checked proof tier: architecture contract (binding for packets 31 to 36)

Read `.handoff/packets/00-architecture.md` first; it stays binding. This contract adds an **opt-in tier** on top of the
existing scoring. Nothing here changes what plain `proven` means.

## Why

The static allowlist auditor is a best-effort check. It keeps getting new bypass spellings, so we stop hardening it and
document it as the best-effort baseline. The new tier asks a different question that does not depend on reading test
syntax: *if the code this test calls is deliberately broken, does the test fail?* A test that cannot fail on broken code
proves nothing about its requirement.

## Vocabulary

- **Link**: one accepted requirement-to-test link in `annotations.json`.
- **Subject**: the non-test **free functions** a Rust test calls directly (one hop, `name(...)` or `path::name(...)`
  calls only, resolved inside the same Cargo package's non-test code: `src/**` outside `#[cfg(test)]`, never `tests/`,
  never the test file itself). v1 supports only an **unambiguous** subject: exactly one definition per called name.
  Methods (`.name()`), macro-generated functions, names with several definitions, tests that only spawn a binary or
  call fixtures, doctests and binary-target tests are `unsupported`. A killed mutant shows the test is sensitive to
  that change, not that the subject is the whole of what the requirement needs.
- **Mutant**: the subject with exactly one small token-level change.
- **Killed**: the linked test fails (assertion failure, panic, or timeout) on the mutant. **Survived**: it still passes.
  **Unviable**: the mutant does not compile (not counted either way).

## File and schema

`<spec dir>/mutation.json`, schema `living-spec.mutation/v1`, contract
`contracts/living-spec-mutation-v1.schema.json` (add it beside the other contracts; unknown fields rejected, like the
annotation layer). Shape:

```json
{
  "schema": "living-spec.mutation/v1",
  "engine": "rust-cargo-v1",
  "links": [
    {
      "requirement": "LS-001",
      "test_id": "libtest:...",
      "test_fingerprint": "sha256:...",
      "requirement_revision": "sha256:...",
      "subject_digest": "sha256:...",
      "target": { "kind": "lib | test", "name": "pipeline" },
      "baseline_passes": 2,
      "status": "checked | survived | unsupported | error",
      "reason": "free text for unsupported or error",
      "mutants": [
        { "id": "m-<12 hex>", "file": "src/x.rs", "function": "clamp", "line": 12, "ordinal": 0,
          "operator": "cmp_flip", "original": "<=", "replacement": ">",
          "outcome": "killed | survived | timeout | unviable" }
      ]
    }
  ],
  "signoffs": [
    { "kind": "link", "requirement": "LS-009", "test_id": "...", "test_fingerprint": "sha256:...",
      "requirement_revision": "sha256:...", "reason": "why mutation is not possible here", "signed_by": "handle" },
    { "kind": "mutant", "requirement": "LS-001", "test_id": "...", "mutant_id": "m-...",
      "test_fingerprint": "sha256:...", "requirement_revision": "sha256:...", "subject_digest": "sha256:...",
      "reason": "why this survivor is an equivalent mutant", "signed_by": "handle" }
  ]
}
```

`mutant.id` is stable: the first 12 hex of sha256 over `file`, `function`, `operator`, `original`, `replacement` and
`ordinal` (the index of that token inside its function). **`status` is evidence, not trust:** the validator recomputes
it from `baseline_passes` and the mutant outcomes and rejects a file whose stored status disagrees. `subject_digest` is sha256 over the sorted `(file, function name, function source
bytes)` of every mutated subject function, so any edit to the code under test invalidates the record.

## Status rules (fail closed)

Per link, computed at scoring time from the file plus current bindings:

1. `status: checked`: at least 1 viable mutant, every viable mutant `killed`, no `timeout`, and `baseline_passes` is
   2 (the unmutated test passed twice). Counts as **checked** only while `test_fingerprint`, `requirement_revision` and `subject_digest` all equal
   the current values; otherwise the link is **stale** (never checked).
2. `status: survived`: at least 1 mutant survived. A `kind: mutant` signoff (equivalent mutant) with matching bindings
   waives exactly that mutant; when every survivor is waived the link counts as **waived**, shown with the waiver
   count. A waiver is a human judgment: it never hides the mutant, and it is **not** a pure mutation check.
3. `unsupported` (non-Rust test, no resolvable subject, zero viable mutants, or mutation disabled for this test) and
   `error` (flaky baseline, baseline fails, timeout storm, crate copy failure): the link is **unchecked**. A matching
   `kind: link` signoff turns it into **signed off**. A signoff is bound to `test_id` + `test_fingerprint` +
   `requirement_revision`; any difference makes it inactive. Signoffs never apply to a `survived` link.
4. A line's tier, only meaningful when the plain badge is `proven`. **`mutation_checked`** iff it has at least one
   accepted admissible link and every one is a pure `checked` link. **`signed_off`** iff every accepted admissible
   link is `checked`, waived or signed off and at least one is waived or signed off (human judgment is visible and is
   never counted as a mutation check). Otherwise the tier is the worst link state: `survived` > `stale` > `error` >
   `unsupported` > `not_run`. A line that is not plain `proven` has no tier (`null`). Dependency qualification is
   unchanged and the tier does not propagate in v1.
5. The mutation file can never change a plain badge, never create `broken`, and never fabricate a test failure.

A timeout is **inconclusive**: it is recorded as `timeout`, is never counted as a kill, and a link with a timeout and
no survivor is `error` (never checked). A kill requires a completed, matching, **failed** test case in parsed libtest
output; a build failure is `unviable`; any other infrastructure failure makes the link `error`. Surviving mutants are
expected and normal; the report must list them (file, function, line, operator).

## Engine (Rust only in v1)

- Reuses nothing from a runner: there is **no existing crate-copy runner**, so the engine builds one. The test id is
  `libtest:<target>:<file>:<name>` (the second part is the executable **target**, not the package): `tests/<stem>.rs`
  means `--test <stem>`, `src/lib.rs` means `--lib`; the owning package is the single nearest `Cargo.toml` above the
  file under the `--tests` root (zero or several candidates, binary targets and doctests are `unsupported`). Only
  self-contained crates are supported: a path dependency or workspace inheritance that reaches outside the crate
  directory is `unsupported: dependencies outside crate`. Copy the crate into a fresh temp directory, excluding
  `target/` and `.git/`, reusing one stable copy and one `CARGO_TARGET_DIR` per worker so artifacts stay valid; the
  original tree is never modified (test: byte-identical before and after). `tempfile` moves from dev-dependency to
  dependency for this (already in the offline cache).
- One mutant at a time: replace one byte span in the copy, run
  `cargo test --offline --locked <--lib | --test <target>> <test name> -- --exact` with env
  `CARGO_TERM_COLOR=never` and `NO_COLOR=1` on **every** spawned cargo (including the baseline), a per-run timeout
  (default 180 s), a shared `CARGO_TARGET_DIR` per worker inside the temp tree so mutants rebuild incrementally, and
  the process killed on timeout. Output is parsed with the existing libtest adapter; a missing or garbled result is
  `error`, never a kill.
- Operators (deterministic order, token-level, no regex rewriting of strings or comments):
  `cmp_flip` (`==`/`!=`, `<`/`>=`, `<=`/`>`, `>`/`<=`, `>=`/`<`), `arith_swap` (`+`/`-`, `*`/`/`), `bool_swap`
  (`&&`/`||`, `true`/`false`), `int_bump` (a plain decimal literal with no suffix, underscore, or prefix: `n` to `n+1`,
  and to `0` when `n != 0`; any other numeric form is skipped). No mutation inside string/char literals, comments,
  attributes, `const`/`static` items, macro invocation bodies, or `#[cfg(test)]` code. The existing lexer is not a
  parser: when a function's extent, generics or attributes cannot be determined with certainty (any `cfg` attribute
  other than exactly `cfg(test)`, nested macro bodies), skip that function and report `unsupported` if none is left.
  Generated sites that fail to compile are `unviable`, which is expected.
- A cap of mutants per link (default 8, `--max-mutants`), chosen deterministically round-robin across operators then in
  source order. `--jobs N` (default 1) runs links in parallel, each with its own copy and target dir.
- Any child process is spawned without a shell; paths are passed as arguments.

## CLI (packet 34)

`living-spec mutate <spec dir> --tests <root> [--crate <dir>] [--max-mutants N] [--jobs N] [--timeout-secs N]
[--only REQ-ID]... [--out mutation.json]` runs the engine for every accepted link and merges into the file.
`living-spec signoff <spec dir> link|mutant ...` records a signoff, binding it to the current values.
`living-spec score ... --mutation [<path>]` ingests the file (default `<spec dir>/mutation.json`). Without
`--mutation`, `status.json` and every output stay **byte-identical** to today (new fields are `skip_serializing_if`
none; a regression test pins it).

## Status and roll-up (packets 33, 35)

`LineStatus` gains `mutation: Option<Tier>` (serde default, skipped when none) with
`{ "state": "mutation_checked|signed_off|survived|stale|error|unsupported|not_run", "waived": n, "signed_off": n,
"survivors": [...] }`; the badge label is derived from it. Shared tier types live in `core` (`src/core/mutation.rs`);
the engine lives in the new layer `src/mutation/` (may import `core`, `adapters`, `agents`, `mutation`); `pipeline` and
`cli` may import `mutation`; `web` imports only `core`. `scripts/check-layers.sh` is updated in the packet that adds
`src/mutation/` (32).
`ScoreInputs` gains `mutation_digest: Option<String>`, **kept out of the plain score digest**: editing or removing
`mutation.json` after scoring marks only the mutation tier `stale` when rendered, never the plain badge. The web
status reader and both schema definitions (line and inputs) must accept the new fields.
Labels: plain `proven` keeps its label; the tier shows a second badge `proven (mutation-checked)`; `signed_off` shows
`proven (mutation sign-off)` with waiver and signoff counts; `survived` shows a visible `mutation survivor` flag next
to plain `proven`. The roll-up reports `proven`, `proven_mutation_checked` (pure only), `proven_signed_off` and
`mutation_survivors`: additive JSON fields, existing keys unchanged; the roll-up HTML grows columns only when some
project has a tier. The spec page and `status.json` are byte-identical when no mutation data is present.
Quantifier Map output never uses proof language (existing rule); the mutation badge lives in the core spec view and the
roll-up only.

## Honesty rules

- Mutation testing shows a test **can fail** on broken code, not that the requirement is met. Docs and the badge text
  must say "mutation-checked", never "verified" or "guaranteed".
- Equivalent mutants make survivors ambiguous; only a human waiver resolves them, and the waiver is bound and visible.
- The static auditor remains the documented best-effort baseline for plain `proven`.
- Offline only. No network, no LLM. No new crate dependencies without a stated reason (the lexer in
  `src/agents/syntax.rs` is reused; `sha2`, `serde`, `serde_json`, `regex` already exist).
- Repo is public: no real paths, names or emails in code, fixtures, docs, mutation output or commits. Store paths
  relative to the crate root only; never absolute temp paths in `mutation.json`.

## Packets

| # | Slug | Lane | Delivers |
|---|---|---|---|
| 31 | `mutation-model` | Terra | `src/core/mutation.rs`: types, parse/validate/write, ids, digests, signoff binding, status rules (pure, no cargo); schema contract; baseline-best-effort docs |
| 32 | `mutation-engine` | Terra (xhigh if high falls short) | `src/mutation/`: subject resolution, mutant generation, crate-copy runner, layer-check update |
| 33 | `mutation-scoring` | Terra | `LineStatus.mutation`, status schema (line and inputs), web status reader, `ScoreInputs.mutation_digest`, pipeline ingestion |
| 34 | `mutation-cli` | Terra | `mutate`, `signoff`, `score --mutation` |
| 35 | `mutation-web` | Luna | spec page badges, roll-up columns and JSON |
| 36 | `mutation-dogfood` | Terra | Makefile, optional CI job, README/CHANGELOG, dogfood run notes |

Each PR stays under the 200 KB review cap (measure each actual diff). Stack order 31, 32, 33, then 34 and 35 in parallel, then 36.
