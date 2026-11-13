use std::io::Cursor;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use zip::write::SimpleFileOptions;

use super::*;
use crate::core::content_identity::content_hash;
use crate::core::model::{LocalState, OperationEventKind, OperationKind, OperationState};
use crate::core::operations::OperationQueue;
use crate::core::test_support::wait_terminal;

fn setup() -> (TempDir, CentralRepo, Arc<Store>, LocalImports) {
    let temp = tempfile::tempdir().unwrap();
    let repo = CentralRepo::new(temp.path().join("central"));
    let store = Arc::new(Store::open(&repo).unwrap());
    (temp, repo, store, LocalImports::default())
}

fn write_skill(root: &Path, name: &str, body: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: 本地导入示例\n---\n{body}"),
    )
    .unwrap();
}

fn add(session: &LocalImports, repo: &CentralRepo, paths: &[&Path]) -> LocalDiscovery {
    session
        .add_paths(
            repo,
            &paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
        .unwrap()
}

fn import(
    session: &LocalImports,
    repo: &CentralRepo,
    store: &Arc<Store>,
    ids: &[String],
) -> LocalImportReport {
    let queue = OperationQueue::new();
    let batch = session.prepare(ids).unwrap();
    let repo = repo.clone();
    let store = store.clone();
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&repo, &store, &context)
        })
        .unwrap();
    let snapshot = wait_terminal(&queue, &id);
    assert_eq!(
        snapshot.state,
        OperationState::Succeeded,
        "{:?}",
        snapshot.error
    );
    serde_json::from_value(snapshot.result.unwrap()).unwrap()
}

fn ids(discovery: &LocalDiscovery) -> Vec<String> {
    discovery
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect()
}

