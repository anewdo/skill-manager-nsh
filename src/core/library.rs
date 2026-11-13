use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::central_repo::CentralRepo;
use super::content_identity::{ContentSummary, content_hash, content_summary, files_identical};
use super::error::{AppError, ErrorCode, Result};
use super::metadata;
use super::model::{
    BatchItemFailure, CheckUpdatesReport, DeleteReport, DeletedItem, InstalledSkill,
    LocalSkillInfo, LocalState, OperationProgress, ProxySettings, SkillDetail, SkillSource,
    UpdateCheckItem, UpdateConflictPolicy, UpdateState,
};
use super::naming::{RenameKind, allocate_name, classify_rename, root_key};
use super::operations::OperationContext;
use super::sources::{DownloadedSkill, download, preview::canonical_source};
use super::store::Store;

const MAX_LOCAL_SKILL_MD_BYTES: u64 = 1024 * 1024;

/// An install result plus a note about a same-named copy the app does not manage.
#[derive(Debug, Serialize)]
pub struct InstallOutcome {
    #[serde(flatten)]
    skill: InstalledSkill,
    #[serde(skip_serializing_if = "Option::is_none")]
    unmanaged_copy: Option<String>,
}

/// A library directory that has no database record yet.
struct UnmanagedRoot {
    name: String,
    identical: bool,
}

pub fn install_from_source(
    repo: &CentralRepo,
    store: &Store,
    source: SkillSource,
    proxy: &ProxySettings,
    context: &OperationContext,
) -> Result<InstallOutcome> {
    let source = canonical_source(source)?;
    let (skill, unmanaged_copy) = install_with_download(
        repo,
        store,
        source.clone(),
        || download(repo, &source, proxy, context),
        context,
    )?;
    Ok(InstallOutcome {
        skill,
        unmanaged_copy,
    })
}

