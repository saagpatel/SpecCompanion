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
            commands::test_gen::save_test_to_disk,
            commands::test_gen::save_settings,
            commands::test_gen::load_settings,
            // Test Execution
            commands::test_exec::execute_tests,
            commands::test_exec::get_test_results,
            commands::test_exec::get_test_result,
            // Reports
            commands::report::generate_alignment_report,
            commands::report::get_alignment_report,
            commands::report::list_reports,
            commands::report::export_report,
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
    use crate::services::{alignment, spec_parser, template_generator, test_runner};
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn desktop_workflow_smoke_generates_executes_and_reports() {
        let fixture = WorkflowFixture::new("desktop_workflow_smoke");
        let codebase_path = fixture.root.join("codebase");
        fs::create_dir_all(&codebase_path).expect("create codebase fixture");
        fs::write(
            codebase_path.join("calculator.py"),
            "def add(left, right):\n    return left + right\n",
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
            "# Workflow Smoke\n\n## Requirements\n\n- The system shall add two numbers\n",
        )
        .expect("create spec");
        let requirements = spec_parser::parse_spec(&spec.id, &spec.content);
        assert_eq!(requirements.len(), 1);
        queries::insert_requirements(&conn, &requirements).expect("insert requirements");
        queries::update_spec_parsed_at(&conn, &spec.id).expect("mark spec parsed");

        let generated = GeneratedTest {
            id: Uuid::new_v4().to_string(),
            requirement_id: requirements[0].id.clone(),
            framework: "pytest".to_string(),
            code: template_generator::generate_pytest_test(&requirements[0], &[]),
            generation_mode: "template".to_string(),
            file_path: None,
            created_at: Utc::now().to_rfc3339(),
        };
        queries::insert_generated_test(&conn, &generated).expect("store generated test");

        let test_path = fixture.root.join("test_workflow_smoke.py");
        fs::write(&test_path, &generated.code).expect("write generated pytest file");
        queries::update_generated_test_path(&conn, &generated.id, &test_path.to_string_lossy())
            .expect("store generated test path");

        let execution = test_runner::run_pytest_test(
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
        };
        queries::insert_test_result(&conn, &result).expect("store test result");

        let report =
            alignment::generate_report(&conn, &project.id).expect("generate alignment report");
        assert_eq!(report.report.total_requirements, 1);
        assert_eq!(report.report.covered_requirements, 1);
        assert_eq!(report.report.coverage_percent, 100.0);
        assert!(report.mismatches.is_empty());
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
