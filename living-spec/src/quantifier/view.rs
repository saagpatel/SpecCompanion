//! Proof-free renderers for quantified coverage reports.
use super::{annotations::Kind, Error, Report, Result};
use serde::Serialize;

/// Reject terms reserved by the living-spec contract in any rendered output.
pub fn assert_proof_free(text: &str) -> Result<()> {
    crate::core::views::assert_quantifier_proof_free(text)
        .map_err(|error| Error::new(error.to_string()))
}

/// Serialize user-facing JSON only after checking the complete serialized form.
pub fn json<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    guard_strings(&value)?;
    let text = serde_json::to_string_pretty(&value)?;
    assert_proof_free(&text)?;
    Ok(text + "\n")
}

pub fn markdown(reports: &[Report]) -> Result<String> {
    for report in reports {
        guard_report(report)?;
    }
    let mut out = String::from("# Quantifier Map\n\n");
    for report in reports {
        out.push_str(&markdown_report(report)?);
        out.push_str("\n---\n\n");
    }
    assert_proof_free(&out)?;
    Ok(out)
}

fn markdown_report(report: &Report) -> Result<String> {
    let domain = serde_json::to_string(&report.quantifier.domain)?;
    let predicate = serde_json::to_string(&report.quantifier.predicate)?;
    let mut out = format!(
        "# Quantifier Map: {}\n\n{}\n\n## Tested here\n\n- Coverage: {}\n- Tested cases: {}\n- Distinct tested: {}\n",
        md(&report.id),
        md(&report.text),
        match report.coverage.mode {
            super::Mode::Exhaustive => format!(
                "checked across all {} cases in the declared finite domain",
                report.coverage.tested_cases
            ),
            super::Mode::Sampled => "sampled".to_owned(),
        },
        report.coverage.tested_cases,
        report.coverage.distinct_tested,
    );
    if let Some(total) = report.coverage.finite_total {
        out.push_str(&format!("- Declared finite domain: {total} cases\n"));
    }
    if let Some(seed) = report.coverage.seed {
        out.push_str(&format!("- Sampling seed: {seed}\n"));
    }
    out.push_str("\n| Input | Output | Predicate result |\n| --- | --- | --- |\n");
    for observation in &report.observations {
        let input = serde_json::to_string(&observation.input)?;
        let output = serde_json::to_string(&observation.output)?;
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            md_table_code(&input),
            md_table_code(&output),
            if observation.satisfied {
                "satisfied"
            } else if report.quantifier.kind == Kind::Existential {
                "non-witness"
            } else {
                "counterexample"
            }
        ));
    }
    if let Some(witness) = &report.witness {
        out.push_str(&format!(
            "\nWitness among tested cases: {} → {}.\n",
            md_code(&serde_json::to_string(&witness.input)?),
            md_code(&serde_json::to_string(&witness.output)?),
        ));
    } else if report.quantifier.kind == Kind::Existential {
        out.push_str("\nNo witness found among tested cases.\n");
    }
    out.push_str(&format!(
        "\nTested result: {}.\n\n## Claimed everywhere\n\nQuantifier: {}. Subject: {}. Declared domain: {}. Predicate AST: {}.\n\nUntested remainder: {}\n",
        md_code(&serde_json::to_string(&report.result)?),
        md_code(&format!("{:?}", report.quantifier.kind).to_lowercase()),
        md_code(&report.quantifier.subject),
        md_code(&domain),
        md_code(&predicate),
        md(&report.coverage.untested_remainder),
    ));
    assert_proof_free(&out)?;
    Ok(out)
}

pub fn html(reports: &[Report]) -> Result<String> {
    for report in reports {
        guard_report(report)?;
    }
    let mut out = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Quantifier Map</title><style>body{font:16px system-ui,sans-serif;max-width:72rem;margin:2rem auto;padding:0 1rem;color:#18212b}main{display:grid;grid-template-columns:1fr 1fr;gap:1rem}section{border:1px solid #aab4bf;border-radius:.5rem;padding:1rem}pre{overflow-wrap:anywhere;white-space:pre-wrap;background:#f3f5f7;padding:.75rem}table{border-collapse:collapse;width:100%}th,td{text-align:left;border-bottom:1px solid #ccd2d8;padding:.45rem}@media(max-width:700px){main{grid-template-columns:1fr}}</style></head><body><h1>Quantifier Map</h1>");
    for report in reports {
        out.push_str(&html_report(report)?);
    }
    out.push_str("</body></html>");
    assert_proof_free(&out)?;
    Ok(out)
}

fn html_report(report: &Report) -> Result<String> {
    crate::core::views::quantifier_html_report(report)
        .map_err(|error| Error::new(error.to_string()))
}

fn md(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\n' | '\r') {
            escaped.push(' ');
        } else {
            if matches!(character, '<' | '>' | '&') {
                escaped.push_str(match character {
                    '<' => "&lt;",
                    '>' => "&gt;",
                    '&' => "&amp;",
                    _ => unreachable!(),
                });
                continue;
            }
            if matches!(
                character,
                '\\' | '|' | '`' | '#' | '-' | '!' | '[' | ']' | '(' | ')' | '*' | '_' | '{' | '}'
            ) {
                escaped.push('\\');
            }
            escaped.push(character);
        }
    }
    escaped
}

