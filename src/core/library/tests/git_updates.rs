use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::common::*;
use crate::core::error::Result;
use crate::core::library::update::{apply_update_with_download, check_updates_with_transport};
use crate::core::model::CheckUpdatesReport;
use crate::core::operations::{OperationContext, test_context};
use crate::core::sources::common::DownloadObserver;
use crate::core::sources::github::input;
use crate::core::sources::github::snapshot::RepositorySnapshot;
use crate::core::sources::github::updates::{
    GitRepositories, GitUpdateTransport, RemoteTarget, resolve_target,
};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn commit(root: &Path) -> String {
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "test change"]);
    git(root, &["rev-parse", "HEAD"])
}

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "tests@example.com"]);
    git(root, &["config", "user.name", "Tests"]);
    git(root, &["config", "core.autocrlf", "false"]);
    git(root, &["config", "uploadpack.allowFilter", "true"]);
    write_skill(&root.join("skills/one"), "one", "first");
    write_skill(&root.join("skills/two"), "two", "second");
    fs::write(root.join("skills/one/payload.txt"), "payload".repeat(10000)).unwrap();
    commit(root);
    temp
}

// 常驻回归：所有文件和数据库均在临时目录，与用户技能库隔离。
struct ControlledGit<'a> {
    repo: &'a CentralRepo,
    context: &'a OperationContext,
    remotes: BTreeMap<String, PathBuf>,
    queries: usize,
    snapshots: usize,
    downloads: Vec<String>,
    fail_query: bool,
    fail_fetch: Option<String>,
    advance_after_download: bool,
    cancel_on_download: Option<usize>,
    payload_missing_at_fetch: Vec<bool>,
}

impl<'a> ControlledGit<'a> {
    fn new(repo: &'a CentralRepo, remote: &Path, context: &'a OperationContext) -> Self {
        Self {
            repo,
            context,
            remotes: [("owner/repo".into(), remote.to_path_buf())].into(),
            queries: 0,
            snapshots: 0,
            downloads: Vec::new(),
            fail_query: false,
            fail_fetch: None,
            advance_after_download: false,
            cancel_on_download: None,
            payload_missing_at_fetch: Vec::new(),
        }
    }

    fn check(&mut self, store: &Store, ids: &[&str]) -> CheckUpdatesReport {
        check_updates_with_transport(
            self.repo,
            store,
            &ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            &ProxySettings::default(),
            self.context,
            self,
        )
        .unwrap()
    }

    fn reset_counts(&mut self) {
        self.queries = 0;
        self.snapshots = 0;
        self.downloads.clear();
        self.payload_missing_at_fetch.clear();
    }
}

impl GitUpdateTransport for ControlledGit<'_> {
    fn resolve(
        &mut self,
        source: &SkillSource,
        observer: &dyn DownloadObserver,
    ) -> Result<RemoteTarget> {
        self.queries += 1;
        if self.fail_query {
            return Err(AppError::new(ErrorCode::Network, "offline").retryable(true));
        }
        let remote = &self.remotes[source.repository.as_deref().unwrap()];
        let url = reqwest::Url::from_file_path(remote).unwrap().to_string();
        resolve_target(
            &url,
            source.git_ref.as_deref(),
            &ProxySettings::default(),
            observer,
        )
    }

    fn fetch(
        &mut self,
        target: &RemoteTarget,
        observer: &dyn DownloadObserver,
    ) -> Result<RepositorySnapshot> {
        self.snapshots += 1;
        if self.fail_fetch.as_deref() == Some(&target.reference) {
            return Err(AppError::new(ErrorCode::Git, "fetch failed").retryable(true));
        }
        let snapshot = GitRepositories {
            staging: &self.repo.staging_dir(),
            proxy: &ProxySettings::default(),
        }
        .fetch(target, observer)?;
        let missing = git(
            &snapshot.checkout,
            &["rev-list", "--objects", "--all", "--missing=print"],
        );
        self.payload_missing_at_fetch
            .push(missing.lines().any(|line| line.starts_with('?')));
        Ok(snapshot)
    }

    fn download(
        &mut self,
        snapshot: &RepositorySnapshot,
        root: &str,
        observer: &dyn DownloadObserver,
    ) -> Result<DownloadedSkill> {
        self.downloads.push(root.to_string());
        if self.cancel_on_download == Some(self.downloads.len()) {
            return Err(crate::core::operations::cancelled_error());
        }
        let downloaded = GitRepositories {
            staging: &self.repo.staging_dir(),
            proxy: &ProxySettings::default(),
        }
        .download(snapshot, root, observer)?;
        if self.advance_after_download {
            self.advance_after_download = false;
            let remote = &self.remotes["owner/repo"];
            write_skill(&remote.join("skills/two"), "two", "remote advanced");
            commit(remote);
        }
        Ok(downloaded)
    }
}

