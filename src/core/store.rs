use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::central_repo::CentralRepo;
use super::error::{AppError, ErrorCode, Result};
use super::migrations;
use super::model::{
    Category, Installation, LocalState, Settings, SkillSource, TrashEntry, UpdateState,
};

const SETTINGS_KEY: &str = "app";
const INSTALLATION_SELECT: &str = "
    SELECT id, category, display_name, root_relpath, root_key,
           source_type, source_key, source_json, installed_revision, remote_revision,
           baseline_hash, current_hash, local_state, update_state, update_checked_at,
           created_at, updated_at, last_error
    FROM installations
";
const TRASH_SELECT: &str = "
    SELECT id, installation_id, category, display_name, original_relpath,
           original_root_key, trash_relpath, installation_json, deleted_at
    FROM trash
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

    pub fn insert_installation(&self, installation: &Installation) -> Result<()> {
        let conn = self.conn()?;
        insert_installation_on(&conn, installation)
    }

    pub fn update_local_state(
        &self,
        id: &str,
        current_hash: Option<&str>,
        local_state: LocalState,
        updated_at: i64,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE installations
             SET current_hash = ?1, local_state = ?2, updated_at = ?3
             WHERE id = ?4",
            params![current_hash, local_state.as_str(), updated_at, id],
        )?;
        if affected == 0 {
            return Err(installation_not_found());
        }
        Ok(())
    }

    pub fn update_installation_rename(
        &self,
        id: &str,
        display_name: &str,
        root_relpath: &str,
        root_key: &str,
        updated_at: i64,
    ) -> Result<()> {
        let affected = self.conn()?.execute(
            "UPDATE installations
             SET display_name = ?1, root_relpath = ?2, root_key = ?3, updated_at = ?4
             WHERE id = ?5",
            params![display_name, root_relpath, root_key, updated_at, id],
        )?;
        if affected == 0 {
            return Err(installation_not_found());
        }
        Ok(())
    }

    pub fn delete_installation_record(&self, id: &str) -> Result<()> {
        let affected = self
            .conn()?
            .execute("DELETE FROM installations WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(installation_not_found());
        }
        Ok(())
    }

    pub fn trash_installation(&self, entry: &TrashEntry) -> Result<()> {
        let installation_json = serde_json::to_string(&entry.installation)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO trash (
                id, installation_id, category, display_name, original_relpath,
                original_root_key, trash_relpath, installation_json, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.installation_id,
                entry.category.as_str(),
                entry.display_name,
                entry.original_relpath,
                entry.original_root_key,
                entry.trash_relpath,
                installation_json,
                entry.deleted_at,
            ],
        )?;
        let deleted = transaction.execute(
            "DELETE FROM installations WHERE id = ?1",
            [&entry.installation_id],
        )?;
        if deleted == 0 {
            return Err(installation_not_found());
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn trash_entries(&self) -> Result<Vec<TrashEntry>> {
        let conn = self.conn()?;
        let mut statement =
            conn.prepare(&format!("{TRASH_SELECT} ORDER BY deleted_at DESC, id"))?;
        let entries = statement
            .query_map([], map_trash_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }

    pub fn trash_entry(&self, id: &str) -> Result<Option<TrashEntry>> {
        let conn = self.conn()?;
        conn.query_row(
            &format!("{TRASH_SELECT} WHERE id = ?1"),
            [id],
            map_trash_entry,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn restore_trash_entry(&self, id: &str, installation: &Installation) -> Result<()> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        insert_installation_on(&transaction, installation)?;
        let deleted = transaction.execute("DELETE FROM trash WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "trash entry was not found",
            ));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_trash_entry(&self, id: &str) -> Result<()> {
        let affected = self
            .conn()?
            .execute("DELETE FROM trash WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "trash entry was not found",
            ));
        }
        Ok(())
    }

    pub fn installation(&self, id: &str) -> Result<Option<Installation>> {
        let conn = self.conn()?;
        conn.query_row(
            &format!("{INSTALLATION_SELECT} WHERE id = ?1"),
            [id],
            map_installation,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_installations(&self, category: Category) -> Result<Vec<Installation>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(&format!(
            "{INSTALLATION_SELECT} WHERE category = ?1 ORDER BY display_name COLLATE NOCASE, id"
        ))?;
        let installations = statement
            .query_map([category.as_str()], map_installation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(installations)
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

fn insert_installation_on(conn: &Connection, installation: &Installation) -> Result<()> {
    let (source_type, source_key, source_json) = source_columns(installation.source.as_ref())?;
    conn.execute(
        "INSERT INTO installations (
            id, category, display_name, root_relpath, root_key,
            source_type, source_key, source_json, installed_revision, remote_revision,
            baseline_hash, current_hash, local_state, update_state, update_checked_at,
            created_at, updated_at, last_error
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
        )",
        params![
            installation.id,
            installation.category.as_str(),
            installation.display_name,
            installation.root_relpath,
            installation.root_key,
            source_type,
            source_key,
            source_json,
            installation.installed_revision,
            installation.remote_revision,
            installation.baseline_hash,
            installation.current_hash,
            installation.local_state.as_str(),
            installation.update_state.as_str(),
            installation.update_checked_at,
            installation.created_at,
            installation.updated_at,
            installation.last_error,
        ],
    )?;
    Ok(())
}

fn map_trash_entry(row: &Row<'_>) -> rusqlite::Result<TrashEntry> {
    let category: String = row.get(2)?;
    let installation_json: String = row.get(7)?;
    let installation =
        serde_json::from_str(&installation_json).map_err(|error| invalid_text_column(7, error))?;
    Ok(TrashEntry {
        id: row.get(0)?,
        installation_id: row.get(1)?,
        category: Category::parse(&category).map_err(|error| invalid_text_column(2, error))?,
        display_name: row.get(3)?,
        original_relpath: row.get(4)?,
        original_root_key: row.get(5)?,
        trash_relpath: row.get(6)?,
        installation,
        deleted_at: row.get(8)?,
    })
}

fn installation_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "installation was not found")
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

fn map_installation(row: &Row<'_>) -> rusqlite::Result<Installation> {
    let category: String = row.get(1)?;
    let source_type: Option<String> = row.get(5)?;
    let source_key: Option<String> = row.get(6)?;
    let source_json: Option<String> = row.get(7)?;
    let local_state: String = row.get(12)?;
    let update_state: String = row.get(13)?;

    let source = match (source_type, source_key, source_json) {
        (Some(_), Some(_), Some(json)) => {
            Some(serde_json::from_str(&json).map_err(|error| invalid_text_column(7, error))?)
        }
        _ => None,
    };

    Ok(Installation {
        id: row.get(0)?,
        category: Category::parse(&category).map_err(|error| invalid_text_column(1, error))?,
        display_name: row.get(2)?,
        root_relpath: row.get(3)?,
        root_key: row.get(4)?,
        source,
        installed_revision: row.get(8)?,
        remote_revision: row.get(9)?,
        baseline_hash: row.get(10)?,
        current_hash: row.get(11)?,
        local_state: LocalState::parse(&local_state)
            .map_err(|error| invalid_text_column(12, error))?,
        update_state: UpdateState::parse(&update_state)
            .map_err(|error| invalid_text_column(13, error))?,
        update_checked_at: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        last_error: row.get(17)?,
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

    fn installation(
        id: &str,
        category: Category,
        root_key: &str,
        source_key: Option<&str>,
    ) -> Installation {
        Installation {
            id: id.to_owned(),
            category,
            display_name: id.to_owned(),
            root_relpath: root_key.to_owned(),
            root_key: root_key.to_owned(),
            source: source_key.map(|key| SkillSource {
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
    fn installation_round_trip_preserves_lineage_and_state() {
        let store = Store::in_memory().unwrap();
        let expected = installation(
            "one",
            Category::Marketplace,
            "demo",
            Some("github:owner/repo:skills/demo"),
        );
        store.insert_installation(&expected).unwrap();

        assert_eq!(store.installation("one").unwrap(), Some(expected.clone()));
        assert_eq!(
            store.list_installations(Category::Marketplace).unwrap(),
            [expected]
        );
        assert!(
            store
                .list_installations(Category::Local)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn root_and_source_uniqueness_are_scoped_to_category() {
        let store = Store::in_memory().unwrap();
        store
            .insert_installation(&installation(
                "one",
                Category::Marketplace,
                "demo",
                Some("source-one"),
            ))
            .unwrap();

        let duplicate_root = installation("two", Category::Marketplace, "demo", Some("source-two"));
        assert!(store.insert_installation(&duplicate_root).is_err());
        let duplicate_source =
            installation("three", Category::Marketplace, "other", Some("source-one"));
        assert!(store.insert_installation(&duplicate_source).is_err());

        let local = installation("four", Category::Local, "demo", None);
        store.insert_installation(&local).unwrap();
        assert!(store.installation("four").unwrap().is_some());
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
    fn marketplace_requires_a_source_lineage() {
        let store = Store::in_memory().unwrap();
        let invalid = installation("one", Category::Marketplace, "demo", None);
        let error = store.insert_installation(&invalid).unwrap_err();
        assert_eq!(error.code, ErrorCode::Database);
    }

    #[test]
    fn local_state_and_rename_updates_leave_lineage_untouched() {
        let store = Store::in_memory().unwrap();
        let original = installation("one", Category::Marketplace, "demo", Some("source-one"));
        store.insert_installation(&original).unwrap();

        store
            .update_local_state("one", None, LocalState::Missing, 300)
            .unwrap();
        let current = store.installation("one").unwrap().unwrap();
        assert_eq!(current.local_state, LocalState::Missing);
        assert_eq!(current.current_hash, None);
        assert_eq!(current.baseline_hash, original.baseline_hash);
        assert_eq!(current.updated_at, 300);

        store
            .update_installation_rename("one", "renamed", "scope/renamed", "scope/renamed", 400)
            .unwrap();
        let current = store.installation("one").unwrap().unwrap();
        assert_eq!(current.display_name, "renamed");
        assert_eq!(current.root_relpath, "scope/renamed");
        assert_eq!(current.root_key, "scope/renamed");
        assert_eq!(current.source, original.source);
        assert_eq!(current.installed_revision, original.installed_revision);
        assert_eq!(current.created_at, original.created_at);
    }

    #[test]
    fn trash_round_trips_restores_and_keeps_the_record() {
        let store = Store::in_memory().unwrap();
        let original = installation("one", Category::Marketplace, "demo", Some("source-one"));
        store.insert_installation(&original).unwrap();
        let entry = TrashEntry {
            id: "trash-one".to_string(),
            installation_id: original.id.clone(),
            category: original.category,
            display_name: original.display_name.clone(),
            original_relpath: original.root_relpath.clone(),
            original_root_key: original.root_key.clone(),
            trash_relpath: "trash-one".to_string(),
            installation: original.clone(),
            deleted_at: 50,
        };

        store.trash_installation(&entry).unwrap();
        assert!(store.installation("one").unwrap().is_none());
        let entries = store.trash_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry);
        assert_eq!(store.trash_entry("trash-one").unwrap(), Some(entry.clone()));

        let mut restored = original.clone();
        restored.updated_at = 500;
        store.restore_trash_entry(&entry.id, &restored).unwrap();
        assert_eq!(store.installation("one").unwrap(), Some(restored));
        assert!(store.trash_entry("trash-one").unwrap().is_none());

        store.trash_installation(&entry).unwrap();
        store.delete_trash_entry(&entry.id).unwrap();
        assert!(store.trash_entries().unwrap().is_empty());
        assert!(store.installation("one").unwrap().is_none());
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
                .update_installation_rename("missing", "name", "name", "name", 1)
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            store
                .delete_installation_record("missing")
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            store.delete_trash_entry("missing").unwrap_err().code,
            ErrorCode::NotFound
        );
    }
}
