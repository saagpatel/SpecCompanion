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
use tauri::{AppHandle, State};
use uuid::Uuid;

const BUNDLE_SCHEMA: &str = "speccompanion.evidence-bundle.v1";
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

#[derive(Serialize)]
pub struct SigningIdentityInfo {
    pub signer_identity: String,
    pub key_fingerprint: String,
    pub public_key: String,
    pub storage: String,
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
    tx.execute(
        "INSERT INTO signer_trust_history (id, project_id, key_fingerprint, signer_identity, status, provenance, recorded_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![Uuid::new_v4().to_string(), project_id, key_fingerprint.to_lowercase(), signer_identity.trim(), status, provenance.trim(), now],
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
                rusqlite::params![first.id, fingerprint],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "revoked");
        let history: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signer_trust_history WHERE project_id = ?1",
                [first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history, 2);
        let other: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signer_trust WHERE project_id = ?1",
                [second.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other, 0);
    }
}