fn add(
    repo: &CentralRepo,
    store: &Store,
    id: &str,
    root: &str,
    reference: Option<&str>,
    body: &str,
) {
    add_existing(
        repo,
        store,
        id,
        input::source("owner/repo", reference, root).unwrap(),
        id,
        body,
    );
}

#[test]
fn shares_a_pinned_snapshot_across_default_and_explicit_branch_and_isolates_other_refs() {
    let remote = fixture();
    git(remote.path(), &["branch", "other"]);
    let original = git(remote.path(), &["rev-parse", "HEAD"]);
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "first");
    add(
        &repo,
        &store,
        "two",
        "skills/two",
        Some("refs/heads/main"),
        "second",
    );
    add(
        &repo,
        &store,
        "other",
        "skills/two",
        Some("refs/heads/other"),
        "second",
    );
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.advance_after_download = true;
    let report = transport.check(&store, &["one", "two", "other"]);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(transport.snapshots, 2);
    assert_eq!(transport.downloads.len(), 3);
    assert!(
        transport
            .payload_missing_at_fetch
            .iter()
            .all(|missing| *missing)
    );
    assert!(
        report
            .checked
            .iter()
            .all(|item| item.remote_revision.as_ref() == Some(&original))
    );
    assert_eq!(stored(&store, "two").update_state, UpdateState::UpToDate);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn same_commit_survives_restart_failure_retry_and_keeps_available_and_local_changes() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "old version");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    let first = transport.check(&store, &["one"]);
    assert_eq!(first.checked[0].update_state, UpdateState::Available);
    drop(store);
    let store = Store::open(&repo).unwrap();
    write_skill(&skill_path(&repo, "one"), "one", "local changes");
    store
        .update_local_state("one", Some("local-hash"), LocalState::Modified, 5)
        .unwrap();
    transport.reset_counts();
    transport.fail_query = true;
    let failed = transport.check(&store, &["one"]);
    assert_eq!(failed.failed[0].error.code, ErrorCode::Network);
    assert_eq!(
        stored(&store, "one").remote_revision,
        first.checked[0].remote_revision
    );
    assert!(store.git_verification("one").unwrap().is_some());
    transport.fail_query = false;
    let report = transport.check(&store, &["one"]);
    assert_eq!(report.checked[0].update_state, UpdateState::Available);
    assert_eq!(transport.snapshots, 0);
    assert!(transport.downloads.is_empty());
    assert_eq!(stored(&store, "one").local_state, LocalState::Modified);
    assert!(
        fs::read_to_string(skill_path(&repo, "one").join("SKILL.md"))
            .unwrap()
            .contains("local changes")
    );
    let error = apply_update_with_download(
        &repo,
        &store,
        "one",
        UpdateConflictPolicy::Reject,
        || panic!("local protection must run before downloading"),
        &context,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::LocalModified);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn changed_commit_only_downloads_changed_skill_and_preserves_available_on_same_tree() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "old");
    add(&repo, &store, "two", "skills/two", None, "second");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.check(&store, &["one", "two"]);
    fs::write(remote.path().join("README.md"), "outside").unwrap();
    let revision = commit(remote.path());
    transport.reset_counts();
    let report = transport.check(&store, &["one", "two"]);
    assert_eq!(transport.snapshots, 1);
    assert!(transport.downloads.is_empty());
    assert!(transport.payload_missing_at_fetch[0]);
    assert_eq!(report.checked[0].update_state, UpdateState::Available);
    assert_eq!(report.checked[0].remote_revision.as_ref(), Some(&revision));
    assert_eq!(report.checked[1].update_state, UpdateState::UpToDate);
    write_skill(&remote.path().join("skills/two"), "two", "changed");
    commit(remote.path());
    transport.reset_counts();
    let report = transport.check(&store, &["one", "two"]);
    assert_eq!(transport.downloads, ["skills/two"]);
    assert!(
        report
            .checked
            .iter()
            .all(|item| item.update_state == UpdateState::Available)
    );
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn tree_changes_use_normalized_content_and_root_tree_includes_other_files() {
    let remote = fixture();
    write_skill(remote.path(), "root", "body");
    commit(remote.path());
    let (_temp, repo, store) = setup();
    add(&repo, &store, "two", "skills/two", None, "second");
    add(&repo, &store, "root", "", None, "body");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.check(&store, &["two", "root"]);
    let path = remote.path().join("skills/two/SKILL.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace('\n', "\r\n")).unwrap();
    commit(remote.path());
    transport.reset_counts();
    let report = transport.check(&store, &["two", "root"]);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(transport.downloads.len(), 2);
    assert_eq!(report.checked[0].update_state, UpdateState::UpToDate);
    git(
        remote.path(),
        &["update-index", "--chmod=+x", "skills/two/SKILL.md"],
    );
    git(remote.path(), &["commit", "-m", "mode"]);
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &["two"]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert_eq!(transport.downloads, ["skills/two"]);
    fs::write(remote.path().join("skills/two/.DS_Store"), "ignored").unwrap();
    commit(remote.path());
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &["two"]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert_eq!(transport.downloads, ["skills/two"]);
    fs::write(remote.path().join("extra.txt"), "included in root").unwrap();
    commit(remote.path());
    transport.reset_counts();
    transport.check(&store, &["two", "root"]);
    assert_eq!(transport.downloads, [""]);
}

#[test]
fn missing_path_and_group_failure_leave_other_items_processable() {
    let remote = fixture();
    git(remote.path(), &["branch", "broken"]);
    let other_remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "missing", "missing", None, "old");
    add(&repo, &store, "one", "skills/one", None, "first");
    add(
        &repo,
        &store,
        "broken",
        "skills/one",
        Some("refs/heads/broken"),
        "first",
    );
    add(
        &repo,
        &store,
        "broken-two",
        "skills/two",
        Some("refs/heads/broken"),
        "second",
    );
    add_existing(
        &repo,
        &store,
        "other",
        input::source("owner/other", None, "skills/two").unwrap(),
        "other",
        "second",
    );
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport
        .remotes
        .insert("owner/other".into(), other_remote.path().to_path_buf());
    transport.fail_fetch = Some("refs/heads/broken".into());
    let report = transport.check(&store, &["missing", "one", "broken", "broken-two", "other"]);
    assert_eq!(report.checked.len(), 2);
    assert_eq!(report.failed.len(), 3);
    assert_eq!(
        report.failed[0].error.details.as_ref().unwrap()["reason"],
        "path_not_found"
    );
    assert_eq!(transport.snapshots, 3);
    assert!(store.git_verification("missing").unwrap().is_none());
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn default_branch_change_invalidates_verification_and_annotated_tag_resolves_commit() {
    let remote = fixture();
    git(remote.path(), &["branch", "next"]);
    git(remote.path(), &["tag", "-a", "v1", "-m", "tag"]);
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "first");
    add(
        &repo,
        &store,
        "tag",
        "skills/two",
        Some("refs/tags/v1"),
        "second",
    );
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    let report = transport.check(&store, &["one", "tag"]);
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(
        report.checked[0].remote_revision,
        report.checked[1].remote_revision
    );
    git(remote.path(), &["symbolic-ref", "HEAD", "refs/heads/next"]);
    transport.reset_counts();
    transport.check(&store, &["one", "tag"]);
    assert_eq!(transport.downloads, ["skills/one"]);
    assert_eq!(
        store
            .git_verification("one")
            .unwrap()
            .unwrap()
            .identity
            .reference,
        "refs/heads/next"
    );
}

