use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::test::{TestProgress, TestResult};
use crate::services::test_runner;
use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

const MAX_EXECUTED_TEST_BYTES: u64 = 1_024_000;

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
    let settings = crate::commands::test_gen::load_settings_internal(&app_handle)?;
    let python_environment = settings.python_environments.get(&project_id);
    let mut results = Vec::new();
    let mut executed_code_updates = Vec::new();

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
                "pytest" | "unittest" => "py",
                "vitest" => "test.ts",
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

        let source_before = if is_temp {
            Ok(test.code.clone())
        } else {
            read_execution_source(&test_file_path)
        };
        let mut exec_result = match source_before.as_ref() {
            Err(error) => test_runner::ExecutionResult {
                status: "blocked".into(),
                execution_time_ms: 0,
                stdout: String::new(),
                stderr: error.clone(),
            },
            Ok(_) => match test.framework.as_str() {
                "pytest" => test_runner::run_pytest_test_with_attested_environment(
                    &test_file_path,
                    &codebase_path,
                    python_environment.map(|runtime| runtime.root.as_str()),
                    python_environment.map(|runtime| runtime.fingerprint.as_str()),
                ),
                "jest" => test_runner::run_jest_test(&test_file_path, &codebase_path),
                "vitest" => test_runner::run_vitest_test(&test_file_path, &codebase_path),
                "unittest" => test_runner::run_unittest_test_with_attested_environment(
                    &test_file_path,
                    &codebase_path,
                    python_environment.map(|runtime| runtime.root.as_str()),
                    python_environment.map(|runtime| runtime.fingerprint.as_str()),
                ),
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
            }),
        };

        if !is_temp {
            if let Ok(source_before) = source_before {
                match stable_execution_source(&test_file_path, &source_before) {
                    Ok(source_after) => {
                        executed_code_updates.push((test.id.clone(), source_after));
                    }
                    Err(error) => {
                        exec_result.status = "blocked".into();
                        exec_result.stderr = error;
                    }
                }
            }
        }

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
        for (test_id, code) in &executed_code_updates {
            queries::update_generated_test_code(&tx, test_id, code)?;
        }
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

#[derive(Debug, Serialize)]
pub struct PythonRuntimeStatus {
    configured: bool,
    valid: bool,
    root: String,
    interpreter: String,
    fingerprint: String,
    reason: String,
}

#[tauri::command]
pub fn configure_project_python_runtime(
    state: State<'_, Database>,
    app_handle: AppHandle,
    project_id: String,
    environment_root: String,
) -> Result<PythonRuntimeStatus, AppError> {
    let codebase_path = {
        let conn = state
            .conn
            .lock()
            .map_err(|error| AppError::General(error.to_string()))?;
        queries::get_project(&conn, &project_id)?
            .project
            .codebase_path
    };
    let attestation = test_runner::attest_python_environment(&codebase_path, &environment_root)
        .map_err(AppError::InvalidInput)?;
    let mut settings = crate::commands::test_gen::load_settings_internal(&app_handle)?;
    settings.python_environments.insert(
        project_id,
        crate::commands::test_gen::TrustedPythonEnvironment {
            root: attestation.root.clone(),
            fingerprint: attestation.fingerprint.clone(),
            interpreter: attestation.interpreter.clone(),
        },
    );
    crate::commands::test_gen::save_settings_internal(&app_handle, &settings)?;
    Ok(PythonRuntimeStatus {
        configured: true,
        valid: true,
        root: attestation.root,
        interpreter: attestation.interpreter,
        fingerprint: attestation.fingerprint,
        reason: "Runtime identity and installed package inventory are attested.".into(),
    })
}

#[tauri::command]
pub fn get_project_python_runtime_status(
    state: State<'_, Database>,
    app_handle: AppHandle,
    project_id: String,
) -> Result<PythonRuntimeStatus, AppError> {
    let codebase_path = {
        let conn = state
            .conn
            .lock()
            .map_err(|error| AppError::General(error.to_string()))?;
        queries::get_project(&conn, &project_id)?
            .project
            .codebase_path
    };
    let settings = crate::commands::test_gen::load_settings_internal(&app_handle)?;
    let Some(saved) = settings.python_environments.get(&project_id) else {
        return Ok(PythonRuntimeStatus {
            configured: false,
            valid: false,
            root: String::new(),
            interpreter: String::new(),
            fingerprint: String::new(),
            reason: "No external Python runtime is trusted for this project.".into(),
        });
    };
    match test_runner::attest_python_environment(&codebase_path, &saved.root) {
        Ok(current) if current.fingerprint == saved.fingerprint => Ok(PythonRuntimeStatus { configured: true, valid: true, root: saved.root.clone(), interpreter: current.interpreter, fingerprint: current.fingerprint, reason: "Runtime attestation matches the saved trust decision.".into() }),
        Ok(current) => Ok(PythonRuntimeStatus { configured: true, valid: false, root: saved.root.clone(), interpreter: current.interpreter, fingerprint: current.fingerprint, reason: "Runtime identity or installed package inventory changed. Trust it again before execution.".into() }),
        Err(reason) => Ok(PythonRuntimeStatus { configured: true, valid: false, root: saved.root.clone(), interpreter: saved.interpreter.clone(), fingerprint: saved.fingerprint.clone(), reason }),
    }
}

#[tauri::command]
pub fn clear_project_python_runtime(
    app_handle: AppHandle,
    project_id: String,
) -> Result<(), AppError> {
    let mut settings = crate::commands::test_gen::load_settings_internal(&app_handle)?;
    settings.python_environments.remove(&project_id);
    crate::commands::test_gen::save_settings_internal(&app_handle, &settings)
}

fn read_execution_source(path: &str) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Test source is unavailable before execution: {error}"))?;
    if metadata.len() > MAX_EXECUTED_TEST_BYTES {
        return Err(format!(
            "Test source exceeds the {} byte execution limit",
            MAX_EXECUTED_TEST_BYTES
        ));
    }
    std::fs::read_to_string(path)
        .map_err(|error| format!("Test source is not readable UTF-8: {error}"))
}

fn stable_execution_source(path: &str, source_before: &str) -> Result<String, String> {
    let source_after = read_execution_source(path)?;
    if source_after != source_before {
        return Err("Test file changed during execution; the process result is not trusted".into());
    }
    Ok(source_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn execution_source_must_stay_stable_and_bounded() {
        let root = std::env::temp_dir().join(format!("spec-source-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("fixture");
        let path = root.join("evidence.test.js");
        fs::write(&path, "expect(result).toBe('before')\n").expect("source");
        let before = read_execution_source(&path.to_string_lossy()).expect("read before");
        assert_eq!(
            stable_execution_source(&path.to_string_lossy(), &before).expect("stable"),
            before
        );

        fs::write(&path, "expect(result).toBe('after')\n").expect("changed source");
        assert!(stable_execution_source(&path.to_string_lossy(), &before)
            .expect_err("changed source must be rejected")
            .contains("changed during execution"));

        fs::write(&path, vec![b'x'; MAX_EXECUTED_TEST_BYTES as usize + 1]).expect("oversized");
        assert!(read_execution_source(&path.to_string_lossy())
            .expect_err("oversized source must be rejected")
            .contains("execution limit"));
        let _ = fs::remove_dir_all(root);
    }
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
