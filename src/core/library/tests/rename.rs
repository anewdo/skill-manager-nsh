use super::common::*;
use crate::core::library::manage::rename_skill;
use crate::core::model::{OperationKind, OperationSnapshot, OperationState};
use crate::core::naming::root_key;
use crate::core::operations::OperationQueue;
use crate::core::test_support::wait_terminal;

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
            rename_skill(&job_repo, &job_store, &job_id, &job_name, &context)
        })
        .unwrap();
    wait_terminal(&queue, &operation)
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
    let renamed: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(renamed.id, original.id);
    assert_eq!(renamed.display_name, "renamed");
    assert_eq!(renamed.root_relpath, "renamed");
    assert_eq!(renamed.root_key, "renamed");
    assert_eq!(renamed.source, original.source);
    assert_eq!(renamed.baseline_hash, original.baseline_hash);
    assert_eq!(renamed.installed_revision, original.installed_revision);
    assert!(!skill_path(&repo, "demo").exists());
    assert!(skill_path(&repo, "renamed").join("SKILL.md").is_file());
    assert_eq!(stored(&store, "one").display_name, "renamed");
}

#[test]
fn rename_replaces_only_the_last_component_of_a_nested_path() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "@owner/repo/demo",
        "nested",
    );

    let snapshot = run_rename(&repo, &store, "one", "renamed");

    assert_eq!(snapshot.state, OperationState::Succeeded);
    let renamed = stored(&store, "one");
    assert_eq!(renamed.display_name, "renamed");
    assert_eq!(renamed.root_relpath, "@owner/repo/renamed");
    assert_eq!(renamed.root_key, root_key("@owner/repo/renamed"));
    assert!(
        skill_path(&repo, "@owner/repo/renamed")
            .join("SKILL.md")
            .is_file()
    );
    assert!(!skill_path(&repo, "@owner/repo/demo").exists());
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
    let root = repo.skills_root();
    assert!(
        fs::read_dir(&root)
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == "DEMO")
    );
    let path = skill_path(&repo, "DEMO");
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
    let result: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(result, original);
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
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
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
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
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
    assert!(skill_path(&repo, "other").join("SKILL.md").is_file());
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
    fs::remove_dir_all(skill_path(&repo, "demo")).unwrap();

    let snapshot = run_rename(&repo, &store, "one", "renamed");

    assert_eq!(snapshot.state, OperationState::Failed);
    assert_eq!(snapshot.error.unwrap().code, ErrorCode::NotFound);
    assert_eq!(stored(&store, "one").root_relpath, "demo");
    assert!(!skill_path(&repo, "renamed").exists());
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
            "CREATE TRIGGER reject_rename BEFORE UPDATE ON skills
             BEGIN SELECT RAISE(ABORT, 'forced update failure'); END;",
        )
        .unwrap();
    drop(connection);

    let snapshot = run_rename(&repo, &store, "one", "renamed");

    assert_eq!(snapshot.state, OperationState::Failed);
    assert_eq!(snapshot.error.unwrap().code, ErrorCode::Database);
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
    assert!(!skill_path(&repo, "renamed").exists());
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

    let error = rename_skill(&repo, &store, "one", "renamed", &context).unwrap_err();

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(store.skill("one").unwrap(), Some(original));
    assert!(skill_path(&repo, "demo").join("SKILL.md").is_file());
    assert!(!skill_path(&repo, "renamed").exists());
}
