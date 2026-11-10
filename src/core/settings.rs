use super::error::Result;
use super::http;
use super::model::{Settings, SettingsPatch};
use super::store::Store;

pub fn update(store: &Store, patch: SettingsPatch) -> Result<Settings> {
    let mut updated = patch.apply_to(&store.settings()?);
    updated.proxy = http::normalize_proxy(&updated.proxy)?;
    store.save_settings(&updated)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;
    use crate::core::model::{Language, ProxyMode, Theme};

    fn patch(value: serde_json::Value) -> SettingsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn update_merges_persists_and_normalizes_the_proxy() {
        let store = Store::in_memory().unwrap();

        let updated = update(
            &store,
            patch(serde_json::json!({
                "theme": "dark",
                "proxy": { "mode": "custom", "custom_url": "127.0.0.1:7897" }
            })),
        )
        .unwrap();
        assert_eq!(updated.language, Language::ZhCn);
        assert_eq!(updated.theme, Theme::Dark);
        assert_eq!(updated.proxy.mode, ProxyMode::Custom);
        assert_eq!(
            updated.proxy.custom_url.as_deref(),
            Some("http://127.0.0.1:7897")
        );

        let updated = update(&store, patch(serde_json::json!({ "language": "en" }))).unwrap();
        assert_eq!(updated.language, Language::En);
        assert_eq!(updated.theme, Theme::Dark);
        assert_eq!(store.settings().unwrap(), updated);
    }

    #[test]
    fn invalid_proxy_rejects_the_whole_patch_without_writing() {
        let store = Store::in_memory().unwrap();
        let before = store.settings().unwrap();

        for custom_url in [None, Some(""), Some("socks5://127.0.0.1:1080")] {
            let error = update(
                &store,
                patch(serde_json::json!({
                    "language": "en",
                    "proxy": { "mode": "custom", "custom_url": custom_url }
                })),
            )
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidInput);
            assert_eq!(store.settings().unwrap(), before);
        }
    }
}
