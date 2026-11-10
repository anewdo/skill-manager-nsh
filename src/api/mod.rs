use std::sync::{Arc, OnceLock};

use nsh_core::{JsonValue, prelude::*};
use serde::Serialize;

use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, Result};
use crate::core::http;
use crate::core::installations;
use crate::core::market;
use crate::core::model::{
    Category, Installation, OperationEvent, OperationKind, Settings, SettingsPatch, SkillSource,
    UpdateConflictPolicy,
};
use crate::core::operations::OperationQueue;
use crate::core::settings;
use crate::core::sources::{preview, skills_sh::LeaderboardKind};
use crate::core::store::Store;

struct AppState {
    repo: CentralRepo,
    store: Arc<Store>,
    market: market::MarketCache,
    preview: preview::PreviewCache,
}

static APP_STATE: OnceLock<Result<Arc<AppState>>> = OnceLock::new();
static OPERATIONS: OnceLock<OperationQueue> = OnceLock::new();

/// Native application services.
#[module]
pub struct AppNative;

#[module]
impl AppNative {
    /// Returns the current SQLite schema version.
    pub fn schema_version() -> u32 {
        crate::core::migrations::LATEST_VERSION
    }

    /// Loads a skills.sh leaderboard from the network.
    pub async fn market_leaderboard(
        kind: String,
        force_refresh: bool,
    ) -> std::result::Result<JsonValue, JsError> {
        let kind = LeaderboardKind::parse(&kind).map_err(js_error)?;
        run_json(move || {
            let state = app_state()?;
            let proxy = state.store.settings()?.proxy;
            market::leaderboard(&state.market, &proxy, kind, force_refresh)
        })
        .await
    }

    /// Searches skills.sh on the network.
    pub async fn market_search(
        query: String,
        limit: u32,
        force_refresh: bool,
    ) -> std::result::Result<JsonValue, JsError> {
        run_json(move || {
            let state = app_state()?;
            let proxy = state.store.settings()?.proxy;
            market::search(&state.market, &proxy, &query, limit as usize, force_refresh)
        })
        .await
    }

    /// Downloads and reads a skill only when the user explicitly requests a preview.
    ///
    /// Reopening the same skill within the short preview TTL is served from the
    /// in-memory cache instead of downloading again.
    pub async fn market_preview(source: JsonValue) -> std::result::Result<JsonValue, JsError> {
        let source: SkillSource = serde_json::from_value(source).map_err(|error| {
            js_error(AppError::invalid_input(format!(
                "invalid skill source: {error}"
            )))
        })?;
        run_json(move || {
            let state = app_state()?;
            let proxy = state.store.settings()?.proxy;
            let preview = preview::load(&state.preview, &state.store, &state.repo, source, &proxy)?;
            Ok(preview)
        })
        .await
    }

    /// Queues a marketplace installation and returns its operation ID.
    pub fn install_marketplace(source: JsonValue) -> std::result::Result<JsonValue, JsError> {
        let source: SkillSource = serde_json::from_value(source).map_err(|error| {
            js_error(AppError::invalid_input(format!(
                "invalid skill source: {error}"
            )))
        })?;
        let state = app_state().map_err(js_error)?;
        let id = operations()
            .enqueue(OperationKind::Install, move |context| {
                let proxy = state.store.settings()?.proxy;
                installations::install_marketplace(
                    &state.repo,
                    &state.store,
                    source,
                    &proxy,
                    &context,
                )
            })
            .map_err(js_error)?;
        to_json(id)
    }

    /// Returns the running operation, or the next queued operation.
    pub fn active_operation() -> std::result::Result<JsonValue, JsError> {
        to_json(operations().active_operation())
    }

    /// Returns a retained operation snapshot, or null for an unknown ID.
    pub fn operation_snapshot(id: String) -> std::result::Result<JsonValue, JsError> {
        to_json(operations().snapshot(&id))
    }

    /// Requests cancellation without interrupting a filesystem commit.
    pub fn cancel_operation(id: String) -> std::result::Result<JsonValue, JsError> {
        operations().cancel(&id).map_err(js_error).and_then(to_json)
    }

