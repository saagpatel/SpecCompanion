# Boundary cases

`living-spec boundary gen <spec-dir>` resolves bounds and writes generated cases
to `annotations.json`. `living-spec boundary coverage <spec-dir> --results
<file> --format libtest|junit [--md out.md] [--json out.json]` reports every case.
Exit codes: 0 for complete passing coverage, 3 for any case not `tested_pass`,
1 for malformed input, a unit contradiction, or a requirement needing annotation.
No command executes tests or accesses the network. Results are supplied execution
receipts; these commands do not independently audit assertion relevance or source
freshness. Passing linked tests must assert the case's expected acceptance or
rejection. A rejected input with a passing rejection assertion is `tested_pass`.

## Literal grammar and numbers

The deliberately closed grammar is `Quantity is <clause>`, optionally ending
in a period. Quantity is a nonempty name using letters, spaces, underscores or
hyphens. Clauses are:

- `at most|no more than|up to|within N unit` (inclusive upper bound)
- `at least N unit`, `more than|over N unit`, `fewer than|less than|under N unit`
- `exactly N unit`, `N or fewer unit`
- `between A and B unit inclusive|exclusive` (same flag for both edges)
- `one of a, b, c`, `only X, Y or Z` (closed enumerations)

`N`, `A`, and `B` are signed integer literals. Units are exactly `ms`, `s`,
`min`, `h`, `d`, `B`, `KB`, `MB`, `GB`, `KiB`, `MiB`, `items`, `chars`,
`lines`, `retries`, `files`, `percent` (or `%`). Other phrasings, decimal prose
without an authored step, locale numbers, vague qualifiers, percentiles, dates,
relations, negations, compound units and multiple/disjunctive clauses return
`NeedsAnnotation`. No partial extraction occurs. Unsupported prose can be resolved
with explicit annotations. An explicitly supplied empty bounds list also wins.

Decimals in authored bounds are strings, with at most nine fractional digits;
checked scaled `i128` arithmetic retains the original literal. Overflow and
nonpositive steps fail. Integers have step 1. Time defaults to a 1 ms step and
size to a 1 B step. Percent defaults to 1 percentage point; authored decimal
types must supply their step. Examples use canonical ms or B values so increments
remain exact even for minutes/days or MB/MiB. KB/MB/GB use powers of 1000;
KiB/MiB use powers of 1024. Counts retain their labels; files cannot stand for
characters. Both range edges get all three cases, including repeated values when
the edges are adjacent. An open domain needs annotation of its rejection semantics;
the generator declines to invent invalid members for it.

## Annotation ownership

Each requirement's `bounds` holds typed `{quantity, kind}` objects. Numeric `kind`
contains `type: "numeric"`, `value`, `unit`, `op`, `step: {value, unit}`, and
`integer`. An op is `{type: "lt"|"le"|"gt"|"ge"|"eq"}` or
`{type: "range", lo, hi, inclusive_lo, inclusive_hi}`. Domain kinds contain
`type: "domain"`, `allowed`, and `closed`. Quantity names must be unique within
a requirement so links identify the intended bound.

The list's `bounds_source` is `{ "origin": "authored" }` or
`{ "origin": "inferred", "revision": "sha256:..." }`. Generation records this
metadata, including for empty authored overrides. Inferred bounds are resolved
again from the current requirement text on every generation, coverage, and
provider run. A changed revision or changed inferred bound invalidates all its
case links, even when only the acceptance operator changed. Coverage ignores
those stale links; generation clears them before recording the current revision.
Relink the current cases after reviewing their expected acceptance/rejection.

Existing bounds without provenance are authored unless they also contain legacy
`boundary_cases` output. Nonempty legacy generated lists have ambiguous origin
and fail with `NeedsAnnotation`: explicitly mark their origin before proceeding.
An explicit `bounds: []` remains an authored override through load/save cycles;
an absent field permits inference. `bounds: null` is invalid input.

The boundary capability also owns `boundary_cases` (generated `{quantity, cases}`
entries) and `boundary_links` (authored execution links), using the core's preserved
extension map. Links have `quantity`, `position`, `value` (string), `test_id`,
optional `unit`, and optional `test_body`. Do not edit the core's requirement-level
`links` to add boundary metadata. Other annotation sections and orphan diagnostics
are preserved. The generator prepares all updates before saving; a parse error
does not leave a partial update.

Positions are `below`, `at`, `above`; ranges use `edge-lo-` and `edge-hi-`
prefixes; domains use `allowed-N`, `invalid-variant`, `invalid-unlisted`.
An optional `happy-path` link is recorded but covers no boundary position.

Only authored `unit` metadata or a single typed literal constructor establishes
units: `Duration::from_millis(N)`, `Duration::from_secs(N)`,
`timedelta(seconds=N)`, `timedelta(milliseconds=N)`. This small source adapter
declines bodies with comments, strings or multiple constructors. The constructor's
literal must equal the link value; more complex source needs authored metadata.
Bare numbers, `_ms` names and multiplication do not count. Unknown units leave
cases `untested` with `unit_unknown`. Incompatible dimensions/scales produce
`UnitContradiction` naming requirement and test units. Equivalent converted values
are allowed. A wrong value in the requirement's unit stays untested with
`value_mismatch`. Duplicate case links/outcome IDs and unknown positions fail.

## Coverage and integration

`coverage` returns a `CoverageMap` for one requirement. `BoundaryEvidence` resolves
current spec lines through the same resolver as the CLI, including lines without
an annotation entry or stored bounds. General scoring visits explicit boundary
sections and prose containing boundary-language cues (the supported comparisons,
closed domains, or vague qualifiers such as `fast` and `normally`). Unsupported
clauses with these cues fail as input errors; unrelated prose keeps its ordinary
core evidence gates. `boundary_gaps` supplies the set expected by `score_line`,
which honors gaps even when inferred bounds have not been stored. The legacy
`gaps_for_annotations` helper supports authored bounds only: without a current
spec it cannot safely validate inferred or ambiguous legacy bounds.
A happy-path-only pass leaves all boundary cases untested and the badge
`unproven` with `boundary_gap`. Missing/skipped results remain untested; failed or
errored tests are `tested_fail`. Markdown prints one row per boundary case,
including each missing range edge, with no aggregation.

The draft 2020-12 coverage schema accepts either one map or an array of maps (the
CLI export). Tests validate exported JSON against the actual checked-in schemas
with a minimal in-crate validator for every keyword used, rejecting unsupported
keywords; strict `deny_unknown_fields` deserialization is an additional check.
Result readers are packet-local CLI helpers pending packet 03 integration. They
retain literal case IDs/names rather than inventing framework/package scope;
JUnit `name` must match authored `test_id`, and duplicate names are rejected.
Synthetic fixture outcomes exercise linkage and coverage accounting, not the
behavior of an external application.

Run the packet gates in `living-spec/`:

```sh
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
cargo test --offline boundary_reference_set -- --nocapture
```
