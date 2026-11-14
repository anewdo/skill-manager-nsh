use std::collections::HashSet;
use std::fs::{OpenOptions, create_dir_all};
use std::io::{Read, Write};
use std::path::Path;

use reqwest::Url;
use reqwest::blocking::{Client, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::common::{DownloadObserver, DownloadProgress, DownloadedSkill};
use super::skills_sh::marketplace_source;
use crate::core::archive::extract_zip;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::metadata::discover_single;
use crate::core::model::{ProxySettings, SkillSource};
use crate::core::skill_path::{PathBudget, safe_relative_path};

const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const TEMP_DIR_PREFIX: &str = "skills-manager-nsh-well-known-";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const SITE_HOSTS: &[&str] = &["skills.sh", "www.skills.sh"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct SiteRef {
    source_url: String,
    domain: String,
    skill_id: String,
}

struct DiscoveryIndex {
    url: Url,
    payload: Value,
}

pub(super) fn canonical_source(source: &SkillSource) -> Result<SkillSource> {
    let url = source
        .url
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("well-known source URL is required"))?;
    let site = parse_site_ref(url)?;
    let canonical = marketplace_source(&site.domain, &site.skill_id)?;
    if source.source_key != canonical.source_key {
        return Err(AppError::invalid_input(
            "well-known source key does not match its URL",
        ));
    }
    Ok(canonical)
}

pub fn download(
    source: &SkillSource,
    proxy: &ProxySettings,
    staging_dir: &Path,
    observer: &dyn DownloadObserver,
) -> Result<DownloadedSkill> {
    let url = source
        .url
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("well-known source URL is required"))?;
    let site = parse_site_ref(url)?;
    let client = http::build_client(proxy, REQUEST_TIMEOUT)?;
    let index = fetch_index(&client, &site.source_url, observer)?;
    let entry = index
        .payload
        .get("skills")
        .and_then(Value::as_array)
        .and_then(|skills| {
            skills.iter().find(|skill| {
                skill.get("name").and_then(Value::as_str) == Some(site.skill_id.as_str())
            })
        })
        .ok_or_else(|| {
            invalid_response(format!(
                "skill '{}' was not found at {}",
                site.skill_id, site.source_url
            ))
        })?;

    let temp_dir = tempfile::Builder::new()
        .prefix(TEMP_DIR_PREFIX)
        .tempdir_in(staging_dir)?;
    let skill_dir = temp_dir.path().join("skill");
    create_dir_all(&skill_dir)?;
    let digest = if index.payload.get("$schema").is_some() {
        download_v2_entry(&client, &index, entry, &skill_dir, observer)?
    } else {
        download_v1_entry(&client, &index, entry, &site.skill_id, &skill_dir, observer)?
    };
    let candidate = discover_single(&skill_dir)?;
    DownloadedSkill::from_candidate(temp_dir, &skill_dir, candidate, None, digest)
}

fn parse_site_ref(input: &str) -> Result<SiteRef> {
    let parsed = Url::parse(input.trim())
        .map_err(|_| AppError::invalid_input("invalid skills.sh source URL"))?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| SITE_HOSTS.contains(&host))
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::invalid_input(
            "expected an HTTPS skills.sh /site/<domain>/<skill> URL",
        ));
    }
    let path = parsed.path().strip_prefix("/site/").ok_or_else(|| {
        AppError::invalid_input("expected a skills.sh /site/<domain>/<skill> URL")
    })?;
    let (domain, skill_id) = path.split_once('/').ok_or_else(|| {
        AppError::invalid_input("expected a skills.sh /site/<domain>/<skill> URL")
    })?;
    marketplace_source(domain, skill_id)?;
    let domain = domain.to_ascii_lowercase();
    Ok(SiteRef {
        source_url: format!("https://{domain}/"),
        domain,
        skill_id: skill_id.to_owned(),
    })
}

