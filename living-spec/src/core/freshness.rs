//! Byte bindings for saved scores. These checks never compute evidence verdicts.
use super::{
    badge::LineStatus,
    spec::Spec,
    test_identity::TestOutcome,
    views::{BoundaryReport, QuantifierReport},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoreInputs {
    pub digest: String,
    pub tool_version: String,
    /// Relative to the spec directory, independent of the renderer's cwd.
    pub results_file: Option<String>,
    pub results_digest: String,
    pub results_format: Option<String>,
    pub outcome_ids: BTreeMap<String, String>,
    pub requirements: BTreeMap<String, RequirementInputs>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequirementInputs {
    pub digest: String,
    pub revision: String,
    pub outcomes: Vec<TestOutcome>,
    pub test_ids: Vec<String>,
    /// The graph at scoring time, never reconstructed from current annotations.
    pub depends_on: Vec<String>,
    /// Bind the saved receipt too, so editing it cannot silently keep green.
    pub receipt_digest: String,
}

#[derive(Default)]
pub struct Freshness {
    pub stale: bool,
    pub lines: BTreeMap<String, String>,
}

fn hash(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    format!("sha256:{:x}", hash.finalize())
}

fn canonical(path: &Path) -> Result<Value> {
    let bytes = fs::read(path)?;
    super::json_input::reject_duplicate_keys(&bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn line_digest(
    revision: &str,
    annotation: &Value,
    outcomes: &[TestOutcome],
    test_ids: &[String],
    depends_on: &[String],
) -> Result<String> {
    Ok(hash(&[
        revision.as_bytes(),
        &serde_json::to_vec(annotation)?,
        &serde_json::to_vec(outcomes)?,
        &serde_json::to_vec(test_ids)?,
        &serde_json::to_vec(depends_on)?,
        VERSION.as_bytes(),
    ]))
}

fn receipt_digest(
    id: &str,
    statuses: &[LineStatus],
    boundary: &[BoundaryReport],
    quantifier: &[QuantifierReport],
) -> Result<String> {
    Ok(hash(&[&serde_json::to_vec(&(
        statuses
            .iter()
            .filter(|line| line.id == id)
            .collect::<Vec<_>>(),
        boundary
            .iter()
            .filter(|report| report.requirement == id)
            .collect::<Vec<_>>(),
        quantifier
            .iter()
            .filter(|report| report.id == id)
            .collect::<Vec<_>>(),
    ))?]))
}

fn parse_results(bytes: &[u8], format: &str) -> Option<Vec<TestOutcome>> {
    let text = std::str::from_utf8(bytes).ok()?;
    match format {
        "libtest" => super::test_results::parse_libtest(text).ok(),
        "junit" => super::test_results::parse_junit(text).ok(),
        _ => None,
    }
}

fn select_outcomes(outcomes: &[TestOutcome], ids: &[String]) -> Vec<TestOutcome> {
    let mut used: Vec<_> = outcomes
        .iter()
        .filter(|outcome| {
            ids.iter().any(|id| {
                let base = id.split_once('#').map_or(id.as_str(), |(base, _)| base);
                outcome.test_id == *id || outcome.test_id.starts_with(&format!("{base}#"))
            })
        })
        .cloned()
        .collect();
    used.sort_by(|a, b| a.test_id.cmp(&b.test_id));
    used
}

impl ScoreInputs {
    pub fn capture(
        dir: &Path,
        outcomes: &[TestOutcome],
        statuses: &[LineStatus],
        boundary: &[BoundaryReport],
        quantifier: &[QuantifierReport],
        results_source: &str,
    ) -> Result<Self> {
        let spec_bytes = fs::read(dir.join("spec.md"))?;
        let spec = Spec::parse(std::str::from_utf8(&spec_bytes)?)?;
        let annotations = canonical(&dir.join("annotations.json"))?;
        let annotations_bytes = serde_json::to_vec(&annotations)?;
        let (results_file, results_bytes) = match fs::read(results_source) {
            Ok(bytes) => (Some(relative_file(dir, Path::new(results_source))?), bytes),
            // Library callers can score outcomes without a backing file. A
            // later file view cannot attest freshness of such a score.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (None, serde_json::to_vec(outcomes)?)
            }
            Err(error) => return Err(error.into()),
        };
        // Persist JUnit's raw-to-scoped identity mapping. Reading current results
        // must not require source discovery or invoke evidence providers.
        let parsed = ["libtest", "junit"].into_iter().find_map(|format| {
            let parsed = parse_results(&results_bytes, format)?;
            (parsed.len() == outcomes.len()
                && parsed
                    .iter()
                    .zip(outcomes)
                    .all(|(a, b)| a.status == b.status))
            .then_some((format.to_owned(), parsed))
        });
        let (results_format, outcome_ids) = match parsed {
            Some((format, parsed))
                if parsed
                    .iter()
                    .zip(outcomes)
                    .any(|(a, b)| a.test_id != b.test_id) =>
            {
                (
                    Some(format),
                    parsed
                        .iter()
                        .zip(outcomes)
                        .map(|(a, b)| (a.test_id.clone(), b.test_id.clone()))
                        .collect(),
                )
            }
            Some((format, _)) => (Some(format), BTreeMap::new()),
            None => (None, BTreeMap::new()),
        };
        let mut requirements = BTreeMap::new();
        for line in &spec.lines {
            let annotation = &annotations["requirements"][&line.id];
            let test_ids: std::collections::BTreeSet<String> = statuses
                .iter()
                .filter(|status| status.id == line.id)
                .flat_map(|status| status.tests.iter().map(|test| test.test_id.clone()))
                .chain(["links", "boundary_links"].into_iter().flat_map(|key| {
                    annotation[key]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|link| link["test_id"].as_str().map(str::to_owned))
                }))
                .chain(
                    boundary
                        .iter()
                        .filter(|report| report.requirement == line.id)
                        .flat_map(|report| report.bounds.iter())
                        .flat_map(|bound| bound.cases.iter())
                        .filter_map(|case| case.test_id.clone()),
                )
                .collect();
            let test_ids: Vec<_> = test_ids.into_iter().collect();
            let used = select_outcomes(outcomes, &test_ids);
            let depends_on: Vec<_> = annotation["depends_on"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|edge| edge["id"].as_str().map(str::to_owned))
                .collect();
            requirements.insert(
                line.id.clone(),
                RequirementInputs {
                    digest: line_digest(&line.revision, annotation, &used, &test_ids, &depends_on)?,
                    revision: line.revision.clone(),
                    outcomes: used,
                    test_ids,
                    depends_on,
                    receipt_digest: receipt_digest(&line.id, statuses, boundary, quantifier)?,
                },
            );
        }
        Ok(Self {
            digest: hash(&[
                &spec_bytes,
                &annotations_bytes,
                &results_bytes,
                VERSION.as_bytes(),
            ]),
            tool_version: VERSION.into(),
            results_file,
            results_digest: hash(&[&results_bytes]),
            results_format,
            outcome_ids,
            requirements,
        })
    }

    pub fn check(
        &self,
        dir: &Path,
        spec: &Spec,
        statuses: &[LineStatus],
        boundary: &[BoundaryReport],
        quantifier: &[QuantifierReport],
    ) -> Result<Freshness> {
        let annotations = match canonical(&dir.join("annotations.json")) {
            Ok(value) => value,
            Err(_) if !dir.join("annotations.json").exists() => {
                return Ok(stale_all(
                    spec,
                    "annotation input file is unavailable; rescore required",
                ));
            }
            Err(error) => return Err(error),
        };
        let spec_bytes = fs::read(dir.join("spec.md"))?;
        let results = self
            .results_file
            .as_ref()
            .and_then(|path| fs::read(dir.join(path)).ok());
        let results_match = results
            .as_ref()
            .is_some_and(|bytes| hash(&[bytes]) == self.results_digest);
        let current_outcomes = if results_match {
            None
        } else {
            results
                .as_ref()
                .zip(self.results_format.as_ref())
                .and_then(|(bytes, format)| {
                    let mut parsed = parse_results(bytes, format)?;
                    for outcome in &mut parsed {
                        if let Some(id) = self.outcome_ids.get(&outcome.test_id) {
                            outcome.test_id.clone_from(id);
                        } else if !self.outcome_ids.is_empty() {
                            // A newly introduced scoped identity cannot be guessed.
                            return None;
                        }
                    }
                    Some(parsed)
                })
        };
        let version_matches = self.tool_version == VERSION;
        let digest_matches = results.as_ref().is_some_and(|bytes| {
            hash(&[
                &spec_bytes,
                &serde_json::to_vec(&annotations).expect("JSON serializes"),
                bytes,
                VERSION.as_bytes(),
            ]) == self.digest
        });
        let mut freshness = Freshness {
            stale: !digest_matches || !version_matches,
            ..Default::default()
        };
        for line in &spec.lines {
            let reason = match self.requirements.get(&line.id) {
                None => Some("requirement was not present in this score"),
                Some(_) if !version_matches => Some("tool version changed since scoring"),
                Some(_) if !results_match && current_outcomes.is_none() => {
                    Some("test-result file changed or is unavailable since scoring")
                }
                Some(saved) => {
                    if saved.revision != line.revision
                        || saved.digest
                            != line_digest(
                                &line.revision,
                                &annotations["requirements"][&line.id],
                                &current_outcomes.as_ref().map_or_else(
                                    || saved.outcomes.clone(),
                                    |outcomes| select_outcomes(outcomes, &saved.test_ids),
                                ),
                                &saved.test_ids,
                                &saved.depends_on,
                            )?
                    {
                        Some("requirement, annotation, or used test outcomes changed since scoring")
                    } else if saved.receipt_digest
                        != receipt_digest(&line.id, statuses, boundary, quantifier)?
                    {
                        Some("stored score receipt changed since scoring")
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                freshness.lines.insert(line.id.clone(), reason.into());
            }
        }
        // Fixed point handles transitive edges and cycles using only the saved graph.
        loop {
            let mut added = false;
            for (id, saved) in &self.requirements {
                if !freshness.lines.contains_key(id) {
                    if let Some(dependency) = saved.depends_on.iter().find(|dep| {
                        freshness.lines.contains_key(*dep)
                            || !spec.lines.iter().any(|line| &line.id == *dep)
                    }) {
                        freshness
                            .lines
                            .insert(id.clone(), format!("prerequisite {dependency} is stale"));
                        added = true;
                    }
                }
            }
            if !added {
                break;
            }
        }
        freshness.stale |= !freshness.lines.is_empty();
        Ok(freshness)
    }
}

pub fn missing_inputs(spec: &Spec) -> Freshness {
    stale_all(spec, "status has no input digest; rescore required")
}

fn stale_all(spec: &Spec, reason: &str) -> Freshness {
    Freshness {
        stale: true,
        lines: spec
            .lines
            .iter()
            .map(|line| (line.id.clone(), reason.into()))
            .collect(),
    }
}

fn relative_file(base: &Path, file: &Path) -> Result<String> {
    let base = fs::canonicalize(base)?;
    let file = fs::canonicalize(file)?;
    let left: Vec<_> = base.components().collect();
    let right: Vec<_> = file.components().collect();
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    let mut path = std::path::PathBuf::new();
    for _ in common..left.len() {
        path.push("..");
    }
    for component in &right[common..] {
        path.push(component);
    }
    Ok(path.to_string_lossy().into_owned())
}
