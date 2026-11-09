use rusqlite::Connection;

use super::error::{AppError, Result};

pub const LATEST_VERSION: u32 = 1;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > LATEST_VERSION {
        return Err(AppError::database(format!(
            "database schema version {current} is newer than supported version {LATEST_VERSION}"
        )));
    }

    for version in current..LATEST_VERSION {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = migrate_step(conn, version).and_then(|()| {
            conn.pragma_update(None, "user_version", version + 1)?;
            Ok(())
        });
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(AppError::database(format!(
                    "migration from version {version} to {} failed: {error}",
                    version + 1
                )));
            }
        }
    }
    Ok(())
}

fn migrate_step(conn: &Connection, from_version: u32) -> Result<()> {
    match from_version {
        0 => migrate_to_v1(conn),
        _ => Err(AppError::database(format!(
            "unknown migration version: {from_version}"
        ))),
    }
}

fn migrate_to_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE installations (
            id TEXT PRIMARY KEY,
            category TEXT NOT NULL CHECK (category IN ('marketplace', 'local')),
            display_name TEXT NOT NULL,
            root_relpath TEXT NOT NULL,
            root_key TEXT NOT NULL,
            source_type TEXT,
            source_key TEXT,
            source_json TEXT,
            installed_revision TEXT,
            remote_revision TEXT,
            baseline_hash TEXT,
            current_hash TEXT,
            local_state TEXT NOT NULL DEFAULT 'unknown'
                CHECK (local_state IN ('unknown', 'clean', 'modified', 'missing')),
            update_state TEXT NOT NULL DEFAULT 'unknown'
                CHECK (update_state IN ('unknown', 'up_to_date', 'available', 'check_failed')),
            update_checked_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_error TEXT,
            UNIQUE (category, root_key),
            CHECK (
                (source_type IS NULL AND source_key IS NULL AND source_json IS NULL)
                OR
                (source_type IS NOT NULL AND source_key IS NOT NULL AND source_json IS NOT NULL)
            ),
            CHECK (category <> 'marketplace' OR source_key IS NOT NULL)
        );

        CREATE UNIQUE INDEX idx_installations_source
            ON installations(category, source_key)
            WHERE source_key IS NOT NULL;
        CREATE INDEX idx_installations_content
            ON installations(category, baseline_hash);

        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE market_cache (
            cache_key TEXT PRIMARY KEY,
            payload_json TEXT NOT NULL,
            fetched_at INTEGER NOT NULL,
            last_accessed_at INTEGER NOT NULL
        );
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_has_complete_v1_schema() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(tables, ["installations", "market_cache", "settings"]);
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn rejects_a_database_from_a_newer_app() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();

        let error = run_migrations(&conn).unwrap_err();
        assert!(error.message.contains("newer than supported version"));
    }
}
