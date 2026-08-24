use crate::db::Database;
use crate::models::research::{
    export_research_package, import_research_package_with_trust_store, ResearchAuthorityTrustStore,
    ResearchPackageImport,
};
use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ResearchAuthorityTrustRecord {
    pub key_fingerprint: String,
    pub status: String,
    pub provenance: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn inspect_research_package(
    state: State<'_, Database>,
    raw: String,
) -> Result<ResearchPackageImport, String> {
    let conn = state.conn.lock().map_err(|error| error.to_string())?;
    inspect_research_package_inner(&conn, &raw)
}

#[tauri::command]
pub fn export_canonical_research_package(
    state: State<'_, Database>,
    raw: String,
) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|error| error.to_string())?;
    let imported = inspect_research_package_inner(&conn, &raw)?;
    export_research_package(&imported)
}

#[tauri::command]
pub fn set_research_authority_trust(
    state: State<'_, Database>,
    key_fingerprint: String,
    status: String,
    provenance: String,
) -> Result<ResearchAuthorityTrustRecord, String> {
    let conn = state.conn.lock().map_err(|error| error.to_string())?;
    set_research_authority_trust_inner(&conn, &key_fingerprint, &status, &provenance)
}

fn inspect_research_package_inner(
    conn: &Connection,
    raw: &str,
) -> Result<ResearchPackageImport, String> {
    let trust_store = load_research_authority_trust(conn)?;
    import_research_package_with_trust_store(raw, &trust_store)
}

fn load_research_authority_trust(conn: &Connection) -> Result<ResearchAuthorityTrustStore, String> {
    let mut stmt = conn
        .prepare("SELECT key_fingerprint, status FROM research_authority_trust")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut trusted = Vec::new();
    let mut revoked = Vec::new();
    for row in rows {
        let (fingerprint, status) = row.map_err(|error| error.to_string())?;
        match status.as_str() {
            "trusted" => trusted.push(fingerprint),
            "revoked" => revoked.push(fingerprint),
            _ => return Err("local research authority trust store contains invalid status".into()),
        }
    }
    Ok(ResearchAuthorityTrustStore::from_fingerprints(
        trusted, revoked,
    ))
}

fn set_research_authority_trust_inner(
    conn: &Connection,
    key_fingerprint: &str,
    status: &str,
    provenance: &str,
) -> Result<ResearchAuthorityTrustRecord, String> {
    let fingerprint = key_fingerprint.trim().to_lowercase();
    if fingerprint.len() != 71
        || !fingerprint.starts_with("sha256:")
        || !fingerprint[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("research authority fingerprint must be sha256 plus 64 hex characters".into());
    }
    if !matches!(status, "trusted" | "revoked") {
        return Err("research authority status must be trusted or revoked".into());
    }
    let provenance = provenance.trim();
    if provenance.is_empty() {
        return Err("research authority trust requires provenance".into());
    }
    let updated_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO research_authority_trust
            (key_fingerprint, status, provenance, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key_fingerprint) DO UPDATE SET
            status = excluded.status,
            provenance = excluded.provenance,
            updated_at = excluded.updated_at",
        rusqlite::params![fingerprint, status, provenance, updated_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(ResearchAuthorityTrustRecord {
        key_fingerprint: fingerprint,
        status: status.into(),
        provenance: provenance.into(),
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::run_migrations;
    use crate::models::research::SourceLifecycleState;
    use rusqlite::Connection;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../fixtures/evidence-centered-research/qualified-package-v4.json");

    #[test]
    fn command_boundary_inspects_and_exports_v2_package() {
        let conn = Connection::open_in_memory().expect("database");
        run_migrations(&conn).expect("migrations");
        let inspected = inspect_research_package_inner(&conn, FIXTURE).expect("inspect fixture");
        assert_eq!(
            inspected.schema_version,
            "evidence-centered.research-package.v2"
        );
        assert!(inspected
            .source_lifecycle
            .iter()
            .any(|item| item.source_id == "source-unknown-authority"));
        let exported = export_research_package(&inspected).expect("export fixture");
        assert_eq!(
            crate::models::research::research_package_digest(&inspected.canonical_package).unwrap(),
            crate::models::research::research_package_digest(
                &serde_json::from_str(&exported).expect("parse exported fixture")
            )
            .unwrap()
        );
    }

    #[test]
    fn command_boundary_uses_durable_local_authority_trust() {
        let conn = Connection::open_in_memory().expect("database");
        run_migrations(&conn).expect("migrations");
        let package: Value = serde_json::from_str(FIXTURE).expect("fixture");
        let fingerprint = package["lifecycle_authorities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|authority| authority["authority_id"] == "fixture-registry-trusted")
            .and_then(|authority| authority["public_key_fingerprint"].as_str())
            .expect("trusted fixture fingerprint");
        set_research_authority_trust_inner(
            &conn,
            fingerprint,
            "trusted",
            "test-only local enrollment",
        )
        .expect("enroll local trust");
        let trusted = inspect_research_package_inner(&conn, FIXTURE).expect("trusted import");
        assert!(trusted.source_lifecycle.iter().any(|item| {
            item.source_id == "source-current" && item.state == SourceLifecycleState::Authenticated
        }));

        set_research_authority_trust_inner(
            &conn,
            fingerprint,
            "revoked",
            "test-only local revocation",
        )
        .expect("revoke local trust");
        let revoked = inspect_research_package_inner(&conn, FIXTURE).expect("revoked import");
        assert!(revoked.source_lifecycle.iter().any(|item| {
            item.source_id == "source-current"
                && item.state == SourceLifecycleState::RevokedAuthority
        }));
    }
}
