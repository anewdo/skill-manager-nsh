use std::fs;

use chrono::Utc;
use uuid::Uuid;

use super::common::{
    batch_progress, cleanup_parent_directories, create_parent_directories, ensure_target_missing,
    local_modified_error, progress, skill_not_found,
};
use super::inspect::inspect_local;
use crate::core::central_repo::CentralRepo;
use crate::core::content_identity::content_hash;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{
    BatchItemFailure, CheckUpdatesReport, InstalledSkill, LocalState, ProxySettings, SkillSource,
    UpdateCheckItem, UpdateConflictPolicy, UpdateState,
};
use crate::core::operations::OperationContext;
use crate::core::sources::common::{DownloadedSkill, canonical_source, download};
use crate::core::store::Store;

pub fn check_updates(
    repo: &CentralRepo,
    store: &Store,
    ids: &[String],
    proxy: &ProxySettings,
    context: &OperationContext,
) -> Result<CheckUpdatesReport> {
    check_updates_with_download(
        store,
        ids,
        |id| {
            let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
            let source = remote_source(&skill)?;
            download(repo, &source, proxy, context)
        },
        context,
    )
}

pub(super) fn check_updates_with_download(
    store: &Store,
    ids: &[String],
    mut download: impl FnMut(&str) -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<CheckUpdatesReport> {
    if ids.is_empty() {
        return Err(AppError::invalid_input("at least one skill id is required"));
    }
    let mut report = CheckUpdatesReport {
        checked: Vec::new(),
        failed: Vec::new(),
        cancelled: false,
    };
    let total = ids.len() as u64;

    for (index, id) in ids.iter().enumerate() {
        match context.progress(batch_progress("checking", index as u64, total, true)) {
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
        match check_update_with_download(store, id, || download(id)) {
            Ok(item) => report.checked.push(item),
            Err(error) => report.failed.push(BatchItemFailure {
                id: id.clone(),
                error,
            }),
        }
    }
    context.library_changed();
    Ok(report)
}

pub(crate) fn check_update_with_download(
    store: &Store,
    id: &str,
    download: impl FnOnce() -> Result<DownloadedSkill>,
) -> Result<UpdateCheckItem> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let checked_at = Utc::now().timestamp_millis();

    let remote = download().and_then(|downloaded| {
        let hash = content_hash(&downloaded.skill_root)?;
        let revision = downloaded
            .revision
            .clone()
            .or_else(|| downloaded.digest.clone());
        Ok((hash, revision))
    });

    match remote {
        Ok((hash, remote_revision)) => {
            let update_state = if skill.baseline_hash.as_deref() == Some(hash.as_str()) {
                UpdateState::UpToDate
            } else {
                UpdateState::Available
            };
            store.update_update_check(
                id,
                remote_revision.as_deref(),
                update_state,
                checked_at,
                None,
            )?;
            Ok(UpdateCheckItem {
                id: skill.id,
                display_name: skill.display_name,
                update_state,
                remote_revision,
            })
        }
        Err(error) => {
            store.update_update_check(
                id,
                skill.remote_revision.as_deref(),
                UpdateState::CheckFailed,
                checked_at,
                Some(&error.message),
            )?;
            Err(error)
        }
    }
}

pub fn apply_update(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    policy: UpdateConflictPolicy,
    proxy: &ProxySettings,
    context: &OperationContext,
) -> Result<InstalledSkill> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let source = remote_source(&skill)?;
    apply_update_with_download(
        repo,
        store,
        id,
        policy,
        || download(repo, &source, proxy, context),
        context,
    )
}

