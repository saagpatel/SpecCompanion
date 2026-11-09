//! Export observed counterexamples without extending them beyond tested inputs.
use super::super::{annotations::Kind, view, Domain, Observation, Report, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CounterexampleDocument {
    schema: String,
    requirements: Vec<CounterexampleReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CounterexampleReport {
    id: String,
    text: String,
    quantifier: super::super::annotations::Quantifier,
    result: super::super::Outcome,
    coverage: super::super::Coverage,
    evidence_role: EvidenceRole,
    observations: Vec<Observation>,
    failing_subdomains: Vec<Domain>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum EvidenceRole {
    Counterexamples,
    NonWitnesses,
}

pub fn write(reports: &[Report], out_dir: &Path) -> Result<()> {
    let entries: Vec<_> = reports
        .iter()
        .filter_map(|report| {
            let observations: Vec<_> = report
                .observations
                .iter()
                .filter(|observation| !observation.satisfied)
                .cloned()
                .collect();
            let existential = report.quantifier.kind == Kind::Existential;
            let failing_subdomains = if existential {
                Vec::new()
            } else {
                report.failing_subdomains.clone()
            };
            if observations.is_empty() && failing_subdomains.is_empty() {
                return None;
            }
            Some(CounterexampleReport {
                id: report.id.clone(),
                text: report.text.clone(),
                quantifier: report.quantifier.clone(),
                result: report.result,
                coverage: report.coverage.clone(),
                evidence_role: if existential {
                    EvidenceRole::NonWitnesses
                } else {
                    EvidenceRole::Counterexamples
                },
                observations,
                failing_subdomains,
            })
        })
        .collect();
    let document = CounterexampleDocument {
        schema: "living-spec.quantifier-counterexamples/v1".to_owned(),
        requirements: entries,
    };
    let json = view::json(&document)?;
    let markdown = markdown(&document)?;
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("counterexamples.json"), json)?;
    fs::write(out_dir.join("counterexamples.md"), markdown)?;
    Ok(())
}

fn markdown(document: &CounterexampleDocument) -> Result<String> {
    let mut out = String::from(
        "# Quantifier counterexamples\n\nEach listed input is an observed case only. No untested region is inferred from it. Existential false observations are non-witnesses; they do not refute existential claims.\n\n",
    );
    for report in &document.requirements {
        out.push_str(&format!(
            "## {}\n\n{}\n\nEvidence role: {}. Result: {}.\n\n",
            md(&report.id),
            md(&report.text),
            match report.evidence_role {
                EvidenceRole::Counterexamples => "counterexamples in tested cases",
                EvidenceRole::NonWitnesses => "non-witnesses among tested cases",
            },
            view::md_code(&serde_json::to_string(&report.result)?),
        ));
        out.push_str("| Input | Output | Predicate result |\n| --- | --- | --- |\n");
        for observation in &report.observations {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                view::md_table_code(&serde_json::to_string(&observation.input)?),
                view::md_table_code(&serde_json::to_string(&observation.output)?),
                match report.evidence_role {
                    EvidenceRole::Counterexamples => "counterexample",
                    EvidenceRole::NonWitnesses => "non-witness",
                },
            ));
        }
        if !report.failing_subdomains.is_empty() {
            out.push_str("\nFailing sub-domains observed:\n\n");
            for domain in &report.failing_subdomains {
                out.push_str(&format!(
                    "- {}\n",
                    view::md_code(&serde_json::to_string(domain)?)
                ));
            }
        }
        out.push('\n');
    }
    view::assert_proof_free(&out)?;
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantifier::{check_claim, load, reference_dir};

    #[test]
    fn planted_failure_exports_observation_and_subdomain() {
        let loaded = load(&reference_dir()).unwrap();
        let report = loaded
            .claims
            .iter()
            .map(check_claim)
            .collect::<Result<Vec<_>>>()
            .unwrap()
            .into_iter()
            .find(|report| !report.failing_subdomains.is_empty())
            .expect("reference set contains a planted failure");
        let tmp =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        write(std::slice::from_ref(&report), tmp.path()).unwrap();
        let json = fs::read_to_string(tmp.path().join("counterexamples.json")).unwrap();
        let md = fs::read_to_string(tmp.path().join("counterexamples.md")).unwrap();
        assert!(json.contains("failing_subdomains"));
        assert!(json.contains("counterexamples"));
        assert!(md.contains("Failing sub-domains observed"));
        view::assert_proof_free(&json).unwrap();
        view::assert_proof_free(&md).unwrap();
    }

    #[test]
    fn existential_false_observations_are_not_refutations() {
        let loaded = load(&reference_dir()).unwrap();
        let Some(report) = loaded
            .claims
            .iter()
            .filter(|claim| claim.quantifier.kind == Kind::Existential)
            .map(check_claim)
            .collect::<Result<Vec<_>>>()
            .unwrap()
            .into_iter()
            .find(|report| {
                report
                    .observations
                    .iter()
                    .any(|observation| !observation.satisfied)
            })
        else {
            return;
        };
        let tmp =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        write(&[report], tmp.path()).unwrap();
        let json = fs::read_to_string(tmp.path().join("counterexamples.json")).unwrap();
        assert!(json.contains("non-witnesses"));
    }

    #[test]
    fn rejected_language_does_not_write_partial_exports() {
        let loaded = load(&reference_dir()).unwrap();
        let mut report = loaded
            .claims
            .iter()
            .map(check_claim)
            .collect::<Result<Vec<_>>>()
            .unwrap()
            .into_iter()
            .find(|report| !report.failing_subdomains.is_empty())
            .expect("reference includes a planted failure");
        report.text = "This is always true".to_owned();
        let tmp =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        assert!(write(&[report], tmp.path()).is_err());
        assert!(!tmp.path().join("counterexamples.json").exists());
        assert!(!tmp.path().join("counterexamples.md").exists());
    }
}
