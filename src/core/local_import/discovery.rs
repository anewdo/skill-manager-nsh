use std::fs;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::common::{
    DiscoveryIssue, LocalCandidate, LocalOrigin, LocalSourceKind, MAX_CANDIDATES, MAX_ENTRIES,
    MAX_ZIP_BYTES, PendingCandidate, SourceSession, check_ancestors, check_file_type, display_path,
    include_entry, read_limited, read_metadata, source_id,
};
use crate::core::archive::extract_zip;
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata::{SkillCandidate, relative_path};
use crate::core::skill_path::safe_relative_path;
use crate::core::sources::common::SilentObserver;

pub(super) fn discover_source(
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
            extract_zip(&bytes, archive.path(), &SilentObserver)?;
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
        safe_relative_path(&relative)?;
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

fn candidate_path(source: &Path, kind: LocalSourceKind, relative: &str) -> String {
    if kind == LocalSourceKind::Folder {
        display_path(&source.join(if relative == "." { "" } else { relative }))
    } else {
        format!("{}!/{relative}", display_path(source))
    }
}
