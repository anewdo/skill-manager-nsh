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
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use tempfile::TempDir;

    use super::*;
    use crate::core::metadata::{SkillCandidate, SkillMetadata};
    use crate::core::model::{OperationKind, OperationSnapshot, OperationState};
    use crate::core::operations::OperationQueue;
    use crate::core::sources::skills_sh;

    fn setup() -> (TempDir, CentralRepo, Arc<Store>) {
        let temp = tempfile::tempdir().unwrap();
        let repo = CentralRepo::new(temp.path().join("repo"));
        let store = Arc::new(Store::open(&repo).unwrap());
        (temp, repo, store)
    }

    fn source(repository: &str) -> SkillSource {
        skills_sh::marketplace_source(repository, "skill").unwrap()
    }

    fn write_skill(root: &Path, name: &str, content: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Demo skill\n---\n{content}"),
        )
        .unwrap();
    }

    fn downloaded(repo: &CentralRepo, name: &str, content: &str) -> DownloadedSkill {
        let temp = tempfile::Builder::new()
            .prefix("install-test-")
            .tempdir_in(repo.staging_dir())
            .unwrap();
        let base = temp.path().to_path_buf();
        let root = base.join("skill");
        write_skill(&root, name, content);
        let candidate = SkillCandidate {
            root,
            relative_root: ".".to_string(),
            metadata: SkillMetadata {
                name: name.to_string(),
                description: "Demo skill".to_string(),
            },
        };
        DownloadedSkill::from_candidate(
            temp,
            &base,
            candidate,
            Some("a".repeat(40)),
            Some("sha256:test".to_string()),
        )
        .unwrap()
    }

    fn add_existing(
        repo: &CentralRepo,
        store: &Store,
        id: &str,
        source: SkillSource,
        name: &str,
        content: &str,
    ) -> Installation {
        let path = repo.installation_path(Category::Marketplace, name).unwrap();
        write_skill(&path, name, content);
        let hash = content_hash(&path).unwrap();
        let installation = Installation {
            id: id.to_string(),
            category: Category::Marketplace,
            display_name: name.to_string(),
            root_relpath: name.to_string(),
            root_key: root_key(name),
            source: Some(source),
            installed_revision: Some("b".repeat(40)),
            remote_revision: None,
            baseline_hash: Some(hash.clone()),
            current_hash: Some(hash),
            local_state: LocalState::Clean,
            update_state: UpdateState::Unknown,
            update_checked_at: None,
            created_at: 1,
            updated_at: 1,
            last_error: None,
        };
        store.insert_installation(&installation).unwrap();
        installation
    }

    fn add_scoped_existing(
        repo: &CentralRepo,
        store: &Store,
        id: &str,
        source: SkillSource,
        root_relpath: &str,
    ) -> Installation {
        let path = repo
            .installation_path(Category::Marketplace, root_relpath)
            .unwrap();
        let name = last_component(root_relpath);
        write_skill(&path, name, "scoped");
        let hash = content_hash(&path).unwrap();
        let installation = Installation {
            id: id.to_string(),
            category: Category::Marketplace,
            display_name: name.to_string(),
            root_relpath: root_relpath.to_string(),
            root_key: root_key(root_relpath),
            source: Some(source),
            installed_revision: Some("b".repeat(40)),
            remote_revision: None,
            baseline_hash: Some(hash.clone()),
            current_hash: Some(hash),
            local_state: LocalState::Clean,
            update_state: UpdateState::Unknown,
            update_checked_at: None,
            created_at: 1,
            updated_at: 1,
            last_error: None,
        };
        store.insert_installation(&installation).unwrap();
        installation
    }

    fn run_rename(
        repo: &CentralRepo,
        store: &Arc<Store>,
        id: &str,
        new_name: &str,
    ) -> OperationSnapshot {
        let queue = OperationQueue::new();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let job_id = id.to_string();
        let job_name = new_name.to_string();
        let operation = queue
            .enqueue(OperationKind::Rename, move |context| {
                rename_installation(&job_repo, &job_store, &job_id, &job_name, &context)
            })
            .unwrap();
        wait_terminal(&queue, &operation)
    }

    fn run_delete(
        repo: &CentralRepo,
        store: &Arc<Store>,
        ids: &[&str],
        confirm_modified: bool,
    ) -> OperationSnapshot {
        let queue = OperationQueue::new();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let job_ids = ids.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
        let operation = queue
            .enqueue(OperationKind::Delete, move |context| {
                delete_installations(&job_repo, &job_store, &job_ids, confirm_modified, &context)
            })
            .unwrap();
        wait_terminal(&queue, &operation)
    }

    fn run_restore(repo: &CentralRepo, store: &Arc<Store>, ids: &[&str]) -> OperationSnapshot {
        let queue = OperationQueue::new();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let job_ids = ids.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
        let operation = queue
            .enqueue(OperationKind::Restore, move |context| {
                restore_trash(&job_repo, &job_store, &job_ids, &context)
            })
            .unwrap();
        wait_terminal(&queue, &operation)
    }

    fn run_purge(repo: &CentralRepo, store: &Arc<Store>, ids: &[&str]) -> OperationSnapshot {
        let queue = OperationQueue::new();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let job_ids = ids.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
        let operation = queue
            .enqueue(OperationKind::PurgeTrash, move |context| {
                purge_trash(&job_repo, &job_store, &job_ids, &context)
            })
            .unwrap();
        wait_terminal(&queue, &operation)
    }

    fn stored(store: &Store, id: &str) -> Installation {
        store
            .installation(id)
            .unwrap()
            .unwrap_or_else(|| panic!("installation {id} should exist"))
    }

    fn installation_path(repo: &CentralRepo, relative: &str) -> PathBuf {
        repo.installation_path(Category::Marketplace, relative)
            .unwrap()
    }

    fn wait_terminal(queue: &OperationQueue, id: &str) -> OperationSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(snapshot) = queue.snapshot(id)
                && matches!(
                    snapshot.state,
                    OperationState::Succeeded | OperationState::Failed | OperationState::Cancelled
                )
            {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "operation did not finish");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn run_install(
        repo: &CentralRepo,
        store: &Arc<Store>,
        source: SkillSource,
        downloaded: DownloadedSkill,
    ) -> OperationSnapshot {
        let queue = OperationQueue::new();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let id = queue
            .enqueue(OperationKind::Install, move |context| {
                install_with_download(&job_repo, &job_store, source, || Ok(downloaded), &context)
            })
            .unwrap();
        wait_terminal(&queue, &id)
    }

    #[test]
    fn fresh_install_commits_files_lineage_and_content_identity() {
        let (_temp, repo, store) = setup();
        let snapshot = run_install(
            &repo,
            &store,
            source("owner/repo"),
            downloaded(&repo, "demo", "fresh"),
        );

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let installation: Installation = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(installation.root_relpath, "demo");
        assert_eq!(installation.local_state, LocalState::Clean);
        assert_eq!(installation.installed_revision, Some("a".repeat(40)));
        assert_eq!(
            installation.source.as_ref().unwrap().digest.as_deref(),
            Some("sha256:test")
        );
        assert_eq!(installation.baseline_hash, installation.current_hash);
        let path = repo
            .installation_path(Category::Marketplace, "demo")
            .unwrap();
        assert!(path.join("SKILL.md").is_file());
        assert_eq!(
            store.installation(&installation.id).unwrap(),
            Some(installation)
        );
        assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
    }

    #[test]
    fn same_lineage_conflicts_before_download_without_overwriting_files() {
        let (_temp, repo, store) = setup();
        let existing = add_existing(
            &repo,
            &store,
            "existing",
            source("owner/repo"),
            "demo",
            "original",
        );
        let queue = OperationQueue::new();
        let called = Arc::new(AtomicBool::new(false));
        let job_called = called.clone();
        let job_repo = repo.clone();
        let job_store = store.clone();
        let id = queue
            .enqueue(OperationKind::Install, move |context| {
                install_with_download(
                    &job_repo,
                    &job_store,
                    source("owner/repo"),
                    || {
                        job_called.store(true, Ordering::Release);
                        Err(AppError::internal("download should not run"))
                    },
                    &context,
                )
            })
            .unwrap();
        let snapshot = wait_terminal(&queue, &id);

        assert!(!called.load(Ordering::Acquire));
        let error = snapshot.error.unwrap();
        assert_eq!(error.code, ErrorCode::Conflict);
        assert_eq!(error.details.unwrap()["installationId"], "existing");
        let path = repo
            .installation_path(Category::Marketplace, "demo")
            .unwrap();
        assert!(files_identical(&path, &path).unwrap());
        assert_eq!(store.installation("existing").unwrap(), Some(existing));
    }

    #[test]
    fn identical_content_from_another_source_reuses_the_existing_record() {
        let (_temp, repo, store) = setup();
        let existing = add_existing(
            &repo,
            &store,
            "existing",
            source("owner/repo"),
            "demo",
            "same",
        );
        let snapshot = run_install(
            &repo,
            &store,
            source("other/repo"),
            downloaded(&repo, "demo", "same"),
        );

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let reused: Installation = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(reused, existing);
        assert_eq!(
            store
                .list_installations(Category::Marketplace)
                .unwrap()
                .len(),
            1
        );
        assert!(
            !repo
                .installation_path(Category::Marketplace, "@other/repo/demo")
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn same_name_with_different_content_uses_a_scoped_real_path() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "existing",
            source("owner/repo"),
            "demo",
            "one",
        );
        let snapshot = run_install(
            &repo,
            &store,
            source("other/repo"),
            downloaded(&repo, "demo", "two"),
        );

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let installation: Installation = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(installation.root_relpath, "@other/repo/demo");
        assert!(
            repo.installation_path(Category::Marketplace, "@other/repo/demo")
                .unwrap()
                .join("SKILL.md")
                .is_file()
        );
        assert_eq!(
            store
                .list_installations(Category::Marketplace)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn an_untracked_existing_target_is_never_overwritten() {
        let (_temp, repo, store) = setup();
        let target = repo
            .installation_path(Category::Marketplace, "demo")
            .unwrap();
        write_skill(&target, "demo", "user files");
        fs::write(target.join("sentinel.txt"), "keep").unwrap();

        let snapshot = run_install(
            &repo,
            &store,
            source("owner/repo"),
            downloaded(&repo, "demo", "download"),
        );

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::Conflict);
        assert_eq!(
            fs::read_to_string(target.join("sentinel.txt")).unwrap(),
            "keep"
        );
        assert!(
            store
                .list_installations(Category::Marketplace)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn database_failure_rolls_the_moved_directory_back_to_staging() {
        let (_temp, repo, store) = setup();
        let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_install BEFORE INSERT ON installations
                 BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END;",
            )
            .unwrap();
        drop(connection);

        let snapshot = run_install(
            &repo,
            &store,
            source("owner/repo"),
            downloaded(&repo, "demo", "download"),
        );

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::Database);
        assert!(
            !repo
                .installation_path(Category::Marketplace, "demo")
                .unwrap()
                .exists()
        );
        assert!(
            store
                .list_installations(Category::Marketplace)
                .unwrap()
                .is_empty()
        );
        assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
    }

    #[test]
    fn detail_reports_clean_modified_and_missing_local_state() {
        let (_temp, repo, store) = setup();
        let installation = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "original",
        );
        let baseline = installation.baseline_hash.clone();

        let detail = installation_detail(&repo, &store, "one").unwrap();
        assert_eq!(detail.local.state, LocalState::Clean);
        assert_eq!(detail.installation.local_state, LocalState::Clean);
        assert_eq!(detail.local.metadata.as_ref().unwrap().name, "demo");
        assert!(
            detail
                .local
                .skill_md
                .as_ref()
                .unwrap()
                .contains("description: Demo skill")
        );
        assert_eq!(detail.local.file_count, Some(1));
        assert!(detail.local.total_bytes.unwrap() > 0);
        assert!(detail.local.path.ends_with("demo"));

        let path = installation_path(&repo, "demo");
        fs::write(
            path.join("SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\nchanged",
        )
        .unwrap();
        let detail = installation_detail(&repo, &store, "one").unwrap();
        assert_eq!(detail.local.state, LocalState::Modified);
        assert_ne!(detail.installation.current_hash, baseline);
        assert_eq!(detail.installation.baseline_hash, baseline);
        assert_eq!(detail.installation.local_state, LocalState::Modified);

        fs::remove_dir_all(&path).unwrap();
        let detail = installation_detail(&repo, &store, "one").unwrap();
        assert_eq!(detail.local.state, LocalState::Missing);
        assert!(detail.local.skill_md.is_none());
        assert!(detail.local.metadata.is_none());
        assert_eq!(detail.local.file_count, None);
        assert_eq!(detail.installation.current_hash, None);
        assert_eq!(detail.installation.baseline_hash, baseline);
        assert_eq!(detail.installation.local_state, LocalState::Missing);
    }

    #[test]
    fn detail_rejects_paths_that_escape_the_central_repository() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "original",
        );
        store
            .update_installation_rename("one", "demo", "../escape", "../escape", 2)
            .unwrap();

        let error = installation_detail(&repo, &store, "one").unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsafePath);
    }

    #[cfg(unix)]
    #[test]
    fn detail_rejects_symbolic_linked_installation_roots() {
        use std::os::unix::fs::symlink;

        let (temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "original",
        );
        let path = installation_path(&repo, "demo");
        let outside = temp.path().join("outside");
        write_skill(&outside, "demo", "outside");
        fs::remove_dir_all(&path).unwrap();
        symlink(&outside, &path).unwrap();

        let error = installation_detail(&repo, &store, "one").unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsafePath);
    }

    #[test]
    fn detail_limits_the_skill_document_size() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "original",
        );
        let path = installation_path(&repo, "demo");
        fs::write(
            path.join("SKILL.md"),
            vec![b'a'; MAX_LOCAL_SKILL_MD_BYTES as usize + 1],
        )
        .unwrap();

        let detail = installation_detail(&repo, &store, "one").unwrap();
        assert_eq!(detail.local.state, LocalState::Modified);
        assert!(detail.local.skill_md.is_none());
        assert!(detail.local.metadata.is_none());
        assert_eq!(
            detail.local.skill_md_error.unwrap().code,
            ErrorCode::InvalidSkill
        );
    }

    #[test]
    fn rename_moves_the_directory_and_updates_identity_fields() {
        let (_temp, repo, store) = setup();
        let original = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );

        let snapshot = run_rename(&repo, &store, "one", "renamed");

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let renamed: Installation = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(renamed.id, original.id);
        assert_eq!(renamed.display_name, "renamed");
        assert_eq!(renamed.root_relpath, "renamed");
        assert_eq!(renamed.root_key, "renamed");
        assert_eq!(renamed.source, original.source);
        assert_eq!(renamed.baseline_hash, original.baseline_hash);
        assert_eq!(renamed.installed_revision, original.installed_revision);
        assert!(!installation_path(&repo, "demo").exists());
        assert!(
            installation_path(&repo, "renamed")
                .join("SKILL.md")
                .is_file()
        );
        assert_eq!(stored(&store, "one").display_name, "renamed");
    }

    #[test]
    fn rename_keeps_the_scoped_parent_and_replaces_the_last_component() {
        let (_temp, repo, store) = setup();
        add_scoped_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "@owner/repo/demo",
        );

        let snapshot = run_rename(&repo, &store, "one", "renamed");

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let renamed = stored(&store, "one");
        assert_eq!(renamed.display_name, "renamed");
        assert_eq!(renamed.root_relpath, "@owner/repo/renamed");
        assert_eq!(renamed.root_key, root_key("@owner/repo/renamed"));
        assert!(
            installation_path(&repo, "@owner/repo/renamed")
                .join("SKILL.md")
                .is_file()
        );
        assert!(!installation_path(&repo, "@owner/repo/demo").exists());
    }

    #[test]
    fn case_only_rename_uses_a_safe_intermediate_path() {
        let (_temp, repo, store) = setup();
        let original = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );

        let snapshot = run_rename(&repo, &store, "one", "DEMO");

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let renamed = stored(&store, "one");
        assert_eq!(renamed.display_name, "DEMO");
        assert_eq!(renamed.root_relpath, "DEMO");
        let root = repo.skills_root(Category::Marketplace);
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .any(|entry| entry.unwrap().file_name() == "DEMO")
        );
        let path = installation_path(&repo, "DEMO");
        assert!(path.join("SKILL.md").is_file());
        assert_eq!(
            content_hash(&path).unwrap(),
            original.baseline_hash.unwrap()
        );
    }

    #[test]
    fn renaming_to_the_current_name_is_a_no_op() {
        let (_temp, repo, store) = setup();
        let original = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );

        let snapshot = run_rename(&repo, &store, "one", "demo");

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let result: Installation = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(result, original);
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
    }

    #[test]
    fn rename_rejects_unsafe_names_without_touching_files() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );

        let snapshot = run_rename(&repo, &store, "one", "bad/name");

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::InvalidInput);
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert_eq!(stored(&store, "one").root_relpath, "demo");
    }

    #[test]
    fn rename_conflicts_when_the_target_root_key_is_taken() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        add_existing(
            &repo,
            &store,
            "two",
            source("owner/other"),
            "other",
            "content",
        );

        let snapshot = run_rename(&repo, &store, "one", "other");

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::Conflict);
        assert_eq!(stored(&store, "one").root_relpath, "demo");
        assert_eq!(stored(&store, "two").root_relpath, "other");
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert!(installation_path(&repo, "other").join("SKILL.md").is_file());
    }

    #[test]
    fn rename_fails_when_the_local_directory_is_missing() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        fs::remove_dir_all(installation_path(&repo, "demo")).unwrap();

        let snapshot = run_rename(&repo, &store, "one", "renamed");

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::NotFound);
        assert_eq!(stored(&store, "one").root_relpath, "demo");
        assert!(!installation_path(&repo, "renamed").exists());
    }

    #[test]
    fn rename_rolls_the_directory_back_when_the_database_update_fails() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_rename BEFORE UPDATE ON installations
                 BEGIN SELECT RAISE(ABORT, 'forced update failure'); END;",
            )
            .unwrap();
        drop(connection);

        let snapshot = run_rename(&repo, &store, "one", "renamed");

        assert_eq!(snapshot.state, OperationState::Failed);
        assert_eq!(snapshot.error.unwrap().code, ErrorCode::Database);
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert!(!installation_path(&repo, "renamed").exists());
        assert_eq!(stored(&store, "one").root_relpath, "demo");
    }

    #[test]
    fn a_cancelled_rename_leaves_files_and_records_untouched() {
        let (_temp, repo, store) = setup();
        let original = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        let context = crate::core::operations::test_context(true);

        let error = rename_installation(&repo, &store, "one", "renamed", &context).unwrap_err();

        assert_eq!(error.code, ErrorCode::Cancelled);
        assert_eq!(store.installation("one").unwrap(), Some(original));
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert!(!installation_path(&repo, "renamed").exists());
    }

    #[test]
    fn delete_moves_files_to_trash_and_removes_the_record() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );

        let snapshot = run_delete(&repo, &store, &["one"], false);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert!(!report.cancelled);
        assert!(report.failed.is_empty());
        assert_eq!(report.deleted.len(), 1);
        assert!(report.deleted[0].trashed);
        assert!(store.installation("one").unwrap().is_none());
        assert!(!installation_path(&repo, "demo").exists());

        let entries = store.trash_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].installation_id, "one");
        assert_eq!(entries[0].original_relpath, "demo");
        assert_eq!(entries[0].display_name, "demo");
        assert!(
            repo.trash_path(&entries[0].trash_relpath)
                .unwrap()
                .join("SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn delete_requires_confirmation_before_discarding_local_changes() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        let path = installation_path(&repo, "demo");
        fs::write(
            path.join("SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\nedited",
        )
        .unwrap();

        let snapshot = run_delete(&repo, &store, &["one"], false);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert!(report.deleted.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].error.code, ErrorCode::LocalModified);
        assert!(path.join("SKILL.md").is_file());
        assert_eq!(stored(&store, "one").local_state, LocalState::Modified);
        assert!(store.trash_entries().unwrap().is_empty());

        let snapshot = run_delete(&repo, &store, &["one"], true);

        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(report.deleted.len(), 1);
        assert!(report.failed.is_empty());
        assert!(store.installation("one").unwrap().is_none());
        let entries = store.trash_entries().unwrap();
        assert_eq!(entries[0].installation.local_state, LocalState::Modified);
    }

    #[test]
    fn batch_delete_continues_after_a_failing_item() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        add_existing(
            &repo,
            &store,
            "two",
            source("owner/other"),
            "other",
            "content",
        );

        let snapshot = run_delete(&repo, &store, &["one", "missing", "two"], false);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(report.deleted.len(), 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].id, "missing");
        assert_eq!(report.failed[0].error.code, ErrorCode::NotFound);
        assert!(store.installation("one").unwrap().is_none());
        assert!(store.installation("two").unwrap().is_none());
        assert_eq!(store.trash_entries().unwrap().len(), 2);
    }

    #[test]
    fn delete_rolls_the_directory_back_when_the_database_transaction_fails() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_trash BEFORE INSERT ON trash
                 BEGIN SELECT RAISE(ABORT, 'forced trash failure'); END;",
            )
            .unwrap();
        drop(connection);

        let snapshot = run_delete(&repo, &store, &["one"], false);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert!(report.deleted.is_empty());
        assert_eq!(report.failed[0].error.code, ErrorCode::Database);
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert_eq!(stored(&store, "one").root_relpath, "demo");
        assert!(store.trash_entries().unwrap().is_empty());
        assert_eq!(fs::read_dir(repo.trash_dir()).unwrap().count(), 0);
    }

    #[test]
    fn delete_of_missing_files_only_removes_the_record() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        fs::remove_dir_all(installation_path(&repo, "demo")).unwrap();

        let snapshot = run_delete(&repo, &store, &["one"], false);

        let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(report.deleted.len(), 1);
        assert!(!report.deleted[0].trashed);
        assert!(store.installation("one").unwrap().is_none());
        assert!(store.trash_entries().unwrap().is_empty());
    }

    #[test]
    fn a_cancelled_delete_processes_nothing() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        let context = crate::core::operations::test_context(true);
        let ids = vec!["one".to_string()];

        let error = delete_installations(&repo, &store, &ids, true, &context).unwrap_err();

        assert_eq!(error.code, ErrorCode::Cancelled);
        assert!(store.installation("one").unwrap().is_some());
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert!(store.trash_entries().unwrap().is_empty());
    }

    #[test]
    fn restore_moves_files_back_and_recreates_the_record() {
        let (_temp, repo, store) = setup();
        let original = add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        run_delete(&repo, &store, &["one"], false);
        let trash_id = store.trash_entries().unwrap()[0].id.clone();

        let snapshot = run_restore(&repo, &store, &[trash_id.as_str()]);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: RestoreReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(report.restored.len(), 1);
        assert!(report.failed.is_empty());
        let restored = stored(&store, "one");
        assert_eq!(restored.display_name, "demo");
        assert_eq!(restored.root_relpath, "demo");
        assert_eq!(restored.baseline_hash, original.baseline_hash);
        assert_eq!(restored.source, original.source);
        assert!(installation_path(&repo, "demo").join("SKILL.md").is_file());
        assert!(store.trash_entries().unwrap().is_empty());
        assert_eq!(fs::read_dir(repo.trash_dir()).unwrap().count(), 0);
    }

    #[test]
    fn restore_conflicts_when_the_target_path_is_taken() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        run_delete(&repo, &store, &["one"], false);
        let trash_id = store.trash_entries().unwrap()[0].id.clone();
        let path = installation_path(&repo, "demo");
        write_skill(&path, "demo", "untracked");

        let snapshot = run_restore(&repo, &store, &[trash_id.as_str()]);

        let report: RestoreReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert!(report.restored.is_empty());
        assert_eq!(report.failed[0].error.code, ErrorCode::Conflict);
        assert!(store.installation("one").unwrap().is_none());
        assert_eq!(store.trash_entries().unwrap().len(), 1);
        assert!(
            fs::read_to_string(path.join("SKILL.md"))
                .unwrap()
                .contains("untracked")
        );
    }

    #[test]
    fn restore_keeps_files_in_trash_when_the_database_insert_fails() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        run_delete(&repo, &store, &["one"], false);
        let trash_id = store.trash_entries().unwrap()[0].id.clone();
        let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_restore BEFORE INSERT ON installations
                 BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END;",
            )
            .unwrap();
        drop(connection);

        let snapshot = run_restore(&repo, &store, &[trash_id.as_str()]);

        let report: RestoreReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert!(report.restored.is_empty());
        assert_eq!(report.failed[0].error.code, ErrorCode::Database);
        assert!(store.installation("one").unwrap().is_none());
        assert_eq!(store.trash_entries().unwrap().len(), 1);
        assert!(!installation_path(&repo, "demo").exists());
        assert!(
            repo.trash_path(&store.trash_entries().unwrap()[0].trash_relpath)
                .unwrap()
                .join("SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn purge_permanently_removes_trash_files_and_records() {
        let (_temp, repo, store) = setup();
        add_existing(
            &repo,
            &store,
            "one",
            source("owner/repo"),
            "demo",
            "content",
        );
        run_delete(&repo, &store, &["one"], false);
        let entry = store.trash_entries().unwrap().remove(0);
        let trash_path = repo.trash_path(&entry.trash_relpath).unwrap();

        let snapshot = run_purge(&repo, &store, &[entry.id.as_str()]);

        assert_eq!(snapshot.state, OperationState::Succeeded);
        let report: PurgeReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
        assert_eq!(report.purged.len(), 1);
        assert_eq!(report.purged[0], entry.id);
        assert!(report.failed.is_empty());
        assert!(store.trash_entries().unwrap().is_empty());
        assert!(!trash_path.exists());
        assert!(!installation_path(&repo, "demo").exists());
    }
}
