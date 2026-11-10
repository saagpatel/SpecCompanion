use crate::{
    agents::{auditor, test_index::TestIndex},
    boundary::coverage::BoundaryEvidence,
    core::{
        annotations::{Annotations, AuditVerdict, Link},
        badge::{Badge, LineStatus},
        mutation::{evaluate_link, line_tier, Bindings, LinkState, MutationFile},
        providers::{MutationTierProvider, Reauditor, Result},
        spec::{RequirementLine, Spec},
        test_identity::{resolve_link_with_identities, Fingerprints, Resolution, TestOutcome},
        views::QuantifierReport,
    },
    deps::propagate::DependencyQualification,
    runner::rescore::{rescore_with_context, RescoreContext, RescoreReport},
};
use std::path::{Path, PathBuf};

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
        PipelineScoreOptions {
            reauditor: None,
            results: None,
            mutation: None,
        },
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
        PipelineScoreOptions {
            reauditor: Some(&SourceReauditor(index)),
            results: None,
            mutation: None,
        },
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
    rescore_with_results_and_mutation(
        spec_dir,
        outcomes,
        fingerprints,
        commit,
        note_source,
        RescoreSnapshot {
            results,
            index,
            mutation: None,
        },
    )
}

pub struct MutationScoreInput<'a> {
    pub file: &'a MutationFile,
    pub digest: &'a str,
    pub tests_root: &'a Path,
    /// Relative to the spec directory; defaults to `mutation.json`.
    pub locator: &'a str,
}

pub struct RescoreSnapshot<'a> {
    pub results: &'a crate::core::freshness::ResultsSnapshot,
    pub index: Option<&'a TestIndex>,
    pub mutation: Option<MutationScoreInput<'a>>,
}

pub fn rescore_with_results_and_mutation(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    snapshot: RescoreSnapshot<'_>,
) -> Result<RescoreReport> {
    let RescoreSnapshot {
        results,
        index,
        mutation,
    } = snapshot;
    let reauditor = index.map(SourceReauditor);
    let identities = index.map(TestIndex::identities);
    let mutation_provider = mutation.map(|input| PipelineMutationProvider {
        file: input.file,
        index,
        identities: index
            .map(TestIndex::identities)
            .unwrap_or_else(|| fingerprints.clone()),
        tests_root: input.tests_root.to_path_buf(),
        digest: input.digest,
        locator: input.locator,
    });
    score(
        spec_dir,
        outcomes,
        identities.as_ref().unwrap_or(fingerprints),
        commit,
        note_source,
        PipelineScoreOptions {
            reauditor: reauditor.as_ref().map(|audit| audit as &dyn Reauditor),
            results: Some(results),
            mutation: mutation_provider
                .as_ref()
                .map(|provider| provider as &dyn MutationTierProvider),
        },
    )
}

struct PipelineScoreOptions<'a> {
    reauditor: Option<&'a dyn Reauditor>,
    results: Option<&'a crate::core::freshness::ResultsSnapshot>,
    mutation: Option<&'a dyn MutationTierProvider>,
}

fn score(
    spec_dir: &Path,
    outcomes: &[TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note_source: &str,
    options: PipelineScoreOptions<'_>,
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
            reauditor: options.reauditor,
            results: options.results,
            quantifier_reports: &quantifier_reports,
            mutation: options.mutation,
        },
    )
}

struct PipelineMutationProvider<'a> {
    file: &'a MutationFile,
    index: Option<&'a TestIndex>,
    identities: Fingerprints,
    tests_root: PathBuf,
    digest: &'a str,
    locator: &'a str,
}