fn staged(repo: &CentralRepo) -> Vec<PathBuf> {
    fs::read_dir(repo.staging_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    fs::write(path, writer.finish().unwrap().into_inner()).unwrap();
}

fn skill_bytes(name: &str) -> Vec<u8> {
    format!("---\nname: {name}\ndescription: ZIP 示例\n---\n内容").into_bytes()
}

fn fail_insert(repo: &CentralRepo) -> rusqlite::Connection {
    let db = rusqlite::Connection::open(repo.database_path()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON skills WHEN NEW.display_name = 'bad' BEGIN SELECT RAISE(ABORT, 'forced import failure'); END;").unwrap();
    db
}

#[test]
fn discovery_appends_deduplicates_and_reports_invalid_metadata_without_writing() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("技能 集合");
    write_skill(&root, "root-skill", "根目录");
    write_skill(&root.join("nested/demo"), "demo", "嵌套");
    fs::create_dir_all(root.join("invalid")).unwrap();
    fs::write(root.join("invalid/SKILL.md"), "invalid YAML").unwrap();
    let first = add(&session, &repo, &[&root]);
    assert_eq!(first.candidates.len(), 2);
    assert_eq!(first.issues.len(), 1);
    assert!(first.issues[0].path.ends_with("invalid"));
    assert_eq!(first.issues[0].error.code, ErrorCode::InvalidSkill);
    let second = add(&session, &repo, &[&root, &root.join("nested")]);
    assert_eq!(ids(&first), ids(&second));
    assert_eq!(second.sources.len(), 2);
    assert!(store.list_skills().unwrap().is_empty());
    assert!(staged(&repo).is_empty());
    assert!(
        fs::read_to_string(root.join("SKILL.md"))
            .unwrap()
            .contains("根目录")
    );
}

#[test]
fn discovery_distinguishes_empty_invalid_and_failed_sources_and_accepts_file_urls() {
    let (temp, repo, _, session) = setup();
    let empty = temp.path().join("empty");
    let invalid = temp.path().join("invalid");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&invalid).unwrap();
    fs::write(invalid.join("SKILL.md"), "bad").unwrap();
    let result = add(
        &session,
        &repo,
        &[&empty, &invalid, &temp.path().join("missing")],
    );
    assert!(result.sources.iter().any(|source| source.status == "empty"));
    assert!(
        result
            .sources
            .iter()
            .any(|source| source.status == "invalid")
    );
    assert_eq!(result.issues.len(), 2);
    let folder = temp.path().join("中文 空格");
    write_skill(&folder, "from-url", "URI");
    let uri = reqwest::Url::from_file_path(&folder).unwrap().to_string();
    let result = session
        .add_paths(&repo, &[uri, "https://example.com/folder".into()])
        .unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].name, "from-url");
    assert_eq!(result.issues.len(), 2);
    fs::remove_dir(&empty).unwrap();
    assert!(
        session
            .rescan(&repo)
            .sources
            .iter()
            .any(|source| source.status == "failed")
    );
}

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
fn zip_snapshot_survives_original_deletion_and_partial_import_retries_release_it() {
    let (temp, repo, store, session) = setup();
    let path = temp.path().join("技能包.zip");
    archive(
        &path,
        &[
            ("a/SKILL.md", &skill_bytes("good")),
            ("b/SKILL.md", &skill_bytes("bad")),
        ],
    );
    let discovery = add(&session, &repo, &[&path]);
    assert_eq!(discovery.candidates.len(), 2);
    assert!(discovery.candidates[0].path.contains("技能包.zip!/a"));
    assert_eq!(ids(&add(&session, &repo, &[&path])), ids(&discovery));
    assert_eq!(staged(&repo).len(), 1);
    fs::remove_file(&path).unwrap();
    let db = fail_insert(&repo);
    let first = import(&session, &repo, &store, &ids(&discovery));
    assert_eq!(first.items[0].status, ImportStatus::Added);
    assert_eq!(first.items[1].status, ImportStatus::Failed);
    assert_eq!(staged(&repo).len(), 1);
    assert_eq!(session.snapshot().candidates.len(), 1);
    db.execute_batch("DROP TRIGGER reject_import").unwrap();
    let retry = import(&session, &repo, &store, &ids(&session.snapshot()));
    assert_eq!(retry.items[0].status, ImportStatus::Added);
    assert!(session.snapshot().sources.is_empty());
    assert!(staged(&repo).is_empty());
    assert_eq!(store.list_skills().unwrap().len(), 2);
    assert!(
        store
            .list_skills()
            .unwrap()
            .iter()
            .all(|skill| skill.source.is_none())
    );
}

#[test]
fn rescan_discards_zip_candidates_and_retains_manual_folder_roots() {
    let (temp, repo, store, session) = setup();
    let folder = temp.path().join("folder");
    write_skill(&folder, "folder", "one");
    let zip = temp.path().join("input.zip");
    archive(&zip, &[("SKILL.md", &skill_bytes("zip"))]);
    let discovery = add(&session, &repo, &[&folder, &zip]);
    assert_eq!(discovery.candidates.len(), 2);
    assert_eq!(staged(&repo).len(), 1);
    let after = session.rescan(&repo);
    assert_eq!(after.candidates.len(), 1);
    assert_eq!(after.candidates[0].name, "folder");
    assert_eq!(after.sources.len(), 1);
    assert_eq!(after.sources[0].kind, LocalSourceKind::Folder);
    assert!(staged(&repo).is_empty());
    assert!(zip.exists());
    assert!(store.list_skills().unwrap().is_empty());
}

#[test]
fn replacing_zip_expires_old_candidates_and_waits_for_batch_owners_to_release() {
    let (temp, repo, store, session) = setup();
    let zip = temp.path().join("input.zip");
    archive(&zip, &[("SKILL.md", &skill_bytes("old"))]);
    let old = add(&session, &repo, &[&zip]);
    let old_root = staged(&repo)[0].clone();
    let batch = session.prepare(&ids(&old)).unwrap();
    archive(&zip, &[("SKILL.md", &skill_bytes("new"))]);
    let new = add(&session, &repo, &[&zip]);
    assert_ne!(ids(&old), ids(&new));
    assert!(session.prepare(&ids(&old)).is_err());
    assert!(old_root.exists());
    let queue = OperationQueue::new();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportLocal, move |context| {
            batch.run(&job_repo, &job_store, &context)
        })
        .unwrap();
    wait_terminal(&queue, &id);
    assert!(!old_root.exists());
    assert_eq!(session.snapshot().candidates[0].name, "new");
    session.discard();
    assert!(staged(&repo).is_empty());
    assert!(zip.exists());
}

