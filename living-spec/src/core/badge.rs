use super::{
    annotations::{AuditVerdict, RequirementAnnotations},
    test_identity::{
        resolve_link_with_identities, Fingerprints, Resolution, TestIdentity, TestOutcome,
        TestStatus,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Badge {
    Proven,
    Broken,
    Unproven,
    Qualified,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NoLink,
    LinkChallenged,
    HumanOverride,
    StaleRevision,
    TestChanged,
    NotExecuted,
    TestFailed,
    LinkUnresolved,
    BoundaryGap,
    PrerequisiteBroken { path: Vec<String> },
    PrerequisiteUnproven { path: Vec<String> },
    PrerequisiteQualified { path: Vec<String> },
    DependencyCycle { path: Vec<String> },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestEvidence {
    pub test_id: String,
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    /// Only a current, accepted, resolved link can contribute a causal failure.
    /// Legacy status files lack this provenance and must not imply admissibility.
    #[serde(default)]
    pub admissible: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineStatus {
    pub id: String,
    pub badge: Badge,
    pub reasons: Vec<Reason>,
    pub tests: Vec<TestEvidence>,
}
/// Compatibility entry point. Without a current requirement revision, proof is unavailable.
pub fn score_line(
    id: &str,
    annotations: &RequirementAnnotations,
    outcomes: &[TestOutcome],
    fingerprints_by_id: &BTreeMap<String, String>,
    boundary_gaps: &BTreeSet<String>,
) -> LineStatus {
    let identities = fingerprints_by_id
        .iter()
        .map(|(id, digest)| {
            (
                id.clone(),
                TestIdentity {
                    fingerprint: digest.clone(),
                    rename_hint: None,
                },
            )
        })
        .collect();
    score_line_at_revision(id, "", annotations, outcomes, &identities, boundary_gaps)
}
fn reason(result: &mut LineStatus, reason: Reason) {
    if !result.reasons.contains(&reason) {
        result.reasons.push(reason);
    }
}
/// R1 gates: unresolved is structural red; only current accepted links supply evidence.
pub fn score_line_at_revision(
    id: &str,
    revision: &str,
    annotations: &RequirementAnnotations,
    outcomes: &[TestOutcome],
    identities: &Fingerprints,
    boundary_gaps: &BTreeSet<String>,
) -> LineStatus {
    let mut result = LineStatus {
        id: id.to_owned(),
        badge: Badge::Unproven,
        reasons: vec![],
        tests: vec![],
    };
    let mut admissible = 0;
    if annotations.links.is_empty() {
        reason(&mut result, Reason::NoLink);
    }
    for link in &annotations.links {
        let resolution = resolve_link_with_identities(link, outcomes, identities);
        let resolved = match &resolution {
            Resolution::Renamed { new_id } => new_id,
            _ => &link.test_id,
        };
        let matched = outcomes
            .iter()
            .filter(|o| &o.test_id == resolved)
            .collect::<Vec<_>>();
        let link_admissible = link.audit.is_accepted()
            && !revision.is_empty()
            && link.revision == revision
            && link.audit.revision == revision
            && matches!(resolution, Resolution::Exact | Resolution::Renamed { .. });
        for o in &matched {
            result.tests.push(TestEvidence {
                test_id: o.test_id.clone(),
                status: o.status,
                duration_ms: o.duration_ms,
                admissible: link_admissible,
            });
        }
        if resolution == Resolution::Unresolved {
            reason(&mut result, Reason::LinkUnresolved);
        }
        if !link.audit.is_accepted() {
            reason(&mut result, Reason::LinkChallenged);
            continue;
        }
        if revision.is_empty() || link.revision != revision || link.audit.revision != revision {
            reason(&mut result, Reason::StaleRevision);
            continue;
        }
        match resolution {
            Resolution::Unresolved => continue,
            Resolution::Changed | Resolution::Renamed { .. } => {
                reason(&mut result, Reason::TestChanged);
                continue;
            }
            Resolution::Exact => {}
        }
        admissible += 1;
        if link.audit.verdict == AuditVerdict::Challenged
            && link.audit.human_override_reason().is_some()
        {
            reason(&mut result, Reason::HumanOverride);
        }
        if matched.is_empty() {
            reason(&mut result, Reason::NotExecuted);
        }
        for outcome in matched {
            match outcome.status {
                TestStatus::Failed | TestStatus::Errored => reason(&mut result, Reason::TestFailed),
                TestStatus::Skipped => reason(&mut result, Reason::NotExecuted),
                TestStatus::Passed => {}
            }
        }
    }
    if result.reasons.contains(&Reason::LinkUnresolved)
        || result.reasons.contains(&Reason::TestFailed)
    {
        result.badge = Badge::Broken;
    } else if admissible > 0 && !result.reasons.contains(&Reason::NotExecuted) {
        if !annotations.bounds.is_empty() && boundary_gaps.contains(id) {
            reason(&mut result, Reason::BoundaryGap);
        } else {
            result.badge = Badge::Proven;
        }
    }
    result
}
