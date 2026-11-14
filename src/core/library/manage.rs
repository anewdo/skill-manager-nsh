use std::fs;
use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use super::common::{
    batch_progress, ensure_target_missing, last_component, local_modified_error, progress,
    skill_not_found,
};
use super::inspect::inspect_local;
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{BatchItemFailure, DeleteReport, DeletedItem, InstalledSkill, LocalState};
use crate::core::naming::{RenameKind, classify_rename, root_key};
use crate::core::operations::OperationContext;
use crate::core::store::Store;

pub fn rename_skill(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    new_name: &str,
    context: &OperationContext,
) -> Result<InstalledSkill> {
    context.progress(progress("renaming", true))?;
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let kind = classify_rename(last_component(&skill.root_relpath), new_name)?;
    if kind == RenameKind::Same {
        return Ok(skill);
    }

    let new_relpath = replace_last_component(&skill.root_relpath, new_name);
    let new_root_key = root_key(&new_relpath);
    reject_root_key_conflict(store, &skill, &new_root_key, &new_relpath)?;

    let source = repo.skill_path(&skill.root_relpath)?;
    let target = repo.skill_path(&new_relpath)?;
    ensure_directory(&source, &skill.root_relpath)?;
    if !(cfg!(windows) && kind == RenameKind::CaseOnly) {
        ensure_target_missing(&target, &new_relpath)?;
    }

    let current = store.skill(id)?.ok_or_else(skill_not_found)?;
    if current.root_relpath != skill.root_relpath {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "skill was renamed by another operation",
        ));
    }
    context.check_cancelled()?;

    move_path(&source, &target, kind == RenameKind::CaseOnly)?;
    if let Err(error) = store.update_skill_rename(
        id,
        new_name,
        &new_relpath,
        &new_root_key,
        Utc::now().timestamp_millis(),
    ) {
        if let Err(rollback) = move_path(&target, &source, kind == RenameKind::CaseOnly) {
            return Err(AppError::internal(format!(
                "database update failed ({error}); restoring the original directory also failed ({rollback}); files remain at {}",
                target.display()
            )));
        }
        return Err(error);
    }

    context.library_changed();
    store
        .skill(id)?
        .ok_or_else(|| AppError::internal("skill disappeared after rename"))
}

pub fn delete_skills(
    repo: &CentralRepo,
    store: &Store,
    ids: &[String],
    confirm_modified: bool,
    context: &OperationContext,
) -> Result<DeleteReport> {
    if ids.is_empty() {
        return Err(AppError::invalid_input("at least one skill id is required"));
    }
    let mut report = DeleteReport {
        deleted: Vec::new(),
        failed: Vec::new(),
        cancelled: false,
    };
    let total = ids.len() as u64;

    for (index, id) in ids.iter().enumerate() {
        match context.progress(batch_progress("deleting", index as u64, total, true)) {
            Ok(()) => {}
            Err(error) if error.code == ErrorCode::Cancelled => {
                if index == 0 {
                    return Err(error);
                }
                report.cancelled = true;
                break;
            }
            Err(error) => return Err(error),
        }
        match delete_one(repo, store, id, confirm_modified) {
            Ok(item) => {
                context.library_changed();
                report.deleted.push(item);
            }
            Err(error) => report.failed.push(BatchItemFailure {
                id: id.clone(),
                error,
            }),
        }
    }
    Ok(report)
}

fn delete_one(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    confirm_modified: bool,
) -> Result<DeletedItem> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let inspection = inspect_local(repo, &skill)?;

    if inspection.state == LocalState::Modified && !confirm_modified {
        store.update_local_state(
            id,
            inspection
                .summary
                .as_ref()
                .map(|summary| summary.hash.as_str()),
            LocalState::Modified,
            Utc::now().timestamp_millis(),
        )?;
        return Err(local_modified_error(
            &skill,
            "local changes must be confirmed before deleting this skill",
        ));
    }

    let item = DeletedItem {
        id: skill.id.clone(),
        display_name: skill.display_name.clone(),
    };
    if inspection.summary.is_none() {
        store.delete_skill_record(id)?;
        return Ok(item);
    }

    let staging_relpath = format!("delete-{}", Uuid::new_v4());
    let staging_path = repo.staging_path(&staging_relpath)?;
    ensure_target_missing(&staging_path, &staging_relpath)?;
    fs::rename(&inspection.path, &staging_path)?;

    if let Err(error) = store.delete_skill_record(id) {
        if let Err(rollback) = fs::rename(&staging_path, &inspection.path) {
            return Err(AppError::internal(format!(
                "database delete failed ({error}); restoring the skill directory also failed ({rollback}); files remain at {}",
                staging_path.display()
            )));
        }
        return Err(error);
    }

    if let Err(error) = fs::remove_dir_all(&staging_path) {
        return Err(AppError::new(
            ErrorCode::Io,
            format!(
                "skill was removed from the database, but its files could not be deleted: {}: {error}",
                staging_path.display()
            ),
        ));
    }
    Ok(item)
}

fn reject_root_key_conflict(
    store: &Store,
    skill: &InstalledSkill,
    new_root_key: &str,
    new_relpath: &str,
) -> Result<()> {
    let conflict = store
        .list_skills()?
        .into_iter()
        .any(|other| other.id != skill.id && other.root_key == new_root_key);
    if conflict {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!("skill target already exists: {new_relpath}"),
        ));
    }
    Ok(())
}

fn ensure_directory(path: &Path, relative: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(AppError::new(
            ErrorCode::Conflict,
            format!("skill path is not a directory: {relative}"),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(AppError::new(
            ErrorCode::NotFound,
            format!("skill files are missing: {relative}"),
        )),
        Err(error) => Err(error.into()),
    }
}

fn move_path(from: &Path, to: &Path, case_only: bool) -> Result<()> {
    if !case_only {
        return fs::rename(from, to).map_err(Into::into);
    }
    let parent = to
        .parent()
        .ok_or_else(|| AppError::unsafe_path("rename target has no parent"))?;
    let intermediate = parent.join(format!(".skills-manager-rename-{}", Uuid::new_v4()));
    fs::rename(from, &intermediate)?;
    match fs::rename(&intermediate, to) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::rename(&intermediate, from);
            Err(error.into())
        }
    }
}

fn replace_last_component(relative: &str, new_name: &str) -> String {
    match relative.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{new_name}"),
        None => new_name.to_owned(),
    }
}
