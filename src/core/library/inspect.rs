use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::common::skill_not_found;
use crate::core::central_repo::CentralRepo;
use crate::core::content_identity::{ContentSummary, content_summary};
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata;
use crate::core::model::{InstalledSkill, LocalSkillInfo, LocalState, SkillDetail};
use crate::core::store::Store;

pub(super) const MAX_LOCAL_SKILL_MD_BYTES: u64 = 1024 * 1024;

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

pub(super) struct LocalInspection {
    pub(super) path: PathBuf,
    pub(super) state: LocalState,
    pub(super) summary: Option<ContentSummary>,
}

pub(super) fn inspect_local(repo: &CentralRepo, skill: &InstalledSkill) -> Result<LocalInspection> {
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
