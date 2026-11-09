use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::core::metadata::{SkillCandidate, SkillMetadata};
use crate::core::model::{OperationKind, OperationSnapshot};
use crate::core::operations::OperationQueue;
use crate::core::sources::skills_sh;
use crate::core::test_support::wait_terminal;

mod delete;
mod detail;
mod install;
mod rename;
mod update;

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
    root_relpath: &str,
    content: &str,
) -> Installation {
    let path = repo
        .installation_path(Category::Marketplace, root_relpath)
        .unwrap();
    let name = last_component(root_relpath);
    write_skill(&path, name, content);
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
