use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::Value;

use super::central_repo::CentralRepo;
use super::error::{AppError, Result};
use super::migrations;
use super::model::{
    Category, Installation, LocalState, MarketCacheEntry, Settings, SkillSource, UpdateState,
};

const SETTINGS_KEY: &str = "app";
const INSTALLATION_SELECT: &str = "
    SELECT id, category, display_name, root_relpath, root_key,
           source_type, source_key, source_json, installed_revision, remote_revision,
           baseline_hash, current_hash, local_state, update_state, update_checked_at,
           created_at, updated_at, last_error
    FROM installations
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
        let (source_type, source_key, source_json) = source_columns(installation.source.as_ref())?;
        self.conn()?.execute(
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

    pub fn market_cache(&self, cache_key: &str) -> Result<Option<MarketCacheEntry>> {
        let conn = self.conn()?;
        let cached = conn
            .query_row(
                "SELECT payload_json, fetched_at, last_accessed_at FROM market_cache WHERE cache_key = ?1",
                [cache_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((payload_json, fetched_at, last_accessed_at)) = cached else {
            return Ok(None);
        };
        let accessed_at = now_millis();
        conn.execute(
            "UPDATE market_cache SET last_accessed_at = ?1 WHERE cache_key = ?2",
            params![accessed_at, cache_key],
        )?;
        Ok(Some(MarketCacheEntry {
            cache_key: cache_key.to_owned(),
            payload: serde_json::from_str(&payload_json).map_err(AppError::database)?,
            fetched_at,
            last_accessed_at: accessed_at.max(last_accessed_at),
        }))
    }

    pub fn put_market_cache(
        &self,
        cache_key: &str,
        payload: &Value,
        fetched_at: i64,
    ) -> Result<()> {
        let payload_json = serde_json::to_string(payload)?;
        let accessed_at = now_millis();
        self.conn()?.execute(
            "INSERT INTO market_cache (cache_key, payload_json, fetched_at, last_accessed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(cache_key) DO UPDATE SET
                payload_json = excluded.payload_json,
                fetched_at = excluded.fetched_at,
                last_accessed_at = excluded.last_accessed_at",
            params![cache_key, payload_json, fetched_at, accessed_at],
        )?;
        Ok(())
    }

    pub fn clear_market_cache(&self) -> Result<usize> {
        Ok(self.conn()?.execute("DELETE FROM market_cache", [])?)
    }
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
    fn market_cache_can_be_read_stale_and_cleared() {
        let store = Store::in_memory().unwrap();
        let payload = serde_json::json!({ "items": [1, 2, 3] });
        store
            .put_market_cache("leaderboard:all", &payload, 123)
            .unwrap();

        let cached = store.market_cache("leaderboard:all").unwrap().unwrap();
        assert_eq!(cached.payload, payload);
        assert_eq!(cached.fetched_at, 123);
        assert_eq!(store.clear_market_cache().unwrap(), 1);
        assert!(store.market_cache("leaderboard:all").unwrap().is_none());
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
}