fn install_with_download(
    repo: &CentralRepo,
    store: &Store,
    source: SkillSource,
    download: impl FnOnce() -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<(InstalledSkill, Option<String>)> {
    clear_missing_lineage(repo, store, &source, context)?;
    context.progress(progress("downloading", true))?;
    let downloaded = download()?;
    context.progress(progress("validating", true))?;
    let outcome = commit_prepared(
        repo,
        store,
        &downloaded,
        Some(source),
        context,
        progress("installing", false),
    )?;
    Ok((outcome.skill, outcome.unmanaged_copy))
}

pub(crate) struct CommitOutcome {
    pub skill: InstalledSkill,
    pub reused: bool,
    pub unmanaged_copy: Option<String>,
}

pub(crate) fn commit_prepared(
    repo: &CentralRepo,
    store: &Store,
    downloaded: &DownloadedSkill,
    source: Option<SkillSource>,
    context: &OperationContext,
    commit_progress: OperationProgress,
) -> Result<CommitOutcome> {
    context.check_cancelled()?;
    let hash = content_hash(&downloaded.skill_root)?;

    let mut existing = store.list_skills()?;
    if let Some(source) = &source {
        reject_existing_lineage(&existing, source)?;
    }

    let skill_name = downloaded.metadata.name.clone();
    let source = source.map(|mut source| {
        source.digest = downloaded.digest.clone();
        source
    });
    if let Some(skill) = find_identical(repo, &existing, &downloaded.skill_root, &hash)? {
        context.check_cancelled()?;
        return Ok(CommitOutcome {
            skill,
            reused: true,
            unmanaged_copy: None,
        });
    }
    let (unmanaged, occupied) =
        inspect_library_roots(repo, &existing, &skill_name, &downloaded.skill_root, &hash)?;
    if let Some(adoptable) = unmanaged.iter().find(|root| root.identical) {
        context.progress(commit_progress)?;
        let skill = adopt_unmanaged_root(
            store,
            adoptable,
            &skill_name,
            downloaded,
            source.as_ref(),
            &hash,
        )?;
        context.library_changed();
        return Ok(CommitOutcome {
            skill,
            reused: false,
            unmanaged_copy: None,
        });
    }
    let unmanaged_copy = unmanaged.first().map(|root| root.name.clone());

    let relative_root = allocate_name(&skill_name, &occupied)?;
    context.progress(commit_progress)?;

    existing = store.list_skills()?;
    if let Some(source) = &source {
        reject_existing_lineage(&existing, source)?;
    }
    let final_root_key = root_key(&relative_root);
    if existing
        .iter()
        .any(|skill| skill.root_key == final_root_key)
    {
        return Err(target_conflict(&relative_root));
    }

    let target = repo.skill_path(&relative_root)?;
    ensure_target_missing(&target, &relative_root)?;
    let created_parents = create_parent_directories(&target)?;
    if let Err(error) = repo
        .skill_path(&relative_root)
        .and_then(|_| ensure_target_missing(&target, &relative_root))
    {
        cleanup_parent_directories(&created_parents);
        return Err(error);
    }
    if let Err(error) = fs::rename(&downloaded.skill_root, &target) {
        cleanup_parent_directories(&created_parents);
        return Err(error.into());
    }

    let now = Utc::now().timestamp_millis();
    let skill = InstalledSkill {
        id: Uuid::new_v4().to_string(),
        display_name: downloaded.metadata.name.clone(),
        root_relpath: relative_root,
        root_key: final_root_key,
        source,
        installed_revision: downloaded.revision.clone(),
        remote_revision: None,
        baseline_hash: Some(hash.clone()),
        current_hash: Some(hash),
        local_state: LocalState::Clean,
        update_state: UpdateState::Unknown,
        update_checked_at: None,
        created_at: now,
        updated_at: now,
        last_error: None,
    };
    if let Err(error) = store.insert_skill(&skill) {
        if let Err(rollback_error) = fs::rename(&target, &downloaded.skill_root) {
            return Err(AppError::internal(format!(
                "database insert failed ({error}); skill rollback failed ({rollback_error}); new files remain at {}",
                target.display()
            )));
        }
        cleanup_parent_directories(&created_parents);
        return Err(error);
    }

    context.library_changed();
    Ok(CommitOutcome {
        skill,
        reused: false,
        unmanaged_copy,
    })
}

/// Takes an unmanaged library directory into the database when its files
/// match the download, so the same skill is not stored twice.
fn adopt_unmanaged_root(
    store: &Store,
    root: &UnmanagedRoot,
    skill_name: &str,
    downloaded: &DownloadedSkill,
    source: Option<&SkillSource>,
    hash: &str,
) -> Result<InstalledSkill> {
    let now = Utc::now().timestamp_millis();
    let skill = InstalledSkill {
        id: Uuid::new_v4().to_string(),
        display_name: skill_name.to_string(),
        root_relpath: root.name.clone(),
        root_key: root_key(&root.name),
        source: source.cloned(),
        installed_revision: downloaded.revision.clone(),
        remote_revision: None,
        baseline_hash: Some(hash.to_string()),
        current_hash: Some(hash.to_string()),
        local_state: LocalState::Clean,
        update_state: UpdateState::Unknown,
        update_checked_at: None,
        created_at: now,
        updated_at: now,
        last_error: None,
    };
    store.insert_skill(&skill)?;
    Ok(skill)
}

/// Scans the library directory for entries without a database record:
/// names that collide with the incoming skill stay available for adoption, and
/// every entry name is reported as occupied so a new install never overwrites one.
fn inspect_library_roots(
    repo: &CentralRepo,
    existing: &[InstalledSkill],
    skill_name: &str,
    downloaded_root: &Path,
    hash: &str,
) -> Result<(Vec<UnmanagedRoot>, BTreeSet<String>)> {
    let managed = existing
        .iter()
        .map(|skill| skill.root_key.clone())
        .collect::<BTreeSet<_>>();
    let mut occupied = managed.clone();
    let mut unmanaged = Vec::new();
    let entries = match fs::read_dir(repo.skills_root()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((unmanaged, occupied));
        }
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let key = root_key(&name);
        occupied.insert(key.clone());
        if managed.contains(&key) || !is_skill_name_variant(&key, skill_name) {
            continue;
        }
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let identical = content_hash(&path).is_ok_and(|local| local == hash)
            && files_identical(downloaded_root, &path).unwrap_or(false);
        unmanaged.push(UnmanagedRoot { name, identical });
    }
    unmanaged.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((unmanaged, occupied))
}