/// Render serialized values in a code span without escaping their punctuation.
/// A delimiter longer than any run in the content keeps embedded backticks inert.
pub(super) fn md_code(value: &str) -> String {
    let normalized = value.replace(['\n', '\r'], " ");
    let (longest, _) = normalized.chars().fold((0, 0), |(longest, run), c| {
        if c == '`' {
            let run = run + 1;
            (longest.max(run), run)
        } else {
            (longest, 0)
        }
    });
    let delimiter = "`".repeat(longest + 1);
    format!("{delimiter}{normalized}{delimiter}")
}

/// GFM splits table cells even inside code spans; only pipes need escaping here.
pub(super) fn md_table_code(value: &str) -> String {
    md_code(value).replace('|', "\\|")
}

fn guard_report(report: &Report) -> Result<()> {
    guard_strings(&serde_json::to_value(report)?)
}

fn guard_strings(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::String(text) => assert_proof_free(text),
        serde_json::Value::Array(values) => {
            for value in values {
                guard_strings(value)?;
            }
            Ok(())
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                assert_proof_free(key)?;
                guard_strings(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantifier::{check_claim, load, reference_dir};

    #[test]
    fn coverage_view_no_proof_in_authored_text_or_encoded_strings() {
        let loaded = load(&reference_dir()).unwrap();
        let mut report = check_claim(&loaded.claims[0]).unwrap();
        for phrase in [
            "PROVEN",
            "proof",
            "proves",
            "guaranteed",
            "guarantee",
            "always",
            "verified",
            "correct\nfor all",
            "holds for all",
        ] {
            report.text = phrase.into();
            let reports = std::slice::from_ref(&report);
            assert!(markdown(reports).is_err());
            assert!(html(reports).is_err());
            assert!(json(&reports).is_err());
        }
        report.text = "<script>synthetic_markup()</script>".into();
        let reports = std::slice::from_ref(&report);
        assert!(!markdown(reports).unwrap().contains("<script>"));
        assert!(!html(reports).unwrap().contains("<script>"));
        assert_proof_free("unprovenance proofread alwayson").unwrap();
    }

    #[test]
    fn view_labels_coverage_and_blocks_proof_vocabulary() {
        let loaded = load(&reference_dir()).unwrap();
        let report = check_claim(&loaded.claims[0]).unwrap();
        let markdown = markdown(std::slice::from_ref(&report)).unwrap();
        let html = html(std::slice::from_ref(&report)).unwrap();
        assert!(markdown.contains("## Tested here"));
        assert!(markdown.contains("## Claimed everywhere"));
        assert!(html.contains("<h3 id=\"tested-"));
        assert!(html.contains(">Claimed everywhere</h3>"));
        assert_proof_free(&markdown).unwrap();
        assert_proof_free(&html).unwrap();
        assert!(assert_proof_free("Correct for all cases").is_err());
        assert!(assert_proof_free("correct \n for   all cases").is_err());
        assert!(assert_proof_free("<script>verified</script>").is_err());
        let rejected = json(&serde_json::json!({"requirement": "proof\n for all"})).unwrap_err();
        assert!(!rejected.to_string().contains("proof"));
    }

    #[test]
    fn markdown_code_spans_keep_json_punctuation_literal() {
        let loaded = load(&reference_dir()).unwrap();
        let report = check_claim(&loaded.claims[0]).unwrap();
        let rendered = markdown(std::slice::from_ref(&report)).unwrap();
        let domain = serde_json::to_string(&report.quantifier.domain).unwrap();
        assert!(rendered.contains(&format!("Declared domain: `{domain}`.")));
        assert!(!domain.contains("\\{"));
        assert!(!domain.contains("\\-"));
    }
}
