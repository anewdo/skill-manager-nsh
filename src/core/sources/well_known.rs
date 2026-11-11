use std::collections::HashSet;
use std::fs::{OpenOptions, create_dir_all};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path};

use reqwest::Url;
use reqwest::blocking::{Client, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::skills_sh::marketplace_source;
use super::{DownloadObserver, DownloadProgress, DownloadedSkill};
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::metadata::discover_single;
use crate::core::model::{ProxySettings, SkillSource};

const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_PATH_BYTES: usize = 512;
const MAX_PATH_DEPTH: usize = 16;
const MAX_TOTAL_PATH_BYTES: usize = 64 * 1024;
const MAX_TOTAL_COMPONENTS: usize = 16 * 1024;
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

fn extract_zip(bytes: &[u8], destination: &Path, observer: &dyn DownloadObserver) -> Result<()> {
    extract_zip_with_limits(
        bytes,
        destination,
        MAX_EXTRACTED_BYTES,
        MAX_ARCHIVE_ENTRIES,
        observer,
    )
}

fn extract_zip_with_limits(
    bytes: &[u8],
    destination: &Path,
    max_bytes: u64,
    max_entries: usize,
    observer: &dyn DownloadObserver,
) -> Result<()> {
    reject_duplicate_archive_entries(bytes)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() > max_entries {
        return Err(invalid_skill("archive contains too many entries"));
    }

    let mut declared_bytes = 0u64;
    let mut paths = PathBudget::default();
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        observer.check_cancelled()?;
        let entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().trim_end_matches('/');
        paths.add(name)?;
        if !seen.insert(name.to_owned()) {
            return Err(invalid_skill("archive contains duplicate paths"));
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                return Err(AppError::unsafe_path(
                    "archive contains a link or special file",
                ));
            }
        }
        declared_bytes = declared_bytes
            .checked_add(entry.size())
            .ok_or_else(|| invalid_skill("archive size overflow"))?;
        if declared_bytes > max_bytes {
            return Err(invalid_skill("archive exceeds extraction size limit"));
        }
    }

    let mut remaining = max_bytes;
    for index in 0..archive.len() {
        observer.check_cancelled()?;
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let relative = safe_relative_path(entry.name().trim_end_matches('/'))?;
        let target = destination.join(relative);
        if entry.is_dir() {
            create_dir_all(target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let written = copy_with_cancel(&mut entry, &mut file, remaining, observer)?;
        remaining -= written;
        let mut extra = [0u8; 1];
        if entry.read(&mut extra)? != 0 {
            return Err(invalid_skill("archive exceeds extraction size limit"));
        }
    }
    Ok(())
}

/// Copies one archive entry block by block, so cancelling does not wait for a
/// whole file to be written.
fn copy_with_cancel(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
    observer: &dyn DownloadObserver,
) -> Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = limit;
    let mut written = 0u64;
    while remaining > 0 {
        observer.check_cancelled()?;
        let want = buffer.len().min(remaining as usize);
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        remaining -= read as u64;
        written += read as u64;
    }
    Ok(written)
}

fn reject_duplicate_archive_entries(bytes: &[u8]) -> Result<()> {
    const EOCD_LEN: usize = 22;
    const CENTRAL_HEADER_LEN: usize = 46;
    let Some(eocd) = (0..=bytes.len().saturating_sub(EOCD_LEN))
        .rev()
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(b"PK\x05\x06")
                && u16_at(bytes, *offset + 20).is_some_and(|comment_len| {
                    *offset + EOCD_LEN + comment_len as usize == bytes.len()
                })
        })
    else {
        return Ok(());
    };
    let entries = u16_at(bytes, eocd + 10)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?;
    let central_size = u32_at(bytes, eocd + 12)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?
        as usize;
    let central_offset = u32_at(bytes, eocd + 16)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?
        as usize;
    if u16_at(bytes, eocd + 4) != Some(0)
        || u16_at(bytes, eocd + 6) != Some(0)
        || u16_at(bytes, eocd + 8) != Some(entries)
        || entries == u16::MAX
        || central_size == u32::MAX as usize
        || central_offset == u32::MAX as usize
    {
        return Err(invalid_skill(
            "multi-disk and ZIP64 skill archives are not supported",
        ));
    }
    let central_end = central_offset
        .checked_add(central_size)
        .filter(|end| *end <= eocd)
        .ok_or_else(|| invalid_skill("archive directory is out of bounds"))?;
    let mut cursor = central_offset;
    let mut seen = HashSet::new();
    for _ in 0..entries {
        if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") {
            return Err(invalid_skill("archive has an invalid central directory"));
        }
        let name_len = u16_at(bytes, cursor + 28)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let extra_len = u16_at(bytes, cursor + 30)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let comment_len = u16_at(bytes, cursor + 32)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let name_start = cursor + CENTRAL_HEADER_LEN;
        let name_end = name_start
            .checked_add(name_len)
            .filter(|end| *end <= central_end)
            .ok_or_else(|| invalid_skill("archive entry name is out of bounds"))?;
        let mut name = &bytes[name_start..name_end];
        while name.last() == Some(&b'/') {
            name = &name[..name.len() - 1];
        }
        if !seen.insert(name.to_vec()) {
            return Err(invalid_skill("archive contains duplicate paths"));
        }
        cursor = name_end
            .checked_add(extra_len)
            .and_then(|end| end.checked_add(comment_len))
            .filter(|end| *end <= central_end)
            .ok_or_else(|| invalid_skill("archive entry is out of bounds"))?;
    }
    Ok(())
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn safe_relative_path(path: &str) -> Result<&Path> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path.split('/').count() > MAX_PATH_DEPTH
        || path.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || part.ends_with(['.', ' '])
                || is_windows_device(part)
        })
    {
        return Err(AppError::unsafe_path("invalid skill file path"));
    }
    let path = Path::new(path);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::unsafe_path("invalid skill file path"));
    }
    Ok(path)
}