/// Whether `candidate` is the skill name or one of its numeric suffixes.
fn is_skill_name_variant(candidate: &str, skill_name: &str) -> bool {
    let base = root_key(skill_name);
    if candidate == base {
        return true;
    }
    candidate
        .strip_prefix(&format!("{base}-"))
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// A recorded lineage whose files disappeared is reinstalled from scratch: the
/// stale record is dropped first, otherwise installing it again would be
/// rejected as a duplicate source.
fn clear_missing_lineage(
    repo: &CentralRepo,
    store: &Store,
    source: &SkillSource,
    context: &OperationContext,
) -> Result<()> {
    let Some(skill) = store.skill_by_source(&source.source_key)? else {
        return Ok(());
    };
    let path = repo.skill_path(&skill.root_relpath)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "this source is already installed; use update instead",
            )
            .with_details(serde_json::json!({ "skillId": skill.id })));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    store.delete_skill_record(&skill.id)?;
    context.library_changed();
    Ok(())
}

/// Listings also report skills whose files are gone, so a marketplace
/// card returns to the installable state as soon as the local copy disappears.
pub fn list_skills(repo: &CentralRepo, store: &Store) -> Result<Vec<InstalledSkill>> {
    let mut skills = store.list_skills()?;
    let now = Utc::now().timestamp_millis();
    for skill in &mut skills {
        let path = repo.skill_path(&skill.root_relpath)?;
        let missing = matches!(
            fs::symlink_metadata(&path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        );
        if missing && skill.local_state != LocalState::Missing {
            store.update_local_state(
                &skill.id,
                skill.current_hash.as_deref(),
                LocalState::Missing,
                now,
            )?;
            skill.local_state = LocalState::Missing;
        }
    }
    Ok(skills)
}

fn reject_existing_lineage(existing: &[InstalledSkill], source: &SkillSource) -> Result<()> {
    if let Some(skill) = existing.iter().find(|skill| {
        skill
            .source
            .as_ref()
            .is_some_and(|installed| installed.source_key == source.source_key)
    }) {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "this source is already installed; use update instead",
        )
        .with_details(serde_json::json!({ "skillId": skill.id })));
    }
    Ok(())
}

fn find_identical(
    repo: &CentralRepo,
    existing: &[InstalledSkill],
    downloaded_root: &Path,
    hash: &str,
) -> Result<Option<InstalledSkill>> {
    for skill in existing
        .iter()
        .filter(|skill| skill.baseline_hash.as_deref() == Some(hash))
    {
        let path = repo.skill_path(&skill.root_relpath)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        if files_identical(downloaded_root, &path)? {
            return Ok(Some(skill.clone()));
        }
    }
    Ok(None)
}

fn progress(phase: &str, cancellable: bool) -> OperationProgress {
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

fn target_conflict(relative_root: &str) -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        format!("skill target already exists: {relative_root}"),
    )
}