#[test]
fn changed_baseline_invalidates_verification_and_apply_update_creates_a_new_one() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "two", "skills/two", None, "old");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.check(&store, &["two"]);
    let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
    connection
        .execute(
            "UPDATE skills SET baseline_hash = 'changed' WHERE id = 'two'",
            [],
        )
        .unwrap();
    transport.reset_counts();
    transport.check(&store, &["two"]);
    assert_eq!(transport.downloads, ["skills/two"]);
    let source = input::source("owner/repo", None, "skills/two").unwrap();
    let target = transport.resolve(&source, &context).unwrap();
    let snapshot = transport.fetch(&target, &context).unwrap();
    apply_update_with_download(
        &repo,
        &store,
        "two",
        UpdateConflictPolicy::Overwrite,
        || transport.download(&snapshot, "skills/two", &context),
        &context,
    )
    .unwrap();
    drop(snapshot);
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &["two"]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert_eq!(transport.snapshots, 0);
    assert!(transport.downloads.is_empty());
}

#[test]
fn cancellation_preserves_completed_checks_and_releases_shared_snapshots() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "first");
    add(&repo, &store, "two", "skills/two", None, "second");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.cancel_on_download = Some(2);
    let report = transport.check(&store, &["one", "two"]);
    assert!(report.cancelled);
    assert_eq!(report.checked.len(), 1);
    assert!(report.failed.is_empty());
    assert_eq!(stored(&store, "one").update_state, UpdateState::Available);
    assert_eq!(stored(&store, "two").update_state, UpdateState::Unknown);
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn install_establishes_verification_and_missing_records_or_changed_sources_download_again() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    let source = input::source("owner/repo", None, "skills/two").unwrap();
    let target = transport.resolve(&source, &context).unwrap();
    let snapshot = transport.fetch(&target, &context).unwrap();
    let (skill, _) = crate::core::library::install::install_with_download(
        &repo,
        &store,
        source,
        || transport.download(&snapshot, "skills/two", &context),
        &context,
    )
    .unwrap();
    drop(snapshot);
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &[&skill.id]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert_eq!(transport.snapshots, 0);
    assert!(transport.downloads.is_empty());
    let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
    connection
        .execute(
            "UPDATE skills SET git_verification = NULL WHERE id = ?1",
            [&skill.id],
        )
        .unwrap();
    transport.check(&store, &[&skill.id]);
    assert_eq!(transport.downloads, ["skills/two"]);
    let source = input::source("owner/repo", None, "skills/one").unwrap();
    connection
        .execute(
            "UPDATE skills SET source_json = ?1, source_key = ?2 WHERE id = ?3",
            rusqlite::params![
                serde_json::to_string(&source).unwrap(),
                source.source_key,
                skill.id
            ],
        )
        .unwrap();
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &[&skill.id]).checked[0].update_state,
        UpdateState::Available
    );
    assert_eq!(transport.downloads, ["skills/one"]);
}

