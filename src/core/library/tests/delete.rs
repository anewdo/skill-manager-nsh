use super::common::*;
use crate::core::library::manage::delete_skills;
use crate::core::model::{OperationKind, OperationSnapshot, OperationState};
use crate::core::operations::OperationQueue;
use crate::core::test_support::wait_terminal;

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
            delete_skills(&job_repo, &job_store, &job_ids, confirm_modified, &context)
        })
        .unwrap();
    wait_terminal(&queue, &operation)
}

#[test]
fn delete_removes_the_local_copy_and_the_record_immediately() {
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
    assert_eq!(report.deleted[0].id, "one");
    assert_eq!(report.deleted[0].display_name, "demo");
    assert!(store.skill("one").unwrap().is_none());
    assert!(!skill_path(&repo, "demo").exists());
    assert!(staging_entries(&repo).is_empty());
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
    let path = skill_path(&repo, "demo");
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
    assert!(staging_entries(&repo).is_empty());

    let snapshot = run_delete(&repo, &store, &["one"], true);

    let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(report.deleted.len(), 1);
    assert!(report.failed.is_empty());
    assert!(store.skill("one").unwrap().is_none());
    assert!(!path.exists());
    assert!(staging_entries(&repo).is_empty());
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
    assert!(store.skill("one").unwrap().is_none());
    assert!(store.skill("two").unwrap().is_none());
    assert!(!skill_path(&repo, "demo").exists());
    assert!(!skill_path(&repo, "other").exists());
}

#[test]
fn delete_restores_the_directory_when_the_database_delete_fails() {
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
            "CREATE TRIGGER reject_delete BEFORE DELETE ON skills
             BEGIN SELECT RAISE(ABORT, 'forced delete failure'); END;",
        )
        .unwrap();
    drop(connection);

    let snapshot = run_delete(&repo, &store, &["one"], false);

    let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert!(report.deleted.is_empty());
    assert_eq!(report.failed[0].error.code, ErrorCode::Database);
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
    assert_eq!(stored(&store, "one").root_relpath, "demo");
    assert!(staging_entries(&repo).is_empty());
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
    fs::remove_dir_all(skill_path(&repo, "demo")).unwrap();

    let snapshot = run_delete(&repo, &store, &["one"], false);

    let report: DeleteReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(report.deleted.len(), 1);
    assert!(store.skill("one").unwrap().is_none());
    assert!(staging_entries(&repo).is_empty());
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

    let error = delete_skills(&repo, &store, &ids, true, &context).unwrap_err();

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(store.skill("one").unwrap().is_some());
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
    assert!(staging_entries(&repo).is_empty());
}
