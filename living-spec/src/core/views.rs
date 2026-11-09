//! Plain status and rendering models shared by core, web, and capability layers.
use regex::Regex;
use std::{fmt, sync::LazyLock};

static FORBIDDEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(proven|proof|proves|guaranteed|guarantee|always|verified|correct\s+for\s+all|holds\s+for\s+all)\b")
        .expect("constant regex is valid")
});

#[derive(Debug)]
pub struct QuantifierViewError(String);
impl fmt::Display for QuantifierViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for QuantifierViewError {}

pub type BoundaryReport = crate::core::boundary::CoverageMap;
pub type QuantifierReport = crate::core::quantifier::Report;

/// Render one article using the same typed models and checks as the standalone view.
pub fn quantifier_html_report(report: &QuantifierReport) -> Result<String, QuantifierViewError> {
    let serialized = serde_json::to_value(report).map_err(view_error)?;
    guard_quantifier_value(&serialized)?;
    use crate::core::quantifier::{Kind, Mode};
    let domain = serde_json::to_string_pretty(&report.quantifier.domain).map_err(view_error)?;
    let predicate =
        serde_json::to_string_pretty(&report.quantifier.predicate).map_err(view_error)?;
    let mut out = format!(
        "<article><h2>{}</h2><p>{}</p><main><section aria-labelledby=\"tested-{}\"><h3 id=\"tested-{}\">Tested here</h3><p>Coverage: {}. Tested cases: {}. Distinct tested: {}.</p>",
        html_escape(&report.id), html_escape(&report.text), html_escape(&report.id),
        html_escape(&report.id),
        match report.coverage.mode {
            Mode::Exhaustive => format!("checked across all {} cases in the declared finite domain", report.coverage.tested_cases),
            Mode::Sampled => "sampled".to_owned(),
        }, report.coverage.tested_cases, report.coverage.distinct_tested,
    );
    if let Some(total) = report.coverage.finite_total {
        out.push_str(&format!("<p>Declared finite domain: {total} cases.</p>"));
    }
    if let Some(seed) = report.coverage.seed {
        out.push_str(&format!("<p>Sampling seed: {seed}.</p>"));
    }
    out.push_str("<table><thead><tr><th>Input</th><th>Output</th><th>Predicate result</th></tr></thead><tbody>");
    for observation in &report.observations {
        out.push_str(&format!(
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            html_escape(&serde_json::to_string(&observation.input).map_err(view_error)?),
            html_escape(&serde_json::to_string(&observation.output).map_err(view_error)?),
            if observation.satisfied {
                "satisfied"
            } else if report.quantifier.kind == Kind::Existential {
                "non-witness"
            } else {
                "counterexample"
            },
        ));
    }
    out.push_str("</tbody></table>");
    if let Some(witness) = &report.witness {
        out.push_str(&format!(
            "<p>Witness among tested cases: <code>{}</code> → <code>{}</code>.</p>",
            html_escape(&serde_json::to_string(&witness.input).map_err(view_error)?),
            html_escape(&serde_json::to_string(&witness.output).map_err(view_error)?),
        ));
    } else if report.quantifier.kind == Kind::Existential {
        out.push_str("<p>No witness found among tested cases.</p>");
    }
    out.push_str(&format!(
        "<p>Tested result: <code>{}</code></p>",
        html_escape(&serde_json::to_string(&report.result).map_err(view_error)?)
    ));
    out.push_str(&format!(
        "</section><section aria-labelledby=\"claimed-{}\"><h3 id=\"claimed-{}\">Claimed everywhere</h3><p>Quantifier: <code>{}</code></p><p>Subject: <code>{}</code></p><p>Untested remainder: {}</p><h4>Declared domain</h4><pre>{}</pre><h4>Predicate AST</h4><pre>{}</pre></section></main></article>",
        html_escape(&report.id), html_escape(&report.id),
        html_escape(&format!("{:?}", report.quantifier.kind).to_lowercase()),
        html_escape(&report.quantifier.subject), html_escape(&report.coverage.untested_remainder),
        html_escape(&domain), html_escape(&predicate),
    ));
    assert_quantifier_proof_free(&out)?;
    Ok(out)
}

pub fn assert_quantifier_proof_free(text: &str) -> Result<(), QuantifierViewError> {
    if FORBIDDEN.is_match(text) {
        return Err(QuantifierViewError(
            "render rejected: unsupported certainty language in output".into(),
        ));
    }
    Ok(())
}

fn guard_quantifier_value(value: &serde_json::Value) -> Result<(), QuantifierViewError> {
    match value {
        serde_json::Value::String(text) => assert_quantifier_proof_free(text),
        serde_json::Value::Array(values) => values.iter().try_for_each(guard_quantifier_value),
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                assert_quantifier_proof_free(key)?;
                guard_quantifier_value(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn view_error(error: serde_json::Error) -> QuantifierViewError {
    QuantifierViewError(error.to_string())
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
