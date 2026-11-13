use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use walkdir::WalkDir;

use super::central_repo::CentralRepo;
use super::error::{AppError, ErrorCode, Result};
use super::library::commit_prepared;
use super::metadata::{SkillCandidate, SkillMetadata, parse_skill_md_content, relative_path};
use super::model::{InstalledSkill, OperationProgress};
use super::operations::OperationContext;
use super::sources::{DownloadedSkill, SilentObserver, well_known};
use super::store::Store;

const MAX_ENTRIES: usize = 4096;
const MAX_CANDIDATES: usize = 256;
const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;
const MAX_ZIP_BYTES: u64 = 50 * 1024 * 1024;
const MAX_SKILL_BYTES: u64 = 100 * 1024 * 1024;

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
struct PendingCandidate {
    view: LocalCandidate,
    root: PathBuf,
    metadata: SkillMetadata,
    // 批次快照持有同一资源，替换或丢弃会话不会提前删除运行中的 ZIP。
    _archive: Option<Arc<TempDir>>,
}

struct SourceSession {
    id: String,
    path: PathBuf,
    kind: LocalSourceKind,
    digest: Option<String>,
    candidates: Vec<PendingCandidate>,
    issues: Vec<DiscoveryIssue>,
    failed: bool,
}

#[derive(Default)]
struct Session {
    sources: Vec<SourceSession>,
    issues: Vec<DiscoveryIssue>,
}

#[derive(Clone, Default)]
pub struct LocalImports(Arc<Mutex<Session>>);

pub struct LocalImportBatch {
    session: LocalImports,
    candidates: Vec<PendingCandidate>,
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

impl LocalImports {
    pub fn snapshot(&self) -> LocalDiscovery {
        let session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snapshot(&session)
    }

    pub fn add_paths(&self, repo: &CentralRepo, inputs: &[String]) -> Result<LocalDiscovery> {
        if inputs.is_empty() {
            return Err(AppError::invalid_input(
                "at least one local path is required",
            ));
        }
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session.issues.clear();
        for input in inputs {
            let result = input_path(input).and_then(|path| {
                check_ancestors(&path)?;
                let path = path.canonicalize()?;
                let metadata = fs::symlink_metadata(&path)?;
                let kind = if metadata.is_dir() {
                    LocalSourceKind::Folder
                } else if metadata.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                {
                    LocalSourceKind::Zip
                } else {
                    return Err(AppError::invalid_input("choose a folder or a ZIP archive"));
                };
                let id = source_id(&path);
                let existing = session.sources.iter().position(|source| source.id == id);
                let old_digest =
                    existing.and_then(|index| session.sources[index].digest.as_deref());
                if let Some(source) = discover_source(repo, &path, kind, old_digest) {
                    for previous in &mut session.sources {
                        previous.candidates.retain(|old| {
                            !source
                                .candidates
                                .iter()
                                .any(|new| old.root == new.root && old.view.id != new.view.id)
                        });
                    }
                    if let Some(index) = existing {
                        session.sources[index] = source;
                    } else {
                        session.sources.push(source);
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                session.issues.push(DiscoveryIssue {
                    path: input.clone(),
                    error,
                });
            }
        }
        Ok(snapshot(&session))
    }

    pub fn rescan(&self, repo: &CentralRepo) -> LocalDiscovery {
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let folders: Vec<_> = session
            .sources
            .iter()
            .filter(|source| source.kind == LocalSourceKind::Folder)
            .map(|source| source.path.clone())
            .collect();
        session.sources.clear();
        session.issues.clear();
        for path in folders {
            if let Some(source) = discover_source(repo, &path, LocalSourceKind::Folder, None) {
                session.sources.push(source);
            }
        }
        snapshot(&session)
    }

    pub fn discard(&self) {
        let old = {
            let mut session = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *session)
        };
        drop(old);
    }

    pub fn prepare(&self, ids: &[String]) -> Result<LocalImportBatch> {
        if ids.is_empty() {
            return Err(AppError::invalid_input("select at least one candidate"));
        }
        let session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let candidate = session
                .sources
                .iter()
                .flat_map(|source| &source.candidates)
                .find(|candidate| &candidate.view.id == id)
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::Conflict,
                        "candidate expired; rescan and select again",
                    )
                })?;
            candidates.push(candidate.clone());
        }
        Ok(LocalImportBatch {
            session: self.clone(),
            candidates,
        })
    }

    fn forget(&self, id: &str) {
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for source in &mut session.sources {
            source
                .candidates
                .retain(|candidate| candidate.view.id != id);
        }
        session.sources.retain(|source| {
            source.kind == LocalSourceKind::Folder || !source.candidates.is_empty()
        });
    }
}

