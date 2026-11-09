# Quantifier Map

Quantifier annotations live in the existing `annotations.json` requirement entry. A claim has a `kind`, a typed `domain`, an allowlisted compiled `subject`, and a versioned JSON `predicate`. Infinite-universal claims also require a deterministic seed and requested sample count.

```json
{
  "kind": "finite-universal",
  "domain": { "type": "int-range", "lo": -5, "hi": 5 },
  "subject": "abs_i8",
  "predicate": {
    "version": 1,
    "expression": {
      "op": "ge",
      "left": { "kind": "field", "source": "output" },
      "right": { "kind": "literal", "value": { "type": "int", "value": 0 } }
    }
  }
}
```

Domains are inclusive integer ranges, typed enumerations, products, or unbounded bases (`int`, `string`, `bytes`, and `vec-int`) with a size hint. A product is finite when every axis is finite; it is unbounded when any axis is unbounded. Empty, malformed, type-incompatible, or over-cap work is rejected explicitly. Finite-universal checks enumerate their entire domain up to 1,000,000 cases; they never switch to sampling. Existential checks search a finite domain exhaustively or search the requested samples for an example. A sampled search that finds none reports that no witness was found among the tested cases. For an unbounded claim, the size hint bounds random string, byte-array, and vector draws; mandatory edge values can exceed that hint. The hint does not bound the claim's domain.

Predicate version 1 is a closed AST: typed literals, input and output fields (with tuple paths), comparisons, membership, and, or, and not. The interpreter enforces depth 16, 1,024 AST nodes, and a 10,000,000-unit total work budget. It rejects unknown operations and catalog subjects. The catalog runs compiled pure Rust fixture subjects and records their observations before evaluating the independently supplied predicate. No expression is evaluated as code, and the checker cannot invoke a shell or load plugins.

An embedding project can implement the `ClaimChecker` trait for a reviewed, compiled checker and pass its observations to the same predicate interpreter. This is a source-level integration point: checker implementations are compiled with the application and selected by an explicit allowlist. The annotation cannot name a crate, command, dynamic library, or arbitrary function.

The Markdown, HTML, and JSON coverage renderers check authored text and all output text against the contract's forbidden proof vocabulary before returning it. User-authored requirement text is subject to the same check, so a rejected phrase cannot leak into an export. The edition JSON preserves the structured claim and execution coverage; its human-readable edition uses the same guarded renderer.

Run the standalone CLI from `living-spec/`:

```sh
cargo run --offline -- quantifier count
cargo run --offline -- quantifier check tests/fixtures/quantifier/specs
cargo run --offline -- quantifier view tests/fixtures/quantifier/specs --out target/quantifier-view.html
cargo run --offline -- quantifier edition tests/fixtures/quantifier/specs --out-dir target/edition
cargo run --offline -- quantifier counterexample tests/fixtures/quantifier/specs --out-dir target/counterexamples
```

The annotation schema at `../contracts/living-spec-annotations-v1.schema.json` defines the input shape. The edition output is defined by `../contracts/living-spec-quantifier-edition-v1.schema.json`.

`samples` counts the complete execution budget, including mandatory edge cases; a count too small for those edges is rejected. Integer observations use signed 64-bit values. An unbounded annotation describes the claim, while generated observations cover only representable values and the stated draw sizes. Every other value remains untested.

The edition's `failing_subdomains` describe predicate-failing tested inputs only. For existential claims these inputs are non-witnesses, even when another input supplies a witness. Counterexample exports label such observations separately and do not present them as regions refuting the existential claim. Product regions may overlap to retain maximal per-axis regions without filling holes between observed failures.

`check` reports counterexamples as structured data and exits successfully after a completed check. Malformed input, unsupported claims, exceeded limits, or file errors exit nonzero. `--finite-only` and `--sampled-only` select execution modes. They do not change an annotation's quantifier or convert a finite domain into samples.

Quantifier loading rejects duplicate object keys in the original JSON bytes, for both shared annotations and standalone fixtures. This includes duplicate requirement IDs, nested keys, and unknown sections: ambiguous input is an input error, with no value selected.

Readable exports preserve serialized observation and subdomain JSON as literal code spans with delimiters longer than embedded backtick runs. Observation tables escape pipes for GitHub-flavored Markdown so input, output, and predicate result remain separate cells. Prose escaping does not apply inside these evidence spans.