#[test]
fn dropping_a_session_releases_zip_resources_and_restart_has_no_candidates() {
    let (temp, repo, _, session) = setup();
    let zip = temp.path().join("input.zip");
    archive(&zip, &[("SKILL.md", &skill_bytes("demo"))]);
    add(&session, &repo, &[&zip]);
    assert_eq!(staged(&repo).len(), 1);
    drop(session);
    assert!(staged(&repo).is_empty());
    assert!(LocalImports::default().snapshot().candidates.is_empty());
    assert!(zip.exists());
}

#[test]
fn unsafe_zip_sources_are_rejected_without_losing_other_candidates_or_temp_files() {
    let (temp, repo, _, session) = setup();
    let folder = temp.path().join("folder");
    write_skill(&folder, "safe", "one");
    let safe = add(&session, &repo, &[&folder]);
    let zip = temp.path().join("unsafe.zip");
    for path in [
        "../outside",
        "/absolute",
        "C:/escape",
        "a\\escape",
        "CON.txt",
        "a./SKILL.md",
    ] {
        archive(
            &zip,
            &[("SKILL.md", &skill_bytes("demo")), (path, b"unsafe")],
        );
        let result = add(&session, &repo, &[&zip]);
        assert_eq!(ids(&result), ids(&safe));
        assert!(
            result
                .sources
                .iter()
                .any(|source| source.status == "failed")
        );
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.error.code == ErrorCode::UnsafePath)
        );
        assert!(staged(&repo).is_empty());
    }
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("linked", "../outside", SimpleFileOptions::default())
        .unwrap();
    fs::write(&zip, writer.finish().unwrap().into_inner()).unwrap();
    assert_eq!(ids(&add(&session, &repo, &[&zip])), ids(&safe));
    assert!(staged(&repo).is_empty());
    archive(
        &zip,
        &[
            ("SKILL.md", &skill_bytes("demo")),
            ("OTHER.md", b"duplicate"),
        ],
    );
    let mut bytes = fs::read(&zip).unwrap();
    for index in 0..=bytes.len() - 8 {
        if &bytes[index..index + 8] == b"OTHER.md" {
            bytes[index..index + 8].copy_from_slice(b"SKILL.md");
        }
    }
    fs::write(&zip, bytes).unwrap();
    let duplicate = add(&session, &repo, &[&zip]);
    assert_eq!(ids(&duplicate), ids(&safe));
    assert!(
        duplicate
            .issues
            .iter()
            .any(|issue| issue.error.code == ErrorCode::InvalidSkill)
    );
    assert!(staged(&repo).is_empty());
}

#[test]
fn discovery_limits_report_failure_instead_of_truncating_candidates() {
    let (temp, repo, _, session) = setup();
    let root = temp.path().join("many");
    for index in 0..=MAX_CANDIDATES {
        write_skill(&root.join(index.to_string()), "demo", "one");
    }
    let result = add(&session, &repo, &[&root]);
    assert!(result.candidates.is_empty());
    assert_eq!(result.sources[0].status, "failed");
    assert_eq!(result.issues[0].error.code, ErrorCode::InvalidInput);
    assert!(staged(&repo).is_empty());
}

#[cfg(any(windows, unix))]
fn directory_link(source: &Path, target: &Path) {
    #[cfg(windows)]
    junction::create(source, target).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, target).unwrap();
}

