pub use crate::core::providers::{
    BoundaryGapProvider, NoBoundaryEvidence, NoQualification, QualifiedProvider, Reauditor, Result,
};
use crate::core::{
    annotations::{Annotations, RequirementAnnotations},
    badge::{score_line_at_revision, Badge, LineStatus},
    history::{History, Transition},
    spec::Spec,
    test_identity::{
        resolve_link_with_identities, Fingerprints, Resolution, TestOutcome, TestStatus,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenamedLink {
    pub requirement: String,
    pub old_id: String,
    pub new_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedLink {
    pub requirement: String,
    pub test_id: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RescoreReport {
    pub statuses: Vec<LineStatus>,
    pub transitions: Vec<Transition>,
    pub renamed_links: Vec<RenamedLink>,
    pub unresolved: Vec<UnresolvedLink>,
    pub orphans: Vec<String>,
    #[serde(default)]
    pub quantifier_reports: Vec<crate::core::views::QuantifierReport>,
    #[serde(default)]
    pub boundary_reports: Vec<crate::core::views::BoundaryReport>,
}
impl RescoreReport {
    pub fn exit_code(&self) -> i32 {
        if self.statuses.iter().any(|s| s.badge == Badge::Broken) {
            2
        } else {
            0
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusDocument {
    pub schema: String,
    pub commit: String,
    #[serde(default = "stored_audit")]
    pub audit: String,
    #[serde(default)]
    pub human_overrides: usize,
    pub statuses: Vec<LineStatus>,
    #[serde(default)]
    pub quantifier_reports: Vec<crate::core::views::QuantifierReport>,
    #[serde(default)]
    pub boundary_reports: Vec<crate::core::views::BoundaryReport>,
}
fn stored_audit() -> String {
    "stored".into()
}
pub fn rescore_with_providers(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    boundary: &dyn BoundaryGapProvider,
    qualified: &dyn QualifiedProvider,
) -> Result<RescoreReport> {
    rescore_with_context(
        spec_dir,
        outcomes,
        fingerprints,
        commit,
        note_source,
        RescoreContext {
            boundary,
            qualified,
            reauditor: None,
            quantifier_reports: &|_, _| Ok(Vec::new()),
        },
    )
}

pub type QuantifierReports =
    dyn Fn(&Spec, &Annotations) -> Result<Vec<crate::core::views::QuantifierReport>>;

/// The pipeline supplies concrete providers; the runner owns scoring and persistence.
pub struct RescoreContext<'a> {
    pub boundary: &'a dyn BoundaryGapProvider,
    pub qualified: &'a dyn QualifiedProvider,
    pub reauditor: Option<&'a dyn Reauditor>,
    pub quantifier_reports: &'a QuantifierReports,
}

pub fn rescore_with_context(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    context: RescoreContext<'_>,
) -> Result<RescoreReport> {
    let RescoreContext {
        boundary,
        qualified,
        reauditor: sources,
        quantifier_reports,
    } = context;
    if outcomes.is_empty() || commit.trim().is_empty() {
        return Err("nonempty outcomes and commit required".into());
    }
    let mut seen = BTreeSet::new();
    for o in outcomes {
        if o.test_id.is_empty() || !seen.insert(&o.test_id) {
            return Err("empty or duplicate outcome identity".into());
        }
    }
    let spec = Spec::parse(&fs::read_to_string(spec_dir.join("spec.md"))?)?;
    let mut annotations = Annotations::load(spec_dir.join("annotations.json"))?;
    if annotations.schema != "living-spec.annotations/v1" {
        return Err("unsupported annotations schema".into());
    }
    let orphans = annotations.orphans(&spec);
    let history = History::new(spec_dir.join("history.jsonl"));
    // Read before any writes: corrupt history/status must never reset the previous badge.
    let previous_history = history.read()?;
    let mut previous = BTreeMap::new();
    for t in previous_history {
        previous.insert(t.requirement, t.to);
    }
    match fs::read(spec_dir.join("status.json")) {
        Ok(bytes) => {
            let doc: StatusDocument = serde_json::from_slice(&bytes)?;
            if doc.schema != "living-spec.status/v1" {
                return Err("unsupported status schema".into());
            }
            for status in doc.statuses {
                previous.insert(status.id, status.badge);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut report = RescoreReport {
        statuses: vec![],
        transitions: vec![],
        renamed_links: vec![],
        unresolved: vec![],
        orphans,
        quantifier_reports: vec![],
        boundary_reports: vec![],
    };
    let at = timestamp();
    let mut annotations_changed = false;
    let mut rename_events = vec![];
    let mut unchanged_events = vec![];
    for line in &spec.lines {
        if let Some(a) = annotations.requirements.get_mut(&line.id) {
            for link in &mut a.links {
                let before = link.clone();
                let mut changed_source = false;
                match resolve_link_with_identities(link, outcomes, fingerprints) {
                    Resolution::Renamed { new_id } => {
                        let previous_override = link.invalidate_override();
                        let old_id = std::mem::replace(&mut link.test_id, new_id.clone());
                        // A fresh audit replaces this invalidated digest with the current one.
                        link.fingerprint.clear();
                        changed_source = true;
                        report.renamed_links.push(RenamedLink {
                            requirement: line.id.clone(),
                            old_id: old_id.clone(),
                            new_id: new_id.clone(),
                        });
                        let badge = previous.get(&line.id).copied().unwrap_or(Badge::Unproven);
                        rename_events.push(Transition {
                            at: at.clone(),
                            commit: commit.into(),
                            requirement: line.id.clone(),
                            from: badge,
                            to: badge,
                            test_id: new_id.clone(),
                            note: format!(
                                "renamed link {old_id} -> {new_id} ({note_source}){}",
                                previous_override.map_or_else(String::new, |value| format!(
                                    "; previous audit override invalidated: {value}"
                                ))
                            ),
                        });
                    }
                    Resolution::Changed => {
                        changed_source = true;
                    }
                    Resolution::Unresolved => report.unresolved.push(UnresolvedLink {
                        requirement: line.id.clone(),
                        test_id: link.test_id.clone(),
                    }),
                    _ => {}
                }
                if let Some(reauditor) = sources {
                    reauditor.reaudit(line, link, changed_source);
                }
                annotations_changed |= *link != before;
            }
        }
    }
    if sources.is_some() {
        for id in &report.orphans {
            for link in &mut annotations.requirements.get_mut(id).unwrap().links {
                let before = link.audit.clone();
                link.audit.verdict = crate::core::annotations::AuditVerdict::Challenged;
                link.audit.revision.clear();
                link.audit.notes =
                    vec!["Requirement is absent from the spec (orphan link).".into()];
                annotations_changed |= link.audit != before;
            }
        }
    }
    let gaps = boundary.gaps(&spec, &annotations, outcomes)?;
    report.boundary_reports = boundary.reports(&spec, &annotations, outcomes)?;
    for line in &spec.lines {
        let empty = RequirementAnnotations::default();
        let a = annotations.requirements.get(&line.id).unwrap_or(&empty);
        report.statuses.push(score_line_at_revision(
            &line.id,
            &line.revision,
            a,
            outcomes,
            fingerprints,
            &gaps,
        ));
    }
    let own_statuses = report.statuses.clone();
    qualified.qualify(&spec, &annotations, &mut report.statuses)?;
    for (own, qualified) in own_statuses.iter().zip(&report.statuses) {
        if own.id != qualified.id
            || own.tests != qualified.tests
            || (own.badge != qualified.badge
                && !(own.badge == Badge::Proven && qualified.badge == Badge::Qualified))
            || own.reasons.iter().any(|r| !qualified.reasons.contains(r))
        {
            return Err(
                "qualification provider must preserve own evidence and only qualify proven lines"
                    .into(),
            );
        }
    }
    // Quantifier observations remain separate from badges and test outcomes.
    report.quantifier_reports = quantifier_reports(&spec, &annotations)?;
    for status in &report.statuses {
        let from = previous.get(&status.id).copied().unwrap_or(Badge::Unproven);
        let failing = status
            .tests
            .iter()
            .find(|t| t.admissible && matches!(t.status, TestStatus::Failed | TestStatus::Errored));
        let unresolved = report
            .unresolved
            .iter()
            .find(|u| u.requirement == status.id);
        let test_id = unresolved
            .map(|u| u.test_id.as_str())
            .or_else(|| failing.map(|t| t.test_id.as_str()))
            .or_else(|| {
                (status.badge != Badge::Broken)
                    .then(|| status.tests.first().map(|t| t.test_id.as_str()))
                    .flatten()
            })
            .unwrap_or("");
        let note = if status.badge == Badge::Broken {
            if unresolved.is_some() {
                format!("test {test_id} link unresolved at {commit}")
            } else {
                let failure = outcomes
                    .iter()
                    .find(|o| o.test_id == test_id)
                    .and_then(|o| o.failure_message.as_deref())
                    .and_then(|s| s.lines().next());
                format!(
                    "test {} failed at {commit}{}",
                    if test_id.is_empty() {
                        "unknown"
                    } else {
                        test_id
                    },
                    failure.map(|s| format!(": {s}")).unwrap_or_default()
                )
            }
        } else {
            format!("rescored from {note_source}")
        };
        let event = Transition {
            at: at.clone(),
            commit: commit.into(),
            requirement: status.id.clone(),
            from,
            to: status.badge,
            test_id: test_id.into(),
            note,
        };
        if from == status.badge {
            unchanged_events.push(event);
        } else {
            report.transitions.push(event);
        }
    }
    if annotations_changed || !report.renamed_links.is_empty() {
        annotations.save(spec_dir.join("annotations.json"))?;
    }
    for event in rename_events
        .iter()
        .chain(report.transitions.iter())
        .chain(unchanged_events.iter())
    {
        history.append(event)?;
    }
    let document = StatusDocument {
        schema: "living-spec.status/v1".into(),
        commit: commit.into(),
        audit: if sources.is_some() { "fresh" } else { "stored" }.into(),
        human_overrides: spec
            .lines
            .iter()
            .map(|line| {
                annotations.requirements.get(&line.id).map_or(0, |a| {
                    a.links
                        .iter()
                        .filter(|link| {
                            link.audit.verdict == crate::core::annotations::AuditVerdict::Challenged
                                && link.human_override_reason().is_some()
                                && link.revision == line.revision
                                && link.audit.revision == line.revision
                                && fingerprints.get(&link.test_id).is_some_and(|identity| {
                                    identity.fingerprint == link.fingerprint
                                })
                        })
                        .count()
                })
            })
            .sum(),
        statuses: report.statuses.clone(),
        quantifier_reports: report.quantifier_reports.clone(),
        boundary_reports: report.boundary_reports.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    fs::write(spec_dir.join("status.json"), bytes)?;
    Ok(report)
}
/// Resolve HEAD relative to the spec's repository, never the caller's cwd.
pub fn default_commit(spec_dir: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(spec_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "uncommitted".into())
}
fn timestamp() -> String {
    // Gregorian civil date from days since the Unix epoch (UTC).
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let z = (seconds / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let time = seconds % 86400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}