fn fetch_index(
    client: &Client,
    source_url: &str,
    observer: &dyn DownloadObserver,
) -> Result<DiscoveryIndex> {
    for path in [".well-known/agent-skills", ".well-known/skills"] {
        observer.check_cancelled()?;
        let base = Url::parse(source_url)
            .map_err(|_| AppError::invalid_input("invalid well-known source URL"))?;
        let url = base
            .join(&format!("{path}/index.json"))
            .map_err(|_| invalid_response("cannot construct the skills index URL"))?;
        let Ok(response) = client.get(url.clone()).send() else {
            continue;
        };
        let Ok(response) = response.error_for_status() else {
            continue;
        };
        let bytes = match read_response(
            response,
            &url,
            MAX_DOWNLOAD_BYTES,
            observer,
            ReadProgress::None,
        ) {
            Ok(bytes) => bytes,
            Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
            Err(_) => continue,
        };
        let Ok(payload) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if payload.get("skills").and_then(Value::as_array).is_some() {
            return Ok(DiscoveryIndex { url, payload });
        }
    }
    Err(AppError::new(
        ErrorCode::Network,
        format!("no supported skills index found at {source_url}"),
    ))
}

fn download_v1_entry(
    client: &Client,
    index: &DiscoveryIndex,
    entry: &Value,
    skill_id: &str,
    skill_dir: &Path,
    observer: &dyn DownloadObserver,
) -> Result<Option<String>> {
    let files = entry
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("skills index entry is missing files"))?;
    if files.len() > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_skill("skill contains too many files"));
    }
    let mut paths = PathBudget::default();
    let mut seen = HashSet::new();
    for file in files {
        let file = file
            .as_str()
            .ok_or_else(|| invalid_response("skills index contains an invalid file path"))?;
        paths.add(file)?;
        if !seen.insert(file) {
            return Err(invalid_skill("skill contains duplicate files"));
        }
    }

    let base = index
        .url
        .join("./")
        .and_then(|url| url.join(&format!("{skill_id}/")))
        .map_err(|_| invalid_response("skills index contains an invalid skill name"))?;
    let mut remaining = MAX_DOWNLOAD_BYTES;
    let total = files.len() as u64;
    let mut received = 0u64;
    observer.report(DownloadProgress {
        current: 0,
        total: Some(total),
        bytes: None,
        bytes_total: None,
        detail: None,
    })?;
    for (position, file) in files.iter().enumerate() {
        observer.check_cancelled()?;
        let file = file.as_str().expect("file paths were preflighted");
        let destination = skill_dir.join(safe_relative_path(file)?);
        let url = base
            .join(file)
            .map_err(|_| invalid_response("skills index contains an invalid file URL"))?;
        let bytes = get_bytes_limited(client, &url, remaining, observer, ReadProgress::None)?;
        remaining -= bytes.len() as u64;
        received += bytes.len() as u64;
        if let Some(parent) = destination.parent() {
            create_dir_all(parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        output.write_all(&bytes)?;
        observer.report(DownloadProgress {
            current: position as u64 + 1,
            total: Some(total),
            bytes: Some(received),
            bytes_total: None,
            detail: None,
        })?;
    }
    if !skill_dir.join("SKILL.md").is_file() {
        return Err(invalid_skill("skills index entry is missing SKILL.md"));
    }
    Ok(None)
}

fn download_v2_entry(
    client: &Client,
    index: &DiscoveryIndex,
    entry: &Value,
    skill_dir: &Path,
    observer: &dyn DownloadObserver,
) -> Result<Option<String>> {
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_response("skills index entry is missing type"))?;
    let artifact = entry
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_response("skills index entry is missing url"))?;
    let url = index
        .url
        .join(artifact)
        .map_err(|_| invalid_response("skills index entry has an invalid url"))?;
    let bytes = get_bytes_limited(
        client,
        &url,
        MAX_DOWNLOAD_BYTES,
        observer,
        ReadProgress::Bytes,
    )?;
    let digest = entry.get("digest").and_then(Value::as_str);
    if let Some(digest) = digest {
        verify_digest(&bytes, digest)?;
    }

    match kind {
        "skill-md" => std::fs::write(skill_dir.join("SKILL.md"), bytes)?,
        "archive" => extract_zip(&bytes, skill_dir, observer)?,
        other => {
            return Err(invalid_response(format!(
                "unsupported skills index entry type: {other}"
            )));
        }
    }
    if !skill_dir.join("SKILL.md").is_file() {
        return Err(invalid_skill("downloaded skill is missing SKILL.md"));
    }
    Ok(digest.map(str::to_owned))
}