impl LocalImportBatch {
    pub fn run(
        self,
        repo: &CentralRepo,
        store: &Store,
        context: &OperationContext,
    ) -> Result<LocalImportReport> {
        let mut report = LocalImportReport { items: Vec::new() };
        let total = self.candidates.len() as u64;
        for (index, candidate) in self.candidates.into_iter().enumerate() {
            let progress = |phase, cancellable| OperationProgress {
                phase: String::from(phase),
                current: Some(index as u64 + 1),
                total: Some(total),
                bytes: None,
                bytes_total: None,
                detail: Some(candidate.view.name.clone()),
                cancellable,
            };
            let result = context.progress(progress("copying", true)).and_then(|_| {
                let prepared = stage_candidate(repo, &candidate, context)?;
                context.progress(progress("validating", true))?;
                commit_prepared(
                    repo,
                    store,
                    &prepared,
                    None,
                    context,
                    progress("installing", true),
                )
            });
            let (status, skill, error) = match result {
                Ok(outcome) => {
                    self.session.forget(&candidate.view.id);
                    (
                        if outcome.reused {
                            ImportStatus::Existing
                        } else {
                            ImportStatus::Added
                        },
                        Some(outcome.skill),
                        None,
                    )
                }
                Err(error) => (
                    if error.code == ErrorCode::Cancelled {
                        ImportStatus::Cancelled
                    } else {
                        ImportStatus::Failed
                    },
                    None,
                    Some(error),
                ),
            };
            report.items.push(LocalImportItem {
                candidate_id: candidate.view.id.clone(),
                name: candidate.view.name.clone(),
                path: candidate.view.path.clone(),
                status,
                skill,
                error,
            });
        }
        Ok(report)
    }
}

fn snapshot(session: &Session) -> LocalDiscovery {
    let mut seen = BTreeSet::new();
    LocalDiscovery {
        candidates: session
            .sources
            .iter()
            .flat_map(|source| &source.candidates)
            .filter(|candidate| seen.insert(candidate.view.id.clone()))
            .map(|candidate| candidate.view.clone())
            .collect(),
        sources: session
            .sources
            .iter()
            .map(|source| LocalSource {
                id: source.id.clone(),
                path: display_path(&source.path),
                kind: source.kind,
                status: if source.failed {
                    "failed"
                } else if !source.candidates.is_empty() {
                    "ready"
                } else if !source.issues.is_empty() {
                    "invalid"
                } else {
                    "empty"
                }
                .to_string(),
                candidate_count: source.candidates.len(),
            })
            .collect(),
        issues: session
            .issues
            .iter()
            .chain(session.sources.iter().flat_map(|source| &source.issues))
            .cloned()
            .collect(),
    }
}

