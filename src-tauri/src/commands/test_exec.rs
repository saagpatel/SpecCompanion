use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::test::{TestProgress, TestResult};
use crate::services::test_runner;
use chrono::Utc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

#[tauri::command]
pub async fn execute_tests(
    state: State<'_, Database>,
    app_handle: AppHandle,
    project_id: String,
    test_ids: Vec<String>,
) -> Result<Vec<TestResult>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    if test_ids.is_empty() {
        return Err(AppError::InvalidInput(
            "No tests selected for execution".into(),
        ));
    }

    // Gather test info under a single lock
    let (tests_to_run, codebase_path) = {
        let conn = state
            .conn
            .lock()
            .map_err(|e| AppError::General(e.to_string()))?;
        let project = queries::get_project(&conn, &project_id)?;
        let codebase_path = project.project.codebase_path.clone();

        let mut tests = Vec::new();
        for test_id in &test_ids {
            tests.push(queries::get_generated_test_for_project(
                &conn,
                test_id,
                &project_id,
            )?);
        }
        (tests, codebase_path)
    }; // lock released before any I/O

    let total = tests_to_run.len();
    let mut results = Vec::new();

    for (i, test) in tests_to_run.iter().enumerate() {
        let _ = app_handle.emit(
            "test-progress",
            TestProgress {
                total,
                completed: i,
                current_test: test.id.clone(),
                status: "running".to_string(),
            },
        );

        // Write test to temp file if no file_path
        let mut is_temp = false;
        let test_file_path = if let Some(ref path) = test.file_path {
            path.clone()
        } else {
            is_temp = true;
            let ext = match test.framework.as_str() {
                "pytest" => "py",
                _ => "test.js",
            };
            let temp_dir = std::env::temp_dir().join("spec-companion-tests");
            if temp_dir.exists()
                && std::fs::symlink_metadata(&temp_dir)?
                    .file_type()
                    .is_symlink()
            {
                return Err(AppError::InvalidInput(
                    "SpecCompanion temporary directory cannot be a symlink".into(),
                ));
            }
            std::fs::create_dir_all(&temp_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&temp_dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let temp_path = temp_dir.join(format!("{}.{}", Uuid::new_v4(), ext));
            std::fs::write(&temp_path, &test.code)?;
            temp_path.to_string_lossy().to_string()
        };

        let exec_result = match test.framework.as_str() {
            "pytest" => test_runner::run_pytest_test(&test_file_path, &codebase_path),
            "jest" => test_runner::run_jest_test(&test_file_path, &codebase_path),
            unsupported => Ok(test_runner::ExecutionResult {
                status: "unsupported".into(),
                execution_time_ms: 0,
                stdout: String::new(),
                stderr: format!("Unsupported test framework: {unsupported}"),
            }),
        }
        .unwrap_or_else(|error| test_runner::ExecutionResult {
            status: "blocked".into(),
            execution_time_ms: 0,
            stdout: String::new(),
            stderr: format!("Execution blocked: {error}"),
        });

        // Clean up temp file after execution
        if is_temp {
            let _ = std::fs::remove_file(&test_file_path);
        }

        results.push(TestResult {
            id: Uuid::new_v4().to_string(),
            generated_test_id: test.id.clone(),
            status: exec_result.status,
            execution_time_ms: exec_result.execution_time_ms,
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            executed_at: Utc::now().to_rfc3339(),
        });
    }

    // Batch insert all results under a single lock + transaction
    {
        let conn = state
            .conn
            .lock()
            .map_err(|e| AppError::General(e.to_string()))?;
        let tx = conn.unchecked_transaction().map_err(AppError::Database)?;
        for result in &results {
            queries::insert_test_result(&tx, result)?;
        }
        let _ = queries::touch_project_updated_at(&tx, &project_id);
        tx.commit().map_err(AppError::Database)?;
    }

    let _ = app_handle.emit(
        "test-progress",
        TestProgress {
            total,
            completed: total,
            current_test: String::new(),
            status: "completed".to_string(),
        },
    );

    Ok(results)
}

#[tauri::command]
pub fn get_test_results(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<TestResult>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_test_results_for_project(&conn, &project_id)
}

#[tauri::command]
pub fn get_test_result(state: State<'_, Database>, id: String) -> Result<TestResult, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Test result ID cannot be empty".into(),
        ));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_test_result(&conn, &id)
}
