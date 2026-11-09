use chrono::Utc;
use serde_json::Value;

use super::error::{AppError, ErrorCode, Result};
use super::model::CacheClearReport;
use super::sources::skills_sh::{LeaderboardKind, MarketplaceSkill, SkillsShClient};
use super::store::Store;

const CACHE_TTL_MILLIS: i64 = 5 * 60 * 1000;

pub fn clear_cache(store: &Store) -> Result<CacheClearReport> {
    Ok(CacheClearReport {
        cleared_entries: store.clear_market_cache()? as u64,
    })
}

pub fn leaderboard(
    store: &Store,
    kind: LeaderboardKind,
    force_refresh: bool,
) -> Result<Vec<MarketplaceSkill>> {
    let proxy = store.settings()?.proxy;
    cached_with_fetch(
        store,
        &format!("leaderboard:{}", kind.as_str()),
        force_refresh,
        || SkillsShClient::new(&proxy)?.fetch_leaderboard(kind),
    )
}

pub fn search(
    store: &Store,
    query: &str,
    limit: usize,
    force_refresh: bool,
) -> Result<Vec<MarketplaceSkill>> {
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
    let cache_key = format!("search:{}:{limit}", query.to_lowercase());
    cached_with_fetch(store, &cache_key, force_refresh, || {
        SkillsShClient::new(&proxy)?.search(query, limit)
    })
}

fn cached_with_fetch(
    store: &Store,
    cache_key: &str,
    force_refresh: bool,
    fetch: impl FnOnce() -> Result<Vec<MarketplaceSkill>>,
) -> Result<Vec<MarketplaceSkill>> {
    let now = Utc::now().timestamp_millis();
    let cached = store
        .market_cache(cache_key)?
        .map(|entry| decode_cache(entry.payload, entry.fetched_at))
        .transpose()?;

    if !force_refresh
        && let Some((skills, fetched_at)) = &cached
        && now.saturating_sub(*fetched_at) < CACHE_TTL_MILLIS
    {
        return Ok(skills.clone());
    }

    match fetch() {
        Ok(skills) => {
            let payload = serde_json::to_value(&skills)?;
            store.put_market_cache(cache_key, &payload, now)?;
            Ok(skills)
        }
        Err(error) if error.code == ErrorCode::Network => match cached {
            Some((skills, _)) => Ok(skills),
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

    fn skill(name: &str) -> MarketplaceSkill {
        MarketplaceSkill {
            id: format!("github:owner/repo/{name}"),
            skill_id: name.to_string(),
            name: name.to_string(),
            source: "owner/repo".to_string(),
            installs: 1,
        }
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
        cache(
            &store,
            "leaderboard:hot",
            &["cached"],
            Utc::now().timestamp_millis(),
        );

        let skills = cached_with_fetch(&store, "leaderboard:hot", false, || {
            panic!("fresh cache must not fetch")
        })
        .unwrap();

        assert_eq!(skills, vec![skill("cached")]);
    }

    #[test]
    fn stale_cache_is_refreshed_and_used_after_network_failure() {
        let store = Store::in_memory().unwrap();
        let old = Utc::now().timestamp_millis() - CACHE_TTL_MILLIS - 1;
        cache(&store, "leaderboard:hot", &["stale"], old);

        let skills = cached_with_fetch(&store, "leaderboard:hot", false, || {
            Err(AppError::new(ErrorCode::Network, "offline").retryable(true))
        })
        .unwrap();
        assert_eq!(skills, vec![skill("stale")]);

        let skills = cached_with_fetch(&store, "leaderboard:hot", false, || {
            Ok(vec![skill("fresh")])
        })
        .unwrap();
        assert_eq!(skills, vec![skill("fresh")]);
        let cached = store.market_cache("leaderboard:hot").unwrap().unwrap();
        assert!(cached.fetched_at > old);
    }

    #[test]
    fn force_refresh_bypasses_fresh_cache() {
        let store = Store::in_memory().unwrap();
        cache(
            &store,
            "leaderboard:hot",
            &["cached"],
            Utc::now().timestamp_millis(),
        );

        let skills = cached_with_fetch(&store, "leaderboard:hot", true, || {
            Ok(vec![skill("refreshed")])
        })
        .unwrap();

        assert_eq!(skills, vec![skill("refreshed")]);
    }

    #[test]
    fn only_network_failures_fall_back_to_stale_data() {
        let store = Store::in_memory().unwrap();
        let old = Utc::now().timestamp_millis() - CACHE_TTL_MILLIS - 1;
        cache(&store, "leaderboard:hot", &["stale"], old);

        let error = cached_with_fetch(&store, "leaderboard:hot", true, || {
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
    fn clearing_the_cache_reports_the_removed_entries() {
        let store = Store::in_memory().unwrap();
        cache(&store, "leaderboard:hot", &["one"], 1);
        cache(&store, "search:react:60", &["two"], 2);

        assert_eq!(clear_cache(&store).unwrap().cleared_entries, 2);
        assert_eq!(clear_cache(&store).unwrap().cleared_entries, 0);
        assert!(store.market_cache("leaderboard:hot").unwrap().is_none());
    }

    #[test]
    fn search_validation_and_cache_keys_are_consistent() {
        let store = Store::in_memory().unwrap();
        assert!(search(&store, " ", 60, false).is_err());
        assert!(search(&store, "react", 0, false).is_err());
        assert!(search(&store, "react", 301, false).is_err());

        let key = "search:react:60";
        cache(&store, key, &["cached"], Utc::now().timestamp_millis());
        let skills = search(&store, " React ", 60, false).unwrap();
        assert_eq!(skills, vec![skill("cached")]);
    }
}