fn discover_source(
    repo: &CentralRepo,
    path: &Path,
    kind: LocalSourceKind,
    old_digest: Option<&str>,
) -> Option<SourceSession> {
    let mut source = SourceSession {
        id: source_id(path),
        path: path.to_path_buf(),
        kind,
        digest: None,
        candidates: Vec::new(),
        issues: Vec::new(),
        failed: false,
    };
    let result = (|| {
        check_ancestors(path)?;
        let (root, archive, digest) = if kind == LocalSourceKind::Zip {
            let bytes = read_limited(path, MAX_ZIP_BYTES)?;
            let digest = hex::encode(Sha256::digest(&bytes));
            if old_digest == Some(digest.as_str()) {
                return Ok(false);
            }
            let archive = Arc::new(
                tempfile::Builder::new()
                    .prefix("local-zip-")
                    .tempdir_in(repo.staging_dir())?,
            );
            well_known::extract_zip(&bytes, archive.path(), &SilentObserver)?;
            (archive.path().to_path_buf(), Some(archive), Some(digest))
        } else {
            (path.to_path_buf(), None, None)
        };
        let ScanResult { candidates, issues } = scan(&root)?;
        source.issues = issues
            .into_iter()
            .map(|(relative, error)| DiscoveryIssue {
                path: candidate_path(path, kind, &relative),
                error,
            })
            .collect();
        let key = source_id(path);
        let label = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        for candidate in candidates {
            let identity = if kind == LocalSourceKind::Folder {
                source_id(&candidate.root)
            } else {
                format!(
                    "{key}:{}:{}",
                    digest.as_deref().unwrap(),
                    candidate.relative_root
                )
            };
            let id = hex::encode(Sha256::digest(
                format!(
                    "{identity}\0{}\0{}",
                    candidate.metadata.name, candidate.metadata.description
                )
                .as_bytes(),
            ));
            let origin = if kind == LocalSourceKind::Folder {
                LocalOrigin::Folder {
                    source_id: key.clone(),
                    label: label.clone(),
                }
            } else {
                LocalOrigin::Zip {
                    source_id: key.clone(),
                    label: label.clone(),
                }
            };
            source.candidates.push(PendingCandidate {
                view: LocalCandidate {
                    id,
                    name: candidate.metadata.name.clone(),
                    description: candidate.metadata.description.clone(),
                    path: candidate_path(path, kind, &candidate.relative_root),
                    origin,
                },
                root: candidate.root,
                metadata: candidate.metadata,
                _archive: archive.clone(),
            });
        }
        source.digest = digest;
        Ok(true)
    })();
    match result {
        Ok(false) => return None,
        Ok(true) => {}
        Err(error) => {
            source.failed = true;
            source.candidates.clear();
            source.issues = vec![DiscoveryIssue {
                path: display_path(path),
                error,
            }];
        }
    }
    Some(source)
}

struct ScanResult {
    candidates: Vec<SkillCandidate>,
    issues: Vec<(String, AppError)>,
}

