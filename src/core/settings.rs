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
    use crate::core::model::{Language, ProxyMode, ProxySettings, Theme};

    fn patch(value: serde_json::Value) -> SettingsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn empty_patch_returns_the_current_settings() {
        let store = Store::in_memory().unwrap();
        assert_eq!(
            update(&store, patch(serde_json::json!({}))).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn partial_patches_merge_fields_and_persist() {
        let store = Store::in_memory().unwrap();

        let updated = update(
            &store,
            patch(serde_json::json!({
                "theme": "dark",
                "proxy": { "mode": "custom", "custom_url": "http://127.0.0.1:7897" }
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
        assert_eq!(
            updated.proxy,
            ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: Some("http://127.0.0.1:7897".to_string()),
            }
        );
        assert_eq!(store.settings().unwrap(), updated);
    }

    #[test]
    fn proxy_patch_keeps_the_previous_url_when_it_is_omitted() {
        let store = Store::in_memory().unwrap();
        update(
            &store,
            patch(serde_json::json!({
                "proxy": { "mode": "custom", "custom_url": "http://127.0.0.1:7897" }
            })),
        )
        .unwrap();

        let updated = update(
            &store,
            patch(serde_json::json!({ "proxy": { "mode": "system" } })),
        )
        .unwrap();
        assert_eq!(updated.proxy.mode, ProxyMode::System);
        assert_eq!(
            updated.proxy.custom_url.as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn custom_proxy_urls_are_persisted_with_a_scheme() {
        let store = Store::in_memory().unwrap();

        let updated = update(
            &store,
            patch(serde_json::json!({
                "proxy": { "mode": "custom", "custom_url": "127.0.0.1:7897" }
            })),
        )
        .unwrap();
        assert_eq!(
            updated.proxy.custom_url.as_deref(),
            Some("http://127.0.0.1:7897")
        );
        assert_eq!(
            store.settings().unwrap().proxy.custom_url.as_deref(),
            Some("http://127.0.0.1:7897")
        );
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

    #[test]
    fn unknown_enum_values_do_not_deserialize() {
        for value in [
            serde_json::json!({ "language": "fr" }),
            serde_json::json!({ "theme": "blue" }),
            serde_json::json!({ "proxy": { "mode": "proxy" } }),
        ] {
            assert!(serde_json::from_value::<SettingsPatch>(value).is_err());
        }
    }
}
