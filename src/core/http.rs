use std::time::{Duration, Instant};

use reqwest::blocking::Client;

use super::error::{AppError, Result};
use super::model::{ConnectivityCheck, ProxyMode, ProxySettings};

const CONNECTIVITY_TARGETS: [(&str, &str); 2] = [
    ("github", "https://github.com/"),
    ("skills_sh", "https://skills.sh/"),
];

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

/// Validates the proxy settings and stores the custom URL with an explicit scheme.
pub fn normalize_proxy(proxy: &ProxySettings) -> Result<ProxySettings> {
    let Some(custom_url) = custom_proxy_url(proxy)? else {
        return Ok(proxy.clone());
    };
    Ok(ProxySettings {
        mode: proxy.mode,
        custom_url: Some(custom_url),
    })
}

/// Probes every download source through the configured proxy.
pub fn test_connectivity(proxy: &ProxySettings) -> Result<Vec<ConnectivityCheck>> {
    let client = build_client(proxy, Duration::from_secs(10))?;
    let mut checks = Vec::with_capacity(CONNECTIVITY_TARGETS.len());
    for (target, url) in CONNECTIVITY_TARGETS {
        let started = Instant::now();
        let check = match client.head(url).send() {
            Ok(response) => {
                let status = response.status();
                ConnectivityCheck {
                    target: target.to_string(),
                    ok: status.is_success(),
                    latency_ms: started.elapsed().as_millis() as u64,
                    error: (!status.is_success()).then(|| format!("HTTP {}", status.as_u16())),
                }
            }
            Err(error) => ConnectivityCheck {
                target: target.to_string(),
                ok: false,
                latency_ms: started.elapsed().as_millis() as u64,
                error: Some(error.to_string()),
            },
        };
        checks.push(check);
    }
    Ok(checks)
}

pub fn custom_proxy_url(proxy: &ProxySettings) -> Result<Option<String>> {
    if proxy.mode != ProxyMode::Custom {
        return Ok(None);
    }
    let proxy_url = proxy
        .custom_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| AppError::invalid_input("custom proxy URL is required"))?;
    let normalized = if proxy_url.contains("://") {
        proxy_url.to_string()
    } else {
        format!("http://{proxy_url}")
    };
    let parsed = reqwest::Url::parse(&normalized)
        .map_err(|_| AppError::invalid_input("custom proxy URL is invalid"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::invalid_input(
            "custom proxy must be an HTTP(S) URL with a host",
        ));
    }
    Ok(Some(normalized))
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
            ProxySettings {
                mode: ProxyMode::Custom,
                custom_url: Some("127.0.0.1:7897".to_string()),
            },
        ] {
            build_client(&settings, Duration::from_secs(1)).unwrap();
        }
    }

    #[test]
    fn normalizes_custom_proxy_urls_to_an_http_scheme() {
        let bare = ProxySettings {
            mode: ProxyMode::Custom,
            custom_url: Some(" 127.0.0.1:7897 ".to_string()),
        };
        assert_eq!(
            custom_proxy_url(&bare).unwrap().as_deref(),
            Some("http://127.0.0.1:7897")
        );

        let secure = ProxySettings {
            mode: ProxyMode::Custom,
            custom_url: Some("https://proxy.local:8443".to_string()),
        };
        assert_eq!(
            custom_proxy_url(&secure).unwrap().as_deref(),
            Some("https://proxy.local:8443")
        );
    }
}
