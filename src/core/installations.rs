use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use super::central_repo::CentralRepo;
use super::content_identity::{ContentSummary, content_hash, content_summary, files_identical};
use super::error::{AppError, ErrorCode, Result};
use super::metadata;
use super::model::{
    BatchItemFailure, Category, DeleteReport, DeletedItem, Installation, InstallationDetail,
    LocalSkillInfo, LocalState, OperationProgress, ProxySettings, PurgeReport, RestoreReport,
    RestoredItem, SkillSource, SourceType, TrashEntry, UpdateState,
};
use super::naming::{RenameKind, allocate_marketplace_name, classify_rename, root_key};
use super::operations::OperationContext;
use super::sources::{DownloadedSkill, download, preview::canonical_source};
use super::store::Store;

const MAX_LOCAL_SKILL_MD_BYTES: u64 = 1024 * 1024;

pub fn install_marketplace(
    repo: &CentralRepo,
    store: &Store,
    source: SkillSource,
    proxy: &ProxySettings,
    context: &OperationContext,
) -> Result<Installation> {
    let source = canonical_source(source)?;
    install_with_download(
        repo,
        store,
        source.clone(),
        || download(repo, &source, proxy),
        context,
    )
}

fn install_with_download(
    repo: &CentralRepo,
    store: &Store,
    source: SkillSource,
    download: impl FnOnce() -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<Installation> {
    reject_existing_lineage(&store.list_installations(Category::Marketplace)?, &source)?;
    context.progress(progress("downloading", true))?;
    let downloaded = download()?;
    context.progress(progress("validating", true))?;
    let hash = content_hash(&downloaded.skill_root)?;

    let existing = store.list_installations(Category::Marketplace)?;
    reject_existing_lineage(&existing, &source)?;
    if let Some(installation) = find_identical(repo, &existing, &downloaded.skill_root, &hash)? {
        return Ok(installation);
    }

    let used_root_keys = existing
        .iter()
        .map(|installation| installation.root_key.clone())
        .collect::<BTreeSet<_>>();
    let relative_root = allocate_marketplace_name(
        &downloaded.metadata.name,
        &source_scope(&source)?,
        &used_root_keys,
    )?;
    context.progress(progress("installing", false))?;

    let final_installations = store.list_installations(Category::Marketplace)?;
    reject_existing_lineage(&final_installations, &source)?;
    let final_root_key = root_key(&relative_root);
    if final_installations
        .iter()
        .any(|installation| installation.root_key == final_root_key)
    {
        return Err(target_conflict(&relative_root));
    }

    let target = repo.installation_path(Category::Marketplace, &relative_root)?;
    ensure_target_missing(&target, &relative_root)?;
    let created_parents = create_parent_directories(&target)?;
    if let Err(error) = repo
        .installation_path(Category::Marketplace, &relative_root)
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
    let mut source = source;
    source.digest = downloaded.digest.clone();
    let installation = Installation {
        id: Uuid::new_v4().to_string(),
        category: Category::Marketplace,
        display_name: downloaded.metadata.name.clone(),
        root_relpath: relative_root,
        root_key: final_root_key,
        source: Some(source),
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
    if let Err(error) = store.insert_installation(&installation) {
        if let Err(rollback_error) = fs::rename(&target, &downloaded.skill_root) {
            return Err(AppError::internal(format!(
                "database insert failed ({error}); installation rollback failed ({rollback_error}); new files remain at {}",
                target.display()
            )));
        }
        cleanup_parent_directories(&created_parents);
        return Err(error);
    }

    context.installations_changed();
    Ok(installation)
}

fn reject_existing_lineage(existing: &[Installation], source: &SkillSource) -> Result<()> {
    if let Some(installation) = existing.iter().find(|installation| {
        installation
            .source
            .as_ref()
            .is_some_and(|installed| installed.source_key == source.source_key)
    }) {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "this source is already installed; use update instead",
        )
        .with_details(serde_json::json!({ "installationId": installation.id })));
    }
    Ok(())
}

fn find_identical(
    repo: &CentralRepo,
    existing: &[Installation],
    downloaded_root: &Path,
    hash: &str,
) -> Result<Option<Installation>> {
    for installation in existing
        .iter()
        .filter(|installation| installation.baseline_hash.as_deref() == Some(hash))
    {
        let path = repo.installation_path(Category::Marketplace, &installation.root_relpath)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        if files_identical(downloaded_root, &path)? {
            return Ok(Some(installation.clone()));
        }
    }
    Ok(None)
}

fn source_scope(source: &SkillSource) -> Result<String> {
    match source.source_type {
        SourceType::GitHub => source
            .repository
            .clone()
            .ok_or_else(|| AppError::invalid_input("GitHub source repository is required")),
        SourceType::WellKnown => {
            let lineage = source
                .source_key
                .strip_prefix("well_known:")
                .ok_or_else(|| AppError::invalid_input("invalid well-known source lineage"))?;
            let (domain, _) = lineage
                .split_once('/')
                .ok_or_else(|| AppError::invalid_input("invalid well-known source lineage"))?;
            Ok(format!("well-known/{domain}"))
        }
    }
}

