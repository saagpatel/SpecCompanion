use crate::db::queries;
use crate::errors::AppError;
use crate::models::test::{GeneratedTest, RepositoryTestCandidate};
use crate::services::evidence::{self, AssertionQuality, TestCandidate};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

pub fn discover(
    project_root: &str,
    exclusions: &[String],
) -> Result<Vec<RepositoryTestCandidate>, AppError> {
    let scan = evidence::scan_project(project_root, exclusions)?;
    Ok(scan.tests.iter().map(to_candidate).collect())
}

pub fn link(
    conn: &Connection,
    project_id: &str,
    requirement_id: &str,
    relative_path: &str,
    exclusions: &[String],
) -> Result<GeneratedTest, AppError> {
    if project_id.trim().is_empty()
        || requirement_id.trim().is_empty()
        || relative_path.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "Project, requirement, and repository test path are required".into(),
        ));
    }

    let project = queries::get_project(conn, project_id)?;
    queries::get_requirement_for_project(conn, requirement_id, project_id)?;
    let scan = evidence::scan_project(&project.project.codebase_path, exclusions)?;
    let test = scan
        .tests
        .iter()
        .find(|candidate| candidate.path == relative_path)
        .ok_or_else(|| {
            AppError::InvalidInput(
                "Repository test path was not found in the contained evidence scan".into(),
            )
        })?;

    let root = std::fs::canonicalize(&project.project.codebase_path)?;
    let absolute = std::fs::canonicalize(root.join(&test.path))?;
    if !absolute.starts_with(&root) || !absolute.is_file() {
        return Err(AppError::InvalidInput(
            "Repository test resolves outside the target project".into(),
        ));
    }

    if let Some(existing) = queries::get_generated_tests_for_requirement(conn, requirement_id)?
        .into_iter()
        .find(|candidate| {
            candidate.generation_mode == "repository_link"
                && candidate.file_path.as_deref() == Some(absolute.to_string_lossy().as_ref())
        })
    {
        return Ok(existing);
    }

    let linked = GeneratedTest {
        id: Uuid::new_v4().to_string(),
        requirement_id: requirement_id.to_string(),
        framework: infer_framework(test).to_string(),
        code: test.code.clone(),
        generation_mode: "repository_link".into(),
        file_path: Some(absolute.to_string_lossy().to_string()),
        created_at: Utc::now().to_rfc3339(),
    };
    queries::insert_generated_test(conn, &linked)?;
    Ok(linked)
}

fn to_candidate(test: &TestCandidate) -> RepositoryTestCandidate {
    let (assertion_status, assertion_lines) = match evidence::analyze_assertions(&test.code) {
        AssertionQuality::Meaningful(lines) => ("meaningful", lines),
        AssertionQuality::Placeholder(lines) => ("placeholder", lines),
        AssertionQuality::Missing => ("missing", Vec::new()),
    };
    RepositoryTestCandidate {
        path: test.path.clone(),
        language: test.language.clone(),
        framework: infer_framework(test).to_string(),
        assertion_status: assertion_status.into(),
        assertion_lines,
    }
}