#[test]
fn changed_tree_rejects_links_and_preserves_the_last_successful_verification() {
    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "first");
    add(&repo, &store, "two", "skills/two", None, "second");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    transport.check(&store, &["one", "two"]);
    let previous = store.git_verification("one").unwrap().unwrap();
    let oid = git(remote.path(), &["rev-parse", "HEAD:skills/one/SKILL.md"]);
    git(
        remote.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "120000",
            &oid,
            "skills/one/link",
        ],
    );
    git(remote.path(), &["commit", "-m", "unsafe link"]);
    transport.reset_counts();
    let report = transport.check(&store, &["one", "two"]);
    assert_eq!(report.failed[0].error.code, ErrorCode::UnsafePath);
    assert_eq!(report.checked[0].id, "two");
    assert_eq!(
        store.git_verification("one").unwrap().unwrap().identity,
        previous.identity
    );
    assert!(staging_entries(&repo).is_empty());
}

#[test]
fn servers_without_object_filtering_still_produce_correct_results() {
    let remote = fixture();
    git(
        remote.path(),
        &["config", "uploadpack.allowFilter", "false"],
    );
    let (_temp, repo, store) = setup();
    add(&repo, &store, "two", "skills/two", None, "second");
    let context = test_context(false);
    let mut transport = ControlledGit::new(&repo, remote.path(), &context);
    assert_eq!(
        transport.check(&store, &["two"]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert!(!transport.payload_missing_at_fetch[0]);
    fs::write(remote.path().join("README.md"), "outside").unwrap();
    commit(remote.path());
    transport.reset_counts();
    assert_eq!(
        transport.check(&store, &["two"]).checked[0].update_state,
        UpdateState::UpToDate
    );
    assert_eq!(transport.snapshots, 1);
    assert!(transport.downloads.is_empty());
}

#[test]
fn transfer_progress_keeps_current_skill_batch_counts_and_committed_check_events() {
    use crate::core::model::{OperationEventKind, OperationKind};
    use crate::core::operations::OperationQueue;

    let remote = fixture();
    let (_temp, repo, store) = setup();
    add(&repo, &store, "one", "skills/one", None, "first");
    add(&repo, &store, "two", "skills/two", None, "second");
    let queue = OperationQueue::new();
    let mut events = queue.subscribe();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let id = queue
        .enqueue(OperationKind::CheckUpdates, move |context| {
            let mut transport = ControlledGit::new(&repo, remote.path(), &context);
            let report = transport.check(&store, &["one", "two"]);
            done_tx.send(()).unwrap();
            Ok(report)
        })
        .unwrap();
    done_rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .unwrap();
    let terminal = crate::core::test_support::wait_terminal(&queue, &id);
    assert!(terminal.error.is_none(), "{terminal:?}");
    let mut transfers = Vec::new();
    let mut library_changed = false;
    while let Ok(event) = events.try_recv() {
        if event.kind == OperationEventKind::LibraryChanged {
            library_changed = true;
        }
        if event.kind == OperationEventKind::OperationProgress {
            let progress = event.operation.unwrap().progress.unwrap();
            assert_eq!(progress.phase, "checking");
            assert_eq!(progress.total, Some(2));
            assert!(matches!(progress.current, Some(0 | 1)));
            let expected = if progress.current == Some(0) {
                "one"
            } else {
                "two"
            };
            assert!(progress.detail.as_ref().unwrap().starts_with(expected));
            if progress.detail.as_ref().unwrap().contains('·') {
                transfers.push(progress);
            }
        }
    }
    assert!(!transfers.is_empty());
    assert!(library_changed);
}
