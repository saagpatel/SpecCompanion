mod commands;
mod db;
mod errors;
mod models;
mod services;
mod utils;

use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            let database = Database::new(&app_data_dir).expect("failed to initialize database");
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Projects
            commands::project::create_project,
            commands::project::list_projects,
            commands::project::get_project,
            commands::project::delete_project,
            commands::project::validate_path,
            // Specs
            commands::spec::upload_spec,
            commands::spec::get_spec,
            commands::spec::list_specs,
            commands::spec::delete_spec,
            commands::spec::reparse_spec,
            commands::spec::read_file_content,
            // Test Generation
            commands::test_gen::generate_tests,
            commands::test_gen::get_generated_tests,
            commands::test_gen::get_all_generated_tests,
            commands::test_gen::list_repository_tests,
            commands::test_gen::link_repository_test,
            commands::test_gen::save_test_to_disk,
            commands::test_gen::save_settings,
            commands::test_gen::load_settings,
            // Test Execution
            commands::test_exec::execute_tests,
            commands::test_exec::configure_project_python_runtime,
            commands::test_exec::get_project_python_runtime_status,
            commands::test_exec::clear_project_python_runtime,
            commands::test_exec::get_test_results,
            commands::test_exec::get_test_result,
            // Reports
            commands::report::generate_alignment_report,
            commands::report::get_alignment_report,
            commands::report::list_reports,
            commands::report::export_report,
            commands::report::verify_evidence_bundle,
            commands::report::create_signing_identity,
            commands::report::export_signed_evidence_bundle,
            commands::report::set_signer_trust,
            commands::report::list_signer_trust,
            commands::report::list_signer_trust_history,
            commands::report::get_signer_trust_history_integrity,
            commands::report::rotate_signer_trust,
            commands::report::export_signer_trust_policy,
            commands::report::verify_signer_trust_policy,
            commands::report::advance_trust_anchor_witness,
            commands::report::list_trust_anchor_advancements,
            commands::report::export_trust_anchor_advancements,
            commands::report::verify_trust_anchor_advancements,
            commands::report::import_signer_trust_policy,
            // Git
            commands::git::get_repo_info,
            commands::git::get_changed_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod desktop_workflow_tests {
    use super::*;
    use crate::db::queries;
    use crate::models::project::CreateProjectRequest;
    use crate::models::test::{GeneratedTest, TestResult};
    use crate::services::{alignment, spec_parser, test_runner};
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[cfg(unix)]
    #[test]
    fn desktop_workflow_smoke_generates_executes_and_reports() {
        let fixture = WorkflowFixture::new("desktop_workflow_smoke");
        let codebase_path = fixture.root.join("codebase");
        fs::create_dir_all(&codebase_path).expect("create codebase fixture");
        fs::write(
            codebase_path.join("calculator.js"),
            "export function addNumbers(left, right) { return left + right; }\n",
        )
        .expect("write code fixture");

        let db = Database::new(&fixture.root.join("app-data")).expect("initialize database");
        let conn = db.conn.lock().expect("lock database");

        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Workflow Smoke".to_string(),
                codebase_path: codebase_path.to_string_lossy().to_string(),
            },
        )
        .expect("create project");

        let spec = queries::create_spec(
            &conn,
            &project.id,
            "workflow-smoke.md",
            "# Workflow Smoke\n\n## Requirements\n\n- The system shall expose add numbers\n",
        )
        .expect("create spec");
        let requirements = spec_parser::parse_spec(&spec.id, &spec.content).expect("parse spec");
        assert_eq!(requirements.len(), 1);
        queries::insert_requirements(&conn, &requirements).expect("insert requirements");
        queries::update_spec_parsed_at(&conn, &spec.id).expect("mark spec parsed");

        let generated = GeneratedTest {
            id: Uuid::new_v4().to_string(),
            requirement_id: requirements[0].id.clone(),
            framework: "jest".to_string(),
            code: format!(
                "// Requirement-ID: {}\nexpect(addNumbers(2, 3)).toBe(5);\n",
                requirements[0].id
            ),
            generation_mode: "fixture".to_string(),
            file_path: None,
            created_at: Utc::now().to_rfc3339(),
        };
        queries::insert_generated_test(&conn, &generated).expect("store generated test");

        let test_path = codebase_path.join("calculator.test.js");
        fs::write(&test_path, &generated.code).expect("write generated pytest file");
        queries::update_generated_test_path(&conn, &generated.id, &test_path.to_string_lossy())
            .expect("store generated test path");

        let runner_dir = codebase_path.join("node_modules/.bin");
        fs::create_dir_all(&runner_dir).expect("create runner dir");
        let runner = runner_dir.join("jest");
        fs::write(&runner, "#!/bin/sh\nexit 0\n").expect("write local jest fixture");
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&runner)
            .expect("runner metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&runner, permissions).expect("runner permissions");

        let execution = test_runner::run_jest_test(
            &test_path.to_string_lossy(),
            &codebase_path.to_string_lossy(),
        )
        .expect("run generated pytest");
        assert_eq!(execution.status, "passed", "stderr: {}", execution.stderr);

        let result = TestResult {
            id: Uuid::new_v4().to_string(),
            generated_test_id: generated.id.clone(),
            status: execution.status,
            execution_time_ms: execution.execution_time_ms,
            stdout: execution.stdout,
            stderr: execution.stderr,
            executed_at: Utc::now().to_rfc3339(),
            execution_controls: execution.execution_controls,
            provenance_digest: String::new(),
            provenance_status: String::new(),
        };
        queries::insert_test_result(&conn, &result).expect("store test result");

        let report =
            alignment::generate_report(&conn, &project.id).expect("generate alignment report");
        assert_eq!(report.report.total_requirements, 1);
        assert_eq!(report.report.covered_requirements, 1);
        assert_eq!(report.report.coverage_percent, 100.0);
        assert_eq!(report.report.verified_requirements, 1);
        assert_eq!(
            report.alignments[0].classification,
            crate::models::report::AlignmentClassification::Verified
        );
    }

    struct WorkflowFixture {
        root: PathBuf,
    }

    impl WorkflowFixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "spec-companion-{}-{}-{}",
                name,
                std::process::id(),
                Uuid::new_v4()
            ));
            fs::create_dir_all(&root).expect("create workflow fixture");
            Self { root }
        }
    }

    impl Drop for WorkflowFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
