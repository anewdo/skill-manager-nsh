use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::common::*;

#[test]
fn cancellation_during_commit_finishes_current_item_and_cancels_remaining_items() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root.join("a"), "first", "one");
    write_skill(&root.join("b"), "second", "two");
    let discovery = add(&session, &repo, &[&root]);
    let db = rusqlite::Connection::open(repo.database_path()).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let queue = OperationQueue::new();
    let batch = session.prepare(&ids(&discovery)).unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !repo.skill_path("first").unwrap().exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    queue.cancel(&id).unwrap();
    db.execute_batch("COMMIT").unwrap();
    let snapshot = wait_terminal(&queue, &id);
    let report: LocalImportReport = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(report.items[0].status, ImportStatus::Added);
    assert_eq!(report.items[1].status, ImportStatus::Cancelled);
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert_eq!(session.snapshot().candidates[0].name, "second");
    assert!(staged(&repo).is_empty());
}

#[test]
fn cancellation_before_the_first_item_keeps_all_candidates_and_originals() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root, "demo", "one");
    let discovery = add(&session, &repo, &[&root]);
    let batch = session.prepare(&ids(&discovery)).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let queue = OperationQueue::new();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            entered_tx.send(()).unwrap();
            continue_rx.recv().unwrap();
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    queue.cancel(&id).unwrap();
    continue_tx.send(()).unwrap();
    let report: LocalImportReport =
        serde_json::from_value(wait_terminal(&queue, &id).result.unwrap()).unwrap();
    assert_eq!(report.items[0].status, ImportStatus::Cancelled);
    assert_eq!(ids(&session.snapshot()), ids(&discovery));
    assert!(store.list_skills().unwrap().is_empty());
    assert!(root.join("SKILL.md").exists());
    assert!(staged(&repo).is_empty());
}

#[test]
fn cancelling_queued_import_does_not_run_the_batch() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    write_skill(&root, "demo", "one");
    let discovery = add(&session, &repo, &[&root]);
    let queue = OperationQueue::new();
    let (continue_tx, continue_rx) = mpsc::channel();
    let blocker = queue
        .enqueue(OperationKind::Install, move |_| {
            continue_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    let batch = session.prepare(&ids(&discovery)).unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    assert_eq!(queue.cancel(&id).unwrap().state, OperationState::Cancelled);
    continue_tx.send(()).unwrap();
    wait_terminal(&queue, &blocker);
    let barrier = queue.enqueue(OperationKind::Install, |_| Ok(())).unwrap();
    wait_terminal(&queue, &barrier);
    assert_eq!(ids(&session.snapshot()), ids(&discovery));
    assert!(store.list_skills().unwrap().is_empty());
    assert!(staged(&repo).is_empty());
}

#[test]
fn cancelling_a_queued_zip_batch_releases_its_lease_before_the_worker_is_available() {
    let (temp, repo, store, session) = setup();
    let zip = temp.path().join("input.zip");
    archive(&zip, &[("SKILL.md", &skill_bytes("demo"))]);
    let discovery = add(&session, &repo, &[&zip]);
    let queue = OperationQueue::new();
    let (continue_tx, continue_rx) = mpsc::channel();
    let blocker = queue
        .enqueue(OperationKind::Install, move |_| {
            continue_rx.recv().unwrap();
            Ok(())
        })
        .unwrap();
    let batch = session.prepare(&ids(&discovery)).unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    assert_eq!(queue.cancel(&id).unwrap().state, OperationState::Cancelled);
    session.discard();
    assert!(staged(&repo).is_empty());
    continue_tx.send(()).unwrap();
    wait_terminal(&queue, &blocker);
    assert!(store.list_skills().unwrap().is_empty());
    assert!(zip.exists());
}
