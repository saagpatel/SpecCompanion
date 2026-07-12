use crate::db::queries;
use crate::db::Database;
use crate::errors::AppError;
use crate::models::report::{AlignmentReport, AlignmentReportWithEvidence};
use crate::services::alignment;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tauri::{AppHandle, State};
use uuid::Uuid;

const BUNDLE_SCHEMA: &str = "speccompanion.evidence-bundle.v1";
const TRUST_POLICY_SCHEMA: &str = "speccompanion.signer-trust-policy.v4";
const MAX_TRUST_POLICY_BYTES: usize = 1_048_576;
const MAX_TRUST_POLICY_RECORDS: usize = 100;
const MAX_TRUST_HISTORY_PROOF_EVENTS: usize = 100;
const FRESHNESS_WINDOW_SECONDS: i64 = 86_400;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceBundleManifest {
    schema: String,
    report_id: String,
    project_id: String,
    report_generated_at: String,
    exported_at: String,
    age_seconds_at_export: Option<i64>,
    freshness_status: String,
    report_integrity_status: String,
    payload_sha256: String,
    signature_status: String,
    trust_scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_algorithm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signer_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_fingerprint: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportedEvidenceBundle {
    manifest: EvidenceBundleManifest,
    report: AlignmentReportWithEvidence,
    bundle_sha256: String,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Serialize)]
