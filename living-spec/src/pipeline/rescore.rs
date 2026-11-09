use crate::{
    agents::{auditor, test_index::TestIndex},
    boundary::coverage::BoundaryEvidence,
    core::{
        annotations::{Annotations, AuditVerdict, Link},
        providers::{Reauditor, Result},
        spec::{RequirementLine, Spec},
        test_identity::{Fingerprints, TestOutcome},
        views::QuantifierReport,
    },
    deps::propagate::DependencyQualification,
    runner::rescore::{rescore_with_context, RescoreContext, RescoreReport},
};
use std::path::Path;

pub fn rescore(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
) -> Result<RescoreReport> {
    score(
        spec_dir,
        outcomes,
        fingerprints,
        commit,
        note_source,
        None,
        None,
    )
}

/// Re-audit every resolvable source binding before computing badges.
pub fn rescore_with_sources(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    index: &TestIndex,
    commit: &str,
    note_source: &str,
) -> Result<RescoreReport> {
    score(
        spec_dir,
        outcomes,
        &index.identities(),
        commit,
        note_source,
        Some(&SourceReauditor(index)),
        None,
    )
}

/// CLI entry point carrying the already-read result bytes through scoring.
pub fn rescore_with_results(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    results: &crate::core::freshness::ResultsSnapshot,
    index: Option<&TestIndex>,
) -> Result<RescoreReport> {
    let reauditor = index.map(SourceReauditor);
    let identities = index.map(TestIndex::identities);
    score(
        spec_dir,
        outcomes,
        identities.as_ref().unwrap_or(fingerprints),
        commit,
        note_source,
        reauditor.as_ref().map(|audit| audit as &dyn Reauditor),
        Some(results),
    )
}

fn score(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    reauditor: Option<&dyn Reauditor>,
    results: Option<&crate::core::freshness::ResultsSnapshot>,
) -> Result<RescoreReport> {
    rescore_with_context(
        spec_dir,
        outcomes,
        fingerprints,
        commit,
        note_source,
        RescoreContext {
            boundary: &BoundaryEvidence,
            qualified: &DependencyQualification,
            reauditor,
            results,
            quantifier_reports: &quantifier_reports,
        },
    )
}

struct SourceReauditor<'a>(&'a TestIndex);

impl Reauditor for SourceReauditor<'_> {
    fn reaudit(&self, line: &RequirementLine, link: &mut Link, changed_source: bool) {
        if let Some(entry) = self.0.get(&link.test_id) {
            link.retire_stale_override();
            if changed_source
                || link.fingerprint != entry.fingerprint
                || link.revision != line.revision
                || link.audit.revision != line.revision
            {
                link.invalidate_override();
            }
            link.fingerprint = entry.fingerprint.clone();
            link.rename_hint = Some(entry.rename_hint.clone());
            let mut audit = auditor::audit(line, link, entry);
            audit.extra = link.audit.extra.clone();
            if changed_source {
                audit
                    .notes
                    .push("Test source changed; automatically re-audited current source.".into());
            } else {
                audit
                    .notes
                    .push("Link automatically re-audited against current source.".into());
            }
            link.revision = line.revision.clone();
            link.audit = audit;
            link.retire_stale_override();
        } else {
            link.audit.verdict = AuditVerdict::Challenged;
            // No source was audited: keep its original revision binding so a
            // later resolution cannot revive an override for a different revision.
            link.audit.notes =
                vec!["Test id is unresolved; no current source can be audited.".into()];
        }
    }
}

fn quantifier_reports(spec: &Spec, annotations: &Annotations) -> Result<Vec<QuantifierReport>> {
    let mut reports = Vec::new();
    for line in &spec.lines {
        if let Some(section) = annotations
            .requirements
            .get(&line.id)
            .and_then(|a| a.quantifier.as_ref())
        {
            let claim = crate::quantifier::Claim {
                id: line.id.clone(),
                text: line.text.clone(),
                quantifier: crate::quantifier::annotations::Quantifier::from_section(section)?,
                expected: None,
            };
            let coverage = crate::quantifier::check_claim(&claim)?;
            // Validate the standalone honesty contract before any status/history writes.
            crate::quantifier::view::json(&coverage)?;
            reports.push(coverage);
        }
    }
    Ok(reports)
}
