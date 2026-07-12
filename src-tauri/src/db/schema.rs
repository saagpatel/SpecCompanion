use rusqlite::Connection;

const CURRENT_VERSION: i32 = 3;

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
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [CURRENT_VERSION],
        )?;
        tx.commit()?;
    }

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