fn progress(phase: &str, cancellable: bool) -> OperationProgress {
    OperationProgress {
        phase: phase.to_string(),
        current: None,
        total: None,
        detail: None,
        cancellable,
    }
}

fn target_conflict(relative_root: &str) -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        format!("installation target already exists: {relative_root}"),
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
        .ok_or_else(|| AppError::unsafe_path("installation target has no parent"))?;
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

pub fn installation_detail(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
) -> Result<InstallationDetail> {
    let installation = store.installation(id)?.ok_or_else(installation_not_found)?;
    let inspection = inspect_local(repo, &installation)?;
    store.update_local_state(
        id,
        inspection
            .summary
            .as_ref()
            .map(|summary| summary.hash.as_str()),
        inspection.state,
        Utc::now().timestamp_millis(),
    )?;
    let installation = store.installation(id)?.ok_or_else(installation_not_found)?;
    Ok(InstallationDetail {
        installation,
        local: local_skill_info(&inspection),
    })
}

pub fn rename_installation(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    new_name: &str,
    context: &OperationContext,
) -> Result<Installation> {
    context.progress(progress("renaming", true))?;
    let installation = store.installation(id)?.ok_or_else(installation_not_found)?;
    let kind = classify_rename(last_component(&installation.root_relpath), new_name)?;
    if kind == RenameKind::Same {
        return Ok(installation);
    }

    let new_relpath = replace_last_component(&installation.root_relpath, new_name);
    let new_root_key = root_key(&new_relpath);
    reject_root_key_conflict(store, &installation, &new_root_key, &new_relpath)?;

    let source = repo.installation_path(installation.category, &installation.root_relpath)?;
    let target = repo.installation_path(installation.category, &new_relpath)?;
    ensure_directory(&source, &installation.root_relpath)?;
    if !(cfg!(windows) && kind == RenameKind::CaseOnly) {
        ensure_target_missing(&target, &new_relpath)?;
    }

    let current = store.installation(id)?.ok_or_else(installation_not_found)?;
    if current.root_relpath != installation.root_relpath {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "installation was renamed by another operation",
        ));
    }
    context.check_cancelled()?;

    move_path(&source, &target, kind == RenameKind::CaseOnly)?;
    if let Err(error) = store.update_installation_rename(
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

    context.installations_changed();
    store
        .installation(id)?
        .ok_or_else(|| AppError::internal("installation disappeared after rename"))
}

pub fn delete_installations(
    repo: &CentralRepo,
    store: &Store,
    ids: &[String],
    confirm_modified: bool,
    context: &OperationContext,
) -> Result<DeleteReport> {
    if ids.is_empty() {
        return Err(AppError::invalid_input(
            "at least one installation id is required",
        ));
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
                context.installations_changed();
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

pub fn restore_trash(
    repo: &CentralRepo,
    store: &Store,
    trash_ids: &[String],
    context: &OperationContext,
) -> Result<RestoreReport> {
    if trash_ids.is_empty() {
        return Err(AppError::invalid_input(
            "at least one trash entry id is required",
        ));
    }
    let mut report = RestoreReport {
        restored: Vec::new(),
        failed: Vec::new(),
        cancelled: false,
    };
    let total = trash_ids.len() as u64;

    for (index, id) in trash_ids.iter().enumerate() {
        match context.progress(batch_progress("restoring", index as u64, total, true)) {
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
        match restore_one(repo, store, id) {
            Ok(item) => {
                context.installations_changed();
                report.restored.push(item);
            }
            Err(error) => report.failed.push(BatchItemFailure {
                id: id.clone(),
                error,
            }),
        }
    }
    Ok(report)
}

pub fn purge_trash(
    repo: &CentralRepo,
    store: &Store,
    trash_ids: &[String],
    context: &OperationContext,
) -> Result<PurgeReport> {
    if trash_ids.is_empty() {
        return Err(AppError::invalid_input(
            "at least one trash entry id is required",
        ));
    }
    let mut report = PurgeReport {
        purged: Vec::new(),
        failed: Vec::new(),
        cancelled: false,
    };
    let total = trash_ids.len() as u64;

    for (index, id) in trash_ids.iter().enumerate() {
        match context.progress(batch_progress("purging", index as u64, total, true)) {
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
        match purge_one(repo, store, id) {
            Ok(()) => report.purged.push(id.clone()),
            Err(error) => report.failed.push(BatchItemFailure {
                id: id.clone(),
                error,
            }),
        }
    }
    Ok(report)
}

struct LocalInspection {
    path: PathBuf,
    state: LocalState,
    summary: Option<ContentSummary>,
}

fn inspect_local(repo: &CentralRepo, installation: &Installation) -> Result<LocalInspection> {
    let path = repo.installation_path(installation.category, &installation.root_relpath)?;
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
            "installation root must be a real directory: {}",
            path.display()
        )));
    }

    let summary = content_summary(&path)?;
    let state = match installation.baseline_hash.as_deref() {
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
    let installation = store.installation(id)?.ok_or_else(installation_not_found)?;
    let inspection = inspect_local(repo, &installation)?;

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
        return Err(AppError::new(
            ErrorCode::LocalModified,
            "local changes must be confirmed before deleting this installation",
        )
        .with_details(serde_json::json!({
            "installationId": installation.id,
            "displayName": installation.display_name,
        })));
    }

    if inspection.summary.is_none() {
        store.delete_installation_record(id)?;
        return Ok(DeletedItem {
            id: installation.id,
            display_name: installation.display_name,
            trashed: false,
        });
    }

    let trash_relpath = Uuid::new_v4().to_string();
    let trash_path = repo.trash_path(&trash_relpath)?;
    ensure_target_missing(&trash_path, &trash_relpath)?;
    fs::rename(&inspection.path, &trash_path)?;

    let mut installation = installation;
    installation.current_hash = inspection
        .summary
        .as_ref()
        .map(|summary| summary.hash.clone());
    installation.local_state = inspection.state;
    let entry = TrashEntry {
        id: trash_relpath.clone(),
        installation_id: installation.id.clone(),
        category: installation.category,
        display_name: installation.display_name.clone(),
        original_relpath: installation.root_relpath.clone(),
        original_root_key: installation.root_key.clone(),
        trash_relpath,
        installation,
        deleted_at: Utc::now().timestamp_millis(),
    };
    if let Err(error) = store.trash_installation(&entry) {
        if let Err(rollback) = fs::rename(&trash_path, &inspection.path) {
            return Err(AppError::internal(format!(
                "database delete failed ({error}); restoring the installation directory also failed ({rollback}); files remain at {}",
                trash_path.display()
            )));
        }
        return Err(error);
    }
    Ok(DeletedItem {
        id: entry.installation_id,
        display_name: entry.display_name,
        trashed: true,
    })
}

