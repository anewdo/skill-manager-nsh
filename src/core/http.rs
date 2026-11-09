use std::time::Duration;

use reqwest::blocking::Client;

use super::error::{AppError, Result};
use super::model::{ProxyMode, ProxySettings};

pub fn build_client(proxy: &ProxySettings, timeout: Duration) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent(concat!("skills-manager-nsh/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout)
        .https_only(true);

    match proxy.mode {
        ProxyMode::System => {}
        ProxyMode::None => {
            builder = builder.no_proxy();
        }
        ProxyMode::Custom => {
            let proxy_url =
                custom_proxy_url(proxy)?.expect("custom proxy mode requires a validated proxy URL");
            let configured = reqwest::Proxy::all(proxy_url)
                .map_err(|error| AppError::invalid_input(error.to_string()))?;
            builder = builder.proxy(configured);
        }
    }

    builder.build().map_err(AppError::internal)
}

pub fn custom_proxy_url(proxy: &ProxySettings) -> Result<Option<&str>> {
    if proxy.mode != ProxyMode::Custom {
        return Ok(None);
    }
    let proxy_url = proxy
        .custom_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| AppError::invalid_input("custom proxy URL is required"))?;
    let parsed = reqwest::Url::parse(proxy_url)
        .map_err(|_| AppError::invalid_input("custom proxy URL is invalid"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::invalid_input(
            "custom proxy must be an HTTP(S) URL with a host",
        ));
    }
    Ok(Some(proxy_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_proxy_configuration() {
        for custom_url in [
            None,
            Some(""),
            Some("socks5://127.0.0.1:1080"),
            Some("not a URL"),
        ] {
            let settings = ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: custom_url.map(str::to_string),
            };
            assert!(build_client(&settings, Duration::from_secs(1)).is_err());
        }
    }

    #[test]
    fn accepts_all_supported_proxy_modes() {
        for settings in [
            ProxySettings::default(),
            ProxySettings {
                mode: ProxyMode::None,
                custom_url: None,
            },
            ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: Some("http://127.0.0.1:7897".to_string()),
            },
        ] {
            build_client(&settings, Duration::from_secs(1)).unwrap();
        }
    }
}
