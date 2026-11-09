# Packet 07: Quantifier Map — domain and quantifier annotations, finite/sampled checkers, proof-free coverage view, edition and counterexample export

Read `.handoff/packets/00-architecture.md` (binding) and `.handoff/stress-test-findings.md` (section 5 recommends how predicates execute offline; follow it unless it conflicts with this packet). Build on `src/core/`; additive helpers only.

## Goal
Each requirement can declare the domain it ranges over and a quantifier kind — `existential` (holds for some case), `finite-universal` (every case in a finite set, checked exhaustively), `infinite-universal` (an unbounded domain, only ever sampled). The Quantifier Map shows exactly how much of a quantified claim was tested, kept separate from logical proof, and never reads as a proof.

## Scope
1. `src/quantifier/annotations.rs`: typed view over the annotation layer's `quantifier` section: `{ kind, domain: Domain, predicate: PredicateRef, sampling?: { seed: u64, samples: u32 } }`. `Domain`: `IntRange{lo,hi}` (finite), `Enum{values}`, `Product(Vec<Domain>)` (finite if all parts finite), `Unbounded{ base: Int|String|Bytes|VecInt, size_hint }`. Validation: `finite-universal` requires a finite domain (error otherwise, with a note that it should be `infinite-universal`); a declared finite domain above a cap (default 1,000,000 cases) is an error, not silently sampled. Tighten `$defs/quantifier` in the annotations schema.
2. Predicate execution (offline, no arbitrary code): a built-in, versioned catalog of named predicates in `src/quantifier/catalog.rs` (pure Rust fns over the domain values, e.g. `abs_nonnegative_i8`, `clamp_within`, `saturating_add_monotone`, `utf8_roundtrip`, `sort_idempotent`, `dedup_no_adjacent_equal`, plus deliberately buggy variants with planted failing sub-domains such as `abs_nonnegative_wrapping_i8` failing at -128, `percent_within_0_100_buggy` failing above 100). `PredicateRef` names one catalog entry. Unknown names are an error. Document how a real project would plug in its own checker (trait `ClaimChecker`).
3. `src/quantifier/check/finite.rs`: enumerate the whole finite domain, record every failing case, collapse failing cases into maximal failing sub-domains (contiguous int intervals, enum subsets, per-axis for products). `src/quantifier/check/sampled.rs`: deterministic seeded sampling (implement a small PRNG in-crate, e.g. SplitMix64; no `rand` dep), includes edge values (min, max, 0, empty) before random draws; records `tested_cases`, `distinct_tested`, and the untested remainder described honestly ("unbounded; N distinct values sampled; all other values untested"). Existential: search finite domain or samples for a witness; report witness or "no witness found among tested cases".
4. `src/quantifier/view.rs`: coverage view (Markdown and self-contained HTML) per requirement showing **"Tested here"** (domain portion actually executed, counts, sampled seed) visibly separated from **"Claimed everywhere"** (the full quantified claim and the untested remainder). Forbidden proof language from the contract must never appear (enforce with a `assert_proof_free(&str)` checked on every render, and a test). Wording uses "tested", "checked across all N cases in the declared finite domain", "sampled", "untested remainder", "counterexample".
5. `src/quantifier/export/edition.rs`: readable verification edition (Markdown + JSON, `contracts/living-spec-quantifier-edition-v1.schema.json`) summarizing each requirement's domain, quantifier, coverage, result; includes a source manifest (catalog version, seeds, fixture hashes). `export/counterexample.rs`: for any claim failing in part of its domain, export JSON + Markdown with the concrete counterexample(s) and the failing sub-domain(s).
6. **Reference set:** `living-spec/tests/fixtures/quantifier/specs/` with exactly **30** original synthetic specs (one dir or JSON each with requirement text + quantifier annotation + expected results): ≥10 finite-universal, ≥10 infinite-universal sampled, ≥4 existential, ≥6 with planted failing sub-domains (mixed kinds).
7. CLI `living-spec quantifier <subcmd>`: `check <spec-or-fixture> [--finite-only|--sampled-only]`, `view <…> --out`, `edition <…> --out-dir`, `counterexample <…> --out-dir`, `count` (prints the number of loaded reference specs).

## Out of scope
Generating hidden acceptance exams, changing requirements automatically, declaring universal correctness from samples, LLMs.

## Acceptance (mirrors the pick's work-packet done checks)
```sh
cd living-spec && cargo fmt -- --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo test --offline quantifier::annotations   # each quantifier kind parses
cargo test --offline finite_exhaustive         # whole finite domain checked; planted in-domain failure caught
cargo test --offline sampled_coverage          # sampled coverage + untested remainder reported
cargo test --offline quantifier_specs_count    # 30 specs, each loads annotated
cargo test --offline coverage_view_no_proof    # tested vs claimed distinguished; no proof terms
cargo test --offline export_edition            # edition emitted and structure-validated for all 30
cargo test --offline counterexample_export     # planted failure exports with its sub-domain
```

## Contract revisions that apply
R7 replaces scope item 2's 'named predicate functions'. Claims are a closed, versioned JSON predicate AST (typed literals, `input`/`output` field refs, comparisons, membership, and/or/not, with depth and work caps). They are evaluated over observations from an allowlisted catalog of compiled pure fixture **subjects** (e.g. `clamp`, `abs_i8`, `percent_of`, `utf8_roundtrip`, `sort_vec`, plus faulty variants with planted failing sub-domains). Each spec names a subject and an independently authored predicate. No eval, shell or plugins. Reject unsupported AST nodes, empty or invalid domains, and over-cap work explicitly. The forbidden-language check applies to all rendered user-facing text.
