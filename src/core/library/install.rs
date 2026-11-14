use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::common::{
    cleanup_parent_directories, create_parent_directories, ensure_target_missing, progress,
    target_conflict,
};
use crate::core::central_repo::CentralRepo;
use crate::core::content_identity::{content_hash, files_identical};
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{
    InstalledSkill, LocalState, OperationProgress, ProxySettings, SkillSource, UpdateState,
};
use crate::core::naming::{allocate_name, root_key};
use crate::core::operations::OperationContext;
use crate::core::sources::common::{DownloadedSkill, canonical_source, download};
use crate::core::store::Store;

/// An install result plus a note about a same-named copy the app does not manage.
#[derive(Debug, Serialize)]
pub struct InstallOutcome {
    #[serde(flatten)]
    pub(super) skill: InstalledSkill,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unmanaged_copy: Option<String>,
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

pub(super) fn install_with_download(
    repo: &CentralRepo,
    store: &Store,
    source: SkillSource,
    download: impl FnOnce() -> Result<DownloadedSkill>,
    context: &OperationContext,
) -> Result<(InstalledSkill, Option<String>)> {
    if source.subpath.is_some() {
        clear_missing_lineage(repo, store, &source, context)?;
    }
    context.progress(progress("downloading", true))?;
    let downloaded = download()?;
    let source = if source.source_type == crate::core::model::SourceType::GitHub
        && source.git_ref.is_none()
    {
        let mut resolved = crate::core::sources::github::input::source(
            source.repository.as_deref().unwrap(),
            None,
            if downloaded.relative_root == "." {
                ""
            } else {
                &downloaded.relative_root
            },
        )?;
        resolved.marketplace_skill_id = source.marketplace_skill_id;
        resolved
    } else {
        source
    };
    clear_missing_lineage(repo, store, &source, context)?;
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
