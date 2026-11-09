use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::core::model::OperationState;

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
    let content = fs::read_to_string(installation_path(&repo, "demo").join("SKILL.md")).unwrap();
    assert!(content.contains("original"));
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