impl MutationTierProvider for PipelineMutationProvider<'_> {
    fn attach(
        &self,
        spec: &Spec,
        annotations: &Annotations,
        statuses: &mut [LineStatus],
    ) -> Result<()> {
        for status in statuses {
            if status.badge != Badge::Proven {
                continue;
            }
            let Some(line) = spec.lines.iter().find(|line| line.id == status.id) else {
                continue;
            };
            let Some(annotation) = annotations.requirements.get(&line.id) else {
                continue;
            };
            let states: Vec<LinkState> = annotation
                .links
                .iter()
                .filter_map(|link| {
                    if !link.is_accepted()
                        || line.revision.is_empty()
                        || link.revision != line.revision
                        || link.audit.revision != line.revision
                        || !matches!(
                            resolve_link_with_identities(link, &[], &self.identities),
                            Resolution::Exact
                        )
                    {
                        return None;
                    }
                    let base_id = link
                        .test_id
                        .split_once('#')
                        .map_or(link.test_id.as_str(), |(base, _)| base);
                    let case_prefix = format!("{base_id}#");
                    let has_cases = self
                        .identities
                        .keys()
                        .any(|id| id.starts_with(&case_prefix))
                        || status
                            .tests
                            .iter()
                            .any(|test| test.test_id.starts_with(&case_prefix));
                    if !status.tests.iter().any(|test| {
                        test.admissible
                            && if has_cases {
                                test.test_id.starts_with(&case_prefix)
                            } else {
                                test.test_id == link.test_id
                            }
                    }) {
                        return None;
                    }
                    let entry = self.index.and_then(|index| index.get(base_id));
                    let subject_digest = entry.and_then(|entry| {
                        crate::mutation::subject::subject_digest_for(entry, &self.tests_root)
                            .ok()
                            .flatten()
                    });
                    let fingerprint = entry
                        .map(|entry| entry.fingerprint.as_str())
                        .or_else(|| {
                            self.identities
                                .get(&link.test_id)
                                .map(|id| id.fingerprint.as_str())
                        })
                        .unwrap_or("");
                    Some(evaluate_link(
                        self.file.links.iter().find(|record| {
                            record.requirement == line.id && record.test_id == link.test_id
                        }),
                        &Bindings {
                            test_fingerprint: fingerprint,
                            requirement_revision: &line.revision,
                            subject_digest: subject_digest.as_deref(),
                        },
                        &self.file.signoffs,
                    ))
                })
                .collect();
            status.mutation = line_tier(&states, true);
        }
        Ok(())
    }
    fn digest(&self) -> &str {
        self.digest
    }

    fn locator(&self) -> &str {
        self.locator
    }
}

struct SourceReauditor<'a>(&'a TestIndex);

