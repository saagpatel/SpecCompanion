use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::test::{GenerateTestsRequest, GeneratedTest};
use crate::services::{codebase_scanner, llm_generator, template_generator};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub api_key: String,
    pub default_framework: String,
    pub default_mode: String,
    pub scan_exclusions: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            default_framework: "jest".to_string(),
            default_mode: "template".to_string(),
            scan_exclusions: Vec::new(),
        }
    }
}

#[tauri::command]
pub async fn generate_tests(
    state: State<'_, Database>,
    app_handle: AppHandle,
    request: GenerateTestsRequest,
) -> Result<Vec<GeneratedTest>, AppError> {
    if request.project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    if request.requirement_ids.is_empty() {
        return Err(AppError::InvalidInput("No requirements selected".into()));
    }
    if !matches!(request.framework.as_str(), "jest" | "pytest") {
        return Err(AppError::InvalidInput(format!(
            "Unsupported framework: {}",
            request.framework
        )));
    }
    if !matches!(request.mode.as_str(), "template" | "llm") {
        return Err(AppError::InvalidInput(format!(
            "Unsupported mode: {}",
            request.mode
        )));
    }

    let settings = load_settings_internal(&app_handle)?;
    if request.mode == "llm" && settings.api_key.is_empty() {
        return Err(AppError::InvalidInput(
            "API key is required for LLM mode. Set it in Settings.".into(),
        ));
    }

    // Fetch project + requirements under a single lock
    let (codebase_path, requirements) = {
        let conn = state
            .conn
            .lock()
            .map_err(|e| AppError::General(e.to_string()))?;
        let project = queries::get_project(&conn, &request.project_id)?;
        let codebase_path = project.project.codebase_path.clone();

        let mut requirements = Vec::new();
        for req_id in &request.requirement_ids {
            requirements.push(queries::get_requirement_for_project(
                &conn,
                req_id,
                &request.project_id,
            )?);
        }
        (codebase_path, requirements)
    }; // lock released

    let symbols = codebase_scanner::scan_codebase(&codebase_path, &settings.scan_exclusions)?;

    let mut generated_tests = Vec::new();

    for req in &requirements {
        let code = match request.mode.as_str() {
            "llm" => {
                llm_generator::generate_test_with_llm(
                    &settings.api_key,
                    req,
                    &request.framework,
                    &symbols,
                )
                .await?
            }
            _ => match request.framework.as_str() {
                "pytest" => template_generator::generate_pytest_test(req, &symbols),
                _ => template_generator::generate_jest_test(req, &symbols),
            },
        };

        generated_tests.push(GeneratedTest {
            id: Uuid::new_v4().to_string(),
            requirement_id: req.id.clone(),
            framework: request.framework.clone(),
            code,
            generation_mode: request.mode.clone(),
            file_path: None,
            created_at: Utc::now().to_rfc3339(),
        });
    }

    // Batch insert all tests under a single lock + transaction
    {
        let conn = state
            .conn
            .lock()
            .map_err(|e| AppError::General(e.to_string()))?;
        let tx = conn.unchecked_transaction().map_err(AppError::Database)?;
        for test in &generated_tests {
            queries::insert_generated_test(&tx, test)?;
        }
        tx.commit().map_err(AppError::Database)?;
    }

    Ok(generated_tests)
}

#[tauri::command]
pub fn get_generated_tests(
    state: State<'_, Database>,
    requirement_id: String,
) -> Result<Vec<GeneratedTest>, AppError> {
    if requirement_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Requirement ID cannot be empty".into(),
        ));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_generated_tests_for_requirement(&conn, &requirement_id)
}

#[tauri::command]
pub fn get_all_generated_tests(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<GeneratedTest>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_generated_tests_for_project(&conn, &project_id)
}

#[tauri::command]
pub fn save_test_to_disk(
    state: State<'_, Database>,
    test_id: String,
    path: String,
) -> Result<String, AppError> {
    if test_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Test ID cannot be empty".into()));
    }
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("File path cannot be empty".into()));
    }
    // Validate path is within user home directory
    let home = crate::utils::home_dir()
        .ok_or_else(|| AppError::General("Cannot determine home directory".into()))?;
    let abs_path = if std::path::Path::new(&path).is_absolute() {
        std::path::PathBuf::from(&path)
    } else {
        std::env::current_dir().map_err(AppError::Io)?.join(&path)
    };
    // Validate path is within home directory BEFORE creating any directories
    if let Some(parent) = abs_path.parent() {
        // Walk up to find the deepest existing ancestor for validation
        let mut check = parent.to_path_buf();
        while !check.exists() {
            if !check.pop() {
                return Err(AppError::InvalidInput(
                    "Invalid path: no existing ancestor directory".into(),
                ));
            }
        }
        let canonical_ancestor = std::fs::canonicalize(&check).map_err(AppError::Io)?;
        if !canonical_ancestor.starts_with(&home) {
            return Err(AppError::InvalidInput(
                "Access denied: path is outside home directory".into(),
            ));
        }
        std::fs::create_dir_all(parent)?;
    }

    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    let test = queries::get_generated_test(&conn, &test_id)?;

    std::fs::write(&abs_path, &test.code)?;
    let final_path = abs_path.to_string_lossy().to_string();
    queries::update_generated_test_path(&conn, &test_id, &final_path)?;

    Ok(final_path)
}

#[tauri::command]
pub fn save_settings(app_handle: AppHandle, settings: AppSettings) -> Result<(), AppError> {
    if !matches!(settings.default_framework.as_str(), "jest" | "pytest") {
        return Err(AppError::InvalidInput(format!(
            "Unsupported framework: {}",
            settings.default_framework
        )));
    }
    if !matches!(settings.default_mode.as_str(), "template" | "llm") {
        return Err(AppError::InvalidInput(format!(
            "Unsupported mode: {}",
            settings.default_mode
        )));
    }
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::General(e.to_string()))?;
    std::fs::create_dir_all(&config_dir)?;
    let path = config_dir.join("settings.json");
    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[tauri::command]
pub fn load_settings(app_handle: AppHandle) -> Result<AppSettings, AppError> {
    load_settings_internal(&app_handle)
}

pub(crate) fn load_settings_internal(app_handle: &AppHandle) -> Result<AppSettings, AppError> {
    let config_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::General(e.to_string()))?;
    let path = config_dir.join("settings.json");
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let settings: AppSettings = serde_json::from_str(&content)?;
        Ok(settings)
    } else {
        Ok(AppSettings::default())
    }
}
