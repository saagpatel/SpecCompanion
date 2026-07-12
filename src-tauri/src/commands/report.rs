use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::report::{AlignmentReport, AlignmentReportWithEvidence};
use crate::services::alignment;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn generate_alignment_report(
    state: State<'_, Database>,
    app_handle: AppHandle,
    project_id: String,
) -> Result<AlignmentReportWithEvidence, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    let settings = crate::commands::test_gen::load_settings_internal(&app_handle)?;
    alignment::generate_report_with_exclusions(&conn, &project_id, &settings.scan_exclusions)
}

#[tauri::command]
pub fn get_alignment_report(
    state: State<'_, Database>,
    id: String,
) -> Result<AlignmentReportWithEvidence, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::InvalidInput("Report ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::get_alignment_report(&conn, &id)
}

#[tauri::command]
pub fn list_reports(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<AlignmentReport>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID cannot be empty".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    queries::list_reports(&conn, &project_id)
}

#[tauri::command]
pub fn export_report(
    state: State<'_, Database>,
    report_id: String,
    format: String,
) -> Result<String, AppError> {
    if report_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Report ID cannot be empty".into()));
    }
    if !matches!(format.as_str(), "json" | "html" | "csv") {
        return Err(AppError::InvalidInput(format!(
            "Unsupported format: {}",
            format
        )));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| AppError::General(e.to_string()))?;
    let report = queries::get_alignment_report(&conn, &report_id)?;

    match format.as_str() {
        "json" => serde_json::to_string_pretty(&report).map_err(AppError::Serde),
        "csv" => {
            let mut csv = String::from(
                "requirement_id,spec_section,classification,reason,details,verification_policy,policy_status,missing_controls\n",
            );
            for alignment in &report.alignments {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    escape_csv(&alignment.requirement_id),
                    escape_csv(&alignment.section),
                    escape_csv(alignment.classification.as_str()),
                    escape_csv(alignment.reason.as_str()),
                    escape_csv(&alignment.summary),
                    escape_csv(&alignment.verification_policy.policy_id),
                    escape_csv(alignment.verification_policy.status.as_str()),
                    escape_csv(&alignment.verification_policy.missing_controls.join(";")),
                ));
            }
            Ok(csv)
        }
        "html" => {
            let mut html = String::from(
                r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Alignment Report</title>
<style>
body { font-family: -apple-system, sans-serif; margin: 2em; background: #1e1e2e; color: #e4e4f0; }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #333348; padding: 8px 12px; text-align: left; }
th { background: #252538; }
.badge { padding: 2px 8px; border-radius: 4px; font-size: 0.85em; }
.VERIFIED { background: #22c55e; color: #000; }
.FAILED { background: #ef4444; color: #fff; }
.UNKNOWN { background: #6366f1; color: #fff; }
.PARTIAL { background: #f97316; color: #fff; }
</style></head><body>"#,
            );
            html.push_str(&format!(
                "<h1>Alignment Report</h1><p>Coverage: <strong>{:.1}%</strong> ({}/{} requirements)</p>",
                report.report.coverage_percent,
                report.report.covered_requirements,
                report.report.total_requirements,
            ));

            if report.alignments.is_empty() {
                html.push_str("<p>No requirements were available to classify.</p>");
            } else {
                html.push_str("<table><thead><tr><th>Section</th><th>Type</th><th>Details</th><th>Verification policy</th></tr></thead><tbody>");
                for alignment in &report.alignments {
                    let classification = alignment.classification.as_str();
                    html.push_str(&format!(
                        "<tr><td>{}</td><td><span class=\"badge {}\">{}</span></td><td>{}</td><td><strong>{}</strong><br>{}</td></tr>",
                        html_escape(&alignment.section),
                        html_escape(classification),
                        html_escape(classification),
                        html_escape(&alignment.summary),
                        html_escape(alignment.verification_policy.status.as_str()),
                        html_escape(&alignment.verification_policy.summary),
                    ));
                }
                html.push_str("</tbody></table>");
            }

            html.push_str("</body></html>");
            Ok(html)
        }
        _ => Err(AppError::InvalidInput(format!(
            "Unsupported format: {}",
            format
        ))),
    }
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