/// Whether a response body reports transferred bytes while it streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadProgress {
    None,
    Bytes,
}

fn get_bytes_limited(
    client: &Client,
    url: &Url,
    limit: u64,
    observer: &dyn DownloadObserver,
    progress: ReadProgress,
) -> Result<Vec<u8>> {
    observer.check_cancelled()?;
    let response = client
        .get(url.clone())
        .send()
        .map_err(|error| network_error(url, error))?
        .error_for_status()
        .map_err(|error| network_error(url, error))?;
    read_response(response, url, limit, observer, progress)
}

fn read_response(
    response: Response,
    url: &Url,
    limit: u64,
    observer: &dyn DownloadObserver,
    progress: ReadProgress,
) -> Result<Vec<u8>> {
    let total = response.content_length();
    if total.is_some_and(|size| size > limit) {
        return Err(invalid_skill(format!(
            "download exceeds the {limit} byte limit"
        )));
    }
    if progress == ReadProgress::Bytes {
        observer.report(DownloadProgress {
            current: 0,
            total,
            bytes: Some(0),
            bytes_total: total,
            detail: None,
        })?;
    }
    let mut bytes = Vec::new();
    let mut reader = response.take(limit + 1);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        observer.check_cancelled()?;
        let read = reader.read(&mut buffer).map_err(|error| {
            AppError::new(
                ErrorCode::Network,
                format!("failed to read download from {url}: {error}"),
            )
            .retryable(true)
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if progress == ReadProgress::Bytes {
            observer.report(DownloadProgress {
                current: bytes.len() as u64,
                total,
                bytes: Some(bytes.len() as u64),
                bytes_total: total,
                detail: None,
            })?;
        }
    }
    if bytes.len() as u64 > limit {
        return Err(invalid_skill(format!(
            "download exceeds the {limit} byte limit"
        )));
    }
    Ok(bytes)
}

fn verify_digest(bytes: &[u8], expected: &str) -> Result<()> {
    let Some(expected) = expected.strip_prefix("sha256:") else {
        return Err(invalid_response("unsupported skills index digest"));
    };
    if expected.len() != 64
        || !expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(invalid_response("invalid skills index SHA-256 digest"));
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        return Err(invalid_skill(
            "downloaded skill failed its SHA-256 digest check",
        ));
    }
    Ok(())
}

fn network_error(url: &Url, error: reqwest::Error) -> AppError {
    let retryable = error.is_timeout()
        || error.is_connect()
        || error.status().is_some_and(|status| {
            status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error()
        });
    AppError::new(
        ErrorCode::Network,
        format!("well-known download failed for {url}: {error}"),
    )
    .retryable(retryable)
}

fn invalid_response(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::Network, message)
}

fn invalid_skill(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidSkill, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_site_refs_and_rejects_other_urls() {
        let parsed = parse_site_ref("https://www.skills.sh/site/Example.COM/ui-radar").unwrap();
        assert_eq!(parsed.source_url, "https://example.com/");
        assert_eq!(parsed.domain, "example.com");
        assert_eq!(parsed.skill_id, "ui-radar");
        for url in [
            "http://skills.sh/site/example.com/ui",
            "https://example.com/site/example.com/ui",
            "https://skills.sh/other/example.com/ui",
            "https://skills.sh/site/example.com/../ui",
            "https://user@skills.sh/site/example.com/ui",
        ] {
            assert!(parse_site_ref(url).is_err(), "{url}");
        }
    }

    #[test]
    fn verifies_sha256_digests() {
        let bytes = b"skill".to_vec();
        assert!(verify_digest(&bytes, &format!("sha256:{}", "0".repeat(64))).is_err());
        assert!(verify_digest(&bytes, "md5:abcd").is_err());
        assert!(verify_digest(&bytes, &format!("sha256:{:x}", Sha256::digest(&bytes))).is_ok());
    }
}
