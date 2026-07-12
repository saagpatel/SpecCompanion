use crate::db::queries;
use crate::errors::AppError;
use crate::models::report::{
    AlignmentClassification, AlignmentReason, AlignmentReport, AlignmentReportWithEvidence,
    EvidenceKind, EvidenceRecord, RequirementAlignment,
};
use crate::models::spec::Requirement;
use crate::services::evidence::{
    self, AssertionQuality, ImplementationCandidate, ProjectEvidenceScan,
};
use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(test)]
pub fn generate_report(
    conn: &Connection,
    project_id: &str,
) -> Result<AlignmentReportWithEvidence, AppError> {
    generate_report_with_exclusions(conn, project_id, &[])
}

pub fn generate_report_with_exclusions(
    conn: &Connection,
    project_id: &str,
    exclusions: &[String],
) -> Result<AlignmentReportWithEvidence, AppError> {
    let project = queries::get_project(conn, project_id)?;
    let requirements = queries::get_requirements_for_project(conn, project_id)?;
    let scan = match evidence::scan_project(&project.project.codebase_path, exclusions) {
        Ok(scan) => scan,
        Err(error) => ProjectEvidenceScan {
            implementation: Vec::new(),
            tests: Vec::new(),
            checked_languages: Vec::new(),
            skipped_languages: Vec::new(),
            diagnostics: vec![format!("Evidence scan unavailable: {error}")],
        },
    };

    let mut alignments = Vec::with_capacity(requirements.len());
    for requirement in &requirements {
        alignments.push(classify_requirement(conn, requirement, &scan)?);
    }
    alignments.sort_by(|left, right| {
        (left.source_line_start, &left.requirement_id)
            .cmp(&(right.source_line_start, &right.requirement_id))
    });

    let verified = count(&alignments, AlignmentClassification::Verified);
    let partial = count(&alignments, AlignmentClassification::Partial);
    let failed = count(&alignments, AlignmentClassification::Failed);
    let unknown = count(&alignments, AlignmentClassification::Unknown);
    let total = alignments.len() as i64;
    let report = AlignmentReport {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        coverage_percent: if total == 0 {
            0.0
        } else {
            verified as f64 / total as f64 * 100.0
        },
        total_requirements: total,
        covered_requirements: verified,
        verified_requirements: verified,
        partial_requirements: partial,
        failed_requirements: failed,
        unknown_requirements: unknown,
        evidence_digest: digest_alignments(&alignments)?,
        checked_languages: scan.checked_languages.clone(),
        skipped_languages: scan.skipped_languages.clone(),
        diagnostics: scan.diagnostics.clone(),
        generated_at: Utc::now().to_rfc3339(),
    };

    let tx = conn.unchecked_transaction().map_err(AppError::Database)?;
    queries::insert_alignment_report(&tx, &report)?;
    for (index, alignment) in alignments.iter().enumerate() {
        queries::insert_requirement_alignment(&tx, alignment, &report.id, index)?;
    }
    tx.commit().map_err(AppError::Database)?;

    Ok(AlignmentReportWithEvidence { report, alignments })
}

