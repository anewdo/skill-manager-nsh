use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use super::error::{AppError, ErrorCode, Result};
use super::model::CacheClearReport;
use super::sources::skills_sh::{LeaderboardKind, MarketplaceSkill, SkillsShClient};
use super::store::Store;

const CACHE_TTL_MILLIS: i64 = 5 * 60 * 1000;
/// The version of the cached dataset shape, part of every cache key: a payload
/// written by another version of the app is never read as this one.
const CACHE_VERSION: &str = "v2";

/// What the marketplace shows: the entries and how old they are.
#[derive(Debug, Clone, Serialize)]
pub struct MarketDataset {
    pub skills: Vec<MarketplaceSkill>,
    /// When the entries were fetched from skills.sh, in milliseconds since the epoch.
    pub fetched_at: i64,
    /// The network failed and the entries come from the cache.
    pub stale: bool,
}

pub fn clear_cache(store: &Store) -> Result<CacheClearReport> {
    Ok(CacheClearReport {
        cleared_entries: store.clear_market_cache()? as u64,
    })
}

pub fn leaderboard(
    store: &Store,
    kind: LeaderboardKind,
    force_refresh: bool,
) -> Result<MarketDataset> {
    let proxy = store.settings()?.proxy;
    cached_with_fetch(
        store,
        &format!("{CACHE_VERSION}:leaderboard:{}", kind.as_str()),
        force_refresh,
        || SkillsShClient::new(&proxy)?.fetch_leaderboard(kind),
    )
}

pub fn search(
    store: &Store,
    query: &str,
    limit: usize,
    force_refresh: bool,
) -> Result<MarketDataset> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::invalid_input("search query is required"));
    }
    if !(1..=300).contains(&limit) {
        return Err(AppError::invalid_input(
            "search limit must be between 1 and 300",
        ));
    }

    let proxy = store.settings()?.proxy;
    let cache_key = format!("{CACHE_VERSION}:search:{}:{limit}", query.to_lowercase());
    cached_with_fetch(store, &cache_key, force_refresh, || {
        SkillsShClient::new(&proxy)?.search(query, limit)
    })
}

