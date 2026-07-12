use crate::errors::AppError;
use crate::models::project::{CreateProjectRequest, Project, ProjectWithStats};
use crate::models::report::{AlignmentReport, AlignmentReportWithEvidence, RequirementAlignment};
use crate::models::spec::{Requirement, Spec};
use crate::models::test::{GeneratedTest, TestResult};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

// ─── Projects ───────────────────────────────────────────────────

pub fn create_project(conn: &Connection, req: &CreateProjectRequest) -> Result<Project, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO projects (id, name, codebase_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, req.name, req.codebase_path, now, now],
    )?;
    Ok(Project {
        id,
        name: req.name.clone(),
        codebase_path: req.codebase_path.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_projects(conn: &Connection) -> Result<Vec<ProjectWithStats>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.codebase_path, p.created_at, p.updated_at,
                COALESCE((SELECT COUNT(*) FROM specs WHERE project_id = p.id), 0) as spec_count,
                (SELECT coverage_percent FROM alignment_reports WHERE project_id = p.id ORDER BY generated_at DESC LIMIT 1) as coverage_percent,
                (SELECT generated_at FROM alignment_reports WHERE project_id = p.id ORDER BY generated_at DESC LIMIT 1) as last_run_at
         FROM projects p ORDER BY p.updated_at DESC"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ProjectWithStats {
            project: Project {
                id: row.get(0)?,
                name: row.get(1)?,
                codebase_path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            },
            spec_count: row.get(5)?,
            coverage_percent: row.get(6)?,
            last_run_at: row.get(7)?,
        })
    })?;
    let mut projects = Vec::new();
    for row in rows {
        projects.push(row?);
    }
    Ok(projects)
}

pub fn get_project(conn: &Connection, id: &str) -> Result<ProjectWithStats, AppError> {
    conn.query_row(
        "SELECT p.id, p.name, p.codebase_path, p.created_at, p.updated_at,
                COALESCE((SELECT COUNT(*) FROM specs WHERE project_id = p.id), 0) as spec_count,
                (SELECT coverage_percent FROM alignment_reports WHERE project_id = p.id ORDER BY generated_at DESC LIMIT 1) as coverage_percent,
                (SELECT generated_at FROM alignment_reports WHERE project_id = p.id ORDER BY generated_at DESC LIMIT 1) as last_run_at
         FROM projects p WHERE p.id = ?1",
        params![id],
        |row| {
            Ok(ProjectWithStats {
                project: Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    codebase_path: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                },
                spec_count: row.get(5)?,
                coverage_percent: row.get(6)?,
                last_run_at: row.get(7)?,
            })
        },
    ).map_err(|_| AppError::NotFound(format!("Project not found: {}", id)))
}

pub fn delete_project(conn: &Connection, id: &str) -> Result<(), AppError> {
    let affected = conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("Project not found: {}", id)));
    }
    Ok(())
}

pub fn touch_project_updated_at(conn: &Connection, project_id: &str) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
        params![now, project_id],
    )?;
    Ok(())
}

// ─── Specs ──────────────────────────────────────────────────────

pub fn create_spec(
    conn: &Connection,
    project_id: &str,
    filename: &str,
    content: &str,
) -> Result<Spec, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO specs (id, project_id, filename, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, project_id, filename, content, now],
    )?;
    Ok(Spec {
        id,
        project_id: project_id.to_string(),
        filename: filename.to_string(),
        content: content.to_string(),
        parsed_at: None,
        created_at: now,
    })
}

