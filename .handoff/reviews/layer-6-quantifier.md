CHANGES

## 1. major — Duplicate annotation keys silently select a passing subject

Location: `living-spec/src/quantifier/mod.rs:76-77` (before `Quantifier::from_section` at line 93).

Failing input: a spec directory containing this `spec.md`:

```markdown
- [R-001] Synthetic absolute-value claim.
```

and this `annotations.json` (the duplicate `subject` key is intentional):

```json
{
  "schema": "living-spec.annotations/v1",
  "requirements": {
    "R-001": {
      "quantifier": {
        "kind": "finite-universal",
        "domain": { "type": "int-range", "lo": -128, "hi": 127 },
        "subject": "abs_wrapping_i8",
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
    }
  }
}
```

Observed: `living-spec quantifier check <spec-directory>` exits 0, selects `abs_i8`, and reports `no-counterexample-in-tested-cases`. The first subject would produce a counterexample at -128. Parsing through the untyped quantifier section discards the first key before strict typed validation can detect it. Duplicate requirement-map keys can similarly discard an entire claim. This resolves ambiguous authored input by guessing and can hide a failure. The standalone fixture path rejects duplicate struct fields, so the two loading paths also disagree.

Suggested fix: reject duplicate JSON object keys while reading the original annotation bytes, before constructing `serde_json::Value` or requirement maps. Include nested quantifier keys and duplicate requirement IDs in that validation; report invalid input rather than choosing either value.

## 2. minor — Pipes in observed strings can turn a failed table row into “satisfied”

Location: `living-spec/src/quantifier/view.rs:60-63`, using `md_code` at lines 142-153.

Failing input: this standalone fixture:

```json
{
  "id": "R-002",
  "text": "Synthetic string fidelity claim.",
  "quantifier": {
    "kind": "finite-universal",
    "domain": {
      "type": "enum",
      "values": [{ "type": "string", "value": "x|y|satisfied|z" }]
    },
    "subject": "utf8_roundtrip_buggy",
    "predicate": {
      "version": 1,
      "expression": {
        "op": "eq",
        "left": { "kind": "field", "source": "output" },
        "right": { "kind": "field", "source": "input" }
      }
    }
  }
}
```

Observed: `living-spec quantifier view <fixture.json> --out <view.md>` succeeds and emits:

```markdown
| `{"type":"string","value":"x|y|satisfied|z"}` | `{"type":"string","value":""}` | counterexample |
```

In a GitHub-flavored Markdown table, unescaped pipes split cells even inside code spans. The three retained cells therefore contain the input prefix, `y`, and `satisfied`; the actual output and `counterexample` cells are discarded. The summary still reports a counterexample, but its only concrete failed observation displays a passing predicate result. The Markdown edition inherits the same defect through `view::markdown`.

Suggested fix: use a table-aware code renderer that escapes pipes for GFM while preserving the displayed value, or render observations in escaped HTML tables or separate code blocks. Keep dynamic backtick delimiters and verify a failed string containing pipes retains its exact input, output, and failure label in the rendered table.

## 3. minor — Counterexample Markdown changes the concrete evidence

Location: `living-spec/src/quantifier/export/counterexample.rs:98-100` and `109-110`.

Failing input: the existing `living-spec/tests/fixtures/quantifier/specs/Q-011.json`, exported with `living-spec quantifier counterexample <fixture.json> --out-dir <output-directory>`.

Observed: export succeeds, but `counterexamples.md` contains:

```markdown
| `\{"type":"int","value":\-128\}` | `\{"type":"int","value":\-128\}` | counterexample |
- `\{"type":"int\-range","lo":\-128,"hi":\-128\}`
```

Backslash escapes are literal inside Markdown code spans. Consequently the readable export shows `\-128` and `int\-range`, rather than the executed value -128 and the actual domain type, and its displayed snippets are invalid JSON. String evidence is also changed (`&` becomes literal `&amp;`), and embedded backticks can terminate the fixed one-backtick delimiter. The JSON export remains accurate; the readable counterexample artifact does not.

Suggested fix: serialize evidence without prose escaping inside code spans, choose a delimiter longer than any embedded backtick run, and handle table pipes separately. Apply the same faithful rendering to failing subdomains. Check Q-011 plus strings containing ampersands, backslashes, and backticks against the corresponding JSON evidence.