fn is_windows_device(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        || (stem.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
        }) && matches!(
            stem.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        ))
}

#[derive(Default)]
struct PathBudget {
    bytes: usize,
    components: usize,
}

impl PathBudget {
    fn add(&mut self, path: &str) -> Result<()> {
        safe_relative_path(path)?;
        self.bytes += path.len();
        self.components += path.split('/').count();
        if self.bytes > MAX_TOTAL_PATH_BYTES || self.components > MAX_TOTAL_COMPONENTS {
            return Err(invalid_skill("skill exceeds total path budget"));
        }
        Ok(())
    }
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

fn zip_error(error: zip::result::ZipError) -> AppError {
    invalid_skill(format!("invalid skill archive: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sources::SilentObserver;
    use zip::write::SimpleFileOptions;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

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
    fn extracts_valid_bundle_at_exact_byte_limit() {
        let bytes = archive(&[("SKILL.md", b"skill"), ("references/guide.md", b"guide")]);
        let temp = tempfile::tempdir().unwrap();
        extract_zip_with_limits(&bytes, temp.path(), 10, 2, &SilentObserver).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("references/guide.md")).unwrap(),
            b"guide"
        );
    }

    #[test]
    fn rejects_traversal_links_and_duplicates_before_writing() {
        for name in [
            "../escaped",
            "/escaped",
            "nested/../../escaped",
            "C:/escaped",
            "nested\\escaped",
            "./escaped",
        ] {
            let bytes = archive(&[("SKILL.md", b"skill"), (name, b"bad")]);
            let temp = tempfile::tempdir().unwrap();
            assert!(
                extract_zip(&bytes, temp.path(), &SilentObserver).is_err(),
                "{name}"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }

        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink("linked", "../escaped", SimpleFileOptions::default())
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);

        let mut bytes = archive(&[("SKILL.md", b"one"), ("OTHER.md", b"two")]);
        for index in 0..=bytes.len() - b"OTHER.md".len() {
            if &bytes[index..index + b"OTHER.md".len()] == b"OTHER.md" {
                bytes[index..index + b"OTHER.md".len()].copy_from_slice(b"SKILL.md");
            }
        }
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn rejects_expansion_entry_count_and_path_budgets() {
        let bytes = archive(&[
            ("SKILL.md", &[b'a'; 4096]),
            ("references/guide.md", &[b'b'; 4096]),
        ]);
        for (max_bytes, max_entries) in [(8191, 2), (8192, 1)] {
            let temp = tempfile::tempdir().unwrap();
            assert!(
                extract_zip_with_limits(
                    &bytes,
                    temp.path(),
                    max_bytes,
                    max_entries,
                    &SilentObserver
                )
                .is_err()
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }

        let long = "a".repeat(MAX_PATH_BYTES + 1);
        let deep = format!("{}SKILL.md", "a/".repeat(MAX_PATH_DEPTH));
        for name in [long, deep] {
            let bytes = archive(&[("SKILL.md", b"skill"), (&name, b"bad")]);
            let temp = tempfile::tempdir().unwrap();
            assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn refuses_existing_files_and_file_directory_collisions() {
        let bytes = archive(&[("SKILL.md", b"second")]);
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("SKILL.md"), b"first").unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("SKILL.md")).unwrap(),
            b"first"
        );

        for entries in [
            vec![("reference", b"a".as_slice()), ("reference/guide.md", b"b")],
            vec![("reference/guide.md", b"a".as_slice()), ("reference", b"b")],
        ] {
            let temp = tempfile::tempdir().unwrap();
            assert!(extract_zip(&archive(&entries), temp.path(), &SilentObserver).is_err());
        }
    }

    #[test]
    fn verifies_sha256_digests() {
        let bytes = archive(&[("SKILL.md", b"skill")]);
        assert!(verify_digest(&bytes, &format!("sha256:{}", "0".repeat(64))).is_err());
        assert!(verify_digest(&bytes, "md5:abcd").is_err());
        assert!(verify_digest(&bytes, &format!("sha256:{:x}", Sha256::digest(&bytes))).is_ok());
    }

    #[test]
    fn rejects_windows_special_names_but_preserves_unicode() {
        for name in [
            "NUL.md",
            "con",
            "COM1.txt",
            "LPT9",
            "COM¹.md",
            "file.",
            "file ",
            "nested/aux.txt",
            "nul\0.txt",
            "a\nb",
        ] {
            assert!(safe_relative_path(name).is_err(), "{name:?}");
        }
        assert_eq!(
            safe_relative_path("参考/é.md").unwrap(),
            Path::new("参考/é.md")
        );
    }

    #[test]
    fn understated_entry_size_cannot_write_past_the_limit() {
        let mut bytes = archive(&[("SKILL.md", &vec![b'a'; 1_000_000])]);
        let local = bytes.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
        bytes[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip_with_limits(&bytes, temp.path(), 100, 10, &SilentObserver).is_err());
        let written = std::fs::metadata(temp.path().join("SKILL.md"))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        assert!(written <= 100);
    }
}