pub fn get_spec(conn: &Connection, id: &str) -> Result<Spec, AppError> {
    conn.query_row(
        "SELECT id, project_id, filename, content, parsed_at, created_at FROM specs WHERE id = ?1",
        params![id],
        |row| {
            Ok(Spec {
                id: row.get(0)?,
                project_id: row.get(1)?,
                filename: row.get(2)?,
                content: row.get(3)?,
                parsed_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
    .map_err(|_| AppError::NotFound(format!("Spec not found: {}", id)))
}

pub fn list_specs(conn: &Connection, project_id: &str) -> Result<Vec<Spec>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, filename, content, parsed_at, created_at FROM specs WHERE project_id = ?1 ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(Spec {
            id: row.get(0)?,
            project_id: row.get(1)?,
            filename: row.get(2)?,
            content: row.get(3)?,
            parsed_at: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    let mut specs = Vec::new();
    for row in rows {
        specs.push(row?);
    }
    Ok(specs)
}

pub fn delete_spec(conn: &Connection, id: &str) -> Result<(), AppError> {
    let affected = conn.execute("DELETE FROM specs WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("Spec not found: {}", id)));
    }
    Ok(())
}

pub fn update_spec_parsed_at(conn: &Connection, spec_id: &str) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE specs SET parsed_at = ?1 WHERE id = ?2",
        params![now, spec_id],
    )?;
    Ok(())
}

// ─── Requirements ───────────────────────────────────────────────

pub fn insert_requirements(
    conn: &Connection,
    requirements: &[Requirement],
) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "INSERT INTO requirements (id, spec_id, section, description, req_type, priority, content_fingerprint, source_line_start, source_line_end) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;
    for req in requirements {
        stmt.execute(params![
            req.id,
            req.spec_id,
            req.section,
            req.description,
            req.req_type,
            req.priority,
            req.content_fingerprint,
            req.source_line_start,
            req.source_line_end
        ])?;
    }
    Ok(())
}

pub fn get_requirements_for_spec(
    conn: &Connection,
    spec_id: &str,
) -> Result<Vec<Requirement>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, spec_id, section, description, req_type, priority, content_fingerprint, source_line_start, source_line_end FROM requirements WHERE spec_id = ?1 ORDER BY source_line_start, id"
    )?;
    let rows = stmt.query_map(params![spec_id], |row| {
        Ok(Requirement {
            id: row.get(0)?,
            spec_id: row.get(1)?,
            section: row.get(2)?,
            description: row.get(3)?,
            req_type: row.get(4)?,
            priority: row.get(5)?,
            content_fingerprint: row.get(6)?,
            source_line_start: row.get(7)?,
            source_line_end: row.get(8)?,
        })
    })?;
    let mut reqs = Vec::new();
    for row in rows {
        reqs.push(row?);
    }
    Ok(reqs)
}

pub fn get_requirements_for_project(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<Requirement>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.spec_id, r.section, r.description, r.req_type, r.priority, r.content_fingerprint, r.source_line_start, r.source_line_end
         FROM requirements r
         JOIN specs s ON r.spec_id = s.id
         WHERE s.project_id = ?1
         ORDER BY s.filename, r.source_line_start, r.id"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(Requirement {
            id: row.get(0)?,
            spec_id: row.get(1)?,
            section: row.get(2)?,
            description: row.get(3)?,
            req_type: row.get(4)?,
            priority: row.get(5)?,
            content_fingerprint: row.get(6)?,
            source_line_start: row.get(7)?,
            source_line_end: row.get(8)?,
        })
    })?;
    let mut reqs = Vec::new();
    for row in rows {
        reqs.push(row?);
    }
    Ok(reqs)
}

pub fn delete_requirements_for_spec(conn: &Connection, spec_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM requirements WHERE spec_id = ?1",
        params![spec_id],
    )?;
    Ok(())
}

pub fn get_requirement_for_project(
    conn: &Connection,
    id: &str,
    project_id: &str,
) -> Result<Requirement, AppError> {
    conn.query_row(
        "SELECT r.id, r.spec_id, r.section, r.description, r.req_type, r.priority,
                r.content_fingerprint, r.source_line_start, r.source_line_end
         FROM requirements r
         JOIN specs s ON r.spec_id = s.id
         WHERE r.id = ?1 AND s.project_id = ?2",
        params![id, project_id],
        |row| {
            Ok(Requirement {
                id: row.get(0)?,
                spec_id: row.get(1)?,
                section: row.get(2)?,
                description: row.get(3)?,
                req_type: row.get(4)?,
                priority: row.get(5)?,
                content_fingerprint: row.get(6)?,
                source_line_start: row.get(7)?,
                source_line_end: row.get(8)?,
            })
        },
    )
    .map_err(|_| {
        AppError::NotFound(format!(
            "Requirement {id} does not belong to project {project_id}"
        ))
    })
}

// ─── Generated Tests ────────────────────────────────────────────

pub fn insert_generated_test(conn: &Connection, test: &GeneratedTest) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO generated_tests (id, requirement_id, framework, code, generation_mode, file_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![test.id, test.requirement_id, test.framework, test.code, test.generation_mode, test.file_path, test.created_at],
    )?;
    Ok(())
}