fn classify_requirement(
    conn: &Connection,
    requirement: &Requirement,
    scan: &ProjectEvidenceScan,
) -> Result<RequirementAlignment, AppError> {
    let implementations: Vec<&ImplementationCandidate> = scan
        .implementation
        .iter()
        .filter(|candidate| evidence::implementation_matches(requirement, candidate))
        .collect();
    let tests = queries::get_generated_tests_for_requirement(conn, &requirement.id)?;
    let mut evidence_records = vec![EvidenceRecord {
        id: evidence::evidence_id(&[
            &requirement.id,
            "requirement",
            &requirement.source_line_start.to_string(),
        ]),
        kind: EvidenceKind::Requirement,
        path: None,
        line_start: Some(requirement.source_line_start),
        line_end: Some(requirement.source_line_end),
        symbol: None,
        status: "parsed".into(),
        summary: format!("Requirement parsed from section {}", requirement.section),
    }];
    for candidate in &implementations {
        evidence_records.push(EvidenceRecord {
            id: evidence::evidence_id(&[
                &requirement.id,
                "implementation",
                &candidate.path,
                &candidate.line.to_string(),
                &candidate.symbol,
            ]),
            kind: EvidenceKind::Implementation,
            path: Some(candidate.path.clone()),
            line_start: Some(candidate.line),
            line_end: Some(candidate.line),
            symbol: Some(candidate.symbol.clone()),
            status: "candidate".into(),
            summary: "Exact requirement tokens match this implementation symbol; semantic equivalence is not inferred".into(),
        });
    }

    let mut has_meaningful = false;
    let mut has_non_probative = false;
    let mut has_pass = false;
    let mut has_not_run = false;
    let mut has_failure = false;
    let mut has_timeout = false;
    let mut has_runtime_unavailable = false;
    let mut has_linked_test = !tests.is_empty();

    for test in &tests {
        let explicitly_linked = test.generation_mode == "repository_link";
        let traced = explicitly_linked || evidence::has_requirement_trace(&test.code, requirement);
        let quality = evidence::analyze_assertions(&test.code);
        let (quality_status, assertion_lines) = match &quality {
            AssertionQuality::Meaningful(lines)
                if traced
                    && evidence::test_references_implementation(&test.code, &implementations) =>
            {
                has_meaningful = true;
                ("meaningful", lines.clone())
            }
            AssertionQuality::Meaningful(lines) => {
                has_non_probative = true;
                ("unlinked_or_unmatched", lines.clone())
            }
            AssertionQuality::Placeholder(lines) => {
                has_non_probative = true;
                ("placeholder", lines.clone())
            }
            AssertionQuality::Missing => {
                has_non_probative = true;
                ("missing", Vec::new())
            }
        };
        let display_path = test
            .file_path
            .clone()
            .unwrap_or_else(|| format!("generated:/{}/{}", requirement.id, test.id));
        evidence_records.push(EvidenceRecord {
            id: evidence::evidence_id(&[&requirement.id, "test", &test.id]),
            kind: EvidenceKind::Test,
            path: Some(display_path.clone()),
            line_start: Some(1),
            line_end: Some(test.code.lines().count() as i64),
            symbol: Some(test.id.clone()),
            status: if explicitly_linked {
                "explicitly_linked"
            } else if traced {
                "linked"
            } else {
                "unlinked"
            }
            .into(),
            summary: if explicitly_linked {
                "The user explicitly linked this contained repository test; semantic equivalence was not inferred".into()
            } else if traced {
                "Test is linked to the stable requirement identity".into()
            } else {
                "Test lacks a stable requirement trace marker".into()
            },
        });
        evidence_records.push(EvidenceRecord {
            id: evidence::evidence_id(&[&requirement.id, "assertion", &test.id, quality_status]),
            kind: EvidenceKind::Assertion,
            path: Some(display_path),
            line_start: assertion_lines.first().copied(),
            line_end: assertion_lines.last().copied(),
            symbol: None,
            status: quality_status.into(),
            summary: match quality_status {
                "meaningful" => "Assertion observes a non-constant expression".into(),
                "placeholder" => {
                    "Tautology or placeholder assertion cannot verify a requirement".into()
                }
                "unlinked_or_unmatched" => {
                    "Assertion may be meaningful, but its requirement link or implementation target is unavailable".into()
                }
                _ => "No assertion was found".into(),
            },
        });

        if let Some(result) = queries::get_latest_test_result_for_test(conn, &test.id)? {
            match result.status.as_str() {
                "passed" if quality_status == "meaningful" => has_pass = true,
                "failed" | "error" => has_failure = true,
                "timed_out" => has_timeout = true,
                "runtime_unavailable" | "unsupported" | "blocked" => has_runtime_unavailable = true,
                _ => {}
            }
            evidence_records.push(EvidenceRecord {
                id: evidence::evidence_id(&[&requirement.id, "execution", &result.id]),
                kind: EvidenceKind::Execution,
                path: None,
                line_start: None,
                line_end: None,
                symbol: Some(test.id.clone()),
                status: result.status.clone(),
                summary: format!(
                    "{}; controls: {}",
                    execution_summary(&result.status, result.execution_time_ms),
                    execution_controls_summary(&result.execution_controls)
                ),
            });
        } else if quality_status == "meaningful" {
            has_not_run = true;
        }
    }

    for candidate in scan.tests.iter().filter(|candidate| {
        evidence::has_requirement_trace(&candidate.code, requirement)
            && !tests.iter().any(|test| {
                test.file_path
                    .as_deref()
                    .is_some_and(|path| path.replace('\\', "/").ends_with(&candidate.path))
            })
    }) {
        has_linked_test = true;
        let (quality_status, assertion_lines) = match evidence::analyze_assertions(&candidate.code)
        {
            AssertionQuality::Meaningful(lines)
                if evidence::test_references_implementation(&candidate.code, &implementations) =>
            {
                has_meaningful = true;
                has_not_run = true;
                ("meaningful", lines)
            }
            AssertionQuality::Meaningful(lines) => {
                has_non_probative = true;
                ("unmatched_implementation", lines)
            }
            AssertionQuality::Placeholder(lines) => {
                has_non_probative = true;
                ("placeholder", lines)
            }
            AssertionQuality::Missing => {
                has_non_probative = true;
                ("missing", Vec::new())
            }
        };
        evidence_records.push(EvidenceRecord {
            id: evidence::evidence_id(&[&requirement.id, "repository-test", &candidate.path]),
            kind: EvidenceKind::Test,
            path: Some(candidate.path.clone()),
            line_start: Some(1),
            line_end: Some(candidate.code.lines().count() as i64),
            symbol: None,
            status: "linked_unexecuted".into(),
            summary:
                "Repository test contains an exact requirement trace; execution was not requested"
                    .into(),
        });
        evidence_records.push(EvidenceRecord {
            id: evidence::evidence_id(&[
                &requirement.id,
                "repository-assertion",
                &candidate.path,
                quality_status,
            ]),
            kind: EvidenceKind::Assertion,
            path: Some(candidate.path.clone()),
            line_start: assertion_lines.first().copied(),
            line_end: assertion_lines.last().copied(),
            symbol: None,
            status: quality_status.into(),
            summary: if quality_status == "meaningful" {
                "Repository test has a non-constant assertion, but no bounded execution result"
                    .into()
            } else {
                "Repository test assertion is missing or a tautology".into()
            },
        });
    }

    let no_scan_evidence = scan.checked_languages.is_empty() && scan.skipped_languages.is_empty();
    let (classification, reason, summary) = if requirement.content_fingerprint.is_empty()
        || requirement.source_line_start <= 0
    {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::EvidenceUnavailable,
            "This pre-migration requirement must be re-parsed before evidence can be trusted"
                .into(),
        )
    } else if has_timeout {
        (
            AlignmentClassification::Failed,
            AlignmentReason::TestTimedOut,
            "A linked test exceeded the bounded execution timeout".into(),
        )
    } else if has_failure {
        (
            AlignmentClassification::Failed,
            AlignmentReason::TestFailed,
            "A test associated with this requirement failed".into(),
        )
    } else if has_runtime_unavailable {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::RuntimeUnavailable,
            "The required local test runtime was unavailable or execution was blocked".into(),
        )
    } else if no_scan_evidence && !scan.diagnostics.is_empty() {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::EvidenceUnavailable,
            "Project evidence could not be scanned".into(),
        )
    } else if implementations.is_empty()
        && scan.checked_languages.is_empty()
        && !scan.skipped_languages.is_empty()
    {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::UnsupportedLanguage,
            "Only unsupported implementation languages were found".into(),
        )
    } else if implementations.is_empty() {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::NoImplementationEvidence,
            "No deterministic implementation symbol matched this requirement".into(),
        )
    } else if !has_linked_test {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::ImplementationUntested,
            "Implementation evidence exists, but no linked test evidence was found".into(),
        )
    } else if !has_meaningful && has_non_probative {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::TestNonProbative,
            "Test evidence exists, but its assertions are placeholder, missing, or unlinked".into(),
        )
    } else if has_pass && !has_not_run {
        (
            AlignmentClassification::Verified,
            AlignmentReason::MeaningfulTestPassed,
            "A linked meaningful assertion passed against matched implementation evidence".into(),
        )
    } else if has_pass || has_not_run {
        (
            AlignmentClassification::Partial,
            if has_pass {
                AlignmentReason::PartialEvidence
            } else {
                AlignmentReason::TestNotRun
            },
            if has_pass {
                "Some linked meaningful evidence passed, but part of the evidence set was not executed".into()
            } else {
                "Implementation and meaningful linked test evidence exist, but execution evidence is absent".into()
            },
        )
    } else {
        (
            AlignmentClassification::Unknown,
            AlignmentReason::EvidenceUnavailable,
            "Available evidence is insufficient for a trustworthy classification".into(),
        )
    };

    evidence_records
        .sort_by(|left, right| (&left.kind_rank(), &left.id).cmp(&(&right.kind_rank(), &right.id)));
    Ok(RequirementAlignment {
        requirement_id: requirement.id.clone(),
        classification,
        reason,
        description: requirement.description.clone(),
        section: requirement.section.clone(),
        source_line_start: requirement.source_line_start,
        source_line_end: requirement.source_line_end,
        summary,
        evidence: evidence_records,
    })
}