impl Reauditor for SourceReauditor<'_> {
    fn reaudit(&self, line: &RequirementLine, link: &mut Link, changed_source: bool) {
        let base_id = link
            .test_id
            .split_once('#')
            .map_or(link.test_id.as_str(), |(base, _)| base);
        if let Some(entry) = self.0.get(base_id) {
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

#[cfg(test)]
mod mutation_tier_tests {
    use super::*;
    use crate::core::{
        annotations::{Audit, AuditVerdict, Link, RequirementAnnotations},
        badge::TestEvidence,
        mutation::{
            mutant_id, LinkRecord, LinkStatus, Mutant, MutantOutcome, Operator, Target, TargetKind,
            TierState,
        },
        test_identity::TestStatus,
    };
    use std::{collections::BTreeMap, fs};

    fn fixture() -> (
        tempfile::TempDir,
        Spec,
        Annotations,
        MutationFile,
        TestIndex,
    ) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let temp = tempfile::tempdir_in(root.join("target")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(temp.path().join("tests")).unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("src/lib.rs"),
            "pub fn clamp(x: i32) -> i32 { if x < 0 { 0 } else { x } }\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("tests/clamp.rs"),
            "use sample::clamp; #[test] fn clamps() { assert_eq!(clamp(-1), 0); }\n",
        )
        .unwrap();
        let index = TestIndex::build(temp.path()).unwrap();
        let entry = index
            .entries
            .iter()
            .find(|entry| entry.name == "clamps")
            .unwrap();
        let spec = Spec::parse("- [T-001] Negative values clamp to zero.\n").unwrap();
        let line = &spec.lines[0];
        let link = Link {
            test_id: entry.id.clone(),
            fingerprint: entry.fingerprint.clone(),
            rename_hint: Some(entry.rename_hint.clone()),
            revision: line.revision.clone(),
            explanation: "test binding".into(),
            proposed_by: "human".into(),
            audit: Audit {
                verdict: AuditVerdict::Accepted,
                revision: line.revision.clone(),
                notes: vec![],
                extra: BTreeMap::new(),
            },
            extra: BTreeMap::new(),
        };
        let annotations = Annotations {
            schema: "living-spec.annotations/v1".into(),
            requirements: BTreeMap::from([(
                "T-001".into(),
                RequirementAnnotations {
                    links: vec![link],
                    ..Default::default()
                },
            )]),
            extra: BTreeMap::new(),
        };
        let digest = crate::mutation::subject::subject_digest_for(entry, temp.path())
            .unwrap()
            .unwrap();
        let mutant = Mutant {
            id: mutant_id("src/lib.rs", "clamp", Operator::CmpFlip, "<", ">=", 0),
            file: "src/lib.rs".into(),
            function: "clamp".into(),
            line: 1,
            ordinal: 0,
            operator: Operator::CmpFlip,
            original: "<".into(),
            replacement: ">=".into(),
            outcome: MutantOutcome::Killed,
        };
        let file = MutationFile {
            schema: "living-spec.mutation/v1".into(),
            engine: "rust-cargo-v1".into(),
            links: vec![LinkRecord {
                requirement: "T-001".into(),
                test_id: entry.id.clone(),
                test_fingerprint: entry.fingerprint.clone(),
                requirement_revision: line.revision.clone(),
                subject_digest: digest,
                target: Target {
                    kind: TargetKind::Test,
                    name: "clamp".into(),
                },
                baseline_passes: 2,
                status: LinkStatus::Checked,
                reason: None,
                infra_failure: None,
                mutants: vec![mutant],
            }],
            signoffs: vec![],
        };
        (temp, spec, annotations, file, index)
    }

    #[test]
    fn proven_line_gets_checked_tier_and_source_changes_make_it_stale() {
        let (temp, spec, annotations, file, index) = fixture();
        let entry = &index.entries[0];
        let identities = index.identities();
        let mut statuses = vec![LineStatus {
            id: "T-001".into(),
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![TestEvidence {
                test_id: entry.id.clone(),
                status: TestStatus::Passed,
                duration_ms: None,
                admissible: true,
            }],
            mutation: None,
        }];
        let provider = PipelineMutationProvider {
            file: &file,
            index: Some(&index),
            identities,
            tests_root: temp.path().into(),
            digest: "sha256:test",
            locator: "mutation.json",
        };
        provider.attach(&spec, &annotations, &mut statuses).unwrap();
        assert_eq!(
            statuses[0].mutation.as_ref().unwrap().state,
            TierState::MutationChecked
        );
        fs::write(
            temp.path().join("src/lib.rs"),
            "pub fn clamp(x: i32) -> i32 { if x <= 0 { 0 } else { x } }\n",
        )
        .unwrap();
        provider.attach(&spec, &annotations, &mut statuses).unwrap();
        assert_eq!(
            statuses[0].mutation.as_ref().unwrap().state,
            TierState::Stale
        );
    }

    #[test]
    fn non_proven_line_has_no_mutation_tier() {
        let (temp, spec, annotations, file, index) = fixture();
        let mut statuses = vec![LineStatus {
            id: "T-001".into(),
            badge: Badge::Unproven,
            reasons: vec![],
            tests: vec![],
            mutation: None,
        }];
        let provider = PipelineMutationProvider {
            file: &file,
            index: Some(&index),
            identities: index.identities(),
            tests_root: temp.path().into(),
            digest: "sha256:test",
            locator: "mutation.json",
        };
        provider.attach(&spec, &annotations, &mut statuses).unwrap();
        assert!(statuses[0].mutation.is_none());
    }

    #[test]
    fn overridden_unchecked_link_blocks_checked_tier() {
        let (temp, spec, mut annotations, mut file, index) = fixture();
        let link = &mut annotations.requirements.get_mut("T-001").unwrap().links[0];
        link.audit.verdict = AuditVerdict::Challenged;
        link.audit.extra.insert(
            "override".into(),
            serde_json::json!({
                "by": "human", "reason": "reviewed", "test_id": link.test_id,
                "fingerprint": link.fingerprint, "revision": link.revision
            }),
        );
        file.links.clear();
        let mut statuses = vec![LineStatus {
            id: "T-001".into(),
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![TestEvidence {
                test_id: link.test_id.clone(),
                status: TestStatus::Passed,
                duration_ms: None,
                admissible: true,
            }],
            mutation: None,
        }];
        let provider = PipelineMutationProvider {
            file: &file,
            index: Some(&index),
            identities: index.identities(),
            tests_root: temp.path().into(),
            digest: "test",
            locator: "mutation.json",
        };
        provider.attach(&spec, &annotations, &mut statuses).unwrap();
        assert_eq!(
            statuses[0].mutation.as_ref().unwrap().state,
            TierState::NotRun
        );
    }

    #[test]
    fn parameterized_base_link_contributes_not_run_state() {
        let (temp, spec, annotations, mut file, index) = fixture();
        file.links.clear();
        let id = &annotations.requirements["T-001"].links[0].test_id;
        let mut statuses = vec![LineStatus {
            id: "T-001".into(),
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![TestEvidence {
                test_id: format!("{id}#case"),
                status: TestStatus::Passed,
                duration_ms: None,
                admissible: true,
            }],
            mutation: None,
        }];
        let provider = PipelineMutationProvider {
            file: &file,
            index: Some(&index),
            identities: index.identities(),
            tests_root: temp.path().into(),
            digest: "test",
            locator: "mutation.json",
        };
        provider.attach(&spec, &annotations, &mut statuses).unwrap();
        assert_eq!(
            statuses[0].mutation.as_ref().unwrap().state,
            TierState::NotRun
        );
    }
}