#[cfg(any(windows, unix))]
#[test]
fn folder_links_are_skipped_during_discovery_and_copy_without_losing_valid_skills() {
    let (temp, repo, store, session) = setup();
    let root = temp.path().join("input");
    let outside = temp.path().join("outside");
    write_skill(&root.join("a"), "first", "one");
    write_skill(&root.join("b"), "second", "two");
    write_skill(&outside, "external", "sentinel");
    directory_link(&outside, &root.join("linked"));
    directory_link(&outside, &root.join("a/linked"));
    directory_link(&temp.path().join("missing"), &root.join("broken"));
    let discovery = add(&session, &repo, &[&root]);
    assert!(discovery.issues.is_empty());
    assert_eq!(discovery.sources[0].status, "ready");
    assert_eq!(
        discovery
            .candidates
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    directory_link(&outside, &root.join("b/linked"));
    let report = import(&session, &repo, &store, &ids(&discovery));
    assert!(
        report
            .items
            .iter()
            .all(|item| item.status == ImportStatus::Added)
    );
    for name in ["first", "second"] {
        let installed = repo.skill_path(name).unwrap();
        assert!(installed.join("SKILL.md").is_file());
        assert!(fs::symlink_metadata(installed.join("linked")).is_err());
    }
    assert!(fs::symlink_metadata(root.join("linked")).is_ok());
    assert!(
        fs::read_to_string(outside.join("SKILL.md"))
            .unwrap()
            .ends_with("sentinel")
    );
    assert!(staged(&repo).is_empty());
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
fn changed_metadata_from_an_overlapping_root_expires_every_old_candidate() {
    let (temp, repo, _, session) = setup();
    let root = temp.path().join("input");
    let nested = root.join("nested");
    write_skill(&nested, "old", "one");
    let old = add(&session, &repo, &[&root]);
    write_skill(&nested, "new", "two");
    let new = add(&session, &repo, &[&nested]);
    assert_eq!(new.candidates.len(), 1);
    assert_eq!(new.candidates[0].name, "new");
    assert_ne!(ids(&old), ids(&new));
    assert!(session.prepare(&ids(&old)).is_err());
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

#[test]
fn unselected_zip_candidates_stay_available_until_the_last_item_is_imported() {
    let (temp, repo, store, session) = setup();
    let zip = temp.path().join("input.zip");
    archive(
        &zip,
        &[
            ("a/SKILL.md", &skill_bytes("first")),
            ("b/SKILL.md", &skill_bytes("second")),
        ],
    );
    let discovery = add(&session, &repo, &[&zip]);
    let original = fs::read(&zip).unwrap();
    import(&session, &repo, &store, &ids(&discovery)[..1]);
    assert_eq!(session.snapshot().candidates[0].name, "second");
    assert_eq!(staged(&repo).len(), 1);
    import(&session, &repo, &store, &ids(&session.snapshot()));
    assert!(staged(&repo).is_empty());
    assert_eq!(fs::read(zip).unwrap(), original);
}

#[test]
fn oversized_archive_entries_and_archives_with_too_many_entries_are_rejected() {
    let (temp, repo, _, session) = setup();
    let zip = temp.path().join("input.zip");
    archive(&zip, &[("SKILL.md", &skill_bytes("demo"))]);
    let mut bytes = fs::read(&zip).unwrap();
    let central = bytes
        .windows(4)
        .position(|signature| signature == b"PK\x01\x02")
        .unwrap();
    bytes[central + 24..central + 28]
        .copy_from_slice(&((MAX_SKILL_BYTES + 1) as u32).to_le_bytes());
    fs::write(&zip, bytes).unwrap();
    let result = add(&session, &repo, &[&zip]);
    assert!(result.candidates.is_empty());
    assert_eq!(result.sources[0].status, "failed");
    assert!(staged(&repo).is_empty());
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..=MAX_ENTRIES {
        writer
            .start_file(index.to_string(), SimpleFileOptions::default())
            .unwrap();
    }
    fs::write(&zip, writer.finish().unwrap().into_inner()).unwrap();
    let result = add(&session, &repo, &[&zip]);
    assert!(result.candidates.is_empty());
    assert_eq!(result.sources[0].status, "failed");
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
