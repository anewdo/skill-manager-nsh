use super::*;

fn read_skill_md(repo: &CentralRepo, relative: &str) -> String {
    fs::read_to_string(installation_path(repo, relative).join("SKILL.md")).unwrap()
}

#[test]
fn check_updates_compares_content_and_reports_each_item() {
    let (_temp, repo, store) = setup();
    add_existing(&repo, &store, "one", source("owner/one"), "demo", "content");
    add_existing(
        &repo,
        &store,
        "two",
        source("owner/two"),
        "other",
        "content",
    );
    add_existing(
        &repo,
        &store,
        "three",
        source("owner/three"),
        "third",
        "content",
    );
    let ids = vec!["one".to_string(), "two".to_string(), "three".to_string()];

    let report = check_updates_with_download(
        &store,
        &ids,
        |id| match id {
            "one" => Ok(downloaded(&repo, "demo", "content")),
            "two" => Ok(downloaded(&repo, "other", "changed")),
            _ => Err(AppError::new(ErrorCode::Network, "offline")),
        },
        &crate::core::operations::test_context(false),
    )
    .unwrap();

    assert!(!report.cancelled);
    assert_eq!(report.checked.len(), 2);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].id, "three");
    assert_eq!(report.failed[0].error.code, ErrorCode::Network);
    assert_eq!(report.checked[0].update_state, UpdateState::UpToDate);
    assert_eq!(
        report.checked[0].remote_revision.as_deref(),
        Some("a".repeat(40).as_str())
    );
    assert_eq!(report.checked[1].update_state, UpdateState::Available);

    assert_eq!(stored(&store, "one").update_state, UpdateState::UpToDate);
    assert_eq!(stored(&store, "two").update_state, UpdateState::Available);
    let failed = stored(&store, "three");
    assert_eq!(failed.update_state, UpdateState::CheckFailed);
    assert_eq!(failed.last_error.as_deref(), Some("offline"));
    assert_eq!(failed.installed_revision, Some("b".repeat(40)));
    assert!(read_skill_md(&repo, "demo").contains("content"));
}

#[test]
fn check_updates_keeps_the_previous_remote_revision_on_failure() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    store
        .update_update_check("one", Some("previous"), UpdateState::Available, 1, None)
        .unwrap();

    check_update_with_download(&store, "one", || {
        Err(AppError::new(ErrorCode::Network, "timeout"))
    })
    .unwrap_err();

    let failed = stored(&store, "one");
    assert_eq!(failed.remote_revision.as_deref(), Some("previous"));
    assert_eq!(failed.update_state, UpdateState::CheckFailed);
    assert_eq!(failed.last_error.as_deref(), Some("timeout"));
}

#[test]
fn check_updates_is_cancelled_before_any_download() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    let ids = vec!["one".to_string()];

    let error = check_updates(
        &repo,
        &store,
        &ids,
        &ProxySettings::default(),
        &crate::core::operations::test_context(true),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(stored(&store, "one").update_state, UpdateState::Unknown);
    assert!(read_skill_md(&repo, "demo").contains("content"));
}

#[test]
fn apply_update_replaces_files_and_advances_the_revision() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    let context = crate::core::operations::test_context(false);

    let updated = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Ok(downloaded(&repo, "demo", "updated")),
        &context,
    )
    .unwrap();

    assert_eq!(updated.id, "one");
    assert_eq!(updated.display_name, "demo");
    assert_eq!(updated.root_relpath, "demo");
    assert_eq!(updated.installed_revision, Some("a".repeat(40)));
    assert_eq!(updated.remote_revision, Some("a".repeat(40)));
    assert_eq!(updated.local_state, LocalState::Clean);
    assert_eq!(updated.update_state, UpdateState::UpToDate);
    assert!(updated.baseline_hash.is_some());
    assert!(read_skill_md(&repo, "demo").contains("updated"));
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_without_content_changes_keeps_the_local_copy() {
    let (_temp, repo, store) = setup();
    let before = add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    let context = crate::core::operations::test_context(false);

    let updated = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Ok(downloaded(&repo, "demo", "content")),
        &context,
    )
    .unwrap();

    assert_eq!(updated.installed_revision, Some("a".repeat(40)));
    assert_eq!(updated.baseline_hash, before.baseline_hash);
    assert_eq!(updated.current_hash, before.current_hash);
    assert_eq!(updated.update_state, UpdateState::UpToDate);
    assert!(read_skill_md(&repo, "demo").contains("content"));
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_rejects_local_changes_until_they_are_overwritten() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    fs::write(
        installation_path(&repo, "demo").join("SKILL.md"),
        "---\nname: demo\ndescription: Demo skill\n---\nedited",
    )
    .unwrap();
    let context = crate::core::operations::test_context(false);

    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Ok(downloaded(&repo, "demo", "updated")),
        &context,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::LocalModified);
    assert!(error.details.is_some());
    assert!(read_skill_md(&repo, "demo").contains("edited"));
    assert_eq!(stored(&store, "one").local_state, LocalState::Modified);
    assert_eq!(
        stored(&store, "one").installed_revision,
        Some("b".repeat(40))
    );

    let updated = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Overwrite,
        || Ok(downloaded(&repo, "demo", "updated")),
        &context,
    )
    .unwrap();

    assert!(read_skill_md(&repo, "demo").contains("updated"));
    assert_eq!(updated.local_state, LocalState::Clean);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_rechecks_local_changes_before_committing() {
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
    let context = crate::core::operations::test_context(false);

    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || {
            fs::write(
                path.join("SKILL.md"),
                "---\nname: demo\ndescription: Demo skill\n---\nedited during download",
            )
            .unwrap();
            Ok(downloaded(&repo, "demo", "updated"))
        },
        &context,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::LocalModified);
    assert!(read_skill_md(&repo, "demo").contains("edited during download"));
    assert_eq!(
        stored(&store, "one").installed_revision,
        Some("b".repeat(40))
    );
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_restores_the_previous_files_when_the_database_update_fails() {
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
            "CREATE TRIGGER reject_update BEFORE UPDATE ON installations
             BEGIN SELECT RAISE(ABORT, 'forced update failure'); END;",
        )
        .unwrap();
    drop(connection);
    let context = crate::core::operations::test_context(false);

    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Ok(downloaded(&repo, "demo", "updated")),
        &context,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Database);
    assert!(read_skill_md(&repo, "demo").contains("content"));
    assert_eq!(
        stored(&store, "one").installed_revision,
        Some("b".repeat(40))
    );
    assert_eq!(stored(&store, "one").local_state, LocalState::Clean);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_keeps_the_installation_when_the_download_fails() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "content",
    );
    let context = crate::core::operations::test_context(false);

    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Err(AppError::new(ErrorCode::Network, "offline")),
        &context,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Network);
    assert!(read_skill_md(&repo, "demo").contains("content"));
    assert_eq!(
        stored(&store, "one").installed_revision,
        Some("b".repeat(40))
    );
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_restores_missing_local_files() {
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
    let context = crate::core::operations::test_context(false);

    let updated = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Ok(downloaded(&repo, "demo", "updated")),
        &context,
    )
    .unwrap();

    assert!(read_skill_md(&repo, "demo").contains("updated"));
    assert_eq!(updated.local_state, LocalState::Clean);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn apply_update_is_cancelled_before_downloading() {
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

    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || Err(AppError::new(ErrorCode::Internal, "download must not run")),
        &context,
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(read_skill_md(&repo, "demo").contains("content"));
    assert_eq!(
        stored(&store, "one").installed_revision,
        Some("b".repeat(40))
    );
}
