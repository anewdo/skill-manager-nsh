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
    BatchItemFailure, CheckUpdatesReport, InstalledSkill, LocalState, OperationProgress,
    ProxySettings, SkillSource, SourceType, UpdateCheckItem, UpdateConflictPolicy, UpdateState,
};
use crate::core::operations::OperationContext;
use crate::core::sources::common::{
    DownloadObserver, DownloadProgress, DownloadedSkill, canonical_source, download,
};
use crate::core::sources::github::common::GitVerification;
use crate::core::sources::github::updates::{
    GitRepositories, GitUpdateTransport, UpdateSnapshots, skill_root,
};
use crate::core::store::Store;

pub fn check_updates(
    repo: &CentralRepo,
    store: &Store,
    ids: &[String],
    proxy: &ProxySettings,
    context: &OperationContext,
) -> Result<CheckUpdatesReport> {
    check_updates_with_transport(
        repo,
        store,
        ids,
        proxy,
        context,
        &mut GitRepositories {
            staging: &repo.staging_dir(),
            proxy,
        },
    )
}

pub(crate) fn check_updates_with_transport(
    repo: &CentralRepo,
    store: &Store,
    ids: &[String],
    proxy: &ProxySettings,
    context: &OperationContext,
    transport: &mut impl GitUpdateTransport,
) -> Result<CheckUpdatesReport> {
    let mut snapshots = UpdateSnapshots::default();
    check_batch(
        store,
        ids,
        |id, index, total| {
            check_remote(store, id, |skill| {
                let mut progress = batch_progress("checking", index, total, true);
                progress.detail = Some(skill.display_name.clone());
                let observer = CheckObserver { context, progress };
                let source = remote_source(skill)?;
                if source.source_type != SourceType::GitHub {
                    return downloaded_content(skill, download(repo, &source, proxy, &observer)?);
                }
                let target = snapshots.target(&source, transport, &observer)?;
                let previous = store
                    .git_verification(id)?
                    .filter(|previous| previous.matches(skill, &source, &target.reference));
                if let Some(previous) = previous
                    .as_ref()
                    .filter(|previous| previous.identity.revision == target.revision)
                {
                    return Ok(RemoteContent {
                        hash: previous.content_hash.clone(),
                        revision: Some(target.revision),
                        verification: Some(previous.clone()),
                    });
                }
                let snapshot = snapshots.snapshot(&target, transport, &observer)?;
                let root = skill_root(snapshot, &source, proxy, &observer)?;
                let identity = snapshot.identity(&root, proxy, &observer)?;
                if let Some(previous) = previous.filter(|previous| {
                    previous.identity.root == root && previous.identity.tree == identity.tree
                }) {
                    return Ok(RemoteContent {
                        hash: previous.content_hash.clone(),
                        revision: Some(identity.revision.clone()),
                        verification: Some(GitVerification {
                            identity,
                            ..previous
                        }),
                    });
                }
                downloaded_content(skill, transport.download(snapshot, &root, &observer)?)
            })
        },
        context,
    )
}

#[cfg(test)]
pub(super) fn check_updates_with_download(
    store: &Store,
    ids: &[String],
    mut download: impl FnMut(&str) -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<CheckUpdatesReport> {
    check_batch(
        store,
        ids,
        |id, _, _| check_update_with_download(store, id, || download(id)),
        context,
    )
}

fn check_batch(
    store: &Store,
    ids: &[String],
    mut check: impl FnMut(&str, u64, u64) -> Result<UpdateCheckItem>,
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
    let mut changed = false;

    for (index, id) in ids.iter().enumerate() {
        let mut item_progress = batch_progress("checking", index as u64, total, true);
        let before = store.skill(id)?;
        item_progress.detail = before.as_ref().map(|skill| skill.display_name.clone());
        match context.progress(item_progress) {
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
        let result = check(id, index as u64, total);
        changed |= store.skill(id)? != before;
        match result {
            Ok(item) => report.checked.push(item),
            Err(error) if error.code == ErrorCode::Cancelled => {
                if index == 0 {
                    return Err(error);
                }
                report.cancelled = true;
                break;
            }
            Err(error) => report.failed.push(BatchItemFailure {
                id: id.clone(),
                error,
            }),
        }
    }
    if changed {
        context.library_changed();
    }
    Ok(report)
}

#[cfg(test)]
pub(crate) fn check_update_with_download(
    store: &Store,
    id: &str,
    download: impl FnOnce() -> Result<DownloadedSkill>,
) -> Result<UpdateCheckItem> {
    check_remote(store, id, |skill| downloaded_content(skill, download()?))
}

struct RemoteContent {
    hash: String,
    revision: Option<String>,
    verification: Option<GitVerification>,
}

struct CheckObserver<'a> {
    context: &'a OperationContext,
    progress: OperationProgress,
}

impl DownloadObserver for CheckObserver<'_> {
    fn check_cancelled(&self) -> Result<()> {
        self.context.check_cancelled()
    }

    fn report(&self, transfer: DownloadProgress) -> Result<()> {
        let mut progress = self.progress.clone();
        progress.bytes = transfer.bytes;
        progress.bytes_total = transfer.bytes_total;
        if let Some(detail) = transfer.detail {
            progress.detail = Some(format!(
                "{} · {detail}",
                progress.detail.as_deref().unwrap_or_default()
            ));
        }
        self.context.progress(progress)
    }
}

fn downloaded_content(
    skill: &InstalledSkill,
    downloaded: DownloadedSkill,
) -> Result<RemoteContent> {
    let hash = content_hash(&downloaded.skill_root)?;
    let verification = GitVerification::from_download(skill, &downloaded, &hash);
    Ok(RemoteContent {
        hash,
        revision: downloaded
            .revision
            .clone()
            .or_else(|| downloaded.digest.clone()),
        verification,
    })
}

fn check_remote(
    store: &Store,
    id: &str,
    remote: impl FnOnce(&InstalledSkill) -> Result<RemoteContent>,
) -> Result<UpdateCheckItem> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let checked_at = Utc::now().timestamp_millis();

    match remote(&skill) {
        Ok(RemoteContent {
            hash,
            revision: remote_revision,
            verification,
        }) => {
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
                verification.as_ref(),
            )?;
            Ok(UpdateCheckItem {
                id: skill.id,
                display_name: skill.display_name,
                update_state,
                remote_revision,
            })
        }
        Err(error) => {
            if error.code == ErrorCode::Cancelled {
                return Err(error);
            }
            store.update_update_check(
                id,
                skill.remote_revision.as_deref(),
                UpdateState::CheckFailed,
                checked_at,
                Some(&error.message),
                None,
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
    let mut verification = GitVerification::from_download(&current, &downloaded, &hash);
    if let Some(verification) = &mut verification {
        verification.baseline_hash = Some(hash.clone());
    }
    let unchanged =
        local.state == LocalState::Clean && current.baseline_hash.as_deref() == Some(hash.as_str());
    if unchanged {
        store.update_skill_after_update(
            id,
            remote_revision.as_deref(),
            remote_revision.as_deref(),
            &hash,
            now,
            verification.as_ref(),
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
        verification.as_ref(),
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