pub fn get_generated_tests_for_requirement(
    conn: &Connection,
    requirement_id: &str,
) -> Result<Vec<GeneratedTest>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, requirement_id, framework, code, generation_mode, file_path, created_at FROM generated_tests WHERE requirement_id = ?1 ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map(params![requirement_id], |row| {
        Ok(GeneratedTest {
            id: row.get(0)?,
            requirement_id: row.get(1)?,
            framework: row.get(2)?,
            code: row.get(3)?,
            generation_mode: row.get(4)?,
            file_path: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut tests = Vec::new();
    for row in rows {
        tests.push(row?);
    }
    Ok(tests)
}

pub fn get_generated_test(conn: &Connection, id: &str) -> Result<GeneratedTest, AppError> {
    conn.query_row(
        "SELECT id, requirement_id, framework, code, generation_mode, file_path, created_at FROM generated_tests WHERE id = ?1",
        params![id],
        |row| {
            Ok(GeneratedTest {
                id: row.get(0)?,
                requirement_id: row.get(1)?,
                framework: row.get(2)?,
                code: row.get(3)?,
                generation_mode: row.get(4)?,
                file_path: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    ).map_err(|_| AppError::NotFound(format!("Generated test not found: {}", id)))
}

pub fn get_generated_test_for_project(
    conn: &Connection,
    id: &str,
    project_id: &str,
) -> Result<GeneratedTest, AppError> {
    conn.query_row(
        "SELECT gt.id, gt.requirement_id, gt.framework, gt.code, gt.generation_mode, gt.file_path, gt.created_at
         FROM generated_tests gt
         JOIN requirements r ON gt.requirement_id = r.id
         JOIN specs s ON r.spec_id = s.id
         WHERE gt.id = ?1 AND s.project_id = ?2",
        params![id, project_id],
        |row| Ok(GeneratedTest {
            id: row.get(0)?, requirement_id: row.get(1)?, framework: row.get(2)?,
            code: row.get(3)?, generation_mode: row.get(4)?, file_path: row.get(5)?,
            created_at: row.get(6)?,
        }),
    ).map_err(|_| AppError::NotFound(format!("Generated test {id} does not belong to project {project_id}")))
}

pub fn update_generated_test_path(conn: &Connection, id: &str, path: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE generated_tests SET file_path = ?1 WHERE id = ?2",
        params![path, id],
    )?;
    Ok(())
}

pub fn update_generated_test_code(conn: &Connection, id: &str, code: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE generated_tests SET code = ?1 WHERE id = ?2",
        params![code, id],
    )?;
    Ok(())
}

pub fn get_generated_tests_for_project(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<GeneratedTest>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT gt.id, gt.requirement_id, gt.framework, gt.code, gt.generation_mode, gt.file_path, gt.created_at
         FROM generated_tests gt
         JOIN requirements r ON gt.requirement_id = r.id
         JOIN specs s ON r.spec_id = s.id
         WHERE s.project_id = ?1
         ORDER BY gt.created_at DESC"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(GeneratedTest {
            id: row.get(0)?,
            requirement_id: row.get(1)?,
            framework: row.get(2)?,
            code: row.get(3)?,
            generation_mode: row.get(4)?,
            file_path: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut tests = Vec::new();
    for row in rows {
        tests.push(row?);
    }
    Ok(tests)
}

// ─── Test Results ───────────────────────────────────────────────

pub fn insert_test_result(conn: &Connection, result: &TestResult) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO test_results (id, generated_test_id, status, execution_time_ms, stdout, stderr, executed_at, execution_controls_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![result.id, result.generated_test_id, result.status, result.execution_time_ms, result.stdout, result.stderr, result.executed_at, serde_json::to_string(&result.execution_controls)?],
    )?;
    Ok(())
}

pub fn get_test_results_for_project(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<TestResult>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT tr.id, tr.generated_test_id, tr.status, tr.execution_time_ms, tr.stdout, tr.stderr, tr.executed_at, tr.execution_controls_json
         FROM test_results tr
         JOIN generated_tests gt ON tr.generated_test_id = gt.id
         JOIN requirements r ON gt.requirement_id = r.id
         JOIN specs s ON r.spec_id = s.id
         WHERE s.project_id = ?1
         ORDER BY tr.executed_at DESC"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(TestResult {
            id: row.get(0)?,
            generated_test_id: row.get(1)?,
            status: row.get(2)?,
            execution_time_ms: row.get(3)?,
            stdout: row.get(4)?,
            stderr: row.get(5)?,
            executed_at: row.get(6)?,
            execution_controls: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn get_test_result(conn: &Connection, id: &str) -> Result<TestResult, AppError> {
    conn.query_row(
        "SELECT id, generated_test_id, status, execution_time_ms, stdout, stderr, executed_at, execution_controls_json FROM test_results WHERE id = ?1",
        params![id],
        |row| {
            Ok(TestResult {
                id: row.get(0)?,
                generated_test_id: row.get(1)?,
                status: row.get(2)?,
                execution_time_ms: row.get(3)?,
                stdout: row.get(4)?,
                stderr: row.get(5)?,
                executed_at: row.get(6)?,
                execution_controls: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
            })
        },
    ).map_err(|_| AppError::NotFound(format!("Test result not found: {}", id)))
}

pub fn get_latest_test_result_for_test(
    conn: &Connection,
    generated_test_id: &str,
) -> Result<Option<TestResult>, AppError> {
    let result = conn.query_row(
        "SELECT id, generated_test_id, status, execution_time_ms, stdout, stderr, executed_at, execution_controls_json FROM test_results WHERE generated_test_id = ?1 ORDER BY executed_at DESC LIMIT 1",
        params![generated_test_id],
        |row| {
            Ok(TestResult {
                id: row.get(0)?,
                generated_test_id: row.get(1)?,
                status: row.get(2)?,
                execution_time_ms: row.get(3)?,
                stdout: row.get(4)?,
                stderr: row.get(5)?,
                executed_at: row.get(6)?,
                execution_controls: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
            })
        },
    );
    match result {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

// ─── Alignment Reports ─────────────────────────────────────────

pub fn insert_alignment_report(
    conn: &Connection,
    report: &AlignmentReport,
) -> Result<(), AppError> {
    let checked_languages = serde_json::to_string(&report.checked_languages)?;
    let skipped_languages = serde_json::to_string(&report.skipped_languages)?;
    let diagnostics = serde_json::to_string(&report.diagnostics)?;
    conn.execute(
        "INSERT INTO alignment_reports (id, project_id, coverage_percent, total_requirements, covered_requirements, verified_requirements, partial_requirements, failed_requirements, unknown_requirements, evidence_digest, checked_languages_json, skipped_languages_json, diagnostics_json, generated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![report.id, report.project_id, report.coverage_percent, report.total_requirements, report.covered_requirements, report.verified_requirements, report.partial_requirements, report.failed_requirements, report.unknown_requirements, report.evidence_digest, checked_languages, skipped_languages, diagnostics, report.generated_at],
    )?;
    Ok(())
}

pub fn insert_requirement_alignment(
    conn: &Connection,
    alignment: &RequirementAlignment,
    report_id: &str,
    sort_index: usize,
) -> Result<(), AppError> {
    let details = serde_json::to_string(alignment)?;
    conn.execute(
        "INSERT INTO requirement_alignments (report_id, requirement_id, classification, reason, details_json, sort_index) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![report_id, alignment.requirement_id, alignment.classification.as_str(), alignment.reason.as_str(), details, sort_index as i64],
    )?;
    Ok(())
}

pub fn get_alignment_report(
    conn: &Connection,
    id: &str,
) -> Result<AlignmentReportWithEvidence, AppError> {
    let report = conn.query_row(
        "SELECT id, project_id, coverage_percent, total_requirements, covered_requirements, verified_requirements, partial_requirements, failed_requirements, unknown_requirements, evidence_digest, checked_languages_json, skipped_languages_json, diagnostics_json, generated_at FROM alignment_reports WHERE id = ?1",
        params![id],
        |row| {
            Ok(AlignmentReport {
                id: row.get(0)?,
                project_id: row.get(1)?,
                coverage_percent: row.get(2)?,
                total_requirements: row.get(3)?,
                covered_requirements: row.get(4)?,
                verified_requirements: row.get(5)?, partial_requirements: row.get(6)?,
                failed_requirements: row.get(7)?, unknown_requirements: row.get(8)?,
                evidence_digest: row.get(9)?,
                checked_languages: parse_json_column(row.get(10)?)?,
                skipped_languages: parse_json_column(row.get(11)?)?,
                diagnostics: parse_json_column(row.get(12)?)?,
                generated_at: row.get(13)?,
            })
        },
    ).map_err(|_| AppError::NotFound(format!("Report not found: {}", id)))?;

    let alignments = get_requirement_alignments_for_report(conn, &report.id)?;
    Ok(AlignmentReportWithEvidence { report, alignments })
}

pub fn get_requirement_alignments_for_report(
    conn: &Connection,
    report_id: &str,
) -> Result<Vec<RequirementAlignment>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT details_json FROM requirement_alignments WHERE report_id = ?1 ORDER BY sort_index, requirement_id"
    )?;
    let rows = stmt.query_map(params![report_id], |row| {
        let json: String = row.get(0)?;
        serde_json::from_str::<RequirementAlignment>(&json).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        })
    })?;
    let mut alignments = Vec::new();
    for row in rows {
        alignments.push(row?);
    }
    Ok(alignments)
}

pub fn list_reports(conn: &Connection, project_id: &str) -> Result<Vec<AlignmentReport>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, coverage_percent, total_requirements, covered_requirements, verified_requirements, partial_requirements, failed_requirements, unknown_requirements, evidence_digest, checked_languages_json, skipped_languages_json, diagnostics_json, generated_at FROM alignment_reports WHERE project_id = ?1 ORDER BY generated_at DESC"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(AlignmentReport {
            id: row.get(0)?,
            project_id: row.get(1)?,
            coverage_percent: row.get(2)?,
            total_requirements: row.get(3)?,
            covered_requirements: row.get(4)?,
            verified_requirements: row.get(5)?,
            partial_requirements: row.get(6)?,
            failed_requirements: row.get(7)?,
            unknown_requirements: row.get(8)?,
            evidence_digest: row.get(9)?,
            checked_languages: parse_json_column(row.get(10)?)?,
            skipped_languages: parse_json_column(row.get(11)?)?,
            diagnostics: parse_json_column(row.get(12)?)?,
            generated_at: row.get(13)?,
        })
    })?;
    let mut reports = Vec::new();
    for row in rows {
        reports.push(row?);
    }
    Ok(reports)
}