pub(crate) fn apply_update_with_download(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    policy: UpdateConflictPolicy,
    download: impl FnOnce() -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<InstalledSkill> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let initial = inspect_local(repo, &skill)?;
    if initial.state == LocalState::Modified && policy == UpdateConflictPolicy::Reject {
        store.update_local_state(
            id,
            initial
                .summary
                .as_ref()
                .map(|summary| summary.hash.as_str()),
            LocalState::Modified,
            Utc::now().timestamp_millis(),
        )?;
        return Err(local_modified_error(
            &skill,
            "local changes must be confirmed before updating this skill",
        ));
    }

    context.progress(progress("downloading", true))?;
    let downloaded = download()?;
    context.progress(progress("validating", true))?;
    let hash = content_hash(&downloaded.skill_root)?;
    let remote_revision = downloaded
        .revision
        .clone()
        .or_else(|| downloaded.digest.clone());

    context.progress(progress("installing", false))?;
    let current = store.skill(id)?.ok_or_else(skill_not_found)?;
    if current.root_relpath != skill.root_relpath {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "skill was renamed by another operation",
        ));
    }
    let local = inspect_local(repo, &current)?;
    if local.state == LocalState::Modified && policy == UpdateConflictPolicy::Reject {
        store.update_local_state(
            id,
            local.summary.as_ref().map(|summary| summary.hash.as_str()),
            LocalState::Modified,
            Utc::now().timestamp_millis(),
        )?;
        return Err(local_modified_error(
            &current,
            "local changes must be confirmed before updating this skill",
        ));
    }

    let now = Utc::now().timestamp_millis();
    let unchanged =
        local.state == LocalState::Clean && current.baseline_hash.as_deref() == Some(hash.as_str());
    if unchanged {
        store.update_skill_after_update(
            id,
            remote_revision.as_deref(),
            remote_revision.as_deref(),
            &hash,
            now,
        )?;
        context.library_changed();
        return store
            .skill(id)?
            .ok_or_else(|| AppError::internal("skill disappeared after update"));
    }

    let target = repo.skill_path(&current.root_relpath)?;
    let created_parents = if local.summary.is_some() {
        Vec::new()
    } else {
        create_parent_directories(&target)?
    };

    let backup = if local.summary.is_some() {
        let backup_relpath = format!("update-{}", Uuid::new_v4());
        let backup_path = repo.staging_path(&backup_relpath)?;
        ensure_target_missing(&backup_path, &backup_relpath)?;
        if let Err(error) = fs::rename(&target, &backup_path) {
            cleanup_parent_directories(&created_parents);
            return Err(error.into());
        }
        Some(backup_path)
    } else {
        None
    };

    if let Err(error) = fs::rename(&downloaded.skill_root, &target) {
        let restored = match &backup {
            Some(backup_path) => fs::rename(backup_path, &target).map_err(|rollback| {
                AppError::internal(format!(
                    "placing updated files failed ({error}); restoring the original directory also failed ({rollback}); original files remain at {}",
                    backup_path.display()
                ))
            }),
            None => Ok(()),
        };
        cleanup_parent_directories(&created_parents);
        restored?;
        return Err(error.into());
    }

    if let Err(error) = store.update_skill_after_update(
        id,
        remote_revision.as_deref(),
        remote_revision.as_deref(),
        &hash,
        now,
    ) {
        let moved_new = fs::rename(&target, &downloaded.skill_root);
        let restored = match &backup {
            Some(backup_path) => fs::rename(backup_path, &target),
            None => Ok(()),
        };
        if moved_new.is_err() || restored.is_err() {
            return Err(AppError::internal(format!(
                "database update failed ({error}); restoring the previous skill failed; files remain at {}",
                target.display()
            )));
        }
        cleanup_parent_directories(&created_parents);
        return Err(error);
    }

    if let Some(backup_path) = backup {
        let _ = fs::remove_dir_all(&backup_path);
    }
    context.library_changed();
    store
        .skill(id)?
        .ok_or_else(|| AppError::internal("skill disappeared after update"))
}

fn remote_source(skill: &InstalledSkill) -> Result<SkillSource> {
    let source = skill
        .source
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::Unsupported, "skill has no remote source"))?;
    canonical_source(source)
}