trait EvidenceRank {
    fn kind_rank(&self) -> u8;
}

impl EvidenceRank for EvidenceRecord {
    fn kind_rank(&self) -> u8 {
        match self.kind {
            EvidenceKind::Requirement => 0,
            EvidenceKind::Implementation => 1,
            EvidenceKind::Test => 2,
            EvidenceKind::Assertion => 3,
            EvidenceKind::Execution => 4,
            EvidenceKind::Diagnostic => 5,
        }
    }
}

fn execution_summary(status: &str, execution_time_ms: i64) -> String {
    match status {
        "passed" => format!("Test process passed in {execution_time_ms} ms"),
        "failed" => format!("Test process failed in {execution_time_ms} ms"),
        "timed_out" => format!("Test process timed out after {execution_time_ms} ms"),
        "runtime_unavailable" => "Required local runtime was unavailable".into(),
        "blocked" => "Execution was blocked by the containment policy".into(),
        other => format!("Test process returned {other} in {execution_time_ms} ms"),
    }
}

fn execution_controls_summary(controls: &crate::models::test::ExecutionControls) -> String {
    if controls.profile.is_empty() {
        return "unavailable (legacy result)".into();
    }
    format!(
        "profile={}, network={}, filesystem_write={}, child_process={}",
        controls.profile, controls.network, controls.filesystem_write, controls.child_process
    )
}

