use rusqlite::Connection;
use sha2::{Digest, Sha256};

const CURRENT_VERSION: i32 = 9;

pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );",
    )?;

    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if version < CURRENT_VERSION {
        let tx = conn.unchecked_transaction()?;
        if version < 1 {
            migrate_v1(&tx)?;
        }
        if version < 2 {
            migrate_v2(&tx)?;
        }
        if version < 3 {
            migrate_v3(&tx)?;
        }
        if version < 4 {
            migrate_v4(&tx)?;
        }
        if version < 5 {
            migrate_v5(&tx)?;
        }
        if version < 6 {
            migrate_v6(&tx)?;
        }
        if version < 7 {
            migrate_v7(&tx)?;
        }
        if version < 8 {
            migrate_v8(&tx)?;
        }
        if version < 9 {
            migrate_v9(&tx)?;
        }
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [CURRENT_VERSION],
        )?;
        tx.commit()?;
    }

    Ok(())
}

fn advancement_receipt_digest(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE trust_anchor_advancements ADD COLUMN previous_receipt_digest TEXT NOT NULL DEFAULT '';
         ALTER TABLE trust_anchor_advancements ADD COLUMN receipt_digest TEXT NOT NULL DEFAULT '';",
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, source_project_id, package_signer_fingerprint,
                previous_head_digest, previous_event_count, advanced_head_digest,
                advanced_event_count, payload_sha256, provenance, advanced_at
         FROM trust_anchor_advancements
         ORDER BY project_id, source_project_id, package_signer_fingerprint,
                  previous_event_count, advanced_event_count, advanced_at, id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut scope = (String::new(), String::new(), String::new());
    let mut previous = String::new();
    for (
        id,
        project,
        source,
        signer,
        old_head,
        old_count,
        new_head,
        new_count,
        payload,
        provenance,
        at,
    ) in rows
    {
        let next_scope = (project.clone(), source.clone(), signer.clone());
        if next_scope != scope {
            scope = next_scope;
            previous.clear();
        }
        let digest = advancement_receipt_digest(&[
            &previous,
            &id,
            &project,
            &source,
            &signer,
            &old_head,
            &old_count.to_string(),
            &new_head,
            &new_count.to_string(),
            &payload,
            &provenance,
            &at,
        ]);
        conn.execute(
            "UPDATE trust_anchor_advancements SET previous_receipt_digest = ?1, receipt_digest = ?2 WHERE id = ?3",
            rusqlite::params![previous, digest, id],
        )?;
        previous = digest;
    }
    Ok(())
}

fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE trust_anchor_advancements (
            id TEXT PRIMARY KEY NOT NULL,
            project_id TEXT NOT NULL,
            source_project_id TEXT NOT NULL,
            package_signer_fingerprint TEXT NOT NULL,
            previous_head_digest TEXT NOT NULL,
            previous_event_count INTEGER NOT NULL,
            advanced_head_digest TEXT NOT NULL,
            advanced_event_count INTEGER NOT NULL,
            payload_sha256 TEXT NOT NULL,
            provenance TEXT NOT NULL,
            advanced_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_trust_anchor_advancements_scope
          ON trust_anchor_advancements(project_id, source_project_id, package_signer_fingerprint, advanced_at);",
    )
}

fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE trust_anchor_witnesses (
            project_id TEXT NOT NULL,
            source_project_id TEXT NOT NULL,
            package_signer_fingerprint TEXT NOT NULL,
            history_head_digest TEXT NOT NULL,
            history_event_count INTEGER NOT NULL,
            payload_sha256 TEXT NOT NULL,
            witnessed_at TEXT NOT NULL,
            PRIMARY KEY (project_id, source_project_id, package_signer_fingerprint),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );",
    )
}

fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE signer_trust_history ADD COLUMN previous_digest TEXT NOT NULL DEFAULT '';
         ALTER TABLE signer_trust_history ADD COLUMN event_digest TEXT NOT NULL DEFAULT '';",
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, key_fingerprint, signer_identity, status, provenance, recorded_at
         FROM signer_trust_history ORDER BY project_id, rowid",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut project = String::new();
    let mut previous = String::new();
    for (id, project_id, fingerprint, identity, status, provenance, recorded_at) in rows {
        if project != project_id {
            project.clone_from(&project_id);
            previous.clear();
        }
        let digest = history_digest(
            &previous,
            &id,
            &project_id,
            &fingerprint,
            &identity,
            &status,
            &provenance,
            &recorded_at,
        );
        conn.execute(
            "UPDATE signer_trust_history SET previous_digest = ?2, event_digest = ?3 WHERE id = ?1",
            rusqlite::params![id, previous, digest],
        )?;
        previous = digest;
    }
    Ok(())
}

