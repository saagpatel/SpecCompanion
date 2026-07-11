use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::spec::{ParsedSpec, Requirement, Spec};
use crate::services::spec_parser;
use tauri::State;

#[tauri::command]
pub fn upload_spec(
    state: State<'_, Database>,
    project_id: String,
    filename: String,
    content: String,
) -> Result<ParsedSpec, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    if filename.trim().is_empty() {
        return Err(AppError::InvalidInput("Filename cannot be empty".into()));
    }
    if content.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Spec content cannot be empty".into(),
        ));
    }
    // Sanitize filename — strip path components
    let safe_filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed_spec.md")
        .to_string();

    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;

    queries::get_project(&conn, &project_id)?;

    let tx = conn.unchecked_transaction().map_err(AppError::Database)?;

    let spec = queries::create_spec(&tx, &project_id, &safe_filename, &content)?;
    let requirements = spec_parser::parse_spec(&spec.id, &content)?;

    if !requirements.is_empty() {
        queries::insert_requirements(&tx, &requirements)?;
    }

    queries::update_spec_parsed_at(&tx, &spec.id)?;
    queries::touch_project_updated_at(&tx, &project_id)?;
    let updated_spec = queries::get_spec(&tx, &spec.id)?;

    tx.commit().map_err(AppError::Database)?;

    Ok(ParsedSpec {
        spec: updated_spec,
        requirements,
    })
}

#[tauri::command]
pub fn get_spec(state: State<'_, Database>, id: String) -> Result<ParsedSpec, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidInput("Spec ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    let spec = queries::get_spec(&conn, &id)?;
    let requirements = queries::get_requirements_for_spec(&conn, &id)?;
    Ok(ParsedSpec { spec, requirements })
}

#[tauri::command]
pub fn list_specs(state: State<'_, Database>, project_id: String) -> Result<Vec<Spec>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::list_specs(&conn, &project_id)
}

#[tauri::command]
pub fn delete_spec(state: State<'_, Database>, id: String) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidInput("Spec ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    let spec = queries::get_spec(&conn, &id)?;
    queries::delete_spec(&conn, &id)?;
    let _ = queries::touch_project_updated_at(&conn, &spec.project_id);
    Ok(())
}

#[tauri::command]
pub fn reparse_spec(state: State<'_, Database>, id: String) -> Result<Vec<Requirement>, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidInput("Spec ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    let spec = queries::get_spec(&conn, &id)?;

    let tx = conn.unchecked_transaction().map_err(AppError::Database)?;

    queries::delete_requirements_for_spec(&tx, &id)?;

    let requirements = spec_parser::parse_spec(&id, &spec.content)?;

    if !requirements.is_empty() {
        queries::insert_requirements(&tx, &requirements)?;
    }

    queries::update_spec_parsed_at(&tx, &id)?;

    tx.commit().map_err(AppError::Database)?;

    Ok(requirements)
}

#[tauri::command]
pub fn read_file_content(path: String) -> Result<String, AppError> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("File path cannot be empty".into()));
    }
    let canonical = std::fs::canonicalize(&path).map_err(AppError::Io)?;
    // Block paths outside user home directory
    let home = crate::utils::home_dir()
        .ok_or_else(|| AppError::General("Cannot determine home directory".into()))?;
    if !canonical.starts_with(&home) {
        return Err(AppError::InvalidInput(
            "Access denied: path is outside home directory".into(),
        ));
    }
    let metadata = std::fs::metadata(&canonical).map_err(AppError::Io)?;
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
    if metadata.len() > MAX_FILE_SIZE {
        return Err(AppError::InvalidInput(format!(
            "File too large ({:.1} MB). Maximum allowed size is 50 MB.",
            metadata.len() as f64 / (1024.0 * 1024.0)
        )));
    }
    std::fs::read_to_string(&canonical).map_err(AppError::Io)
}
