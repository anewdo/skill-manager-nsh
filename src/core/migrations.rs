use rusqlite::Connection;

use super::error::{AppError, Result};

pub const LATEST_VERSION: u32 = 2;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > LATEST_VERSION {
        return Err(AppError::database(format!(
            "database schema version {current} is newer than supported version {LATEST_VERSION}"
        )));
    }
    if current == LATEST_VERSION {
        return Ok(());
    }

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result: Result<()> = (|| {
        if current == 0 {
            migrate_to_v1(conn)?;
        }
        conn.execute_batch("ALTER TABLE skills ADD COLUMN git_verification TEXT;")?;
        conn.pragma_update(None, "user_version", LATEST_VERSION)?;
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(AppError::database(format!(
                "migration to version {LATEST_VERSION} failed: {error}"
            )));
        }
    }
    Ok(())
}

fn migrate_to_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE skills (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            root_relpath TEXT NOT NULL,
            root_key TEXT NOT NULL UNIQUE,
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
            CHECK (
                (source_type IS NULL AND source_key IS NULL AND source_json IS NULL)
                OR
                (source_type IS NOT NULL AND source_key IS NOT NULL AND source_json IS NOT NULL)
            )
        );

        CREATE UNIQUE INDEX idx_skills_source
            ON skills(source_key)
            WHERE source_key IS NOT NULL;
        CREATE INDEX idx_skills_content ON skills(baseline_hash);

        -- User overrides for built-in agents, plus user-defined agents.
        CREATE TABLE agents (
            key TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
            custom_path TEXT,
            mode TEXT CHECK (mode IN ('link', 'copy')),
            is_custom INTEGER NOT NULL DEFAULT 0 CHECK (is_custom IN (0, 1)),
            display_name TEXT,
            updated_at INTEGER NOT NULL,
            CHECK (is_custom = 0 OR (display_name IS NOT NULL AND custom_path IS NOT NULL))
        );

        -- One real skills directory. Agents sharing a directory share a target.
        CREATE TABLE deploy_targets (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL CHECK (scope IN ('global', 'project')),
            root_path TEXT NOT NULL,
            root_key TEXT NOT NULL UNIQUE,
            project_path TEXT,
            created_at INTEGER NOT NULL,
            CHECK ((scope = 'project') = (project_path IS NOT NULL))
        );

        CREATE TABLE target_agents (
            target_id TEXT NOT NULL REFERENCES deploy_targets(id) ON DELETE CASCADE,
            agent_key TEXT NOT NULL,
            PRIMARY KEY (target_id, agent_key)
        );

        CREATE TABLE presets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            name_key TEXT NOT NULL UNIQUE,
            icon TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE preset_skills (
            preset_id TEXT NOT NULL REFERENCES presets(id) ON DELETE CASCADE,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (preset_id, skill_id)
        );
        CREATE INDEX idx_preset_skills_skill ON preset_skills(skill_id);

        -- Desired state: presets and single skills switched on for a target.
        CREATE TABLE target_presets (
            target_id TEXT NOT NULL REFERENCES deploy_targets(id) ON DELETE CASCADE,
            preset_id TEXT NOT NULL REFERENCES presets(id) ON DELETE CASCADE,
            PRIMARY KEY (target_id, preset_id)
        );
        CREATE INDEX idx_target_presets_preset ON target_presets(preset_id);

        CREATE TABLE target_skills (
            target_id TEXT NOT NULL REFERENCES deploy_targets(id) ON DELETE CASCADE,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            PRIMARY KEY (target_id, skill_id)
        );
        CREATE INDEX idx_target_skills_skill ON target_skills(skill_id);

        -- Actual state: what the app placed in a target. A row records past
        -- intent, never proof of what is on disk now, so it has no cascade:
        -- the files must be removed before the row is.
        CREATE TABLE deployments (
            target_id TEXT NOT NULL REFERENCES deploy_targets(id),
            skill_id TEXT NOT NULL REFERENCES skills(id),
            dir_name TEXT NOT NULL,
            dir_key TEXT NOT NULL,
            mode TEXT NOT NULL CHECK (mode IN ('link', 'copy')),
            deployed_hash TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_error TEXT,
            PRIMARY KEY (target_id, skill_id),
            UNIQUE (target_id, dir_key)
        );
        CREATE INDEX idx_deployments_skill ON deployments(skill_id);

        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_has_the_complete_schema() {
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
        assert_eq!(
            tables,
            [
                "agents",
                "deploy_targets",
                "deployments",
                "preset_skills",
                "presets",
                "settings",
                "skills",
                "target_agents",
                "target_presets",
                "target_skills",
            ]
        );
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
