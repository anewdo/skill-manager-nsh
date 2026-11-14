use super::common::*;
use crate::core::content_identity::content_hash;
use crate::core::model::{LocalState, OperationEventKind};

#[test]
fn imports_source_free_content_and_reuses_it_without_duplicate_events() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root, "demo", "原始内容\r\n");
    fs::write(root.join("binary.dat"), [0, 13, 10, 255]).unwrap();
    let before = fs::read(root.join("SKILL.md")).unwrap();
    let discovery = add(&session, &repo, &[&root]);
    let queue = OperationQueue::new();
    let mut events = queue.subscribe();
    let batch = session
        .prepare(&[ids(&discovery)[0].clone(), ids(&discovery)[0].clone()])
        .unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    let snapshot = wait_terminal(&queue, &id);
    let report: LocalImportReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].status, ImportStatus::Added);
    let skill = report.items[0].skill.as_ref().unwrap();
    assert!(skill.source.is_none());
    assert!(skill.installed_revision.is_none());
    assert_eq!(skill.local_state, LocalState::Clean);
    assert_eq!(skill.baseline_hash, skill.current_hash);
    assert_eq!(
        skill.baseline_hash.as_deref(),
        Some(content_hash(&root).unwrap().as_str())
    );
    assert_eq!(
        fs::read(repo.skill_path("demo").unwrap().join("binary.dat")).unwrap(),
        [0, 13, 10, 255]
    );
    assert_eq!(fs::read(root.join("SKILL.md")).unwrap(), before);
    let mut changes = 0;
    while let Ok(event) = events.try_recv() {
        if event.kind == OperationEventKind::LibraryChanged {
            changes += 1;
            assert!(store.skill(&skill.id).unwrap().is_some());
            assert!(repo.skill_path("demo").unwrap().exists());
        }
    }
    assert_eq!(changes, 1);
    assert!(session.snapshot().candidates.is_empty());
    let second = add(&session, &repo, &[&root]);
    let report = import(&session, &repo, &store, &ids(&second));
    assert_eq!(report.items[0].status, ImportStatus::Existing);
    assert_eq!(report.items[0].skill.as_ref().unwrap().id, skill.id);
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert!(staged(&repo).is_empty());
}

#[test]
fn identical_remote_content_preserves_its_source_and_different_content_gets_a_suffix() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root, "demo", "one");
    let discovery = add(&session, &repo, &[&root]);
    let first = import(&session, &repo, &store, &ids(&discovery));
    let mut existing = first.items[0].skill.clone().unwrap();
    store.delete_skill_record(&existing.id).unwrap();
    existing.source =
        Some(crate::core::sources::skills_sh::marketplace_source("owner/repo", "demo").unwrap());
    store.insert_skill(&existing).unwrap();
    let discovery = add(&session, &repo, &[&root]);
    let same = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(same.items[0].status, ImportStatus::Existing);
    assert_eq!(same.items[0].skill.as_ref().unwrap(), &existing);
    write_skill(&root, "demo", "two");
    let discovery = add(&session, &repo, &[&root]);
    let different = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(
        different.items[0].skill.as_ref().unwrap().root_relpath,
        "demo-2"
    );
    assert!(
        fs::read_to_string(repo.skill_path("demo").unwrap().join("SKILL.md"))
            .unwrap()
            .ends_with("one")
    );
    assert_eq!(store.list_skills().unwrap().len(), 2);
}

#[test]
fn metadata_changes_require_rescan_while_body_changes_use_current_files() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root, "demo", "one");
    let original = add(&session, &repo, &[&root]);
    write_skill(&root, "changed", "two");
    let failed = import(&session, &repo, &store, &ids(&original));
    assert_eq!(
        failed.items[0].error.as_ref().unwrap().code,
        ErrorCode::Conflict
    );
    assert!(store.list_skills().unwrap().is_empty());
    assert_eq!(ids(&session.snapshot()), ids(&original));
    let rescanned = session.rescan(&repo);
    assert_ne!(ids(&rescanned), ids(&original));
    assert!(session.prepare(&ids(&original)).is_err());
    write_skill(&root, "changed", "最新正文");
    let report = import(&session, &repo, &store, &ids(&rescanned));
    assert_eq!(report.items[0].status, ImportStatus::Added);
    assert!(
        fs::read_to_string(repo.skill_path("changed").unwrap().join("SKILL.md"))
            .unwrap()
            .ends_with("最新正文")
    );
    assert!(staged(&repo).is_empty());
}

#[test]
fn a_missing_root_fails_only_its_item_and_successful_candidates_are_removed() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root.join("a"), "missing", "one");
    write_skill(&root.join("b"), "valid", "two");
    let discovery = add(&session, &repo, &[&root]);
    fs::remove_dir_all(root.join("a")).unwrap();
    let report = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(report.items[0].status, ImportStatus::Failed);
    assert_eq!(report.items[1].status, ImportStatus::Added);
    assert_eq!(session.snapshot().candidates.len(), 1);
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert!(staged(&repo).is_empty());
}

#[test]
fn database_failure_rolls_back_one_item_continues_and_emits_only_for_commits() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root.join("a"), "bad", "one");
    write_skill(&root.join("b"), "good", "two");
    let discovery = add(&session, &repo, &[&root]);
    let _db = fail_insert(&repo);
    let queue = OperationQueue::new();
    let mut events = queue.subscribe();
    let batch = session.prepare(&ids(&discovery)).unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    let snapshot = wait_terminal(&queue, &id);
    let report: LocalImportReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(
        report.items[0].error.as_ref().unwrap().code,
        ErrorCode::Database
    );
    assert_eq!(report.items[1].status, ImportStatus::Added);
    assert!(!repo.skill_path("bad").unwrap().exists());
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert_eq!(session.snapshot().candidates[0].name, "bad");
    assert!(staged(&repo).is_empty());
    let mut changes = 0;
    while let Ok(event) = events.try_recv() {
        if event.kind == OperationEventKind::LibraryChanged {
            changes += 1;
        }
    }
    assert_eq!(changes, 1);
}

#[cfg(any(windows, unix))]
#[test]
fn a_candidate_root_replaced_by_a_link_cannot_be_imported() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    let outside = temp.path().join("outside");
    write_skill(&root, "demo", "one");
    write_skill(&outside, "external", "sentinel");
    let discovery = add(&session, &repo, &[&root]);
    fs::rename(&root, temp.path().join("original")).unwrap();
    directory_link(&outside, &root);
    let report = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(
        report.items[0].error.as_ref().unwrap().code,
        ErrorCode::UnsafePath
    );
    assert!(store.list_skills().unwrap().is_empty());
    assert!(
        fs::read_to_string(outside.join("SKILL.md"))
            .unwrap()
            .ends_with("sentinel")
    );
    assert!(staged(&repo).is_empty());
}

#[test]
fn a_skill_containing_the_repository_staging_directory_cannot_import_itself() {
    let (temp, repo, store, session) = setup();
    write_skill(temp.path(), "ancestor", "one");
    let discovery = add(&session, &repo, &[temp.path()]);
    assert_eq!(discovery.candidates.len(), 1);
    let report = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(
        report.items[0].error.as_ref().unwrap().code,
        ErrorCode::UnsafePath
    );
    assert!(staged(&repo).is_empty());
    assert!(store.list_skills().unwrap().is_empty());
}