pub struct EvidenceBundleVerification {
    pub status: String,
    pub schema: String,
    pub report_id: Option<String>,
    pub payload_integrity: String,
    pub bundle_integrity: String,
    pub report_integrity: String,
    pub signature_status: String,
    pub freshness_status: String,
    pub age_seconds: Option<i64>,
    pub diagnostics: Vec<String>,
    pub key_fingerprint: Option<String>,
    pub signer_identity: Option<String>,
    pub trust_status: String,
    pub trust_provenance: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SignerTrustRecord {
    pub project_id: String,
    pub key_fingerprint: String,
    pub signer_identity: String,
    pub status: String,
    pub provenance: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SignerTrustHistoryRecord {
    pub id: String,
    pub project_id: String,
    pub key_fingerprint: String,
    pub signer_identity: String,
    pub status: String,
    pub provenance: String,
    pub recorded_at: String,
    pub previous_digest: String,
    pub event_digest: String,
}

#[derive(Debug, Serialize)]
pub struct SignerTrustHistoryIntegrity {
    pub status: String,
    pub event_count: usize,
    pub head_digest: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Serialize)]
pub struct SigningIdentityInfo {
    pub signer_identity: String,
    pub key_fingerprint: String,
    pub public_key: String,
    pub storage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableSignerPolicy {
    key_fingerprint: String,
    signer_identity: String,
    status: String,
    provenance: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableTrustHistoryEvent {
    id: String,
    key_fingerprint: String,
    signer_identity: String,
    status: String,
    provenance: String,
    recorded_at: String,
    previous_digest: String,
    event_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustPolicyPayload {
    schema: String,
    source_project_id: String,
    source_project_name: String,
    exported_at: String,
    #[serde(default)]
    source_history_head_digest: String,
    #[serde(default)]
    source_history_event_count: usize,
    #[serde(default)]
    proof_base_head_digest: String,
    #[serde(default)]
    proof_base_event_count: usize,
    #[serde(default)]
    history_proof: Vec<PortableTrustHistoryEvent>,
    policies: Vec<PortableSignerPolicy>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedTrustPolicyBundle {
    payload: TrustPolicyPayload,
    signer_identity: String,
    public_key: String,
    key_fingerprint: String,
    signature_algorithm: String,
    payload_sha256: String,
    signature: String,
}

#[derive(Serialize)]
pub struct TrustPolicyVerification {
    pub status: String,
    pub schema: String,
    pub signer_identity: Option<String>,
    pub key_fingerprint: Option<String>,
    pub source_project_name: Option<String>,
    pub policy_count: usize,
    pub payload_sha256: Option<String>,
    pub source_history_head_digest: Option<String>,
    pub source_history_event_count: usize,
    pub proof_base_head_digest: Option<String>,
    pub proof_base_event_count: usize,
    pub anchor_status: String,
    pub witnessed_history_head_digest: Option<String>,
    pub witnessed_history_event_count: Option<usize>,
    pub conflicts: Vec<TrustPolicyConflict>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TrustAnchorAdvancement {
    pub id: String,
    pub project_id: String,
    pub source_project_id: String,
    pub package_signer_fingerprint: String,
    pub previous_head_digest: String,
    pub previous_event_count: usize,
    pub advanced_head_digest: String,
    pub advanced_event_count: usize,
    pub payload_sha256: String,
    pub provenance: String,
    pub advanced_at: String,
}

#[derive(Serialize)]
struct TrustAnchorAdvancementExport {
    schema: String,
    project_id: String,
    receipt_count: usize,
    signature_status: String,
    receipts: Vec<TrustAnchorAdvancement>,
}

struct AnchorAssessment {
    status: String,
    witnessed_head: Option<String>,
    witnessed_count: Option<usize>,
}

#[derive(Serialize)]
pub struct TrustPolicyConflict {
    pub key_fingerprint: String,
    pub signer_identity: String,
    pub incoming_status: String,
    pub current_status: Option<String>,
    pub action: String,
}

const SIGNING_KEYRING_SERVICE: &str = "com.speccompanion.evidence-signing.v1";

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

#[tauri::command]
pub fn verify_evidence_bundle(
    state: State<'_, Database>,
    bundle_json: String,
    project_id: Option<String>,
) -> EvidenceBundleVerification {
    let mut result = verify_evidence_bundle_at(&bundle_json, Utc::now());
    let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) else {
        return result;
    };
    let Some(fingerprint) = result.key_fingerprint.as_deref() else {
        return result;
    };
    if let Ok(conn) = state.conn.lock() {
        let integrity = verify_trust_history_integrity(&conn, &project_id);
        if integrity.status != "verified" {
            result.trust_status = "unknown".into();
            result.trust_provenance = None;
            result.diagnostics.push(format!(
                "Signer trust history integrity is {}; project trust policy was ignored",
                integrity.status
            ));
            result.diagnostics.extend(integrity.diagnostics);
            return result;
        }
        let policy = conn.query_row(
            "SELECT status, provenance FROM signer_trust WHERE project_id = ?1 AND key_fingerprint = ?2",
            rusqlite::params![project_id, fingerprint],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );
        if let Ok((status, provenance)) = policy {
            result.trust_status = status.clone();
            result.trust_provenance = Some(provenance);
            if status == "revoked" {
                result.status = "revoked".into();
                result
                    .diagnostics
                    .push("Signer fingerprint is revoked for this project".into());
            } else if status == "trusted" && result.status == "signed_untrusted" {
                result.status = "trusted_signer".into();
            }
        }
    }
    result
}

#[tauri::command]
pub fn set_signer_trust(
    state: State<'_, Database>,
    project_id: String,
    key_fingerprint: String,
    signer_identity: String,
    status: String,
    provenance: String,
) -> Result<SignerTrustRecord, AppError> {
    if !matches!(status.as_str(), "trusted" | "revoked")
        || key_fingerprint.len() != 64
        || !key_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        || signer_identity.trim().is_empty()
        || provenance.trim().is_empty()
    {
        return Err(AppError::InvalidInput("Invalid signer trust policy".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    upsert_signer_trust(
        &conn,
        &project_id,
        &key_fingerprint,
        &signer_identity,
        &status,
        &provenance,
    )
}

fn upsert_signer_trust(
    conn: &rusqlite::Connection,
    project_id: &str,
    key_fingerprint: &str,
    signer_identity: &str,
    status: &str,
    provenance: &str,
) -> Result<SignerTrustRecord, AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO signer_trust (project_id, key_fingerprint, signer_identity, status, provenance, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(project_id, key_fingerprint) DO UPDATE SET signer_identity = excluded.signer_identity, status = excluded.status, provenance = excluded.provenance, updated_at = excluded.updated_at",
        rusqlite::params![project_id, key_fingerprint.to_lowercase(), signer_identity.trim(), status, provenance.trim(), now],
    )?;
    append_trust_history(
        &tx,
        project_id,
        &key_fingerprint.to_lowercase(),
        signer_identity.trim(),
        status,
        provenance.trim(),
        &now,
    )?;
    tx.commit()?;
    Ok(SignerTrustRecord {
        project_id: project_id.into(),
        key_fingerprint: key_fingerprint.to_lowercase(),
        signer_identity: signer_identity.trim().into(),
        status: status.into(),
        provenance: provenance.trim().into(),
        updated_at: now,
    })
}

#[tauri::command]
pub fn list_signer_trust(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<SignerTrustRecord>, AppError> {
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    let mut stmt = conn.prepare("SELECT project_id, key_fingerprint, signer_identity, status, provenance, updated_at FROM signer_trust WHERE project_id = ?1 ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([project_id], |row| {
        Ok(SignerTrustRecord {
            project_id: row.get(0)?,
            key_fingerprint: row.get(1)?,
            signer_identity: row.get(2)?,
            status: row.get(3)?,
            provenance: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(AppError::Database)
}

#[tauri::command]
pub fn list_signer_trust_history(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<SignerTrustHistoryRecord>, AppError> {
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, key_fingerprint, signer_identity, status, provenance, recorded_at, previous_digest, event_digest
         FROM signer_trust_history WHERE project_id = ?1
         ORDER BY recorded_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([project_id], |row| {
        Ok(SignerTrustHistoryRecord {
            id: row.get(0)?,
            project_id: row.get(1)?,
            key_fingerprint: row.get(2)?,
            signer_identity: row.get(3)?,
            status: row.get(4)?,
            provenance: row.get(5)?,
            recorded_at: row.get(6)?,
            previous_digest: row.get(7)?,
            event_digest: row.get(8)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(AppError::Database)
}

#[tauri::command]
pub fn get_signer_trust_history_integrity(
    state: State<'_, Database>,
    project_id: String,
) -> SignerTrustHistoryIntegrity {
    match state.conn.lock() {
        Ok(conn) => verify_trust_history_integrity(&conn, &project_id),
        Err(error) => SignerTrustHistoryIntegrity {
            status: "unknown".into(),
            event_count: 0,
            head_digest: None,
            diagnostics: vec![format!("Trust history is unavailable: {error}")],
        },
    }
}

#[tauri::command]
pub fn rotate_signer_trust(
    state: State<'_, Database>,
    project_id: String,
    previous_fingerprint: String,
    new_fingerprint: String,
    new_signer_identity: String,
    provenance: String,
) -> Result<Vec<SignerTrustRecord>, AppError> {
    let valid_fingerprint =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid_fingerprint(&previous_fingerprint)
        || !valid_fingerprint(&new_fingerprint)
        || previous_fingerprint.eq_ignore_ascii_case(&new_fingerprint)
        || new_signer_identity.trim().is_empty()
        || provenance.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "Invalid signer rotation policy".into(),
        ));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    rotate_signer_trust_records(
        &conn,
        &project_id,
        &previous_fingerprint,
        &new_fingerprint,
        &new_signer_identity,
        &provenance,
    )
}

fn rotate_signer_trust_records(
    conn: &rusqlite::Connection,
    project_id: &str,
    previous_fingerprint: &str,
    new_fingerprint: &str,
    new_signer_identity: &str,
    provenance: &str,
) -> Result<Vec<SignerTrustRecord>, AppError> {
    let previous = conn.query_row(
        "SELECT signer_identity, status FROM signer_trust WHERE project_id = ?1 AND key_fingerprint = ?2",
        rusqlite::params![project_id, previous_fingerprint.to_lowercase()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    if previous.1 != "trusted" {
        return Err(AppError::InvalidInput(
            "Only a currently trusted fingerprint can be rotated".into(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let previous_fingerprint = previous_fingerprint.to_lowercase();
    let new_fingerprint = new_fingerprint.to_lowercase();
    let provenance = provenance.trim();
    let new_identity = new_signer_identity.trim();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE signer_trust SET status = 'revoked', provenance = ?3, updated_at = ?4
         WHERE project_id = ?1 AND key_fingerprint = ?2",
        rusqlite::params![project_id, previous_fingerprint, provenance, now],
    )?;
    tx.execute(
        "INSERT INTO signer_trust (project_id, key_fingerprint, signer_identity, status, provenance, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'trusted', ?4, ?5, ?5)
         ON CONFLICT(project_id, key_fingerprint) DO UPDATE SET signer_identity = excluded.signer_identity, status = 'trusted', provenance = excluded.provenance, updated_at = excluded.updated_at",
        rusqlite::params![project_id, new_fingerprint, new_identity, provenance, now],
    )?;
    for (fingerprint, identity, status) in [
        (&previous_fingerprint, previous.0.as_str(), "revoked"),
        (&new_fingerprint, new_identity, "trusted"),
    ] {
        append_trust_history(
            &tx,
            project_id,
            fingerprint,
            identity,
            status,
            provenance,
            &now,
        )?;
    }
    tx.commit()?;
    Ok(vec![
        SignerTrustRecord {
            project_id: project_id.into(),
            key_fingerprint: previous_fingerprint,
            signer_identity: previous.0,
            status: "revoked".into(),
            provenance: provenance.into(),
            updated_at: now.clone(),
        },
        SignerTrustRecord {
            project_id: project_id.into(),
            key_fingerprint: new_fingerprint,
            signer_identity: new_identity.into(),
            status: "trusted".into(),
            provenance: provenance.into(),
            updated_at: now,
        },
    ])
}

#[tauri::command]
pub fn create_signing_identity(signer_identity: String) -> Result<SigningIdentityInfo, AppError> {
    let identity = signer_identity.trim();
    if identity.is_empty() || identity.len() > 120 {
        return Err(AppError::InvalidInput(
            "Signer identity must be between 1 and 120 characters".into(),
        ));
    }
    let key = SigningKey::generate(&mut OsRng);
    let entry = keyring::Entry::new(SIGNING_KEYRING_SERVICE, identity)
        .map_err(|error| AppError::General(format!("Keychain unavailable: {error}")))?;
    match entry.get_secret() {
        Ok(_) => {
            return Err(AppError::InvalidInput(
                "Signing identity already exists; implicit key rotation is refused".into(),
            ));
        }
        Err(keyring::Error::NoEntry) => {}
        Err(error) => {
            return Err(AppError::General(format!(
                "Could not inspect signing identity: {error}"
            )));
        }
    }
    entry
        .set_secret(&key.to_bytes())
        .map_err(|error| AppError::General(format!("Could not store signing key: {error}")))?;
    Ok(signing_identity_info(identity, &key))
}

#[tauri::command]
pub fn export_signed_evidence_bundle(
    state: State<'_, Database>,
    report_id: String,
    signer_identity: String,
) -> Result<String, AppError> {
    let identity = signer_identity.trim();
    if report_id.trim().is_empty() || identity.is_empty() {
        return Err(AppError::InvalidInput(
            "Report ID and signer identity are required".into(),
        ));
    }
    let entry = keyring::Entry::new(SIGNING_KEYRING_SERVICE, identity)
        .map_err(|error| AppError::General(format!("Keychain unavailable: {error}")))?;
    let secret = entry
        .get_secret()
        .map_err(|error| AppError::General(format!("Signing identity unavailable: {error}")))?;
    let seed: [u8; 32] = secret
        .as_slice()
        .try_into()
        .map_err(|_| AppError::General("Stored signing key is malformed".into()))?;
    let key = SigningKey::from_bytes(&seed);
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    let report = queries::get_alignment_report(&conn, &report_id)?;
    export_signed_bundle(&report, Utc::now(), identity, &key)
}

#[tauri::command]
pub fn export_signer_trust_policy(
    state: State<'_, Database>,
    project_id: String,
    signer_identity: String,
) -> Result<String, AppError> {
    let identity = signer_identity.trim();
    if project_id.trim().is_empty() || identity.is_empty() {
        return Err(AppError::InvalidInput(
            "Project ID and signer identity are required".into(),
        ));
    }
    let key = load_signing_key(identity)?;
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    let project = queries::get_project(&conn, &project_id)?;
    let history = verify_trust_history_integrity(&conn, &project_id);
    if history.status != "verified" {
        return Err(AppError::InvalidInput(format!(
            "Trust policy export refused because history integrity is {}",
            history.status
        )));
    }
    let history_head = history.head_digest.ok_or_else(|| {
        AppError::InvalidInput("Trust policy export requires at least one history event".into())
    })?;
    let mut history_stmt = conn.prepare(
        "SELECT id, key_fingerprint, signer_identity, status, provenance, recorded_at, previous_digest, event_digest
         FROM signer_trust_history WHERE project_id = ?1 ORDER BY rowid",
    )?;
    let mut history_proof = history_stmt
        .query_map([&project_id], |row| {
            Ok(PortableTrustHistoryEvent {
                id: row.get(0)?,
                key_fingerprint: row.get(1)?,
                signer_identity: row.get(2)?,
                status: row.get(3)?,
                provenance: row.get(4)?,
                recorded_at: row.get(5)?,
                previous_digest: row.get(6)?,
                event_digest: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let proof_base_event_count = history_proof
        .len()
        .saturating_sub(MAX_TRUST_HISTORY_PROOF_EVENTS);
    let proof_base_head_digest = if proof_base_event_count == 0 {
        String::new()
    } else {
        history_proof[proof_base_event_count - 1]
            .event_digest
            .clone()
    };
    history_proof.drain(..proof_base_event_count);
    let mut stmt = conn.prepare(
        "SELECT key_fingerprint, signer_identity, status, provenance FROM signer_trust
         WHERE project_id = ?1 ORDER BY key_fingerprint",
    )?;
    let policies = stmt
        .query_map([&project_id], |row| {
            Ok(PortableSignerPolicy {
                key_fingerprint: row.get(0)?,
                signer_identity: row.get(1)?,
                status: row.get(2)?,
                provenance: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if policies.is_empty() {
        return Err(AppError::InvalidInput(
            "No signer trust policy exists to export".into(),
        ));
    }
    export_signed_trust_policy(
        TrustPolicyPayload {
            schema: TRUST_POLICY_SCHEMA.into(),
            source_project_id: project_id,
            source_project_name: project.project.name,
            exported_at: Utc::now().to_rfc3339(),
            source_history_head_digest: history_head,
            source_history_event_count: history.event_count,
            proof_base_head_digest,
            proof_base_event_count,
            history_proof,
            policies,
        },
        identity,
        &key,
    )
}

#[tauri::command]
pub fn verify_signer_trust_policy(
    state: State<'_, Database>,
    project_id: String,
    bundle_json: String,
) -> TrustPolicyVerification {
    let mut result = verify_trust_policy(&bundle_json);
    if result.status != "valid_untrusted" || project_id.trim().is_empty() {
        return result;
    }
    let bundle: SignedTrustPolicyBundle = match serde_json::from_str(&bundle_json) {
        Ok(bundle) => bundle,
        Err(_) => return result,
    };
    let conn = match state.conn.lock() {
        Ok(conn) => conn,
        Err(error) => {
            result.status = "unknown".into();
            result
                .diagnostics
                .push(format!("Current trust policy is unavailable: {error}"));
            return result;
        }
    };
    if queries::get_project(&conn, &project_id).is_err() {
        result.status = "unknown".into();
        result
            .diagnostics
            .push("Destination project is unavailable".into());
        return result;
    }
    let integrity = verify_trust_history_integrity(&conn, &project_id);
    if integrity.status != "verified" {
        result.status = "unknown".into();
        result.diagnostics.push(format!(
            "Destination trust history integrity is {}; recovery preview is unavailable",
            integrity.status
        ));
        result.diagnostics.extend(integrity.diagnostics);
        return result;
    }
    match assess_trust_anchor(&conn, &project_id, &bundle) {
        Ok(assessment) => {
            result.anchor_status = assessment.status;
            result.witnessed_history_head_digest = assessment.witnessed_head;
            result.witnessed_history_event_count = assessment.witnessed_count;
        }
        Err(error) => {
            result.status = "unknown".into();
            result.anchor_status = "unknown".into();
            result
                .diagnostics
                .push(format!("Anchor witness ledger is unavailable: {error}"));
            return result;
        }
    }
    match preview_trust_policy(&conn, &project_id, &bundle.payload.policies) {
        Ok(conflicts) => result.conflicts = conflicts,
        Err(error) => {
            result.status = "unknown".into();
            result
                .diagnostics
                .push(format!("Could not compare current trust policy: {error}"));
        }
    }
    result
}

fn preview_trust_policy(
    conn: &rusqlite::Connection,
    project_id: &str,
    policies: &[PortableSignerPolicy],
) -> Result<Vec<TrustPolicyConflict>, rusqlite::Error> {
    let mut conflicts = Vec::with_capacity(policies.len());
    for policy in policies {
        let current = match conn.query_row(
                "SELECT status, signer_identity, provenance FROM signer_trust WHERE project_id = ?1 AND key_fingerprint = ?2",
                rusqlite::params![project_id, policy.key_fingerprint.to_lowercase()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            ) {
                Ok(current) => Some(current),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(error) => return Err(error),
            };
        let action = match current.as_ref() {
            None => "add",
            Some((status, identity, provenance))
                if status == &policy.status
                    && identity == &policy.signer_identity
                    && provenance == &policy.provenance =>
            {
                "preserve"
            }
            Some(_) => "replace",
        };
        conflicts.push(TrustPolicyConflict {
            key_fingerprint: policy.key_fingerprint.clone(),
            signer_identity: policy.signer_identity.clone(),
            incoming_status: policy.status.clone(),
            current_status: current.map(|value| value.0),
            action: action.into(),
        });
    }
    Ok(conflicts)
}

fn assess_trust_anchor(
    conn: &rusqlite::Connection,
    project_id: &str,
    bundle: &SignedTrustPolicyBundle,
) -> Result<AnchorAssessment, rusqlite::Error> {
    let witnessed = match conn.query_row(
        "SELECT history_head_digest, history_event_count FROM trust_anchor_witnesses
         WHERE project_id = ?1 AND source_project_id = ?2 AND package_signer_fingerprint = ?3",
        rusqlite::params![
            project_id,
            bundle.payload.source_project_id,
            bundle.key_fingerprint
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(error),
    };
    let Some((head, count)) = witnessed else {
        return Ok(AnchorAssessment {
            status: "first_seen".into(),
            witnessed_head: None,
            witnessed_count: None,
        });
    };
    let witnessed_count = usize::try_from(count).unwrap_or(usize::MAX);
    let status = if head == bundle.payload.source_history_head_digest
        && witnessed_count == bundle.payload.source_history_event_count
    {
        "repeated"
    } else if bundle.payload.source_history_event_count < witnessed_count {
        "rollback"
    } else if bundle.payload.source_history_event_count == witnessed_count {
        "conflict"
    } else if witnessed_count < bundle.payload.proof_base_event_count {
        "checkpoint_gap"
    } else {
        let included_head = if witnessed_count == bundle.payload.proof_base_event_count {
            Some(bundle.payload.proof_base_head_digest.as_str())
        } else {
            bundle
                .payload
                .history_proof
                .get(witnessed_count - bundle.payload.proof_base_event_count - 1)
                .map(|event| event.event_digest.as_str())
        };
        if witnessed_count > 0 && included_head == Some(head.as_str()) {
            "forward_proven"
        } else {
            "fork"
        }
    };
    Ok(AnchorAssessment {
        status: status.into(),
        witnessed_head: Some(head),
        witnessed_count: Some(witnessed_count),
    })
}

#[tauri::command]
pub fn import_signer_trust_policy(
    state: State<'_, Database>,
    project_id: String,
    bundle_json: String,
    expected_signer_fingerprint: String,
    expected_payload_sha256: String,
    recovery_provenance: String,
) -> Result<Vec<SignerTrustRecord>, AppError> {
    let verification = verify_trust_policy(&bundle_json);
    if verification.status != "valid_untrusted" {
        return Err(AppError::InvalidInput(
            "Trust policy signature or payload is invalid".into(),
        ));
    }
    let fingerprint = verification.key_fingerprint.ok_or_else(|| {
        AppError::InvalidInput("Trust policy signer fingerprint is unavailable".into())
    })?;
    let payload_sha256 = verification.payload_sha256.ok_or_else(|| {
        AppError::InvalidInput("Trust policy payload digest is unavailable".into())
    })?;
    if !fingerprint.eq_ignore_ascii_case(expected_signer_fingerprint.trim())
        || payload_sha256 != expected_payload_sha256.trim()
        || recovery_provenance.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "Recovery confirmation does not match the verified signer fingerprint".into(),
        ));
    }
    let bundle: SignedTrustPolicyBundle = serde_json::from_str(&bundle_json)?;
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    let integrity = verify_trust_history_integrity(&conn, &project_id);
    if integrity.status != "verified" {
        return Err(AppError::InvalidInput(format!(
            "Trust policy recovery refused because destination history integrity is {}",
            integrity.status
        )));
    }
    let anchor = assess_trust_anchor(&conn, &project_id, &bundle)?;
    if matches!(
        anchor.status.as_str(),
        "rollback" | "conflict" | "fork" | "checkpoint_gap"
    ) {
        return Err(AppError::InvalidInput(format!(
            "Trust policy recovery refused because the signed anchor is classified as {}",
            anchor.status
        )));
    }
    import_verified_trust_policy(
        &conn,
        &project_id,
        &bundle.payload.policies,
        &bundle.payload.source_project_id,
        &fingerprint,
        &payload_sha256,
        &bundle.payload.source_history_head_digest,
        bundle.payload.source_history_event_count,
        recovery_provenance.trim(),
    )
}

#[tauri::command]
pub fn advance_trust_anchor_witness(
    state: State<'_, Database>,
    project_id: String,
    bundle_json: String,
    expected_signer_fingerprint: String,
    expected_payload_sha256: String,
    provenance: String,
) -> Result<TrustAnchorAdvancement, AppError> {
    let verification = verify_trust_policy(&bundle_json);
    if verification.status != "valid_untrusted" {
        return Err(AppError::InvalidInput(
            "Checkpoint package signature or payload is invalid".into(),
        ));
    }
    let fingerprint = verification.key_fingerprint.ok_or_else(|| {
        AppError::InvalidInput("Checkpoint package signer fingerprint is unavailable".into())
    })?;
    let payload_sha256 = verification.payload_sha256.ok_or_else(|| {
        AppError::InvalidInput("Checkpoint package payload digest is unavailable".into())
    })?;
    if !fingerprint.eq_ignore_ascii_case(expected_signer_fingerprint.trim())
        || payload_sha256 != expected_payload_sha256.trim()
        || provenance.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "Checkpoint confirmation does not match the verified package".into(),
        ));
    }
    let bundle: SignedTrustPolicyBundle = serde_json::from_str(&bundle_json)?;
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    let integrity = verify_trust_history_integrity(&conn, &project_id);
    if integrity.status != "verified" {
        return Err(AppError::InvalidInput(format!(
            "Checkpoint advancement refused because destination history integrity is {}",
            integrity.status
        )));
    }
    advance_verified_trust_anchor(&conn, &project_id, &bundle, provenance.trim())
}

fn advance_verified_trust_anchor(
    conn: &rusqlite::Connection,
    project_id: &str,
    bundle: &SignedTrustPolicyBundle,
    provenance: &str,
) -> Result<TrustAnchorAdvancement, AppError> {
    if provenance.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Checkpoint verification provenance is required".into(),
        ));
    }
    let anchor = assess_trust_anchor(conn, project_id, bundle)?;
    if anchor.status != "forward_proven" {
        return Err(AppError::InvalidInput(format!(
            "Checkpoint advancement requires forward_proven ancestry; package is {}",
            anchor.status
        )));
    }
    let previous_head_digest = anchor
        .witnessed_head
        .ok_or_else(|| AppError::InvalidInput("A prior witnessed anchor is required".into()))?;
    let previous_event_count = anchor
        .witnessed_count
        .ok_or_else(|| AppError::InvalidInput("A prior witnessed height is required".into()))?;
    let advanced_at = Utc::now().to_rfc3339();
    let advancement = TrustAnchorAdvancement {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.into(),
        source_project_id: bundle.payload.source_project_id.clone(),
        package_signer_fingerprint: bundle.key_fingerprint.clone(),
        previous_head_digest,
        previous_event_count,
        advanced_head_digest: bundle.payload.source_history_head_digest.clone(),
        advanced_event_count: bundle.payload.source_history_event_count,
        payload_sha256: bundle.payload_sha256.clone(),
        provenance: provenance.trim().into(),
        advanced_at,
    };
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO trust_anchor_advancements
         (id, project_id, source_project_id, package_signer_fingerprint, previous_head_digest,
          previous_event_count, advanced_head_digest, advanced_event_count, payload_sha256, provenance, advanced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            advancement.id,
            project_id,
            bundle.payload.source_project_id,
            bundle.key_fingerprint,
            advancement.previous_head_digest,
            advancement.previous_event_count as i64,
            advancement.advanced_head_digest,
            advancement.advanced_event_count as i64,
            advancement.payload_sha256,
            advancement.provenance,
            advancement.advanced_at,
        ],
    )?;
    tx.execute(
        "UPDATE trust_anchor_witnesses SET history_head_digest = ?1, history_event_count = ?2,
          payload_sha256 = ?3, witnessed_at = ?4
         WHERE project_id = ?5 AND source_project_id = ?6 AND package_signer_fingerprint = ?7",
        rusqlite::params![
            advancement.advanced_head_digest,
            advancement.advanced_event_count as i64,
            advancement.payload_sha256,
            advancement.advanced_at,
            project_id,
            bundle.payload.source_project_id,
            bundle.key_fingerprint,
        ],
    )?;
    tx.commit()?;
    Ok(advancement)
}

#[tauri::command]
pub fn list_trust_anchor_advancements(
    state: State<'_, Database>,
    project_id: String,
) -> Result<Vec<TrustAnchorAdvancement>, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID is required".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    list_trust_anchor_advancement_receipts(&conn, &project_id)
}

#[tauri::command]
pub fn export_trust_anchor_advancements(
    state: State<'_, Database>,
    project_id: String,
) -> Result<String, AppError> {
    if project_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Project ID is required".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|error| AppError::General(error.to_string()))?;
    queries::get_project(&conn, &project_id)?;
    export_trust_anchor_advancement_receipts(&conn, &project_id)
}

fn export_trust_anchor_advancement_receipts(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> Result<String, AppError> {
    let receipts = list_trust_anchor_advancement_receipts(conn, project_id)?;
    serde_json::to_string_pretty(&TrustAnchorAdvancementExport {
        schema: "speccompanion.trust-anchor-advancements.v1".into(),
        project_id: project_id.into(),
        receipt_count: receipts.len(),
        signature_status: "unsigned_local_receipts".into(),
        receipts,
    })
    .map_err(AppError::Serde)
}

fn list_trust_anchor_advancement_receipts(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> Result<Vec<TrustAnchorAdvancement>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, source_project_id, package_signer_fingerprint,
                previous_head_digest, previous_event_count, advanced_head_digest,
                advanced_event_count, payload_sha256, provenance, advanced_at
         FROM trust_anchor_advancements WHERE project_id = ?1
         ORDER BY source_project_id, package_signer_fingerprint, previous_event_count,
                  advanced_event_count, advanced_at, id",
    )?;
    let receipts = stmt.query_map([project_id], |row| {
        Ok(TrustAnchorAdvancement {
            id: row.get(0)?,
            project_id: row.get(1)?,
            source_project_id: row.get(2)?,
            package_signer_fingerprint: row.get(3)?,
            previous_head_digest: row.get(4)?,
            previous_event_count: usize::try_from(row.get::<_, i64>(5)?).unwrap_or(usize::MAX),
            advanced_head_digest: row.get(6)?,
            advanced_event_count: usize::try_from(row.get::<_, i64>(7)?).unwrap_or(usize::MAX),
            payload_sha256: row.get(8)?,
            provenance: row.get(9)?,
            advanced_at: row.get(10)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;
    Ok(receipts)
}

fn load_signing_key(identity: &str) -> Result<SigningKey, AppError> {
    let entry = keyring::Entry::new(SIGNING_KEYRING_SERVICE, identity)
        .map_err(|error| AppError::General(format!("Keychain unavailable: {error}")))?;
    let secret = entry
        .get_secret()
        .map_err(|error| AppError::General(format!("Signing identity unavailable: {error}")))?;
    let seed: [u8; 32] = secret
        .as_slice()
        .try_into()
        .map_err(|_| AppError::General("Stored signing key is malformed".into()))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn export_signed_trust_policy(
    payload: TrustPolicyPayload,
    identity: &str,
    key: &SigningKey,
) -> Result<String, AppError> {
    let payload_bytes = serde_json::to_vec(&payload)?;
    let payload_sha256 = sha256_hex(&payload_bytes);
    let public = key.verifying_key().to_bytes();
    let signature = key.sign(payload_sha256.as_bytes());
    serde_json::to_string_pretty(&SignedTrustPolicyBundle {
        payload,
        signer_identity: identity.into(),
        public_key: BASE64.encode(public),
        key_fingerprint: sha256_hex(&public),
        signature_algorithm: "ed25519".into(),
        payload_sha256,
        signature: BASE64.encode(signature.to_bytes()),
    })
    .map_err(AppError::Serde)
}

fn verify_trust_policy(bundle_json: &str) -> TrustPolicyVerification {
    let mut result = TrustPolicyVerification {
        status: "invalid".into(),
        schema: "unknown".into(),
        signer_identity: None,
        key_fingerprint: None,
        source_project_name: None,
        policy_count: 0,
        payload_sha256: None,
        source_history_head_digest: None,
        source_history_event_count: 0,
        proof_base_head_digest: None,
        proof_base_event_count: 0,
        anchor_status: "not_checked".into(),
        witnessed_history_head_digest: None,
        witnessed_history_event_count: None,
        conflicts: Vec::new(),
        diagnostics: Vec::new(),
    };
    if bundle_json.len() > MAX_TRUST_POLICY_BYTES {
        result
            .diagnostics
            .push("Trust policy exceeds the 1 MiB size limit".into());
        return result;
    }
    let bundle: SignedTrustPolicyBundle = match serde_json::from_str(bundle_json) {
        Ok(bundle) => bundle,
        Err(error) => {
            result
                .diagnostics
                .push(format!("Malformed trust policy: {error}"));
            return result;
        }
    };
    result.schema = bundle.payload.schema.clone();
    result.signer_identity = Some(bundle.signer_identity.clone());
    result.key_fingerprint = Some(bundle.key_fingerprint.clone());
    result.source_project_name = Some(bundle.payload.source_project_name.clone());
    result.policy_count = bundle.payload.policies.len();
    result.payload_sha256 = Some(bundle.payload_sha256.clone());
    result.source_history_head_digest = Some(bundle.payload.source_history_head_digest.clone());
    result.source_history_event_count = bundle.payload.source_history_event_count;
    result.proof_base_head_digest = if bundle.payload.proof_base_head_digest.is_empty() {
        None
    } else {
        Some(bundle.payload.proof_base_head_digest.clone())
    };
    result.proof_base_event_count = bundle.payload.proof_base_event_count;
    if bundle.payload.schema != TRUST_POLICY_SCHEMA || bundle.signature_algorithm != "ed25519" {
        result.status = "unsupported".into();
        result
            .diagnostics
            .push("Unsupported trust policy contract".into());
        return result;
    }
    if !validate_portable_history_proof(&bundle.payload) {
        result
            .diagnostics
            .push("Trust policy ancestry proof is incomplete or invalid".into());
        return result;
    }
    let mut fingerprints = HashSet::new();
    if bundle.payload.source_history_head_digest.len() != 64
        || !bundle
            .payload
            .source_history_head_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || bundle.payload.source_history_event_count == 0
        || bundle.payload.policies.is_empty()
        || bundle.payload.policies.len() > MAX_TRUST_POLICY_RECORDS
        || bundle.payload.policies.iter().any(|policy| {
            policy.key_fingerprint.len() != 64
                || !policy
                    .key_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !fingerprints.insert(policy.key_fingerprint.to_lowercase())
                || !matches!(policy.status.as_str(), "trusted" | "revoked")
                || policy.signer_identity.trim().is_empty()
                || policy.provenance.trim().is_empty()
        })
    {
        result
            .diagnostics
            .push("Trust policy contains an invalid history anchor or policy record".into());
        return result;
    }
    let payload_digest = match serde_json::to_vec(&bundle.payload) {
        Ok(bytes) => sha256_hex(&bytes),
        Err(error) => {
            result
                .diagnostics
                .push(format!("Could not encode trust policy: {error}"));
            return result;
        }
    };
    if payload_digest != bundle.payload_sha256 {
        result
            .diagnostics
            .push("Trust policy payload digest mismatch".into());
        return result;
    }
    let public_bytes = match BASE64.decode(&bundle.public_key) {
        Ok(bytes) => bytes,
        Err(_) => {
            result
                .diagnostics
                .push("Malformed trust policy public key".into());
            return result;
        }
    };
    if sha256_hex(&public_bytes) != bundle.key_fingerprint {
        result
            .diagnostics
            .push("Trust policy fingerprint mismatch".into());
        return result;
    }
    let public: [u8; 32] = match public_bytes.try_into() {
        Ok(public) => public,
        Err(_) => {
            result
                .diagnostics
                .push("Malformed trust policy public key".into());
            return result;
        }
    };
    let signature = match BASE64
        .decode(&bundle.signature)
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
    {
        Some(signature) => signature,
        None => {
            result
                .diagnostics
                .push("Malformed trust policy signature".into());
            return result;
        }
    };
    let verifying_key = match VerifyingKey::from_bytes(&public) {
        Ok(key) => key,
        Err(_) => {
            result
                .diagnostics
                .push("Malformed trust policy public key".into());
            return result;
        }
    };
    if verifying_key
        .verify(bundle.payload_sha256.as_bytes(), &signature)
        .is_err()
    {
        result
            .diagnostics
            .push("Trust policy signature is invalid".into());
        return result;
    }
    result.status = "valid_untrusted".into();
    result.diagnostics.push(
        "Signature proves package integrity and key possession, not recovery authority".into(),
    );
    result
}

fn validate_portable_history_proof(payload: &TrustPolicyPayload) -> bool {
    if payload.history_proof.is_empty()
        || payload.history_proof.len() > MAX_TRUST_HISTORY_PROOF_EVENTS
        || payload
            .proof_base_event_count
            .checked_add(payload.history_proof.len())
            != Some(payload.source_history_event_count)
        || (payload.proof_base_event_count == 0 && !payload.proof_base_head_digest.is_empty())
        || (payload.proof_base_event_count > 0
            && (payload.proof_base_head_digest.len() != 64
                || !payload
                    .proof_base_head_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())))
        || payload
            .history_proof
            .last()
            .map(|event| event.event_digest.as_str())
            != Some(payload.source_history_head_digest.as_str())
    {
        return false;
    }
    let mut previous = payload.proof_base_head_digest.clone();
    for event in &payload.history_proof {
        if event.previous_digest != previous
            || !matches!(event.status.as_str(), "trusted" | "revoked")
        {
            return false;
        }
        let expected = trust_history_digest(&[
            &previous,
            &event.id,
            &payload.source_project_id,
            &event.key_fingerprint,
            &event.signer_identity,
            &event.status,
            &event.provenance,
            &event.recorded_at,
        ]);
        if event.event_digest != expected {
            return false;
        }
        previous.clone_from(&event.event_digest);
    }
    true
}

fn import_verified_trust_policy(
    conn: &rusqlite::Connection,
    project_id: &str,
    policies: &[PortableSignerPolicy],
    source_project_id: &str,
    package_fingerprint: &str,
    package_payload_sha256: &str,
    source_history_head_digest: &str,
    source_history_event_count: usize,
    recovery_provenance: &str,
) -> Result<Vec<SignerTrustRecord>, AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let mut imported = Vec::with_capacity(policies.len());
    for policy in policies {
        let provenance = format!(
            "{}; recovered from signed policy {} anchored at {} after {} events: {}",
            policy.provenance,
            package_fingerprint,
            source_history_head_digest,
            source_history_event_count,
            recovery_provenance
        );
        tx.execute(
            "INSERT INTO signer_trust (project_id, key_fingerprint, signer_identity, status, provenance, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(project_id, key_fingerprint) DO UPDATE SET signer_identity = excluded.signer_identity, status = excluded.status, provenance = excluded.provenance, updated_at = excluded.updated_at",
            rusqlite::params![project_id, policy.key_fingerprint.to_lowercase(), policy.signer_identity, policy.status, provenance, now],
        )?;
        append_trust_history(
            &tx,
            project_id,
            &policy.key_fingerprint.to_lowercase(),
            &policy.signer_identity,
            &policy.status,
            &provenance,
            &now,
        )?;
        imported.push(SignerTrustRecord {
            project_id: project_id.into(),
            key_fingerprint: policy.key_fingerprint.to_lowercase(),
            signer_identity: policy.signer_identity.clone(),
            status: policy.status.clone(),
            provenance,
            updated_at: now.clone(),
        });
    }
    tx.execute(
        "INSERT INTO trust_anchor_witnesses (project_id, source_project_id, package_signer_fingerprint, history_head_digest, history_event_count, payload_sha256, witnessed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(project_id, source_project_id, package_signer_fingerprint) DO UPDATE SET
           history_head_digest = excluded.history_head_digest,
           history_event_count = excluded.history_event_count,
           payload_sha256 = excluded.payload_sha256,
           witnessed_at = excluded.witnessed_at",
        rusqlite::params![
            project_id,
            source_project_id,
            package_fingerprint,
            source_history_head_digest,
            source_history_event_count as i64,
            package_payload_sha256,
            now,
        ],
    )?;
    tx.commit()?;
    Ok(imported)
}

fn trust_history_digest(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn append_trust_history(
    conn: &rusqlite::Connection,
    project_id: &str,
    fingerprint: &str,
    identity: &str,
    status: &str,
    provenance: &str,
    recorded_at: &str,
) -> Result<(), rusqlite::Error> {
    let previous = match conn.query_row(
        "SELECT event_digest FROM signer_trust_history WHERE project_id = ?1 ORDER BY rowid DESC LIMIT 1",
        [project_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(previous) => previous,
        Err(rusqlite::Error::QueryReturnedNoRows) => String::new(),
        Err(error) => return Err(error),
    };
    let id = Uuid::new_v4().to_string();
    let digest = trust_history_digest(&[
        &previous,
        &id,
        project_id,
        fingerprint,
        identity,
        status,
        provenance,
        recorded_at,
    ]);
    conn.execute(
        "INSERT INTO signer_trust_history (id, project_id, key_fingerprint, signer_identity, status, provenance, recorded_at, previous_digest, event_digest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![id, project_id, fingerprint, identity, status, provenance, recorded_at, previous, digest],
    )?;
    Ok(())
}

fn verify_trust_history_integrity(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> SignerTrustHistoryIntegrity {
    let mut result = SignerTrustHistoryIntegrity {
        status: "verified".into(),
        event_count: 0,
        head_digest: None,
        diagnostics: Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT id, key_fingerprint, signer_identity, status, provenance, recorded_at, previous_digest, event_digest
         FROM signer_trust_history WHERE project_id = ?1 ORDER BY rowid",
    ) { Ok(stmt) => stmt, Err(error) => return SignerTrustHistoryIntegrity { status: "unknown".into(), event_count: 0, head_digest: None, diagnostics: vec![error.to_string()] } };
    let rows = match stmt.query_map([project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    }) {
        Ok(rows) => match rows.collect::<Result<Vec<_>, _>>() {
            Ok(rows) => rows,
            Err(error) => {
                return SignerTrustHistoryIntegrity {
                    status: "unknown".into(),
                    event_count: 0,
                    head_digest: None,
                    diagnostics: vec![error.to_string()],
                }
            }
        },
        Err(error) => {
            return SignerTrustHistoryIntegrity {
                status: "unknown".into(),
                event_count: 0,
                head_digest: None,
                diagnostics: vec![error.to_string()],
            }
        }
    };
    let mut previous = String::new();
    let mut latest = std::collections::HashMap::new();
    for (
        id,
        fingerprint,
        identity,
        status,
        provenance,
        recorded_at,
        stored_previous,
        stored_digest,
    ) in &rows
    {
        let expected = trust_history_digest(&[
            &previous,
            id,
            project_id,
            fingerprint,
            identity,
            status,
            provenance,
            recorded_at,
        ]);
        if stored_previous != &previous || stored_digest != &expected {
            result.status = "invalid".into();
            result
                .diagnostics
                .push(format!("Digest chain mismatch at history event {id}"));
            break;
        }
        previous.clone_from(stored_digest);
        latest.insert(
            fingerprint.clone(),
            (identity.clone(), status.clone(), provenance.clone()),
        );
    }
    result.event_count = rows.len();
    result.head_digest = if previous.is_empty() {
        None
    } else {
        Some(previous)
    };
    if result.status == "verified" {
        let mut current = match conn.prepare("SELECT key_fingerprint, signer_identity, status, provenance FROM signer_trust WHERE project_id = ?1") {
            Ok(stmt) => stmt, Err(error) => { result.status = "unknown".into(); result.diagnostics.push(error.to_string()); return result; }
        };
        let rows = current.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        });
        match rows.and_then(|rows| rows.collect::<Result<Vec<_>, _>>()) {
            Ok(rows) => {
                for (fingerprint, identity, status, provenance) in rows {
                    if latest.get(&fingerprint) != Some(&(identity, status, provenance)) {
                        result.status = "invalid".into();
                        result.diagnostics.push(format!(
                            "Current trust policy does not match history for {fingerprint}"
                        ));
                        break;
                    }
                }
            }
            Err(error) => {
                result.status = "unknown".into();
                result.diagnostics.push(error.to_string());
            }
        }
    }
    result
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
        schema: BUNDLE_SCHEMA.into(),
        report_id: report.report.id.clone(),
        project_id: report.report.project_id.clone(),
        report_generated_at: report.report.generated_at.clone(),
        exported_at: exported_at.to_rfc3339(),
        age_seconds_at_export: age_seconds,
        freshness_status: freshness_status.into(),
        report_integrity_status: report.report.integrity_status.clone(),
        payload_sha256: sha256_hex(&payload),
        signature_status: "unsigned".into(),
        trust_scope: "self_hash_integrity_only_no_authorship_or_external_attestation".into(),
        signature_algorithm: None,
        signer_identity: None,
        public_key: None,
        key_fingerprint: None,
    };
    let unsigned = serde_json::to_vec(&UnsignedEvidenceBundle {
        manifest: &manifest,
        report,
    })?;
    serde_json::to_string_pretty(&EvidenceBundle {
        manifest: &manifest,
        report,
        bundle_sha256: sha256_hex(&unsigned),
        signature: None,
    })
    .map_err(AppError::Serde)
}

fn signing_identity_info(identity: &str, key: &SigningKey) -> SigningIdentityInfo {
    let public = key.verifying_key().to_bytes();
    SigningIdentityInfo {
        signer_identity: identity.into(),
        key_fingerprint: sha256_hex(&public),
        public_key: BASE64.encode(public),
        storage: "os_keychain".into(),
    }
}

fn export_signed_bundle(
    report: &AlignmentReportWithEvidence,
    exported_at: DateTime<Utc>,
    identity: &str,
    key: &SigningKey,
) -> Result<String, AppError> {
    let unsigned_json = export_evidence_bundle(report, exported_at)?;
    let unsigned: ImportedEvidenceBundle = serde_json::from_str(&unsigned_json)?;
    let public = key.verifying_key().to_bytes();
    let mut manifest = unsigned.manifest;
    manifest.signature_status = "signed_untrusted_identity".into();
    manifest.trust_scope = "signature_proves_key_possession_identity_requires_trust".into();
    manifest.signature_algorithm = Some("ed25519".into());
    manifest.signer_identity = Some(identity.into());
    manifest.public_key = Some(BASE64.encode(public));
    manifest.key_fingerprint = Some(sha256_hex(&public));
    let bytes = serde_json::to_vec(&UnsignedEvidenceBundle {
        manifest: &manifest,
        report,
    })?;
    let digest = sha256_hex(&bytes);
    let signature = key.sign(digest.as_bytes());
    serde_json::to_string_pretty(&EvidenceBundle {
        manifest: &manifest,
        report,
        bundle_sha256: digest,
        signature: Some(BASE64.encode(signature.to_bytes())),
    })
    .map_err(AppError::Serde)
}

fn verify_evidence_bundle_at(
    bundle_json: &str,
    verified_at: DateTime<Utc>,
) -> EvidenceBundleVerification {
    let mut result = EvidenceBundleVerification {
        status: "invalid".into(),
        schema: "unknown".into(),
        report_id: None,
        payload_integrity: "invalid".into(),
        bundle_integrity: "invalid".into(),
        report_integrity: "invalid".into(),
        signature_status: "unknown".into(),
        freshness_status: "unknown".into(),
        age_seconds: None,
        diagnostics: Vec::new(),
        key_fingerprint: None,
        signer_identity: None,
        trust_status: "unknown".into(),
        trust_provenance: None,
    };
    let bundle: ImportedEvidenceBundle = match serde_json::from_str(bundle_json) {
        Ok(bundle) => bundle,
        Err(error) => {
            result
                .diagnostics
                .push(format!("Malformed evidence bundle: {error}"));
            return result;
        }
    };
    result.schema = bundle.manifest.schema.clone();
    result.report_id = Some(bundle.report.report.id.clone());
    result.signature_status = bundle.manifest.signature_status.clone();
    if bundle.manifest.schema != BUNDLE_SCHEMA {
        result.status = "unsupported".into();
        result.diagnostics.push(format!(
            "Unsupported bundle schema: {}",
            bundle.manifest.schema
        ));
        return result;
    }
    let unsigned_contract = bundle.manifest.signature_status == "unsigned"
        && bundle.manifest.trust_scope
            == "self_hash_integrity_only_no_authorship_or_external_attestation"
        && bundle.signature.is_none();
    let signed_contract = bundle.manifest.signature_status == "signed_untrusted_identity"
        && bundle.manifest.trust_scope == "signature_proves_key_possession_identity_requires_trust"
        && bundle.manifest.signature_algorithm.as_deref() == Some("ed25519");
    if !unsigned_contract && !signed_contract {
        result.status = "unsupported".into();
        result
            .diagnostics
            .push("Unsupported or misleading signature metadata".into());
        return result;
    }

    let payload_digest = serde_json::to_vec(&bundle.report)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default();
    if payload_digest == bundle.manifest.payload_sha256 {
        result.payload_integrity = "verified".into();
    } else {
        result
            .diagnostics
            .push("Embedded report payload digest does not match".into());
    }
    let unsigned_digest = serde_json::to_vec(&UnsignedEvidenceBundle {
        manifest: &bundle.manifest,
        report: &bundle.report,
    })
    .map(|bytes| sha256_hex(&bytes))
    .unwrap_or_default();
    if unsigned_digest == bundle.bundle_sha256 {
        result.bundle_integrity = "verified".into();
    } else {
        result
            .diagnostics
            .push("Whole-bundle digest does not match".into());
    }
    if signed_contract {
        result.key_fingerprint = bundle.manifest.key_fingerprint.clone();
        result.signer_identity = bundle.manifest.signer_identity.clone();
        let signature_valid = (|| {
            let public_bytes = BASE64.decode(bundle.manifest.public_key.as_ref()?).ok()?;
            let public_array: [u8; 32] = public_bytes.try_into().ok()?;
            let verifying_key = VerifyingKey::from_bytes(&public_array).ok()?;
            if sha256_hex(&public_array) != *bundle.manifest.key_fingerprint.as_ref()? {
                return None;
            }
            let signature_bytes = BASE64.decode(bundle.signature.as_ref()?).ok()?;
            let signature = Signature::from_slice(&signature_bytes).ok()?;
            verifying_key
                .verify(bundle.bundle_sha256.as_bytes(), &signature)
                .ok()?;
            Some(())
        })()
        .is_some();
        if !signature_valid {
            result
                .diagnostics
                .push("Ed25519 signature or key fingerprint is invalid".into());
            result.signature_status = "invalid".into();
        } else {
            result.signature_status = "valid_untrusted_identity".into();
        }
    }
    if bundle.manifest.report_id != bundle.report.report.id
        || bundle.manifest.project_id != bundle.report.report.project_id
        || bundle.manifest.report_generated_at != bundle.report.report.generated_at
        || bundle.manifest.report_integrity_status != bundle.report.report.integrity_status
    {
        result
            .diagnostics
            .push("Manifest identity does not match the embedded report".into());
    }
    match alignment::digest_alignments(&bundle.report.alignments) {
        Ok(digest)
            if digest == bundle.report.report.evidence_digest
                && bundle.report.report.integrity_status == "verified" =>
        {
            result.report_integrity = "verified".into();
        }
        _ => result
            .diagnostics
            .push("Embedded report evidence integrity is not verified".into()),
    }

    match DateTime::parse_from_rfc3339(&bundle.report.report.generated_at) {
        Ok(generated) => {
            let age = verified_at
                .signed_duration_since(generated.with_timezone(&Utc))
                .num_seconds();
            if age < 0 {
                result
                    .diagnostics
                    .push("Report generation time is in the future".into());
            } else {
                result.age_seconds = Some(age);
                result.freshness_status = if age <= FRESHNESS_WINDOW_SECONDS {
                    "fresh"
                } else {
                    "stale"
                }
                .into();
            }
        }
        Err(_) => result
            .diagnostics
            .push("Report generation time is malformed".into()),
    }

    let identity_matches = bundle.manifest.report_id == bundle.report.report.id
        && bundle.manifest.project_id == bundle.report.report.project_id
        && bundle.manifest.report_generated_at == bundle.report.report.generated_at
        && bundle.manifest.report_integrity_status == bundle.report.report.integrity_status;
    if result.payload_integrity == "verified"
        && result.bundle_integrity == "verified"
        && result.report_integrity == "verified"
        && identity_matches
        && (unsigned_contract || result.signature_status == "valid_untrusted_identity")
    {
        result.status = if result.freshness_status == "fresh" {
            if signed_contract {
                "signed_untrusted"
            } else {
                "verified"
            }
        } else if result.freshness_status == "stale" {
            "stale"
        } else {
            "invalid"
        }
        .into();
    }
    result
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
    use crate::db::{queries, schema};
    use crate::models::project::CreateProjectRequest;
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

    #[test]
    fn verifier_accepts_intact_bundle_but_never_claims_a_signature() {
        let now = DateTime::parse_from_rfc3339("2026-07-11T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut source = report();
        source.report.evidence_digest = alignment::digest_alignments(&source.alignments).unwrap();
        let bundle = export_evidence_bundle(&source, now).unwrap();
        let verified = verify_evidence_bundle_at(&bundle, now);
        assert_eq!(verified.status, "verified");
        assert_eq!(verified.payload_integrity, "verified");
        assert_eq!(verified.bundle_integrity, "verified");
        assert_eq!(verified.report_integrity, "verified");
        assert_eq!(verified.signature_status, "unsigned");
        assert_eq!(verified.freshness_status, "fresh");
    }

    #[test]
    fn verifier_rejects_tampering_and_unsupported_contracts() {
        let now = DateTime::parse_from_rfc3339("2026-07-11T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut source = report();
        source.report.evidence_digest = alignment::digest_alignments(&source.alignments).unwrap();
        let bundle = export_evidence_bundle(&source, now).unwrap();

        let tampered = bundle.replace("project-1", "project-2");
        let invalid = verify_evidence_bundle_at(&tampered, now);
        assert_eq!(invalid.status, "invalid");
        assert_eq!(invalid.payload_integrity, "invalid");
        assert_eq!(invalid.bundle_integrity, "invalid");

        let unsupported = bundle.replace(BUNDLE_SCHEMA, "speccompanion.evidence-bundle.v2");
        let unsupported = verify_evidence_bundle_at(&unsupported, now);
        assert_eq!(unsupported.status, "unsupported");
        assert!(unsupported.diagnostics[0].contains("Unsupported bundle schema"));

        let forged_signature = bundle.replace("\"unsigned\"", "\"verified\"");
        let unsupported = verify_evidence_bundle_at(&forged_signature, now);
        assert_eq!(unsupported.status, "unsupported");
        assert!(unsupported.diagnostics[0].contains("signature metadata"));

        let stale_at = DateTime::parse_from_rfc3339("2026-07-13T00:00:01Z")
            .unwrap()
            .with_timezone(&Utc);
        let stale = verify_evidence_bundle_at(&bundle, stale_at);
        assert_eq!(stale.status, "stale");
        assert_eq!(stale.payload_integrity, "verified");
        assert_eq!(stale.bundle_integrity, "verified");
    }

    #[test]
    fn signed_bundle_proves_key_possession_but_not_identity_trust() {
        let now = DateTime::parse_from_rfc3339("2026-07-11T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut source = report();
        source.report.evidence_digest = alignment::digest_alignments(&source.alignments).unwrap();
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let bundle = export_signed_bundle(&source, now, "Release Engineering", &key).unwrap();
        let verified = verify_evidence_bundle_at(&bundle, now);
        assert_eq!(verified.status, "signed_untrusted");
        assert_eq!(verified.signature_status, "valid_untrusted_identity");
        assert_eq!(
            verified.signer_identity.as_deref(),
            Some("Release Engineering")
        );
        assert_eq!(verified.key_fingerprint.as_deref().unwrap().len(), 64);

        let tampered = bundle.replace("Release Engineering", "Security Team");
        let invalid = verify_evidence_bundle_at(&tampered, now);
        assert_eq!(invalid.status, "invalid");
        assert_eq!(invalid.bundle_integrity, "invalid");
        assert_eq!(invalid.signature_status, "valid_untrusted_identity");
    }

    #[test]
    fn signer_trust_is_project_scoped_and_history_is_append_only() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        schema::run_migrations(&conn).unwrap();
        let first = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "First".into(),
                codebase_path: "/tmp/first".into(),
            },
        )
        .unwrap();
        let second = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Second".into(),
                codebase_path: "/tmp/second".into(),
            },
        )
        .unwrap();
        let fingerprint = "a".repeat(64);
        upsert_signer_trust(
            &conn,
            &first.id,
            &fingerprint,
            "Release",
            "trusted",
            "verified out of band",
        )
        .unwrap();
        upsert_signer_trust(
            &conn,
            &first.id,
            &fingerprint,
            "Release",
            "revoked",
            "key retired",
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM signer_trust WHERE project_id = ?1 AND key_fingerprint = ?2",
                rusqlite::params![&first.id, &fingerprint],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "revoked");
        let history: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signer_trust_history WHERE project_id = ?1",
                [&first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history, 2);
        let integrity = verify_trust_history_integrity(&conn, &first.id);
        assert_eq!(integrity.status, "verified");
        assert_eq!(integrity.event_count, 2);
        assert_eq!(integrity.head_digest.as_deref().unwrap().len(), 64);
        let other: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signer_trust WHERE project_id = ?1",
                [&second.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other, 0);

        let replacement = "b".repeat(64);
        upsert_signer_trust(
            &conn,
            &first.id,
            &fingerprint,
            "Release",
            "trusted",
            "re-approved before rotation",
        )
        .unwrap();
        let rotated = rotate_signer_trust_records(
            &conn,
            &first.id,
            &fingerprint,
            &replacement,
            "Release 2027",
            "rotation ceremony ticket SEC-42",
        )
        .unwrap();
        assert_eq!(rotated[0].status, "revoked");
        assert_eq!(rotated[1].status, "trusted");
        let current: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT key_fingerprint, status FROM signer_trust WHERE project_id = ?1 ORDER BY key_fingerprint",
                )
                .unwrap();
            stmt.query_map([&first.id], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            current,
            vec![
                (fingerprint, "revoked".into()),
                (replacement.clone(), "trusted".into())
            ]
        );
        let history_after_rotation: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signer_trust_history WHERE project_id = ?1",
                [&first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_after_rotation, 5);

        let export_integrity = verify_trust_history_integrity(&conn, &first.id);
        assert_eq!(export_integrity.status, "verified");
        let history_proof = {
            let mut stmt = conn.prepare(
                "SELECT id, key_fingerprint, signer_identity, status, provenance, recorded_at, previous_digest, event_digest
                 FROM signer_trust_history WHERE project_id = ?1 ORDER BY rowid",
            ).unwrap();
            stmt.query_map([&first.id], |row| {
                Ok(PortableTrustHistoryEvent {
                    id: row.get(0)?,
                    key_fingerprint: row.get(1)?,
                    signer_identity: row.get(2)?,
                    status: row.get(3)?,
                    provenance: row.get(4)?,
                    recorded_at: row.get(5)?,
                    previous_digest: row.get(6)?,
                    event_digest: row.get(7)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        let payload = TrustPolicyPayload {
            schema: TRUST_POLICY_SCHEMA.into(),
            source_project_id: first.id.clone(),
            source_project_name: first.name.clone(),
            exported_at: "2026-07-12T00:00:00Z".into(),
            source_history_head_digest: export_integrity.head_digest.clone().unwrap(),
            source_history_event_count: export_integrity.event_count,
            proof_base_head_digest: String::new(),
            proof_base_event_count: 0,
            history_proof,
            policies: vec![PortableSignerPolicy {
                key_fingerprint: replacement.clone(),
                signer_identity: "Release 2027".into(),
                status: "trusted".into(),
                provenance: "rotation ceremony ticket SEC-42".into(),
            }],
        };
        let key = SigningKey::from_bytes(&[11_u8; 32]);
        let portable = export_signed_trust_policy(payload, "Recovery signer", &key).unwrap();
        let verified = verify_trust_policy(&portable);
        let recovery_fingerprint = sha256_hex(&key.verifying_key().to_bytes());
        assert_eq!(verified.status, "valid_untrusted");
        assert_eq!(verified.policy_count, 1);
        assert_eq!(verified.source_history_event_count, 5);
        assert_eq!(
            verified.source_history_head_digest,
            export_integrity.head_digest
        );
        assert_eq!(
            verified.key_fingerprint.as_deref(),
            Some(recovery_fingerprint.as_str())
        );

        let tampered = portable.replace("Release 2027", "Attacker");
        assert_eq!(verify_trust_policy(&tampered).status, "invalid");
        let mut tampered_anchor: serde_json::Value = serde_json::from_str(&portable).unwrap();
        tampered_anchor["payload"]["source_history_head_digest"] =
            serde_json::Value::String("0".repeat(64));
        assert_eq!(
            verify_trust_policy(&serde_json::to_string(&tampered_anchor).unwrap()).status,
            "invalid"
        );
        let legacy = portable.replace(TRUST_POLICY_SCHEMA, "speccompanion.signer-trust-policy.v1");
        assert_eq!(verify_trust_policy(&legacy).status, "unsupported");

        let recovery_policy = PortableSignerPolicy {
            key_fingerprint: replacement.clone(),
            signer_identity: "Release 2027".into(),
            status: "trusted".into(),
            provenance: "rotation ceremony ticket SEC-42".into(),
        };
        let preview = preview_trust_policy(&conn, &second.id, &[recovery_policy.clone()]).unwrap();
        assert_eq!(preview[0].action, "add");
        assert_eq!(preview[0].current_status, None);

        let imported = import_verified_trust_policy(
            &conn,
            &second.id,
            &[recovery_policy.clone()],
            &first.id,
            &recovery_fingerprint,
            verified.payload_sha256.as_deref().unwrap(),
            verified.source_history_head_digest.as_deref().unwrap(),
            verified.source_history_event_count,
            "fingerprint matched printed recovery record",
        )
        .unwrap();
        assert_eq!(imported.len(), 1);
        assert!(imported[0]
            .provenance
            .contains("fingerprint matched printed recovery record"));
        let witnessed_bundle: SignedTrustPolicyBundle = serde_json::from_str(&portable).unwrap();
        let repeated = assess_trust_anchor(&conn, &second.id, &witnessed_bundle).unwrap();
        assert_eq!(repeated.status, "repeated");
        let mut rollback = witnessed_bundle;
        rollback.payload.source_history_event_count -= 1;
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "rollback"
        );
        rollback.payload.source_history_event_count += 1;
        rollback.payload.source_history_head_digest = "0".repeat(64);
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "conflict"
        );
        let previous_head = rollback
            .payload
            .history_proof
            .last()
            .unwrap()
            .event_digest
            .clone();
        let next_id = "next-event".to_string();
        let next_recorded_at = "2026-07-12T01:00:00Z".to_string();
        let next_digest = trust_history_digest(&[
            &previous_head,
            &next_id,
            &first.id,
            &replacement,
            "Release 2027",
            "trusted",
            "continued policy",
            &next_recorded_at,
        ]);
        rollback
            .payload
            .history_proof
            .push(PortableTrustHistoryEvent {
                id: next_id,
                key_fingerprint: replacement.clone(),
                signer_identity: "Release 2027".into(),
                status: "trusted".into(),
                provenance: "continued policy".into(),
                recorded_at: next_recorded_at,
                previous_digest: previous_head,
                event_digest: next_digest.clone(),
            });
        rollback.payload.source_history_event_count += 1;
        rollback.payload.source_history_head_digest = next_digest;
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "forward_proven"
        );
        rollback.payload.proof_base_event_count = 6;
        rollback.payload.proof_base_head_digest = "2".repeat(64);
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "checkpoint_gap"
        );
        assert!(advance_verified_trust_anchor(
            &conn,
            &second.id,
            &rollback,
            "must not skip checkpoint"
        )
        .unwrap_err()
        .to_string()
        .contains("requires forward_proven"));
        rollback.payload.proof_base_event_count = 0;
        rollback.payload.proof_base_head_digest.clear();
        let included_witness = rollback.payload.history_proof
            [verified.source_history_event_count - 1]
            .event_digest
            .clone();
        rollback.payload.history_proof[verified.source_history_event_count - 1].event_digest =
            "1".repeat(64);
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "fork"
        );
        rollback.payload.history_proof[verified.source_history_event_count - 1].event_digest =
            included_witness;
        let advancement =
            advance_verified_trust_anchor(&conn, &second.id, &rollback, "bridge package SEC-43")
                .unwrap();
        assert_eq!(advancement.previous_event_count, 5);
        assert_eq!(advancement.advanced_event_count, 6);
        assert_eq!(
            assess_trust_anchor(&conn, &second.id, &rollback)
                .unwrap()
                .status,
            "repeated"
        );
        let receipt: (i64, i64, String) = conn
            .query_row(
                "SELECT previous_event_count, advanced_event_count, provenance
                 FROM trust_anchor_advancements WHERE id = ?1",
                [&advancement.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(receipt, (5, 6, "bridge package SEC-43".into()));
        let listed = list_trust_anchor_advancement_receipts(&conn, &second.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].package_signer_fingerprint, recovery_fingerprint);
        let first_export = export_trust_anchor_advancement_receipts(&conn, &second.id).unwrap();
        let second_export = export_trust_anchor_advancement_receipts(&conn, &second.id).unwrap();
        assert_eq!(first_export, second_export);
        assert!(first_export.contains("unsigned_local_receipts"));
        let replacement_preview =
            preview_trust_policy(&conn, &second.id, &[recovery_policy]).unwrap();
        assert_eq!(replacement_preview[0].action, "replace");
        assert_eq!(
            replacement_preview[0].current_status.as_deref(),
            Some("trusted")
        );

        let oversized = "x".repeat(MAX_TRUST_POLICY_BYTES + 1);
        let oversized_result = verify_trust_policy(&oversized);
        assert_eq!(oversized_result.status, "invalid");
        assert!(oversized_result.diagnostics[0].contains("1 MiB"));
    }

    #[test]
    fn compact_history_proof_validates_bounded_suffix_and_rejects_tampered_checkpoint() {
        let source_project_id = "source-project".to_string();
        let fingerprint = "a".repeat(64);
        let mut previous = String::new();
        let mut events = Vec::new();
        for index in 0..105 {
            let id = format!("event-{index}");
            let recorded_at = format!("2026-07-12T00:{:02}:00Z", index % 60);
            let event_digest = trust_history_digest(&[
                &previous,
                &id,
                &source_project_id,
                &fingerprint,
                "Release",
                "trusted",
                "ceremony",
                &recorded_at,
            ]);
            events.push(PortableTrustHistoryEvent {
                id,
                key_fingerprint: fingerprint.clone(),
                signer_identity: "Release".into(),
                status: "trusted".into(),
                provenance: "ceremony".into(),
                recorded_at,
                previous_digest: previous,
                event_digest: event_digest.clone(),
            });
            previous = event_digest;
        }
        let proof_base_event_count = events.len() - MAX_TRUST_HISTORY_PROOF_EVENTS;
        let proof_base_head_digest = events[proof_base_event_count - 1].event_digest.clone();
        let history_proof = events.split_off(proof_base_event_count);
        let mut payload = TrustPolicyPayload {
            schema: TRUST_POLICY_SCHEMA.into(),
            source_project_id,
            source_project_name: "Source".into(),
            exported_at: "2026-07-12T00:00:00Z".into(),
            source_history_head_digest: previous,
            source_history_event_count: 105,
            proof_base_head_digest,
            proof_base_event_count,
            history_proof,
            policies: vec![PortableSignerPolicy {
                key_fingerprint: fingerprint,
                signer_identity: "Release".into(),
                status: "trusted".into(),
                provenance: "ceremony".into(),
            }],
        };
        assert!(validate_portable_history_proof(&payload));
        payload.proof_base_head_digest = "0".repeat(64);
        assert!(!validate_portable_history_proof(&payload));
    }

    #[test]
    fn tampered_trust_history_and_projection_are_invalid() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        schema::run_migrations(&conn).unwrap();
        let project = queries::create_project(
            &conn,
            &CreateProjectRequest {
                name: "Integrity".into(),
                codebase_path: "/tmp/integrity".into(),
            },
        )
        .unwrap();
        let fingerprint = "c".repeat(64);
        upsert_signer_trust(
            &conn,
            &project.id,
            &fingerprint,
            "Release",
            "trusted",
            "ceremony",
        )
        .unwrap();
        upsert_signer_trust(
            &conn,
            &project.id,
            &fingerprint,
            "Release",
            "revoked",
            "retired",
        )
        .unwrap();
        conn.execute(
            "UPDATE signer_trust_history SET provenance = 'tampered' WHERE project_id = ?1 AND status = 'trusted'",
            [&project.id],
        ).unwrap();
        let integrity = verify_trust_history_integrity(&conn, &project.id);
        assert_eq!(integrity.status, "invalid");
        assert!(integrity.diagnostics[0].contains("Digest chain mismatch"));

        conn.execute(
            "DELETE FROM signer_trust_history WHERE project_id = ?1",
            [&project.id],
        )
        .unwrap();
        let projection = verify_trust_history_integrity(&conn, &project.id);
        assert_eq!(projection.status, "invalid");
        assert!(projection.diagnostics[0].contains("does not match history"));
    }
}