fn count(alignments: &[RequirementAlignment], classification: AlignmentClassification) -> i64 {
    alignments
        .iter()
        .filter(|alignment| alignment.classification == classification)
        .count() as i64
}

fn digest_alignments(alignments: &[RequirementAlignment]) -> Result<String, AppError> {
    let json = serde_json::to_vec(alignments)?;
    let digest = Sha256::digest(json);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::project::CreateProjectRequest;
    use crate::models::test::{GeneratedTest, TestResult};
    use crate::services::spec_parser;
    use std::fs;

    struct Fixture {
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("spec-evidence-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("src")).expect("create src");
            fs::write(
                root.join("src/math.py"),
                "def add_numbers(left, right):\n    return left + right\n",
            )
            .expect("write source");
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn seed(test_code: &str, status: Option<&str>) -> AlignmentReportWithEvidence {
        seed_with_mode(test_code, status, "fixture")
    }

    fn seed_with_mode(
        test_code: &str,
        status: Option<&str>,
        generation_mode: &str,
    ) -> AlignmentReportWithEvidence {
        let fixture = Fixture::new();
        let db = Database::new(&fixture.root.join("app-data")).expect("db");
        let conn = db.conn.lock().expect("lock");
        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Fixture".into(),
                codebase_path: fixture.root.to_string_lossy().to_string(),
            },
        )
        .expect("project");
        let spec = queries::create_spec(
            &conn,
            &project.id,
            "spec.md",
            "## Requirements\n- The system shall expose add numbers\n",
        )
        .expect("spec");
        let reqs = spec_parser::parse_spec(&spec.id, &spec.content).expect("parse");
        queries::insert_requirements(&conn, &reqs).expect("requirements");
        let code = test_code.replace("$REQ", &reqs[0].id);
        let test = GeneratedTest {
            id: "test-1".into(),
            requirement_id: reqs[0].id.clone(),
            framework: "pytest".into(),
            code,
            generation_mode: generation_mode.into(),
            file_path: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        queries::insert_generated_test(&conn, &test).expect("test");
        if let Some(status) = status {
            queries::insert_test_result(
                &conn,
                &TestResult {
                    id: "result-1".into(),
                    generated_test_id: test.id,
                    status: status.into(),
                    execution_time_ms: 5,
                    stdout: String::new(),
                    stderr: String::new(),
                    executed_at: "2026-01-01T00:00:00Z".into(),
                    execution_controls: Default::default(),
                },
            )
            .expect("result");
        }
        let first = generate_report(&conn, &project.id).expect("first report");
        let second = generate_report(&conn, &project.id).expect("second report");
        assert_eq!(first.report.evidence_digest, second.report.evidence_digest);
        second
    }

    fn seed_without_test(
        source_name: &str,
        source: &str,
        description: &str,
    ) -> AlignmentReportWithEvidence {
        let root = std::env::temp_dir().join(format!("spec-evidence-empty-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("fixture");
        fs::write(root.join(source_name), source).expect("source");
        let db = Database::new(&root.join("app-data")).expect("db");
        let conn = db.conn.lock().expect("lock");
        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Fixture".into(),
                codebase_path: root.to_string_lossy().to_string(),
            },
        )
        .expect("project");
        let spec_content = format!("## Requirements\n- {description}\n");
        let spec =
            queries::create_spec(&conn, &project.id, "spec.md", &spec_content).expect("spec");
        let requirements = spec_parser::parse_spec(&spec.id, &spec.content).expect("parse");
        queries::insert_requirements(&conn, &requirements).expect("requirements");
        let report = generate_report(&conn, &project.id).expect("report");
        drop(conn);
        drop(db);
        let _ = fs::remove_dir_all(root);
        report
    }

    #[test]
    fn placeholder_pass_is_never_verified() {
        let report = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert True\n",
            Some("passed"),
        );
        assert_eq!(
            report.alignments[0].classification,
            AlignmentClassification::Unknown
        );
        assert_eq!(
            report.alignments[0].reason,
            AlignmentReason::TestNonProbative
        );
        assert_eq!(report.report.verified_requirements, 0);
    }

    #[test]
    fn unrelated_meaningful_assertion_is_not_verified() {
        let report = seed(
            "# Requirement-ID: $REQ\ndef test_other():\n    assert subtract_numbers(5, 2) == 3\n",
            Some("passed"),
        );
        assert_eq!(
            report.alignments[0].classification,
            AlignmentClassification::Unknown
        );
        assert_eq!(
            report.alignments[0].reason,
            AlignmentReason::TestNonProbative
        );
    }

    #[test]
    fn meaningful_linked_pass_is_verified() {
        let report = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert add_numbers(2, 3) == 5\n",
            Some("passed"),
        );
        assert_eq!(
            report.alignments[0].classification,
            AlignmentClassification::Verified
        );
    }

    #[test]
    fn explicit_repository_link_replaces_source_marker_not_assertion_proof() {
        let meaningful = seed_with_mode(
            "def test_add():\n    assert add_numbers(2, 3) == 5\n",
            Some("passed"),
            "repository_link",
        );
        assert_eq!(
            meaningful.alignments[0].classification,
            AlignmentClassification::Verified
        );
        assert!(meaningful.alignments[0].evidence.iter().any(|evidence| {
            evidence.kind == EvidenceKind::Test && evidence.status == "explicitly_linked"
        }));

        let placeholder = seed_with_mode(
            "def test_add():\n    assert True\n",
            Some("passed"),
            "repository_link",
        );
        assert_eq!(
            placeholder.alignments[0].reason,
            AlignmentReason::TestNonProbative
        );
    }

    #[test]
    fn meaningful_unexecuted_test_is_partial() {
        let report = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert add_numbers(2, 3) == 5\n",
            None,
        );
        assert_eq!(
            report.alignments[0].classification,
            AlignmentClassification::Partial
        );
    }

    #[test]
    fn failures_and_timeouts_are_failed() {
        let failed = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert add_numbers(2, 3) == 5\n",
            Some("failed"),
        );
        assert_eq!(
            failed.alignments[0].classification,
            AlignmentClassification::Failed
        );
        let timeout = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert add_numbers(2, 3) == 5\n",
            Some("timed_out"),
        );
        assert_eq!(timeout.alignments[0].reason, AlignmentReason::TestTimedOut);
    }

    #[test]
    fn deterministic_evidence_digest() {
        let first = seed(
            "# Requirement-ID: $REQ\ndef test_add():\n    assert add_numbers(2, 3) == 5\n",
            Some("passed"),
        );
        assert_eq!(first.report.evidence_digest.len(), 64);
        assert_eq!(
            digest_alignments(&first.alignments).expect("digest"),
            first.report.evidence_digest
        );
    }

    #[test]
    fn implementation_without_test_is_unknown() {
        let report = seed_without_test(
            "math.py",
            "def add_numbers(left, right):\n    return left + right\n",
            "The system shall expose add numbers",
        );
        assert_eq!(
            report.alignments[0].reason,
            AlignmentReason::ImplementationUntested
        );
        assert_eq!(
            report.alignments[0].classification,
            AlignmentClassification::Unknown
        );
    }

    #[test]
    fn requirement_without_implementation_is_unknown() {
        let report = seed_without_test(
            "math.py",
            "def add_numbers(left, right):\n    return left + right\n",
            "The system shall archive invoices",
        );
        assert_eq!(
            report.alignments[0].reason,
            AlignmentReason::NoImplementationEvidence
        );
    }

    #[test]
    fn unsupported_language_is_unknown() {
        let report = seed_without_test(
            "main.rs",
            "pub fn add_numbers(left: i32, right: i32) -> i32 { left + right }",
            "The system shall expose add numbers",
        );
        assert_eq!(
            report.alignments[0].reason,
            AlignmentReason::UnsupportedLanguage
        );
        assert_eq!(report.report.skipped_languages, vec!["rs"]);
    }
}