fn history_digest(
    previous: &str,
    id: &str,
    project: &str,
    fingerprint: &str,
    identity: &str,
    status: &str,
    provenance: &str,
    recorded_at: &str,
) -> String {
    let fields = [
        previous,
        id,
        project,
        fingerprint,
        identity,
        status,
        provenance,
        recorded_at,
    ];
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE signer_trust (
            project_id TEXT NOT NULL,
            key_fingerprint TEXT NOT NULL,
            signer_identity TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('trusted', 'revoked')),
            provenance TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (project_id, key_fingerprint),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE TABLE signer_trust_history (
            id TEXT PRIMARY KEY NOT NULL,
            project_id TEXT NOT NULL,
            key_fingerprint TEXT NOT NULL,
            signer_identity TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('trusted', 'revoked')),
            provenance TEXT NOT NULL,
            recorded_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_signer_trust_history_project
            ON signer_trust_history(project_id, recorded_at);",
    )
}

fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE test_results ADD COLUMN provenance_digest TEXT NOT NULL DEFAULT '';",
    )?;
    Ok(())
}

fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE test_results ADD COLUMN execution_controls_json TEXT NOT NULL DEFAULT '{}';",
    )?;
    Ok(())
}

fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE requirements ADD COLUMN content_fingerprint TEXT NOT NULL DEFAULT '';
         ALTER TABLE requirements ADD COLUMN source_line_start INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE requirements ADD COLUMN source_line_end INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE alignment_reports ADD COLUMN verified_requirements INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE alignment_reports ADD COLUMN partial_requirements INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE alignment_reports ADD COLUMN failed_requirements INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE alignment_reports ADD COLUMN unknown_requirements INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE alignment_reports ADD COLUMN evidence_digest TEXT NOT NULL DEFAULT '';
         ALTER TABLE alignment_reports ADD COLUMN checked_languages_json TEXT NOT NULL DEFAULT '[]';
         ALTER TABLE alignment_reports ADD COLUMN skipped_languages_json TEXT NOT NULL DEFAULT '[]';
         ALTER TABLE alignment_reports ADD COLUMN diagnostics_json TEXT NOT NULL DEFAULT '[]';

         CREATE TABLE requirement_alignments (
             report_id TEXT NOT NULL,
             requirement_id TEXT NOT NULL,
             classification TEXT NOT NULL,
             reason TEXT NOT NULL,
             details_json TEXT NOT NULL,
             sort_index INTEGER NOT NULL,
             PRIMARY KEY (report_id, requirement_id),
             FOREIGN KEY (report_id) REFERENCES alignment_reports(id) ON DELETE CASCADE,
             FOREIGN KEY (requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
         );
         CREATE INDEX idx_requirement_alignments_report_id
             ON requirement_alignments(report_id, sort_index);",
    )?;
    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            codebase_path TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS specs (
            id TEXT PRIMARY KEY NOT NULL,
            project_id TEXT NOT NULL,
            filename TEXT NOT NULL,
            content TEXT NOT NULL,
            parsed_at TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS requirements (
            id TEXT PRIMARY KEY NOT NULL,
            spec_id TEXT NOT NULL,
            section TEXT NOT NULL,
            description TEXT NOT NULL,
            req_type TEXT NOT NULL DEFAULT 'functional',
            priority TEXT NOT NULL DEFAULT 'medium',
            FOREIGN KEY (spec_id) REFERENCES specs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS generated_tests (
            id TEXT PRIMARY KEY NOT NULL,
            requirement_id TEXT NOT NULL,
            framework TEXT NOT NULL,
            code TEXT NOT NULL,
            generation_mode TEXT NOT NULL,
            file_path TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS test_results (
            id TEXT PRIMARY KEY NOT NULL,
            generated_test_id TEXT NOT NULL,
            status TEXT NOT NULL,
            execution_time_ms INTEGER NOT NULL DEFAULT 0,
            stdout TEXT NOT NULL DEFAULT '',
            stderr TEXT NOT NULL DEFAULT '',
            executed_at TEXT NOT NULL,
            FOREIGN KEY (generated_test_id) REFERENCES generated_tests(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS alignment_reports (
            id TEXT PRIMARY KEY NOT NULL,
            project_id TEXT NOT NULL,
            coverage_percent REAL NOT NULL DEFAULT 0.0,
            total_requirements INTEGER NOT NULL DEFAULT 0,
            covered_requirements INTEGER NOT NULL DEFAULT 0,
            generated_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS alignment_mismatches (
            id TEXT PRIMARY KEY NOT NULL,
            report_id TEXT NOT NULL,
            requirement_id TEXT NOT NULL,
            spec_section TEXT NOT NULL,
            code_element TEXT,
            mismatch_type TEXT NOT NULL,
            details TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (report_id) REFERENCES alignment_reports(id) ON DELETE CASCADE,
            FOREIGN KEY (requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_specs_project_id ON specs(project_id);
        CREATE INDEX IF NOT EXISTS idx_requirements_spec_id ON requirements(spec_id);
        CREATE INDEX IF NOT EXISTS idx_generated_tests_requirement_id ON generated_tests(requirement_id);
        CREATE INDEX IF NOT EXISTS idx_test_results_generated_test_id ON test_results(generated_test_id);
        CREATE INDEX IF NOT EXISTS idx_alignment_reports_project_id ON alignment_reports(project_id);
        CREATE INDEX IF NOT EXISTS idx_alignment_mismatches_report_id ON alignment_mismatches(report_id);"
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_reaches_evidence_schema() {
        let conn = Connection::open_in_memory().expect("database");
        run_migrations(&conn).expect("migrations");
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("version");
        assert_eq!(version, CURRENT_VERSION);
        conn.prepare(
            "SELECT content_fingerprint, source_line_start, source_line_end FROM requirements",
        )
        .expect("requirement evidence columns");
        conn.prepare("SELECT evidence_digest, checked_languages_json FROM alignment_reports")
            .expect("report evidence columns");
        conn.prepare("SELECT details_json FROM requirement_alignments")
            .expect("alignment evidence table");
        conn.prepare("SELECT execution_controls_json FROM test_results")
            .expect("typed execution controls");
        conn.prepare("SELECT provenance_digest FROM test_results")
            .expect("execution provenance digest");
        conn.prepare("SELECT status, provenance FROM signer_trust")
            .expect("signer trust policy");
        conn.prepare("SELECT recorded_at, previous_digest, event_digest FROM signer_trust_history")
            .expect("signer trust history");
        conn.prepare("SELECT history_head_digest, history_event_count FROM trust_anchor_witnesses")
            .expect("trust anchor witness ledger");
        conn.prepare("SELECT previous_head_digest, advanced_head_digest, provenance, previous_receipt_digest, receipt_digest FROM trust_anchor_advancements")
            .expect("trust anchor advancement receipts");
    }

    #[test]
    fn version_eight_upgrade_backfills_advancement_receipt_chain() {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("CREATE TABLE schema_version (version INTEGER NOT NULL); INSERT INTO schema_version VALUES (8);")
            .expect("version table");
        for migration in [
            migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, migrate_v6, migrate_v7,
            migrate_v8,
        ] {
            migration(&conn).expect("legacy migration");
        }
        conn.execute(
            "INSERT INTO projects VALUES ('project', 'Project', '/tmp/project', 'now', 'now')",
            [],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO trust_anchor_advancements VALUES
             ('receipt', 'project', 'source', 'signer', 'old', 1, 'new', 2, 'payload', 'verified locally', 'now')",
            [],
        )
        .expect("legacy receipt");
        run_migrations(&conn).expect("upgrade");
        let (previous, digest): (String, String) = conn
            .query_row(
                "SELECT previous_receipt_digest, receipt_digest FROM trust_anchor_advancements",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("backfilled receipt");
        assert!(previous.is_empty());
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn version_one_upgrade_preserves_existing_requirements_as_untrusted() {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("CREATE TABLE schema_version (version INTEGER NOT NULL); INSERT INTO schema_version VALUES (1);")
            .expect("version table");
        migrate_v1(&conn).expect("v1 schema");
        conn.execute(
            "INSERT INTO projects VALUES ('project', 'Project', '/tmp/project', 'now', 'now')",
            [],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO specs VALUES ('spec', 'project', 'spec.md', 'content', NULL, 'now')",
            [],
        )
        .expect("spec");
        conn.execute("INSERT INTO requirements (id, spec_id, section, description, req_type, priority) VALUES ('legacy', 'spec', 'Requirements', 'Legacy requirement', 'functional', 'medium')", [])
            .expect("legacy requirement");

        run_migrations(&conn).expect("upgrade");
        let row: (String, i64) = conn.query_row(
            "SELECT content_fingerprint, source_line_start FROM requirements WHERE id = 'legacy'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).expect("legacy evidence defaults");
        assert_eq!(row, (String::new(), 0));
    }
}
