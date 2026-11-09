use super::*;
use crate::core::model::OperationState;

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
