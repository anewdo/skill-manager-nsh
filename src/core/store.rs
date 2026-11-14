use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::central_repo::CentralRepo;
use super::error::{AppError, ErrorCode, Result};
use super::migrations;
use super::model::{InstalledSkill, LocalState, Settings, SkillSource, UpdateState};
use super::sources::github::common::GitVerification;

const SETTINGS_KEY: &str = "app";
const SKILL_SELECT: &str = "
    SELECT id, display_name, root_relpath, root_key,
           source_type, source_key, source_json, installed_revision, remote_revision,
           baseline_hash, current_hash, local_state, update_state, update_checked_at,
           created_at, updated_at, last_error
    FROM skills
";

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(repo: &CentralRepo) -> Result<Self> {
        repo.ensure()?;
        Self::open_database(repo.database_path())
    }

    pub fn open_database(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        migrations::run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|error| AppError::internal(format!("database mutex is poisoned: {error}")))
    }

    pub fn insert_skill(&self, skill: &InstalledSkill) -> Result<()> {
        self.insert_verified_skill(skill, None)
    }

    pub fn insert_verified_skill(
        &self,
        skill: &InstalledSkill,
        verification: Option<&GitVerification>,
    ) -> Result<()> {
        let conn = self.conn()?;
        insert_skill_on(&conn, skill, verification)
    }

    pub fn git_verification(&self, id: &str) -> Result<Option<GitVerification>> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT git_verification FROM skills WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        json.map(|json| serde_json::from_str(&json).map_err(AppError::database))
            .transpose()
    }

    pub fn update_local_state(
        &self,
        id: &str,
        current_hash: Option<&str>,
        local_state: LocalState,
        updated_at: i64,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE skills
             SET current_hash = ?1, local_state = ?2, updated_at = ?3
             WHERE id = ?4",
            params![current_hash, local_state.as_str(), updated_at, id],
        )?;
        if affected == 0 {
            return Err(skill_not_found());
        }
        Ok(())
    }

    pub fn update_skill_rename(
        &self,
        id: &str,
        display_name: &str,
        root_relpath: &str,
        root_key: &str,
        updated_at: i64,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE skills
             SET display_name = ?1, root_relpath = ?2, root_key = ?3, updated_at = ?4
             WHERE id = ?5",
            params![display_name, root_relpath, root_key, updated_at, id],
        )?;
        if affected == 0 {
            return Err(skill_not_found());
        }
        Ok(())
    }

    pub fn update_update_check(
        &self,
        id: &str,
        remote_revision: Option<&str>,
        update_state: UpdateState,
        checked_at: i64,
        last_error: Option<&str>,
        verification: Option<&GitVerification>,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE skills
             SET remote_revision = ?1, update_state = ?2, update_checked_at = ?3,
                 last_error = ?4, updated_at = ?3,
                 git_verification = CASE WHEN ?4 IS NULL THEN ?6 ELSE git_verification END
             WHERE id = ?5",
            params![
                remote_revision,
                update_state.as_str(),
                checked_at,
                last_error,
                id,
                verification.map(serde_json::to_string).transpose()?
            ],
        )?;
        if affected == 0 {
            return Err(skill_not_found());
        }
        Ok(())
    }

    pub fn update_skill_after_update(
        &self,
        id: &str,
        installed_revision: Option<&str>,
        remote_revision: Option<&str>,
        content_hash: &str,
        updated_at: i64,
        verification: Option<&GitVerification>,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE skills
             SET installed_revision = ?1, remote_revision = ?2, baseline_hash = ?3,
                 current_hash = ?3, local_state = ?4, update_state = ?5,
                 update_checked_at = ?6, last_error = NULL, updated_at = ?6,
                 git_verification = ?8
             WHERE id = ?7",
            params![
                installed_revision,
                remote_revision,
                content_hash,
                LocalState::Clean.as_str(),
                UpdateState::UpToDate.as_str(),
                updated_at,
                id,
                verification.map(serde_json::to_string).transpose()?
            ],
        )?;
        if affected == 0 {
            return Err(skill_not_found());
        }
        Ok(())
    }

    pub fn delete_skill_record(&self, id: &str) -> Result<()> {
        let affected = self
            .conn()?
            .execute("DELETE FROM skills WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(skill_not_found());
        }
        Ok(())
    }

    pub fn skill(&self, id: &str) -> Result<Option<InstalledSkill>> {
        let conn = self.conn()?;
        conn.query_row(&format!("{SKILL_SELECT} WHERE id = ?1"), [id], map_skill)
            .optional()
            .map_err(Into::into)
    }

    pub fn skill_by_source(&self, source_key: &str) -> Result<Option<InstalledSkill>> {
        let conn = self.conn()?;
        conn.query_row(
            &format!("{SKILL_SELECT} WHERE source_key = ?1"),
            [source_key],
            map_skill,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn skill_by_marketplace_entry(
        &self,
        repository: &str,
        marketplace_skill_id: &str,
    ) -> Result<Option<InstalledSkill>> {
        self.conn()?
            .query_row(
                &format!(
                    "{SKILL_SELECT} WHERE source_type = 'github'
                AND json_extract(source_json, '$.repository') = ?1
                AND json_extract(source_json, '$.marketplace_skill_id') = ?2
                AND json_extract(source_json, '$.git_ref') IS NULL"
                ),
                params![repository, marketplace_skill_id],
                map_skill,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_skills(&self) -> Result<Vec<InstalledSkill>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(&format!(
            "{SKILL_SELECT} ORDER BY display_name COLLATE NOCASE, id"
        ))?;
        let skills = statement
            .query_map([], map_skill)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(skills)
    }

    pub fn settings(&self) -> Result<Settings> {
        let conn = self.conn()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                [SETTINGS_KEY],
                |row| row.get(0),
            )
            .optional()?;
        value.map_or(Ok(Settings::default()), |json| {
            serde_json::from_str(&json).map_err(AppError::database)
        })
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        self.conn()?.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![SETTINGS_KEY, json, now_millis()],
        )?;
        Ok(())
    }
}