    /// Posts operation state, progress and installation-change events to TypeScript.
    pub async fn subscribe_operations(
        on_event: Callback<JsonValue>,
    ) -> std::result::Result<(), JsError> {
        let mut events = operations().subscribe();
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    OperationEvent::resync(operations().active_operation())
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
            };
            on_event.post(to_json(event)?);
        }
    }

    /// Reads the persisted application settings.
    pub async fn get_settings() -> std::result::Result<JsonValue, JsError> {
        run_json(|| {
            let settings: Settings = app_state()?.store.settings()?;
            Ok(settings)
        })
        .await
    }

    /// Applies a partial settings patch and returns the persisted settings.
    pub async fn update_settings(patch: JsonValue) -> std::result::Result<JsonValue, JsError> {
        let patch: SettingsPatch = serde_json::from_value(patch).map_err(|error| {
            js_error(AppError::invalid_input(format!(
                "invalid settings patch: {error}"
            )))
        })?;
        run_json(move || settings::update(&app_state()?.store, patch)).await
    }

    /// Clears the in-memory market and preview caches and reports the removed count.
    pub async fn clear_cache() -> std::result::Result<JsonValue, JsError> {
        run_json(|| {
            let state = app_state()?;
            let mut report = market::clear_cache(&state.market);
            report.cleared_entries += state.preview.clear();
            Ok(report)
        })
        .await
    }

    /// Probes the download sources through the configured proxy.
    pub async fn test_connectivity() -> std::result::Result<JsonValue, JsError> {
        run_json(|| {
            let proxy = app_state()?.store.settings()?.proxy;
            http::test_connectivity(&proxy)
        })
        .await
    }

    /// Returns the absolute data directory owned by this application.
    pub async fn data_directory() -> std::result::Result<JsonValue, JsError> {
        run_json(|| Ok(app_state()?.repo.root().display().to_string())).await
    }

    /// Lists installations in one category.
    pub async fn list_installations(category: String) -> std::result::Result<JsonValue, JsError> {
        let category = parse_category(&category).map_err(js_error)?;
        run_json(move || {
            let installations: Vec<Installation> =
                app_state()?.store.list_installations(category)?;
            Ok(installations)
        })
        .await
    }

    /// Reads one installation together with its locally observed Skill state.
    pub async fn installation_detail(id: String) -> std::result::Result<JsonValue, JsError> {
        run_json(move || {
            let state = app_state()?;
            installations::installation_detail(&state.repo, &state.store, &id)
        })
        .await
    }

    /// Queues a rename and returns its operation ID.
    pub fn rename_installation(
        id: String,
        new_name: String,
    ) -> std::result::Result<JsonValue, JsError> {
        let state = app_state().map_err(js_error)?;
        let operation = operations()
            .enqueue(OperationKind::Rename, move |context| {
                installations::rename_installation(
                    &state.repo,
                    &state.store,
                    &id,
                    &new_name,
                    &context,
                )
            })
            .map_err(js_error)?;
        to_json(operation)
    }

    /// Queues a batch delete; modified installations require explicit confirmation.
    pub fn delete_installations(
        ids: JsonValue,
        confirm_modified: bool,
    ) -> std::result::Result<JsonValue, JsError> {
        let ids = parse_id_list(ids).map_err(js_error)?;
        let state = app_state().map_err(js_error)?;
        let operation = operations()
            .enqueue(OperationKind::Delete, move |context| {
                installations::delete_installations(
                    &state.repo,
                    &state.store,
                    &ids,
                    confirm_modified,
                    &context,
                )
            })
            .map_err(js_error)?;
        to_json(operation)
    }

    /// Queues a manual update check for the given installations.
    pub fn check_updates(ids: JsonValue) -> std::result::Result<JsonValue, JsError> {
        let ids = parse_id_list(ids).map_err(js_error)?;
        let state = app_state().map_err(js_error)?;
        let operation = operations()
            .enqueue(OperationKind::CheckUpdates, move |context| {
                let proxy = state.store.settings()?.proxy;
                installations::check_updates(&state.repo, &state.store, &ids, &proxy, &context)
            })
            .map_err(js_error)?;
        to_json(operation)
    }

    /// Queues an update for one installation and returns its operation ID.
    pub fn apply_update(
        id: String,
        conflict_policy: String,
    ) -> std::result::Result<JsonValue, JsError> {
        let policy = UpdateConflictPolicy::from_request(&conflict_policy).map_err(js_error)?;
        let state = app_state().map_err(js_error)?;
        let operation = operations()
            .enqueue(OperationKind::ApplyUpdate, move |context| {
                let proxy = state.store.settings()?.proxy;
                installations::apply_update(
                    &state.repo,
                    &state.store,
                    &id,
                    policy,
                    &proxy,
                    &context,
                )
            })
            .map_err(js_error)?;
        to_json(operation)
    }
}

fn app_state() -> Result<Arc<AppState>> {
    APP_STATE
        .get_or_init(|| {
            let repo = CentralRepo::default_location()?;
            let store = Store::open(&repo)?;
            Ok(Arc::new(AppState {
                repo,
                store: Arc::new(store),
                market: market::MarketCache::new(),
                preview: preview::PreviewCache::new(),
            }))
        })
        .clone()
}

fn operations() -> &'static OperationQueue {
    OPERATIONS.get_or_init(OperationQueue::new)
}

fn parse_category(value: &str) -> Result<Category> {
    match value {
        "marketplace" => Ok(Category::Marketplace),
        "local" => Ok(Category::Local),
        _ => Err(AppError::invalid_input("unknown installation category")),
    }
}

fn parse_id_list(value: JsonValue) -> Result<Vec<String>> {
    serde_json::from_value(value)
        .map_err(|error| AppError::invalid_input(format!("invalid id list: {error}")))
}

async fn run_json<T>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> std::result::Result<JsonValue, JsError>
where
    T: Serialize + Send + 'static,
{
    let value = blocking(work)
        .await
        .map_err(|error| js_error(AppError::internal(format!("blocking task failed: {error}"))))?
        .map_err(js_error)?;
    to_json(value)
}

fn to_json<T: Serialize>(value: T) -> std::result::Result<JsonValue, JsError> {
    serde_json::to_value(value)
        .map_err(AppError::from)
        .map_err(js_error)
}

fn js_error(error: AppError) -> JsError {
    let mut js = JsError::Error(error.message)
        .with("code", error.code.as_str())
        .with("retryable", error.retryable);
    if let Some(details) = error.details {
        js = js.with("details", details);
    }
    js
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;

    #[test]
    fn native_errors_keep_structured_fields() {
        let error = js_error(
            AppError::new(ErrorCode::Network, "offline")
                .retryable(true)
                .with_details(serde_json::json!({ "source": "skills.sh" })),
        );
        let JsError::Fields(fields) = error else {
            panic!("structured error should carry fields");
        };
        assert_eq!(fields.message, "offline");
        assert_eq!(fields.field("code"), Some(&JsonValue::from("NETWORK")));
        assert_eq!(fields.field("retryable"), Some(&JsonValue::from(true)));
        assert_eq!(
            fields.field("details"),
            Some(&serde_json::json!({ "source": "skills.sh" }))
        );
    }
}