fn scan(root: &Path) -> Result<ScanResult> {
    check_ancestors(root)?;
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(AppError::unsafe_path(
            "discovery root must be a real directory",
        ));
    }
    let mut candidates = Vec::new();
    let mut issues = Vec::new();
    let mut count = 0;
    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(include_entry);
    for entry in walker {
        let entry = entry.map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        check_file_type(&fs::symlink_metadata(entry.path())?, entry.path())?;
        if entry.depth() == 0 {
            continue;
        }
        count += 1;
        if count > MAX_ENTRIES {
            return Err(AppError::invalid_input(
                "source contains more than 4096 entries",
            ));
        }
        let relative = relative_path(root, entry.path())?;
        well_known::safe_relative_path(&relative)?;
        if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
            if candidates.len() + issues.len() >= MAX_CANDIDATES {
                return Err(AppError::invalid_input(
                    "source contains more than 256 SKILL.md candidates",
                ));
            }
            let skill_root = entry.path().parent().unwrap();
            let relative_root = relative_path(root, skill_root)?;
            match read_metadata(skill_root) {
                Ok(metadata) => candidates.push(SkillCandidate {
                    root: skill_root.to_path_buf(),
                    relative_root,
                    metadata,
                }),
                Err(error) if error.code == ErrorCode::InvalidSkill => {
                    issues.push((relative_root, error))
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(ScanResult { candidates, issues })
}

fn stage_candidate(
    repo: &CentralRepo,
    candidate: &PendingCandidate,
    context: &OperationContext,
) -> Result<DownloadedSkill> {
    check_ancestors(&candidate.root)?;
    if !fs::symlink_metadata(&candidate.root)?.is_dir() {
        return Err(AppError::unsafe_path(
            "candidate root must be a real directory",
        ));
    }
    if repo
        .staging_dir()
        .canonicalize()?
        .starts_with(candidate.root.canonicalize()?)
    {
        return Err(AppError::unsafe_path(
            "candidate contains the application's staging directory",
        ));
    }
    let temp = tempfile::Builder::new()
        .prefix("local-import-")
        .tempdir_in(repo.staging_dir())?;
    let root = temp.path().join("skill");
    fs::create_dir(&root)?;
    let walker = WalkDir::new(&candidate.root)
        .follow_links(false)
        .into_iter()
        .filter_entry(include_entry);
    let mut remaining = MAX_SKILL_BYTES;
    for (count, entry) in walker.enumerate() {
        context.check_cancelled()?;
        if count > MAX_ENTRIES {
            return Err(AppError::invalid_input(
                "skill contains more than 4096 entries",
            ));
        }
        let entry = entry.map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        check_ancestors(entry.path())?;
        let metadata = fs::symlink_metadata(entry.path())?;
        check_file_type(&metadata, entry.path())?;
        if entry.depth() == 0 {
            continue;
        }
        let relative = relative_path(&candidate.root, entry.path())?;
        let target = root.join(well_known::safe_relative_path(&relative)?);
        if metadata.is_dir() {
            fs::create_dir_all(target)?;
        } else {
            let mut input = open_regular(entry.path())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                context.check_cancelled()?;
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                if read as u64 > remaining {
                    return Err(AppError::invalid_input(
                        "skill exceeds the 100 MiB size limit",
                    ));
                }
                output.write_all(&buffer[..read])?;
                remaining -= read as u64;
            }
        }
    }
    let metadata = read_metadata(&root)?;
    if metadata != candidate.metadata {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "candidate metadata changed; rescan and select again",
        )
        .with_details(serde_json::json!({ "path": candidate.view.path, "rescanRequired": true })));
    }
    let base = temp.path().to_path_buf();
    DownloadedSkill::from_candidate(
        temp,
        &base,
        SkillCandidate {
            root,
            relative_root: "skill".into(),
            metadata,
        },
        None,
        None,
    )
}

fn read_metadata(root: &Path) -> Result<SkillMetadata> {
    let bytes = read_limited(&root.join("SKILL.md"), MAX_SKILL_MD_BYTES)?;
    let content = String::from_utf8(bytes)
        .map_err(|_| AppError::new(ErrorCode::InvalidSkill, "SKILL.md is not UTF-8"))?;
    parse_skill_md_content(&content, "SKILL.md")
}

fn include_entry(entry: &walkdir::DirEntry) -> bool {
    entry.depth() == 0
        || (entry.file_name() != ".git"
            && entry
                .metadata()
                .map(|metadata| !is_link(&metadata))
                .unwrap_or(true))
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
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

fn open_regular(path: &Path) -> Result<File> {
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

fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        check_file_type(&fs::symlink_metadata(ancestor)?, ancestor)?;
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    reparse || metadata.file_type().is_symlink()
}

fn check_file_type(metadata: &fs::Metadata, path: &Path) -> Result<()> {
    if is_link(metadata) || (!metadata.is_dir() && !metadata.is_file()) {
        return Err(AppError::unsafe_path(format!(
            "links and special files are not allowed: {}",
            display_path(path)
        )));
    }
    Ok(())
}

fn input_path(input: &str) -> Result<PathBuf> {
    let path = if input.starts_with("file:") {
        reqwest::Url::parse(input)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| AppError::invalid_input("invalid local file URL"))?
    } else {
        PathBuf::from(input)
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(AppError::unsafe_path(
            "local source must be an absolute path without traversal",
        ));
    }
    Ok(path)
}

fn source_id(path: &Path) -> String {
    let value = display_path(path);
    let value = if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    };
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
    }
}

fn candidate_path(source: &Path, kind: LocalSourceKind, relative: &str) -> String {
    if kind == LocalSourceKind::Folder {
        display_path(&source.join(if relative == "." { "" } else { relative }))
    } else {
        format!("{}!/{relative}", display_path(source))
    }
}

#[cfg(test)]
mod tests;