fn insert_skill_on(
    conn: &Connection,
    skill: &InstalledSkill,
    verification: Option<&GitVerification>,
) -> Result<()> {
    let (source_type, source_key, source_json) = source_columns(skill.source.as_ref())?;
    conn.execute(
        "INSERT INTO skills (
            id, display_name, root_relpath, root_key,
            source_type, source_key, source_json, installed_revision, remote_revision,
            baseline_hash, current_hash, local_state, update_state, update_checked_at,
            created_at, updated_at, last_error, git_verification
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
        )",
        params![
            skill.id,
            skill.display_name,
            skill.root_relpath,
            skill.root_key,
            source_type,
            source_key,
            source_json,
            skill.installed_revision,
            skill.remote_revision,
            skill.baseline_hash,
            skill.current_hash,
            skill.local_state.as_str(),
            skill.update_state.as_str(),
            skill.update_checked_at,
            skill.created_at,
            skill.updated_at,
            skill.last_error,
            verification.map(serde_json::to_string).transpose()?,
        ],
    )?;
    Ok(())
}

fn skill_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "skill was not found")
}

fn source_columns(
    source: Option<&SkillSource>,
) -> Result<(Option<&str>, Option<&str>, Option<String>)> {
    source.map_or(Ok((None, None, None)), |source| {
        Ok((
            Some(source.source_type.as_str()),
            Some(source.source_key.as_str()),
            Some(serde_json::to_string(source)?),
        ))
    })
}

fn map_skill(row: &Row<'_>) -> rusqlite::Result<InstalledSkill> {
    let source_type: Option<String> = row.get(4)?;
    let source_key: Option<String> = row.get(5)?;
    let source_json: Option<String> = row.get(6)?;
    let local_state: String = row.get(11)?;
    let update_state: String = row.get(12)?;

    let source = match (source_type, source_key, source_json) {
        (Some(_), Some(_), Some(json)) => {
            Some(serde_json::from_str(&json).map_err(|error| invalid_text_column(6, error))?)
        }
        _ => None,
    };

    Ok(InstalledSkill {
        id: row.get(0)?,
        display_name: row.get(1)?,
        root_relpath: row.get(2)?,
        root_key: row.get(3)?,
        source,
        installed_revision: row.get(7)?,
        remote_revision: row.get(8)?,
        baseline_hash: row.get(9)?,
        current_hash: row.get(10)?,
        local_state: LocalState::parse(&local_state)
            .map_err(|error| invalid_text_column(11, error))?,
        update_state: UpdateState::parse(&update_state)
            .map_err(|error| invalid_text_column(12, error))?,
        update_checked_at: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        last_error: row.get(16)?,
    })
}

