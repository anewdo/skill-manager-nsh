use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata::{SkillMetadata, parse_skill_md_content};
use crate::core::model::InstalledSkill;

pub(super) const MAX_ENTRIES: usize = 4096;
pub(super) const MAX_CANDIDATES: usize = 256;
pub(super) const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_ZIP_BYTES: u64 = 50 * 1024 * 1024;
pub(super) const MAX_SKILL_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSourceKind {
    Folder,
    Zip,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalOrigin {
    Folder { source_id: String, label: String },
    Zip { source_id: String, label: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalCandidate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub origin: LocalOrigin,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryIssue {
    pub path: String,
    pub error: AppError,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalSource {
    pub id: String,
    pub path: String,
    pub kind: LocalSourceKind,
    pub status: String,
    pub candidate_count: usize,
}

#[derive(Debug, Serialize)]
pub struct LocalDiscovery {
    pub candidates: Vec<LocalCandidate>,
    pub sources: Vec<LocalSource>,
    pub issues: Vec<DiscoveryIssue>,
}

#[derive(Clone)]
pub(super) struct PendingCandidate {
    pub(super) view: LocalCandidate,
    pub(super) root: PathBuf,
    pub(super) metadata: SkillMetadata,
    // 批次快照持有同一资源，替换或丢弃会话不会提前删除运行中的 ZIP。
    pub(super) _archive: Option<Arc<TempDir>>,
}

pub(super) struct SourceSession {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) kind: LocalSourceKind,
    pub(super) digest: Option<String>,
    pub(super) candidates: Vec<PendingCandidate>,
    pub(super) issues: Vec<DiscoveryIssue>,
    pub(super) failed: bool,
}

#[derive(Default)]
pub(super) struct Session {
    pub(super) sources: Vec<SourceSession>,
    pub(super) issues: Vec<DiscoveryIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Added,
    Existing,
    Failed,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalImportItem {
    pub candidate_id: String,
    pub name: String,
    pub path: String,
    pub status: ImportStatus,
    pub skill: Option<InstalledSkill>,
    pub error: Option<AppError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalImportReport {
    pub items: Vec<LocalImportItem>,
}

pub(super) fn read_metadata(root: &Path) -> Result<SkillMetadata> {
    let bytes = read_limited(&root.join("SKILL.md"), MAX_SKILL_MD_BYTES)?;
    let content = String::from_utf8(bytes)
        .map_err(|_| AppError::new(ErrorCode::InvalidSkill, "SKILL.md is not UTF-8"))?;
    parse_skill_md_content(&content, "SKILL.md")
}

pub(super) fn include_entry(entry: &walkdir::DirEntry) -> bool {
    entry.depth() == 0
        || (entry.file_name() != ".git"
            && entry
                .metadata()
                .map(|metadata| !is_link(&metadata))
                .unwrap_or(true))
}

pub(super) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_regular(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(AppError::new(
            ErrorCode::InvalidSkill,
            format!("file exceeds the {limit} byte limit"),
        ));
    }
    Ok(bytes)
}

pub(super) fn open_regular(path: &Path) -> Result<File> {
    check_ancestors(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    check_file_type(&metadata, path)?;
    if !metadata.is_file() {
        return Err(AppError::unsafe_path("expected a regular file"));
    }
    Ok(file)
}

pub(super) fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        check_file_type(&fs::symlink_metadata(ancestor)?, ancestor)?;
    }
    Ok(())
}

pub(super) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    reparse || metadata.file_type().is_symlink()
}

pub(super) fn check_file_type(metadata: &fs::Metadata, path: &Path) -> Result<()> {
    if is_link(metadata) || (!metadata.is_dir() && !metadata.is_file()) {
        return Err(AppError::unsafe_path(format!(
            "links and special files are not allowed: {}",
            display_path(path)
        )));
    }
    Ok(())
}

pub(super) fn source_id(path: &Path) -> String {
    let value = display_path(path);
    let value = if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    };
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub(super) fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
    }
}
