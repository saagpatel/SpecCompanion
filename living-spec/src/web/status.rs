use crate::core::{
    badge::{Badge, LineStatus, Reason, TestEvidence},
    test_identity::TestStatus,
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Deserialize)]
struct RawLine {
    id: String,
    badge: Badge,
    #[serde(default)]
    reasons: Vec<Reason>,
    #[serde(default)]
    tests: Vec<RawTest>,
}

#[derive(Deserialize)]
struct RawTest {
    test_id: String,
    status: TestStatus,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    admissible: bool,
}

impl From<RawLine> for LineStatus {
    fn from(raw: RawLine) -> Self {
        Self {
            id: raw.id,
            badge: raw.badge,
            reasons: raw.reasons,
            tests: raw
                .tests
                .into_iter()
                .map(|test| TestEvidence {
                    test_id: test.test_id,
                    status: test.status,
                    duration_ms: test.duration_ms,
                    admissible: test.admissible,
                })
                .collect(),
        }
    }
}

/// Read a status report. Accepts a bare array or an object with `lines` or `statuses`.
/// Both aliases together are invalid, even if identical. Unknown fields are
/// ignored. Missing `reasons`, `tests`, and `duration_ms` are empty.
pub fn load_status(path: &Path) -> Result<Vec<LineStatus>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    parse_status(&text).map_err(|error| format!("{}: {error}", path.display()).into())
}

pub(crate) fn load_quantifier_reports(
    path: &Path,
) -> Result<Vec<crate::core::views::QuantifierReport>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value = parse_json(&text)?;
    let Some(reports) = value.get("quantifier_reports") else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_value(reports.clone())?)
}

pub(crate) fn load_boundary_reports(
    path: &Path,
) -> Result<Vec<crate::core::views::BoundaryReport>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value = parse_json(&text)?;
    let Some(reports) = value.get("boundary_reports") else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_value(reports.clone())?)
}

pub(crate) fn load_freshness(
    dir: &Path,
    spec: &crate::core::spec::Spec,
    statuses: &[LineStatus],
    boundary: &[crate::core::views::BoundaryReport],
    quantifier: &[crate::core::views::QuantifierReport],
) -> Result<crate::core::freshness::Freshness, Box<dyn std::error::Error>> {
    let value = parse_json(&fs::read_to_string(dir.join("status.json"))?)?;
    match value.get("inputs") {
        Some(value) => {
            let inputs: crate::core::freshness::ScoreInputs =
                serde_json::from_value(value.clone())?;
            inputs.check(dir, spec, statuses, boundary, quantifier)
        }
        None => Ok(crate::core::freshness::missing_inputs(spec)),
    }
}

fn parse_status(text: &str) -> Result<Vec<LineStatus>, String> {
    let value = parse_json(text)?;
    let array = status_array(&value)?;
    let mut lines = Vec::with_capacity(array.len());
    let mut seen = BTreeSet::new();
    for (index, item) in array.iter().enumerate() {
        let raw: RawLine = serde_json::from_value(item.clone())
            .map_err(|error| format!("status.json entry {index}: {error}"))?;
        if !seen.insert(raw.id.clone()) {
            return Err(format!("duplicate id {}", raw.id));
        }
        lines.push(raw.into());
    }
    Ok(lines)
}

fn status_array(value: &Value) -> Result<&Vec<Value>, String> {
    if let Some(array) = value.as_array() {
        return Ok(array);
    }
    let object = value
        .as_object()
        .ok_or("status.json must be a JSON object or array")?;
    if object.contains_key("lines") && object.contains_key("statuses") {
        return Err("status.json must not contain both lines and statuses".to_owned());
    }
    if let Some(lines) = object.get("lines") {
        return lines
            .as_array()
            .ok_or_else(|| "status.json field lines must be an array".to_owned());
    }
    if let Some(lines) = object.get("statuses") {
        return lines
            .as_array()
            .ok_or_else(|| "status.json field statuses must be an array".to_owned());
    }
    Err("status.json is missing lines".to_owned())
}

fn parse_json(text: &str) -> Result<Value, String> {
    crate::core::json_input::reject_duplicate_keys(text.as_bytes())
        .map_err(|error| format!("invalid status.json: {error}"))?;
    serde_json::from_str(text).map_err(|error| format!("invalid status.json: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drill2_all_status_report_loaders_reject_nested_duplicate_keys() {
        let dir = tempfile::tempdir_in("target").unwrap();
        let path = dir.path().join("status.json");
        for text in [
            r#"{"statuses":[],"statuses":[],"boundary_reports":[],"quantifier_reports":[]}"#,
            r#"{"statuses":[],"boundary_reports":[],"quantifier_reports":[],"future":[{"status":"failed","status":"passed"}]}"#,
        ] {
            fs::write(&path, text).unwrap();
            assert!(load_status(&path).is_err());
            assert!(load_boundary_reports(&path).is_err());
            assert!(load_quantifier_reports(&path).is_err());
        }
    }
}