fn ensure_target_missing(target: &Path, relative_root: &str) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(_) => Err(target_conflict(relative_root)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn create_parent_directories(target: &Path) -> Result<Vec<PathBuf>> {
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

fn cleanup_parent_directories(created: &[PathBuf]) {
    for path in created {
        let _ = fs::remove_dir(path);
    }
}

pub fn skill_detail(repo: &CentralRepo, store: &Store, id: &str) -> Result<SkillDetail> {
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    let inspection = inspect_local(repo, &skill)?;
    store.update_local_state(
        id,
        inspection
            .summary
            .as_ref()
            .map(|summary| summary.hash.as_str()),
        inspection.state,
        Utc::now().timestamp_millis(),
    )?;
    let skill = store.skill(id)?.ok_or_else(skill_not_found)?;
    Ok(SkillDetail {
        skill,
        local: local_skill_info(&inspection),
    })
}

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

fn check_updates_with_download(
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

fn check_update_with_download(
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

fn apply_update_with_download(
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

struct LocalInspection {
    path: PathBuf,
    state: LocalState,
    summary: Option<ContentSummary>,
}

fn inspect_local(repo: &CentralRepo, skill: &InstalledSkill) -> Result<LocalInspection> {
    let path = repo.skill_path(&skill.root_relpath)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LocalInspection {
                path,
                state: LocalState::Missing,
                summary: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::unsafe_path(format!(
            "skill root must be a real directory: {}",
            path.display()
        )));
    }

    let summary = content_summary(&path)?;
    let state = match skill.baseline_hash.as_deref() {
        Some(baseline) if baseline == summary.hash => LocalState::Clean,
        Some(_) => LocalState::Modified,
        None => LocalState::Unknown,
    };
    Ok(LocalInspection {
        path,
        state,
        summary: Some(summary),
    })
}

fn local_skill_info(inspection: &LocalInspection) -> LocalSkillInfo {
    let mut info = LocalSkillInfo {
        state: inspection.state,
        path: inspection.path.display().to_string(),
        current_hash: inspection
            .summary
            .as_ref()
            .map(|summary| summary.hash.clone()),
        file_count: inspection
            .summary
            .as_ref()
            .map(|summary| summary.file_count),
        total_bytes: inspection
            .summary
            .as_ref()
            .map(|summary| summary.total_bytes),
        metadata: None,
        skill_md: None,
        skill_md_error: None,
        inspected_at: Utc::now().timestamp_millis(),
    };
    if inspection.summary.is_none() {
        return info;
    }
    match read_skill_md_limited(&inspection.path) {
        Ok(Some(content)) => {
            match metadata::parse_skill_md_content(&content, "SKILL.md") {
                Ok(metadata) => info.metadata = Some(metadata),
                Err(error) => info.skill_md_error = Some(error),
            }
            info.skill_md = Some(content);
        }
        Ok(None) => {}
        Err(error) => info.skill_md_error = Some(error),
    }
    info
}

fn read_skill_md_limited(root: &Path) -> Result<Option<String>> {
    let path = root.join("SKILL.md");
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > MAX_LOCAL_SKILL_MD_BYTES {
        return Err(skill_md_too_large());
    }
    let bytes = fs::read(&path)?;
    if bytes.len() as u64 > MAX_LOCAL_SKILL_MD_BYTES {
        return Err(skill_md_too_large());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| AppError::new(ErrorCode::InvalidSkill, "SKILL.md is not UTF-8"))
}

fn skill_md_too_large() -> AppError {
    AppError::new(
        ErrorCode::InvalidSkill,
        "SKILL.md exceeds the 1 MiB detail limit",
    )
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

fn last_component(relative: &str) -> &str {
    relative.rsplit(['/', '\\']).next().unwrap_or(relative)
}

fn replace_last_component(relative: &str, new_name: &str) -> String {
    match relative.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{new_name}"),
        None => new_name.to_owned(),
    }
}

fn batch_progress(phase: &str, current: u64, total: u64, cancellable: bool) -> OperationProgress {
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

fn skill_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "skill was not found")
}

fn local_modified_error(skill: &InstalledSkill, message: &str) -> AppError {
    AppError::new(ErrorCode::LocalModified, message).with_details(serde_json::json!({
        "skillId": skill.id,
        "displayName": skill.display_name,
    }))
}

#[cfg(test)]
mod tests;