fn invalid_text_column(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;
    use crate::core::model::{SourceType, Theme};

    fn skill(id: &str, root_key: &str, source_key: Option<&str>) -> InstalledSkill {
        InstalledSkill {
            id: id.to_owned(),
            display_name: id.to_owned(),
            root_relpath: root_key.to_owned(),
            root_key: root_key.to_owned(),
            source: source_key.map(|key| SkillSource {
                marketplace_skill_id: None,
                source_type: SourceType::GitHub,
                source_key: key.to_owned(),
                repository: Some("owner/repo".to_owned()),
                git_ref: Some("main".to_owned()),
                subpath: Some("skills/demo".to_owned()),
                url: None,
                digest: None,
            }),
            installed_revision: Some("abc123".to_owned()),
            remote_revision: None,
            baseline_hash: Some("hash".to_owned()),
            current_hash: Some("hash".to_owned()),
            local_state: LocalState::Clean,
            update_state: UpdateState::Unknown,
            update_checked_at: None,
            created_at: 100,
            updated_at: 200,
            last_error: None,
        }
    }

    #[test]
    fn skill_round_trip_preserves_lineage_and_state() {
        let store = Store::in_memory().unwrap();
        let expected = skill("one", "demo", Some("github:owner/repo:skills/demo"));
        store.insert_skill(&expected).unwrap();

        assert_eq!(store.skill("one").unwrap(), Some(expected.clone()));
        assert_eq!(store.list_skills().unwrap(), [expected]);
    }

    #[test]
    fn root_and_source_are_unique_across_the_library() {
        let store = Store::in_memory().unwrap();
        store
            .insert_skill(&skill("one", "demo", Some("source-one")))
            .unwrap();

        let duplicate_root = skill("two", "demo", Some("source-two"));
        assert!(store.insert_skill(&duplicate_root).is_err());
        let duplicate_source = skill("three", "other", Some("source-one"));
        assert!(store.insert_skill(&duplicate_source).is_err());

        let local_duplicate_root = skill("four", "demo", None);
        assert!(store.insert_skill(&local_duplicate_root).is_err());
        let local = skill("five", "local-demo", None);
        store.insert_skill(&local).unwrap();
        let another_local = skill("six", "local-other", None);
        store.insert_skill(&another_local).unwrap();
        assert_eq!(store.skill("five").unwrap().unwrap().source, None);
    }

    #[test]
    fn settings_default_and_survive_reopening() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("skills-manager.db");
        {
            let store = Store::open_database(&path).unwrap();
            assert_eq!(store.settings().unwrap(), Settings::default());
            let settings = Settings {
                theme: Theme::Dark,
                ..Settings::default()
            };
            store.save_settings(&settings).unwrap();
        }

        let store = Store::open_database(path).unwrap();
        assert_eq!(store.settings().unwrap().theme, Theme::Dark);
    }

    #[test]
    fn opens_with_sqlite_safety_pragmas() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open_database(temp.path().join("skills-manager.db")).unwrap();
        let conn = store.conn().unwrap();

        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let foreign_keys: u32 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let busy_timeout: u32 = conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(busy_timeout, 5000);
    }

    #[test]
    fn local_state_and_rename_updates_leave_lineage_untouched() {
        let store = Store::in_memory().unwrap();
        let original = skill("one", "demo", Some("source-one"));
        store.insert_skill(&original).unwrap();

        store
            .update_local_state("one", None, LocalState::Missing, 300)
            .unwrap();
        let current = store.skill("one").unwrap().unwrap();
        assert_eq!(current.local_state, LocalState::Missing);
        assert_eq!(current.current_hash, None);
        assert_eq!(current.baseline_hash, original.baseline_hash);
        assert_eq!(current.updated_at, 300);

        store
            .update_skill_rename("one", "renamed", "scope/renamed", "scope/renamed", 400)
            .unwrap();
        let current = store.skill("one").unwrap().unwrap();
        assert_eq!(current.display_name, "renamed");
        assert_eq!(current.root_relpath, "scope/renamed");
        assert_eq!(current.root_key, "scope/renamed");
        assert_eq!(current.source, original.source);
        assert_eq!(current.installed_revision, original.installed_revision);
        assert_eq!(current.created_at, original.created_at);
    }

    #[test]
    fn update_checks_and_applied_updates_persist_their_state() {
        let store = Store::in_memory().unwrap();
        let original = skill("one", "demo", Some("source-one"));
        store.insert_skill(&original).unwrap();

        store
            .update_update_check(
                "one",
                Some("remote-rev"),
                UpdateState::Available,
                500,
                None,
                None,
            )
            .unwrap();
        let checked = store.skill("one").unwrap().unwrap();
        assert_eq!(checked.remote_revision.as_deref(), Some("remote-rev"));
        assert_eq!(checked.update_state, UpdateState::Available);
        assert_eq!(checked.update_checked_at, Some(500));
        assert_eq!(checked.installed_revision, original.installed_revision);
        assert_eq!(checked.baseline_hash, original.baseline_hash);

        store
            .update_update_check(
                "one",
                Some("remote-rev"),
                UpdateState::CheckFailed,
                600,
                Some("offline"),
                None,
            )
            .unwrap();
        let failed = store.skill("one").unwrap().unwrap();
        assert_eq!(failed.update_state, UpdateState::CheckFailed);
        assert_eq!(failed.last_error.as_deref(), Some("offline"));

        store
            .update_skill_after_update(
                "one",
                Some("remote-rev"),
                Some("remote-rev"),
                "next",
                700,
                None,
            )
            .unwrap();
        let updated = store.skill("one").unwrap().unwrap();
        assert_eq!(updated.installed_revision.as_deref(), Some("remote-rev"));
        assert_eq!(updated.remote_revision.as_deref(), Some("remote-rev"));
        assert_eq!(updated.baseline_hash.as_deref(), Some("next"));
        assert_eq!(updated.current_hash.as_deref(), Some("next"));
        assert_eq!(updated.local_state, LocalState::Clean);
        assert_eq!(updated.update_state, UpdateState::UpToDate);
        assert_eq!(updated.update_checked_at, Some(700));
        assert_eq!(updated.last_error, None);
    }

    #[test]
    fn missing_records_report_not_found_instead_of_silently_succeeding() {
        let store = Store::in_memory().unwrap();
        assert_eq!(
            store
                .update_local_state("missing", None, LocalState::Missing, 1)
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            store
                .update_skill_rename("missing", "name", "name", "name", 1)
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            store.delete_skill_record("missing").unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            store
                .update_update_check("missing", None, UpdateState::UpToDate, 1, None, None)
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }
}
