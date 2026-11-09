use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use super::central_repo::CentralRepo;
use super::content_identity::{content_hash, files_identical};
use super::error::{AppError, ErrorCode, Result};
use super::model::{
    Category, Installation, LocalState, OperationProgress, ProxySettings, SkillSource, SourceType,
    UpdateState,
};
use super::naming::{allocate_marketplace_name, root_key};
use super::operations::OperationContext;
use super::sources::{DownloadedSkill, download, preview::canonical_source};
use super::store::Store;

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
}