fn infer_framework(test: &TestCandidate) -> &'static str {
    match test.language.as_str() {
        "python"
            if test.code.contains("unittest.TestCase")
                || test.code.contains("from unittest")
                || test.code.contains("import unittest") =>
        {
            "unittest"
        }
        "python" => "pytest",
        _ if test.code.contains("from 'vitest'")
            || test.code.contains("from \"vitest\"")
            || test.code.contains("require('vitest')")
            || test.code.contains("require(\"vitest\")") =>
        {
            "vitest"
        }
        _ => "jest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::project::CreateProjectRequest;
    use crate::services::spec_parser;
    use std::fs;
    use std::path::Path;

    struct Fixture {
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("spec-repository-tests-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("src")).expect("source directory");
            fs::create_dir_all(root.join("tests")).expect("test directory");
            fs::write(
                root.join("src/recovery.py"),
                "def recover_history():\n    return 'restored'\n",
            )
            .expect("source");
            fs::write(
                root.join("tests/test_recovery.py"),
                "import unittest\nfrom src.recovery import recover_history\n\nclass RecoveryTests(unittest.TestCase):\n    def test_restore(self):\n        self.assertEqual(recover_history(), 'restored')\n",
            )
            .expect("test");
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn discovers_unittest_and_links_only_scanned_contained_paths() {
        let fixture = Fixture::new();
        let candidates = discover(&fixture.root.to_string_lossy(), &[]).expect("discover");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].framework, "unittest");
        assert_eq!(candidates[0].assertion_status, "meaningful");

        let db = Database::new(&fixture.root.join("app-data")).expect("db");
        let conn = db.conn.lock().expect("lock");
        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Dogfood".into(),
                codebase_path: fixture.root.to_string_lossy().to_string(),
            },
        )
        .expect("project");
        let spec = queries::create_spec(
            &conn,
            &project.id,
            "requirements.md",
            "## Requirements\n- The system shall recover history\n",
        )
        .expect("spec");
        let requirements = spec_parser::parse_spec(&spec.id, &spec.content).expect("parse");
        queries::insert_requirements(&conn, &requirements).expect("requirements");

        let linked = link(
            &conn,
            &project.id,
            &requirements[0].id,
            "tests/test_recovery.py",
            &[],
        )
        .expect("link");
        assert_eq!(linked.framework, "unittest");
        assert_eq!(linked.generation_mode, "repository_link");
        assert!(Path::new(linked.file_path.as_deref().expect("path")).is_absolute());

        let duplicate = link(
            &conn,
            &project.id,
            &requirements[0].id,
            "tests/test_recovery.py",
            &[],
        )
        .expect("idempotent link");
        assert_eq!(duplicate.id, linked.id);

        let traversal = link(
            &conn,
            &project.id,
            &requirements[0].id,
            "../outside.py",
            &[],
        )
        .expect_err("unscanned traversal must fail");
        assert!(traversal.to_string().contains("contained evidence scan"));
    }

    #[test]
    #[ignore = "requires disposable copies of real TypeScript and Python repositories"]
    fn real_repository_dogfood_vitest_and_unittest() {
        let typescript = std::env::var("SPECCOMPANION_DOGFOOD_TYPESCRIPT")
            .expect("TypeScript dogfood copy path");
        let python =
            std::env::var("SPECCOMPANION_DOGFOOD_PYTHON").expect("Python dogfood copy path");

        dogfood_repository(
            Path::new(&typescript),
            "The system shall simulate deployment boundaries",
            "src/model.test.ts",
            "vitest",
        );
        dogfood_repository(
            Path::new(&python),
            "The system shall recover competing repository histories",
            "tests/test_recovery.py",
            "unittest",
        );
    }

    fn dogfood_repository(root: &Path, description: &str, test_path: &str, framework: &str) {
        use crate::models::test::TestResult;
        use crate::services::{alignment, test_runner};

        let db = Database::new(&root.join(".spec-companion-dogfood")).expect("dogfood db");
        let conn = db.conn.lock().expect("dogfood db lock");
        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: format!("{framework} dogfood"),
                codebase_path: root.to_string_lossy().to_string(),
            },
        )
        .expect("dogfood project");
        let spec_content = format!("## Requirements\n- {description}\n");
        let spec =
            queries::create_spec(&conn, &project.id, "dogfood-requirements.md", &spec_content)
                .expect("dogfood spec");
        let requirements = spec_parser::parse_spec(&spec.id, &spec.content).expect("dogfood parse");
        queries::insert_requirements(&conn, &requirements).expect("dogfood requirements");
        let linked =
            link(&conn, &project.id, &requirements[0].id, test_path, &[]).expect("dogfood link");
        assert_eq!(linked.framework, framework);

        let execution = match framework {
            "vitest" => test_runner::run_vitest_test(
                linked.file_path.as_deref().expect("vitest path"),
                &root.to_string_lossy(),
            ),
            "unittest" => test_runner::run_unittest_test(
                linked.file_path.as_deref().expect("unittest path"),
                &root.to_string_lossy(),
            ),
            other => panic!("unsupported dogfood framework: {other}"),
        }
        .expect("bounded dogfood execution");
        assert_eq!(
            execution.status, "passed",
            "{framework} dogfood stderr: {}",
            execution.stderr
        );
        queries::insert_test_result(
            &conn,
            &TestResult {
                id: Uuid::new_v4().to_string(),
                generated_test_id: linked.id,
                status: execution.status,
                execution_time_ms: execution.execution_time_ms,
                stdout: execution.stdout,
                stderr: execution.stderr,
                executed_at: Utc::now().to_rfc3339(),
                execution_controls: execution.execution_controls,
                provenance_digest: String::new(),
                provenance_status: String::new(),
            },
        )
        .expect("dogfood result");
        let report = alignment::generate_report(&conn, &project.id).expect("dogfood report");
        assert_eq!(
            report.alignments[0].classification,
            crate::models::report::AlignmentClassification::Verified
        );
        assert_eq!(report.report.verified_requirements, 1);
    }
}
