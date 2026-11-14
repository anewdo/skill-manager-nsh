use std::fs::{self, OpenOptions};
use std::io::{Read, Write};

use walkdir::WalkDir;

use super::common::{
    MAX_ENTRIES, MAX_SKILL_BYTES, PendingCandidate, check_ancestors, check_file_type,
    include_entry, open_regular, read_metadata,
};
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata::{SkillCandidate, relative_path};
use crate::core::operations::OperationContext;
use crate::core::skill_path::safe_relative_path;
use crate::core::sources::common::DownloadedSkill;

pub(super) fn stage_candidate(
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
        let target = root.join(safe_relative_path(&relative)?);
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
