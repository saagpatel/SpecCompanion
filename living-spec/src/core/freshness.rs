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

/// Result bytes and explicit raw-to-scoped identities captured before scoring.
pub struct ResultsSnapshot {
    file: Option<String>,
    bytes: Vec<u8>,
    format: Option<String>,
    outcome_ids: BTreeMap<String, String>,
    outcomes: Vec<TestOutcome>,
}

impl ResultsSnapshot {
    pub fn from_bytes(
        dir: &Path,
        file: &Path,
        bytes: Vec<u8>,
        format: &str,
        outcome_ids: BTreeMap<String, String>,
    ) -> Result<Self> {
        let mut outcomes =
            parse_results(&bytes, format).ok_or("results input is malformed or incomplete")?;
        let raw_ids: std::collections::BTreeSet<_> = outcomes
            .iter()
            .map(|outcome| outcome.test_id.clone())
            .collect();
        if !outcome_ids.is_empty()
            && outcome_ids.keys().collect::<Vec<_>>() != raw_ids.iter().collect::<Vec<_>>()
        {
            return Err("results input identity mapping has missing or extra ids".into());
        }
        for outcome in &mut outcomes {
            if let Some(id) = outcome_ids.get(&outcome.test_id) {
                outcome.test_id.clone_from(id);
            }
        }
        outcome_map(&outcomes)?;
        Ok(Self {
            file: Some(relative_file(dir, file)?),
            bytes,
            format: Some(format.into()),
            outcome_ids,
            outcomes,
        })
    }

    /// Library callers without a backing file remain stale in file views.
    /// Existing files must parse and reconcile exactly with the scored outcomes.
    pub fn read(dir: &Path, source: &str, outcomes: &[TestOutcome]) -> Result<Self> {
        match fs::read(source) {
            Ok(bytes) => {
                let format = ["libtest", "junit"]
                    .into_iter()
                    .find(|format| parse_results(&bytes, format).is_some())
                    .ok_or("results input is malformed or incomplete")?;
                let snapshot =
                    Self::from_bytes(dir, Path::new(source), bytes, format, BTreeMap::new())?;
                snapshot.validate(outcomes)?;
                Ok(snapshot)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                outcome_map(outcomes)?;
                Ok(Self {
                    file: None,
                    bytes: serde_json::to_vec(outcomes)?,
                    format: None,
                    outcome_ids: BTreeMap::new(),
                    outcomes: outcomes.to_vec(),
                })
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn validate(&self, outcomes: &[TestOutcome]) -> Result<()> {
        if outcome_map(&self.outcomes)? != outcome_map(outcomes)? {
            return Err("results input does not match the scored outcome set".into());
        }
        Ok(())
    }
}

fn outcome_map(outcomes: &[TestOutcome]) -> Result<BTreeMap<&str, &TestOutcome>> {
    let mut mapped = BTreeMap::new();
    for outcome in outcomes {
        if outcome.test_id.trim().is_empty()
            || mapped.insert(outcome.test_id.as_str(), outcome).is_some()
        {
            return Err("results input has an empty or duplicate outcome identity".into());
        }
    }
    Ok(mapped)
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_badge: Option<super::badge::Badge>,
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
        spec_bytes: &[u8],
        annotations: &Value,
        results: &ResultsSnapshot,
        outcomes: &[TestOutcome],
        statuses: &[LineStatus],
        boundary: &[BoundaryReport],
        quantifier: &[QuantifierReport],
    ) -> Result<Self> {
        results.validate(outcomes)?;
        let spec = Spec::parse(std::str::from_utf8(spec_bytes)?)?;
        let annotations_bytes = serde_json::to_vec(annotations)?;
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
                    own_badge: statuses
                        .iter()
                        .find(|status| status.id == line.id)
                        .map(|status| {
                            if status.badge == super::badge::Badge::Qualified {
                                super::badge::Badge::Proven
                            } else {
                                status.badge
                            }
                        }),
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
                spec_bytes,
                &annotations_bytes,
                &results.bytes,
                VERSION.as_bytes(),
            ]),
            tool_version: VERSION.into(),
            results_file: results.file.clone(),
            results_digest: hash(&[&results.bytes]),
            results_format: results.format.clone(),
            outcome_ids: results.outcome_ids.clone(),
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
                    outcome_map(&parsed).ok()?;
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
    let cwd = std::env::current_dir()?;
    let base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        cwd.join(base)
    };
    let file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };
    let left: Vec<_> = base.components().collect();
    let right: Vec<_> = file.components().collect();
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    // Walking back through a symlink (or a lexical `..`) may not reach the
    // shared prefix. Keep the absolute caller path in that case.
    let mut prefix = std::path::PathBuf::new();
    for component in &left[..common] {
        prefix.push(component);
    }
    for component in &left[common..] {
        prefix.push(component);
        if !matches!(component, std::path::Component::Normal(_))
            || fs::symlink_metadata(&prefix).map_or(true, |m| m.file_type().is_symlink())
        {
            return Ok(file.to_string_lossy().into_owned());
        }
    }
    if common == 0 {
        return Ok(file.to_string_lossy().into_owned());
    }
    let mut path = std::path::PathBuf::new();
    for _ in common..left.len() {
        path.push("..");
    }
    for component in &right[common..] {
        path.push(component);
    }
    Ok(path.to_string_lossy().into_owned())
}
