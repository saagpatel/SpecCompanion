use super::{
    gen::{generate, Example, Expected},
    model::*,
};
use crate::core::test_identity::{TestOutcome, TestStatus};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseLink {
    pub quantity: String,
    pub position: String,
    pub value: String,
    #[serde(default)]
    pub unit: Option<Unit>,
    pub test_id: String,
    #[serde(default)]
    pub test_body: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    TestedPass,
    TestedFail,
    Untested,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    NoLink,
    UnitUnknown,
    NotExecuted,
    ValueMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoveredCase {
    pub position: String,
    pub value: String,
    pub unit: Option<Unit>,
    pub expected: Expected,
    pub test_id: Option<String>,
    pub status: CaseStatus,
    pub reason: Option<GapReason>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundCoverage {
    pub bound: Bound,
    pub cases: Vec<CoveredCase>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageMap {
    pub requirement: String,
    pub bounds: Vec<BoundCoverage>,
}

/// Only typed constructors count. Suffixes, multiplication and bare numbers do not.
/// Multiple constructors are ambiguous without authored metadata.
fn typed_measurement(source: &str) -> Option<(Unit, Decimal)> {
    // This is a deliberately small source adapter, not a language parser.
    // Decline comments/literals rather than recognizing constructors inside them.
    if ["//", "/*", "#", "\"", "'"]
        .iter()
        .any(|s| source.contains(s))
    {
        return None;
    }
    let re = Regex::new(r"(?:\bDuration::from_(millis|secs)\s*\(\s*([0-9]+)\s*\)|\btimedelta\s*\(\s*(seconds|milliseconds)\s*=\s*([0-9]+)\s*\))").unwrap();
    let units: Vec<_> = re
        .captures_iter(source)
        .map(|c| {
            let unit = if c.get(1).or_else(|| c.get(3)).unwrap().as_str() == "secs"
                || c.get(3).is_some_and(|v| v.as_str() == "seconds")
            {
                Unit::S
            } else {
                Unit::Ms
            };
            Some((
                unit,
                c.get(2)
                    .or_else(|| c.get(4))
                    .unwrap()
                    .as_str()
                    .parse()
                    .ok()?,
            ))
        })
        .collect();
    if units.len() == 1 {
        units.into_iter().next().flatten()
    } else {
        None
    }
}

fn link_matches(
    bound: &Bound,
    case: &Example,
    link: &CaseLink,
) -> Result<Result<(), GapReason>, BoundaryError> {
    let BoundKind::Numeric { unit, .. } = &bound.kind else {
        return Ok(if link.value == case.value && link.unit.is_none() {
            Ok(())
        } else {
            Err(GapReason::ValueMismatch)
        });
    };
    let inferred = link.test_body.as_deref().and_then(typed_measurement);
    let known = link.unit.or_else(|| inferred.as_ref().map(|(u, _)| *u));
    let Some(test_unit) = known else {
        return Ok(Err(GapReason::UnitUnknown));
    };
    let n: Decimal = link.value.parse()?;
    if link.unit.is_none()
        && inferred
            .as_ref()
            .is_some_and(|(_, v)| v.scaled().ok() != n.scaled().ok())
    {
        return Ok(Err(GapReason::ValueMismatch));
    }
    let wanted: Decimal = case.value.parse()?;
    let same_dimension = test_unit.base() == unit.base();
    if same_dimension
        && test_unit.normalize(&n)? != case.unit.unwrap().normalize(&wanted)?
        && test_unit == *unit
    {
        return Ok(Err(GapReason::ValueMismatch));
    }
    if !same_dimension || test_unit.normalize(&n)? != case.unit.unwrap().normalize(&wanted)? {
        return Err(BoundaryError::UnitContradiction {
            requirement_unit: *unit,
            test_unit,
            detail: format!(
                "{} {}: expected {} {}, linked {} {}",
                bound.quantity,
                case.position,
                case.value,
                case.unit.unwrap(),
                link.value,
                test_unit
            ),
        });
    }
    Ok(Ok(()))
}

pub fn coverage(
    requirement: &str,
    bounds: &[Bound],
    links: &[CaseLink],
    outcomes: &[TestOutcome],
) -> Result<CoverageMap, BoundaryError> {
    let mut result = CoverageMap {
        requirement: requirement.into(),
        bounds: Vec::new(),
    };
    // Duplicate receipts are malformed, never resolved by input order.
    let mut ids = BTreeSet::new();
    if outcomes.iter().any(|o| !ids.insert(&o.test_id)) {
        return Err(BoundaryError::InvalidBound(
            "duplicate test outcomes".into(),
        ));
    }
    let mut names = BTreeSet::new();
    for bound in bounds {
        if !names.insert(&bound.quantity) {
            return Err(BoundaryError::InvalidBound(
                "duplicate boundary quantity".into(),
            ));
        }
        let mut cases = Vec::new();
        for case in generate(bound)? {
            let candidates: Vec<_> = links
                .iter()
                .filter(|l| l.quantity == bound.quantity && l.position == case.position)
                .collect();
            if candidates.len() > 1 {
                return Err(BoundaryError::InvalidBound(format!(
                    "duplicate boundary link {} {}",
                    bound.quantity, case.position
                )));
            }
            let mut covered = CoveredCase {
                position: case.position.clone(),
                value: case.value.clone(),
                unit: case.unit,
                expected: case.expected.clone(),
                test_id: None,
                status: CaseStatus::Untested,
                reason: Some(GapReason::NoLink),
            };
            if let Some(link) = candidates.first() {
                covered.test_id = Some(link.test_id.clone());
                match link_matches(bound, &case, link)? {
                    Err(reason) => covered.reason = Some(reason),
                    Ok(()) => match outcomes
                        .iter()
                        .find(|o| o.test_id == link.test_id)
                        .map(|o| o.status)
                    {
                        Some(TestStatus::Passed) => {
                            covered.status = CaseStatus::TestedPass;
                            covered.reason = None;
                        }
                        Some(TestStatus::Failed | TestStatus::Errored) => {
                            covered.status = CaseStatus::TestedFail;
                            covered.reason = None;
                        }
                        _ => covered.reason = Some(GapReason::NotExecuted),
                    },
                }
            }
            cases.push(covered);
        }
        result.bounds.push(BoundCoverage {
            bound: bound.clone(),
            cases,
        });
    }
    // Happy-path links are permitted, but an unknown position/quantity must not disappear.
    for link in links {
        if !bounds.iter().any(|b| b.quantity == link.quantity) {
            return Err(BoundaryError::InvalidBound(format!(
                "unknown quantity {}",
                link.quantity
            )));
        }
        if link.position != "happy-path"
            && !result.bounds.iter().any(|b| {
                b.bound.quantity == link.quantity
                    && b.cases.iter().any(|c| c.position == link.position)
            })
        {
            return Err(BoundaryError::InvalidBound(format!(
                "unknown position {}",
                link.position
            )));
        }
    }
    Ok(result)
}
impl CoverageMap {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn to_markdown(&self) -> String {
        fn cell(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('|', "&#124;")
                .replace(['\n', '\r'], " ")
        }
        let mut text = String::from("| Requirement | Boundary | Position | Value | Unit | Expected | Status | Test | Reason |\n|---|---|---|---|---|---|---|---|---|\n");
        for b in &self.bounds {
            for c in &b.cases {
                text.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {:?} | {:?} | {} | {:?} |\n",
                    cell(&self.requirement),
                    cell(&b.bound.quantity),
                    cell(&c.position),
                    cell(&c.value),
                    c.unit.map(|u| u.to_string()).unwrap_or_default(),
                    c.expected,
                    c.status,
                    cell(c.test_id.as_deref().unwrap_or("")),
                    c.reason
                ));
            }
        }
        text
    }
}