fn cached_with_fetch(
    store: &Store,
    cache_key: &str,
    force_refresh: bool,
    fetch: impl FnOnce() -> Result<Vec<MarketplaceSkill>>,
) -> Result<MarketDataset> {
    let now = Utc::now().timestamp_millis();
    let cached = store
        .market_cache(cache_key)?
        .map(|entry| decode_cache(entry.payload, entry.fetched_at))
        .transpose()?;

    if !force_refresh
        && let Some((skills, fetched_at)) = &cached
        && now.saturating_sub(*fetched_at) < CACHE_TTL_MILLIS
    {
        return Ok(MarketDataset {
            skills: skills.clone(),
            fetched_at: *fetched_at,
            stale: false,
        });
    }

    match fetch() {
        Ok(skills) => {
            let payload = serde_json::to_value(&skills)?;
            store.put_market_cache(cache_key, &payload, now)?;
            Ok(MarketDataset {
                skills,
                fetched_at: now,
                stale: false,
            })
        }
        Err(error) if error.code == ErrorCode::Network => match cached {
            Some((skills, fetched_at)) => Ok(MarketDataset {
                skills,
                fetched_at,
                stale: true,
            }),
            None => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn decode_cache(payload: Value, fetched_at: i64) -> Result<(Vec<MarketplaceSkill>, i64)> {
    let skills = serde_json::from_value(payload)
        .map_err(|error| AppError::database(format!("invalid market cache payload: {error}")))?;
    Ok((skills, fetched_at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sources::skills_sh::marketplace_source;

    fn skill(name: &str) -> MarketplaceSkill {
        MarketplaceSkill {
            id: format!("github:owner/repo/{name}"),
            skill_id: name.to_string(),
            name: name.to_string(),
            source: "owner/repo".to_string(),
            installs: 1,
            lineage: marketplace_source("owner/repo", name).unwrap(),
        }
    }

    fn leaderboard_key() -> String {
        format!("{CACHE_VERSION}:leaderboard:hot")
    }

    fn cache(store: &Store, key: &str, names: &[&str], fetched_at: i64) {
        let skills = names.iter().map(|name| skill(name)).collect::<Vec<_>>();
        store
            .put_market_cache(key, &serde_json::to_value(skills).unwrap(), fetched_at)
            .unwrap();
    }

    #[test]
    fn fresh_cache_avoids_fetching() {
        let store = Store::in_memory().unwrap();
        let fetched_at = Utc::now().timestamp_millis();
        cache(&store, &leaderboard_key(), &["cached"], fetched_at);

        let dataset = cached_with_fetch(&store, &leaderboard_key(), false, || {
            panic!("fresh cache must not fetch")
        })
        .unwrap();

        assert_eq!(dataset.skills, vec![skill("cached")]);
        assert_eq!(dataset.fetched_at, fetched_at);
        assert!(!dataset.stale);
    }

    #[test]
    fn stale_cache_is_refreshed_and_used_after_network_failure() {
        let store = Store::in_memory().unwrap();
        let old = Utc::now().timestamp_millis() - CACHE_TTL_MILLIS - 1;
        cache(&store, &leaderboard_key(), &["stale"], old);

        let dataset = cached_with_fetch(&store, &leaderboard_key(), false, || {
            Err(AppError::new(ErrorCode::Network, "offline").retryable(true))
        })
        .unwrap();
        assert_eq!(dataset.skills, vec![skill("stale")]);
        assert_eq!(dataset.fetched_at, old);
        assert!(dataset.stale);

        let dataset = cached_with_fetch(&store, &leaderboard_key(), false, || {
            Ok(vec![skill("fresh")])
        })
        .unwrap();
        assert_eq!(dataset.skills, vec![skill("fresh")]);
        assert!(dataset.fetched_at > old);
        assert!(!dataset.stale);
        let cached = store.market_cache(&leaderboard_key()).unwrap().unwrap();
        assert!(cached.fetched_at > old);
    }

    #[test]
    fn force_refresh_bypasses_fresh_cache() {
        let store = Store::in_memory().unwrap();
        cache(
            &store,
            &leaderboard_key(),
            &["cached"],
            Utc::now().timestamp_millis(),
        );

        let dataset = cached_with_fetch(&store, &leaderboard_key(), true, || {
            Ok(vec![skill("refreshed")])
        })
        .unwrap();

        assert_eq!(dataset.skills, vec![skill("refreshed")]);
        assert!(!dataset.stale);
    }

    #[test]
    fn only_network_failures_fall_back_to_stale_data() {
        let store = Store::in_memory().unwrap();
        let old = Utc::now().timestamp_millis() - CACHE_TTL_MILLIS - 1;
        cache(&store, &leaderboard_key(), &["stale"], old);

        let error = cached_with_fetch(&store, &leaderboard_key(), true, || {
            Err(AppError::invalid_input("bad proxy"))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn network_failure_without_cache_preserves_structured_error() {
        let store = Store::in_memory().unwrap();

        let error = cached_with_fetch(&store, "missing", false, || {
            Err(AppError::new(ErrorCode::Network, "offline").retryable(true))
        })
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::Network);
        assert!(error.retryable);
    }

    #[test]
    fn cache_payloads_of_another_version_are_never_read() {
        let store = Store::in_memory().unwrap();
        cache(
            &store,
            "leaderboard:hot",
            &["cached"],
            Utc::now().timestamp_millis(),
        );

        let dataset = cached_with_fetch(&store, &leaderboard_key(), false, || {
            Ok(vec![skill("fetched")])
        })
        .unwrap();

        assert_eq!(dataset.skills, vec![skill("fetched")]);
    }

    #[test]
    fn clearing_the_cache_reports_the_removed_entries() {
        let store = Store::in_memory().unwrap();
        cache(&store, &leaderboard_key(), &["one"], 1);
        cache(
            &store,
            &format!("{CACHE_VERSION}:search:react:60"),
            &["two"],
            2,
        );

        assert_eq!(clear_cache(&store).unwrap().cleared_entries, 2);
        assert_eq!(clear_cache(&store).unwrap().cleared_entries, 0);
        assert!(store.market_cache(&leaderboard_key()).unwrap().is_none());
    }

    #[test]
    fn search_validation_and_cache_keys_are_consistent() {
        let store = Store::in_memory().unwrap();
        assert!(search(&store, " ", 60, false).is_err());
        assert!(search(&store, "react", 0, false).is_err());
        assert!(search(&store, "react", 301, false).is_err());

        let key = format!("{CACHE_VERSION}:search:react:60");
        cache(&store, &key, &["cached"], Utc::now().timestamp_millis());
        let dataset = search(&store, " React ", 60, false).unwrap();
        assert_eq!(dataset.skills, vec![skill("cached")]);
        assert!(!dataset.stale);
    }
}