fn restore_one(repo: &CentralRepo, store: &Store, trash_id: &str) -> Result<RestoredItem> {
    let entry = store
        .trash_entry(trash_id)?
        .ok_or_else(trash_entry_not_found)?;
    let target = repo.installation_path(entry.category, &entry.original_relpath)?;
    ensure_target_missing(&target, &entry.original_relpath)?;
    reject_restore_conflicts(store, &entry)?;

    let trash_path = repo.trash_path(&entry.trash_relpath)?;
    ensure_directory(&trash_path, &entry.trash_relpath)?;
    fs::rename(&trash_path, &target)?;

    let mut installation = entry.installation.clone();
    installation.updated_at = Utc::now().timestamp_millis();
    if let Err(error) = store.restore_trash_entry(&entry.id, &installation) {
        if let Err(rollback) = fs::rename(&target, &trash_path) {
            return Err(AppError::internal(format!(
                "database restore failed ({error}); moving files back to trash also failed ({rollback}); files remain at {}",
                target.display()
            )));
        }
        return Err(error);
    }
    Ok(RestoredItem {
        trash_id: entry.id,
        installation_id: installation.id,
        display_name: installation.display_name,
    })
}

fn purge_one(repo: &CentralRepo, store: &Store, trash_id: &str) -> Result<()> {
    let entry = store
        .trash_entry(trash_id)?
        .ok_or_else(trash_entry_not_found)?;
    let path = repo.trash_path(&entry.trash_relpath)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError::unsafe_path(format!(
                "symbolic links are not allowed: {}",
                path.display()
            )));
        }
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&path)?,
        Ok(_) => {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "trash entry is not a directory",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    store.delete_trash_entry(&entry.id)?;
    Ok(())
}

fn reject_root_key_conflict(
    store: &Store,
    installation: &Installation,
    new_root_key: &str,
    new_relpath: &str,
) -> Result<()> {
    let conflict = store
        .list_installations(installation.category)?
        .into_iter()
        .any(|other| other.id != installation.id && other.root_key == new_root_key);
    if conflict {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!("installation target already exists: {new_relpath}"),
        ));
    }
    Ok(())
}

fn reject_restore_conflicts(store: &Store, entry: &TrashEntry) -> Result<()> {
    let existing = store.list_installations(entry.category)?;
    if existing
        .iter()
        .any(|installation| installation.root_key == entry.original_root_key)
    {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!(
                "installation target already exists: {}",
                entry.original_relpath
            ),
        )
        .with_details(serde_json::json!({ "relpath": entry.original_relpath })));
    }
    if let Some(source_key) = entry
        .installation
        .source
        .as_ref()
        .map(|source| source.source_key.as_str())
        && existing.iter().any(|installation| {
            installation
                .source
                .as_ref()
                .is_some_and(|source| source.source_key == source_key)
        })
    {
        return Err(
            AppError::new(ErrorCode::Conflict, "this source is already installed")
                .with_details(serde_json::json!({ "sourceKey": source_key })),
        );
    }
    Ok(())
}

fn ensure_directory(path: &Path, relative: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(AppError::new(
            ErrorCode::Conflict,
            format!("installation path is not a directory: {relative}"),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(AppError::new(
            ErrorCode::NotFound,
            format!("installation files are missing: {relative}"),
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
        detail: None,
        cancellable,
    }
}

fn installation_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "installation was not found")
}

fn trash_entry_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "trash entry was not found")
}

#[cfg(test)]
mod tests;
