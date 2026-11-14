use std::fs;
use std::path::{Path, PathBuf};

use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{InstalledSkill, OperationProgress};

pub(super) fn progress(phase: &str, cancellable: bool) -> OperationProgress {
    OperationProgress {
        phase: phase.to_string(),
        current: None,
        total: None,
        bytes: None,
        bytes_total: None,
        detail: None,
        cancellable,
    }
}

pub(super) fn target_conflict(relative_root: &str) -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        format!("skill target already exists: {relative_root}"),
    )
}

pub(super) fn ensure_target_missing(target: &Path, relative_root: &str) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(_) => Err(target_conflict(relative_root)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn create_parent_directories(target: &Path) -> Result<Vec<PathBuf>> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::unsafe_path("skill target has no parent"))?;
    let mut missing = Vec::new();
    let mut current = Some(parent);
    while let Some(path) = current {
        match fs::symlink_metadata(path) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(path.to_path_buf());
                current = path.parent();
            }
            Err(error) => return Err(error.into()),
        }
    }
    if let Err(error) = fs::create_dir_all(parent) {
        cleanup_parent_directories(&missing);
        return Err(error.into());
    }
    Ok(missing)
}

pub(super) fn cleanup_parent_directories(created: &[PathBuf]) {
    for path in created {
        let _ = fs::remove_dir(path);
    }
}

pub(super) fn last_component(relative: &str) -> &str {
    relative.rsplit(['/', '\\']).next().unwrap_or(relative)
}

pub(super) fn batch_progress(
    phase: &str,
    current: u64,
    total: u64,
    cancellable: bool,
) -> OperationProgress {
    OperationProgress {
        phase: phase.to_string(),
        current: Some(current),
        total: Some(total),
        bytes: None,
        bytes_total: None,
        detail: None,
        cancellable,
    }
}

pub(super) fn skill_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "skill was not found")
}

pub(super) fn local_modified_error(skill: &InstalledSkill, message: &str) -> AppError {
    AppError::new(ErrorCode::LocalModified, message).with_details(serde_json::json!({
        "skillId": skill.id,
        "displayName": skill.display_name,
    }))
}
