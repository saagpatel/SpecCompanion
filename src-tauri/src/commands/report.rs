use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::report::{AlignmentReport, AlignmentReportWithEvidence};
use crate::services::alignment;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

const BUNDLE_SCHEMA: &str = "speccompanion.evidence-bundle.v1";
const FRESHNESS_WINDOW_SECONDS: i64 = 86_400;

#[derive(Serialize)]
struct EvidenceBundleManifest {
    schema: &'static str,
    report_id: String,
    project_id: String,
    report_generated_at: String,
    exported_at: String,
    age_seconds_at_export: Option<i64>,
    freshness_status: &'static str,
    report_integrity_status: String,
    payload_sha256: String,
    signature_status: &'static str,
    trust_scope: &'static str,
}

#[derive(Serialize)]
struct UnsignedEvidenceBundle<'a> {
    manifest: &'a EvidenceBundleManifest,
    report: &'a AlignmentReportWithEvidence,
}

#[derive(Serialize)]
struct EvidenceBundle<'a> {
    manifest: &'a EvidenceBundleManifest,
    report: &'a AlignmentReportWithEvidence,
    bundle_sha256: String,
}

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
    if !matches!(format.as_str(), "json" | "html" | "csv" | "bundle") {
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
        "bundle" => export_evidence_bundle(&report, Utc::now()),
        "json" => serde_json::to_string_pretty(&report).map_err(AppError::Serde),
        "csv" => {
            let mut csv = String::from(
                "requirement_id,spec_section,classification,reason,details,verification_policy,policy_status,missing_controls,report_integrity\n",
            );
            for alignment in &report.alignments {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&alignment.requirement_id),
                    escape_csv(&alignment.section),
                    escape_csv(alignment.classification.as_str()),
                    escape_csv(alignment.reason.as_str()),
                    escape_csv(&alignment.summary),
                    escape_csv(&alignment.verification_policy.policy_id),
                    escape_csv(alignment.verification_policy.status.as_str()),
                    escape_csv(&alignment.verification_policy.missing_controls.join(";")),
                    escape_csv(&report.report.integrity_status),
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
                "<h1>Alignment Report</h1><p>Coverage: <strong>{:.1}%</strong> ({}/{} requirements)</p><p>Report integrity: <strong>{}</strong></p>",
                report.report.coverage_percent,
                report.report.covered_requirements,
                report.report.total_requirements,
                html_escape(&report.report.integrity_status),
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

fn export_evidence_bundle(
    report: &AlignmentReportWithEvidence,
    exported_at: DateTime<Utc>,
) -> Result<String, AppError> {
    let payload = serde_json::to_vec(report)?;
    let generated_at = DateTime::parse_from_rfc3339(&report.report.generated_at).ok();
    let age_seconds = generated_at.map(|generated| {
        exported_at
            .signed_duration_since(generated.with_timezone(&Utc))
            .num_seconds()
            .max(0)
    });
    let freshness_status = match age_seconds {
        Some(age) if age <= FRESHNESS_WINDOW_SECONDS => "fresh",
        Some(_) => "stale",
        None => "unknown",
    };
    let manifest = EvidenceBundleManifest {
        schema: BUNDLE_SCHEMA,
        report_id: report.report.id.clone(),
        project_id: report.report.project_id.clone(),
        report_generated_at: report.report.generated_at.clone(),
        exported_at: exported_at.to_rfc3339(),
        age_seconds_at_export: age_seconds,
        freshness_status,
        report_integrity_status: report.report.integrity_status.clone(),
        payload_sha256: sha256_hex(&payload),
        signature_status: "unsigned",
        trust_scope: "self_hash_integrity_only_no_authorship_or_external_attestation",
    };
    let unsigned = serde_json::to_vec(&UnsignedEvidenceBundle {
        manifest: &manifest,
        report,
    })?;
    serde_json::to_string_pretty(&EvidenceBundle {
        manifest: &manifest,
        report,
        bundle_sha256: sha256_hex(&unsigned),
    })
    .map_err(AppError::Serde)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::report::AlignmentReport;

    fn report() -> AlignmentReportWithEvidence {
        AlignmentReportWithEvidence {
            report: AlignmentReport {
                id: "report-1".into(),
                project_id: "project-1".into(),
                coverage_percent: 0.0,
                total_requirements: 0,
                covered_requirements: 0,
                verified_requirements: 0,
                partial_requirements: 0,
                failed_requirements: 0,
                unknown_requirements: 0,
                evidence_digest: "evidence".into(),
                integrity_status: "verified".into(),
                checked_languages: vec!["typescript".into()],
                skipped_languages: vec![],
                diagnostics: vec![],
                generated_at: "2026-07-11T00:00:00Z".into(),
            },
            alignments: vec![],
        }
    }

    #[test]
    fn evidence_bundle_is_deterministic_and_explicitly_unsigned() {
        let now = DateTime::parse_from_rfc3339("2026-07-11T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = export_evidence_bundle(&report(), now).expect("bundle");
        let second = export_evidence_bundle(&report(), now).expect("bundle");
        assert_eq!(first, second);
        let value: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(value["manifest"]["schema"], BUNDLE_SCHEMA);
        assert_eq!(value["manifest"]["freshness_status"], "fresh");
        assert_eq!(value["manifest"]["signature_status"], "unsigned");
        assert_eq!(
            value["manifest"]["trust_scope"],
            "self_hash_integrity_only_no_authorship_or_external_attestation"
        );
        assert_eq!(
            value["manifest"]["payload_sha256"].as_str().unwrap().len(),
            64
        );
        assert_eq!(value["bundle_sha256"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn malformed_or_old_report_dates_never_claim_freshness() {
        let now = DateTime::parse_from_rfc3339("2026-07-13T00:00:01Z")
            .unwrap()
            .with_timezone(&Utc);
        let stale: serde_json::Value =
            serde_json::from_str(&export_evidence_bundle(&report(), now).unwrap()).unwrap();
        assert_eq!(stale["manifest"]["freshness_status"], "stale");

        let mut malformed = report();
        malformed.report.generated_at = "not-a-date".into();
        let unknown: serde_json::Value =
            serde_json::from_str(&export_evidence_bundle(&malformed, now).unwrap()).unwrap();
        assert_eq!(unknown["manifest"]["freshness_status"], "unknown");
        assert!(unknown["manifest"]["age_seconds_at_export"].is_null());
    }
}
