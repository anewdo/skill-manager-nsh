pub(super) use std::fs;
use std::path::{Path, PathBuf};
pub(super) use std::sync::Arc;

use tempfile::TempDir;

pub(super) use crate::core::central_repo::CentralRepo;
pub(super) use crate::core::content_identity::content_hash;
pub(super) use crate::core::error::{AppError, ErrorCode};
use crate::core::library::common::last_component;
use crate::core::metadata::{SkillCandidate, SkillMetadata};
pub(super) use crate::core::model::{
    DeleteReport, InstalledSkill, LocalState, ProxySettings, SkillSource, UpdateConflictPolicy,
    UpdateState,
};
use crate::core::naming::root_key;
pub(super) use crate::core::sources::common::DownloadedSkill;
use crate::core::sources::skills_sh;
pub(super) use crate::core::store::Store;

pub(super) fn setup() -> (TempDir, CentralRepo, Arc<Store>) {
    let temp = tempfile::tempdir().unwrap();
    let repo = CentralRepo::new(temp.path().join("repo"));
    let store = Arc::new(Store::open(&repo).unwrap());
    (temp, repo, store)
}

pub(super) fn source(repository: &str) -> SkillSource {
    skills_sh::marketplace_source(repository, "skill").unwrap()
}

pub(super) fn write_skill(root: &Path, name: &str, content: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Demo skill\n---\n{content}"),
    )
    .unwrap();
}

pub(super) fn downloaded(repo: &CentralRepo, name: &str, content: &str) -> DownloadedSkill {
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

pub(super) fn add_existing(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    source: SkillSource,
    root_relpath: &str,
    content: &str,
) -> InstalledSkill {
    let path = repo.skill_path(root_relpath).unwrap();
    let name = last_component(root_relpath);
    write_skill(&path, name, content);
    let hash = content_hash(&path).unwrap();
    let skill = InstalledSkill {
        id: id.to_string(),
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
    store.insert_skill(&skill).unwrap();
    skill
}

pub(super) fn stored(store: &Store, id: &str) -> InstalledSkill {
    store
        .skill(id)
        .unwrap()
        .unwrap_or_else(|| panic!("skill {id} should exist"))
}

pub(super) fn skill_path(repo: &CentralRepo, relative: &str) -> PathBuf {
    repo.skill_path(relative).unwrap()
}

pub(super) fn staging_entries(repo: &CentralRepo) -> Vec<PathBuf> {
    fs::read_dir(repo.staging_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}