fn parse_json_column(value: String) -> Result<Vec<String>, rusqlite::Error> {
    serde_json::from_str(&value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use crate::models::test::GeneratedTest;

    #[test]
    fn project_scoped_queries_reject_cross_project_ids() {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("PRAGMA foreign_keys=ON")
            .expect("foreign keys");
        schema::run_migrations(&conn).expect("migrations");
        let first = create_project(
            &conn,
            &CreateProjectRequest {
                name: "First".into(),
                codebase_path: "/tmp/first".into(),
            },
        )
        .expect("first project");
        let second = create_project(
            &conn,
            &CreateProjectRequest {
                name: "Second".into(),
                codebase_path: "/tmp/second".into(),
            },
        )
        .expect("second project");
        let spec = create_spec(&conn, &first.id, "spec.md", "content").expect("spec");
        let requirement = Requirement {
            id: "req-one".into(),
            spec_id: spec.id,
            section: "Requirements".into(),
            description: "The system shall add numbers".into(),
            req_type: "functional".into(),
            priority: "medium".into(),
            content_fingerprint: "fingerprint".into(),
            source_line_start: 1,
            source_line_end: 1,
        };
        insert_requirements(&conn, std::slice::from_ref(&requirement)).expect("requirement");
        let test = GeneratedTest {
            id: "test-one".into(),
            requirement_id: requirement.id.clone(),
            framework: "jest".into(),
            code: "expect(add(2, 3)).toBe(5)".into(),
            generation_mode: "fixture".into(),
            file_path: None,
            created_at: "now".into(),
        };
        insert_generated_test(&conn, &test).expect("test");

        assert!(get_requirement_for_project(&conn, &requirement.id, &second.id).is_err());
        assert!(get_generated_test_for_project(&conn, &test.id, &second.id).is_err());
        assert!(get_requirement_for_project(&conn, &requirement.id, &first.id).is_ok());
        assert!(get_generated_test_for_project(&conn, &test.id, &first.id).is_ok());
    }
}
