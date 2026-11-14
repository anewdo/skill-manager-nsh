use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use super::session::UrlImports;
use crate::core::central_repo::CentralRepo;
use crate::core::error::ErrorCode;
use crate::core::local_import::common::{ImportStatus, LocalImportReport};
use crate::core::model::{OperationKind, ProxySettings};
use crate::core::operations::OperationQueue;
use crate::core::sources::common::SilentObserver;
use crate::core::sources::common::download;
use crate::core::sources::github::input;
use crate::core::store::Store;
use crate::core::test_support::wait_terminal;

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

fn write(root: &Path, path: &str, name: &str, body: &str) {
    let root = root.join(path);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test skill\n---\n{body}\n"),
    )
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, String) {
    let fixture = tempfile::tempdir().unwrap();
    git(fixture.path(), &["init", "-b", "main"]);
    git(
        fixture.path(),
        &["config", "user.email", "tests@example.com"],
    );
    git(fixture.path(), &["config", "user.name", "Tests"]);
    git(
        fixture.path(),
        &["config", "uploadpack.allowFilter", "true"],
    );
    write(fixture.path(), "skills/one", "one", "first");
    write(fixture.path(), "skills/two", "two", "second");
    git(fixture.path(), &["add", "."]);
    git(fixture.path(), &["commit", "-m", "initial"]);
    let url = reqwest::Url::from_file_path(fixture.path())
        .unwrap()
        .to_string();
    (fixture, url)
}

fn setup() -> (tempfile::TempDir, CentralRepo, Arc<Store>) {
    let temp = tempfile::tempdir().unwrap();
    let repo = CentralRepo::new(temp.path().join("library"));
    let store = Arc::new(Store::open(&repo).unwrap());
    (temp, repo, store)
}

fn discover(
    imports: &UrlImports,
    repo: &CentralRepo,
    store: &Store,
    url: &str,
    input: &str,
) -> super::common::UrlDiscovery {
    imports
        .discover_repository(
            repo,
            store,
            &input::parse(input).unwrap(),
            url,
            &ProxySettings::default(),
        )
        .unwrap()
}

fn run(
    imports: &UrlImports,
    discovery: &super::common::UrlDiscovery,
    repo: &CentralRepo,
    store: Arc<Store>,
) -> LocalImportReport {
    let batch = imports
        .prepare(
            &discovery.session_id,
            &discovery
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let repo = repo.clone();
    let queue = OperationQueue::new();
    let id = queue
        .enqueue(OperationKind::ImportUrl, move |context| {
            batch.run(&repo, &store, &ProxySettings::default(), &context)
        })
        .unwrap();
    let terminal = wait_terminal(&queue, &id);
    assert!(terminal.error.is_none(), "{:?}", terminal.error);
    serde_json::from_value(terminal.result.unwrap()).unwrap()
}

#[test]
fn normalizes_addresses_and_rejects_untrusted_transports() {
    for raw in ["Owner/Repo.git", "https://github.com/Owner/Repo/"] {
        assert_eq!(input::parse(raw).unwrap().repository, "owner/repo");
    }
    for raw in [
        "file:///repo",
        "https://evil.invalid/a/b",
        "https://github.com/a/b?x=1",
        "https://user@github.com/a/b",
        "owner",
        "owner/repo/other",
    ] {
        assert!(input::parse(raw).is_err(), "{raw}");
    }
}

#[test]
fn resolves_slash_refs_tags_and_reports_ambiguous_refs() {
    let refs = vec![
        ("refs/heads/main".into(), "a".into()),
        ("refs/heads/feature/ui".into(), "b".into()),
        ("refs/tags/v1".into(), "c".into()),
    ];
    let parsed = input::parse("https://github.com/a/b/tree/feature/ui/skills/one").unwrap();
    assert_eq!(
        input::resolve(&parsed, &refs, "refs/heads/main").unwrap(),
        ("refs/heads/feature/ui".into(), "skills/one".into(), false)
    );
    let parsed = input::parse("https://github.com/a/b/blob/v1/skills/one/SKILL.md").unwrap();
    assert_eq!(
        input::resolve(&parsed, &refs, "refs/heads/main").unwrap(),
        ("refs/tags/v1".into(), "skills/one".into(), true)
    );
    let mut ambiguous = refs;
    ambiguous.push(("refs/heads/feature".into(), "d".into()));
    assert_eq!(
        input::resolve(
            &input::parse("https://github.com/a/b/tree/feature/ui/skills").unwrap(),
            &ambiguous,
            "refs/heads/main"
        )
        .unwrap_err()
        .code,
        ErrorCode::AmbiguousSkill
    );
}

#[test]
fn discovery_is_metadata_only_and_import_uses_original_shared_snapshot() {
    let (remote, url) = fixture();
    fs::write(
        remote.path().join("skills/one/payload.txt"),
        "payload".repeat(10000),
    )
    .unwrap();
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "payload"]);
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "Owner/Repo");
    assert_eq!(discovery.candidates.len(), 2);
    assert_eq!(discovery.git_ref, "refs/heads/main");
    assert!(store.list_skills().unwrap().is_empty());
    let snapshot_dir = fs::read_dir(repo.staging_dir())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("checkout");
    assert!(!snapshot_dir.join("skills").exists());
    let payload = git(remote.path(), &["rev-parse", "HEAD:skills/one/payload.txt"]);
    assert!(
        git(
            &snapshot_dir,
            &["rev-list", "--objects", "--all", "--missing=print"]
        )
        .contains(&format!("?{payload}"))
    );
    write(remote.path(), "skills/one", "one", "advanced");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "advance"]);
    let report = run(&imports, &discovery, &repo, store.clone());
    assert!(
        report
            .items
            .iter()
            .all(|item| item.status == ImportStatus::Added),
        "{report:?}"
    );
    let installed = report.items[0].skill.as_ref().unwrap();
    assert_eq!(
        installed.installed_revision.as_ref(),
        Some(&discovery.revision)
    );
    assert_eq!(
        installed.source.as_ref().unwrap().subpath.as_deref(),
        Some("skills/one")
    );
    assert!(
        fs::read_to_string(
            repo.skill_path(&installed.root_relpath)
                .unwrap()
                .join("SKILL.md")
        )
        .unwrap()
        .contains("first")
    );
    assert!(imports.snapshot().is_none());
    assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
    let reopened = Store::open(&repo).unwrap();
    assert_eq!(
        reopened.skill(&installed.id).unwrap().unwrap().source,
        installed.source
    );
}

#[test]
fn materialization_fetches_only_selected_files_and_reports_transfer_progress() {
    use crate::core::sources::common::{DownloadObserver, DownloadProgress};
    use crate::core::sources::github::snapshot::RepositorySnapshot;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Progress(Mutex<Vec<DownloadProgress>>);

    impl DownloadObserver for Progress {
        fn check_cancelled(&self) -> crate::core::error::Result<()> {
            Ok(())
        }

        fn report(&self, progress: DownloadProgress) -> crate::core::error::Result<()> {
            self.0.lock().unwrap().push(progress);
            Ok(())
        }
    }

    let (remote, url) = fixture();
    for (path, body) in [
        ("skills/one/payload.txt", "selected"),
        ("skills/two/payload.txt", "unselected"),
        ("outside.txt", "outside"),
    ] {
        fs::write(remote.path().join(path), body.repeat(10000)).unwrap();
    }
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "payloads"]);
    let staging = tempfile::tempdir().unwrap();
    let proxy = ProxySettings::default();
    let (snapshot, _) = RepositorySnapshot::discover(
        staging.path(),
        &input::parse("a/b").unwrap(),
        &url,
        &proxy,
        &SilentObserver,
    )
    .unwrap();
    let progress = Progress::default();
    snapshot
        .materialize(&["skills/one".into()], &proxy, &progress)
        .unwrap();
    assert!(snapshot.checkout.join("skills/one/payload.txt").is_file());
    assert!(!snapshot.checkout.join("skills/two").exists());
    assert!(!snapshot.checkout.join("outside.txt").exists());
    let missing = git(
        &snapshot.checkout,
        &["rev-list", "--objects", "--missing=print", "HEAD"],
    );
    for path in ["skills/two/payload.txt", "outside.txt"] {
        let oid = git(remote.path(), &["rev-parse", &format!("HEAD:{path}")]);
        assert!(missing.contains(&format!("?{oid}")), "{path}");
    }
    assert!(progress.0.lock().unwrap().iter().any(|update| {
        update
            .detail
            .as_deref()
            .is_some_and(|detail| detail.starts_with("Receiving objects"))
    }));
}

#[test]
fn an_earlier_discovery_request_cannot_replace_the_current_session() {
    let (_remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let earlier = imports.begin();
    let current = discover(&imports, &repo, &store, &url, "a/b");
    let error = imports
        .discover(&repo, &store, "a/b", &ProxySettings::default(), earlier)
        .unwrap_err();
    assert_eq!(error.details.unwrap()["reason"], "discovery_expired");
    assert_eq!(imports.snapshot().unwrap().session_id, current.session_id);
    assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 1);
}

#[test]
fn directory_blob_default_alias_and_root_skill_have_exact_identity() {
    let (remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let all = discover(&imports, &repo, &store, &url, "a/b");
    let directory = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/tree/main/skills/one",
    );
    let blob = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/blob/main/skills/one/SKILL.md",
    );
    assert_eq!(directory.candidates.len(), 1);
    assert_eq!(blob.candidates.len(), 1);
    assert_eq!(all.candidates[0].source, directory.candidates[0].source);
    assert_eq!(blob.candidates[0].source, directory.candidates[0].source);
    write(remote.path(), "", "root-skill", "root");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "root"]);
    let root = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/blob/main/SKILL.md",
    );
    assert_eq!(root.candidates.len(), 1);
    assert_eq!(root.candidates[0].source.subpath.as_deref(), Some(""));
    assert_eq!(
        run(&imports, &root, &repo, store).items[0].status,
        ImportStatus::Added
    );
}

#[test]
fn invalid_metadata_empty_scopes_and_missing_paths_remain_distinct() {
    let (remote, url) = fixture();
    fs::write(
        remote.path().join("skills/two/SKILL.md"),
        "missing frontmatter",
    )
    .unwrap();
    fs::create_dir(remote.path().join("empty")).unwrap();
    fs::write(remote.path().join("empty/README.md"), "no skill").unwrap();
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "invalid and empty scopes"]);
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "a/b");
    assert_eq!(discovery.candidates.len(), 1);
    assert_eq!(discovery.issues.len(), 1);
    assert_eq!(discovery.issues[0].path, "skills/two");
    assert_eq!(discovery.issues[0].error.code, ErrorCode::InvalidSkill);
    let empty = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/tree/main/empty",
    );
    assert!(empty.candidates.is_empty());
    assert!(empty.issues.is_empty());
    assert!(imports.snapshot().is_none());
    for (address, reason) in [
        ("https://github.com/a/b/tree/main/missing", "path_not_found"),
        (
            "https://github.com/a/b/blob/main/missing/SKILL.md",
            "path_not_found",
        ),
        ("https://github.com/a/b/tree/missing", "reference_not_found"),
    ] {
        let error = imports
            .discover_repository(
                &repo,
                &store,
                &input::parse(address).unwrap(),
                &url,
                &ProxySettings::default(),
            )
            .unwrap_err();
        assert_eq!(error.details.unwrap()["reason"], reason);
    }
    assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
}

#[test]
fn stale_sessions_foreign_ids_and_empty_selection_are_rejected() {
    let (_remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let old = discover(&imports, &repo, &store, &url, "a/b");
    let current = discover(&imports, &repo, &store, &url, "a/b");
    assert!(
        imports
            .prepare(&old.session_id, &[old.candidates[0].id.clone()])
            .is_err()
    );
    assert!(
        imports
            .prepare(&current.session_id, &[old.candidates[0].id.clone()])
            .is_err()
    );
    assert!(imports.prepare(&current.session_id, &[]).is_err());
    imports.discard();
    assert!(
        imports
            .prepare(&current.session_id, &[current.candidates[0].id.clone()])
            .is_err()
    );
}

#[test]
fn same_source_reuses_installed_skill_without_changing_source() {
    let (_remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let first = discover(&imports, &repo, &store, &url, "a/b");
    let initial = run(&imports, &first, &repo, store.clone());
    let second = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/tree/main",
    );
    assert!(
        second
            .candidates
            .iter()
            .all(|candidate| candidate.existing.is_some())
    );
    let report = run(&imports, &second, &repo, store.clone());
    assert!(
        report
            .items
            .iter()
            .all(|item| item.status == ImportStatus::Existing)
    );
    assert_eq!(
        report.items[0].skill.as_ref().unwrap().source,
        initial.items[0].skill.as_ref().unwrap().source
    );
    assert_eq!(store.list_skills().unwrap().len(), 2);
}

#[test]
fn branch_and_tag_imports_preserve_targets_and_deduplicate_content() {
    let (remote, url) = fixture();
    git(remote.path(), &["tag", "v1"]);
    git(remote.path(), &["checkout", "-b", "feature/ui"]);
    write(remote.path(), "skills/one", "one", "feature");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "feature"]);
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let branch = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/tree/feature/ui/skills",
    );
    let initial = run(&imports, &branch, &repo, store.clone());
    assert_eq!(
        initial.items[0]
            .skill
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .git_ref
            .as_deref(),
        Some("refs/heads/feature/ui")
    );
    let tag = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/tree/v1/skills",
    );
    let report = run(&imports, &tag, &repo, store.clone());
    assert_eq!(report.items[0].status, ImportStatus::Added);
    assert_eq!(
        report.items[0].skill.as_ref().unwrap().root_relpath,
        "one-2"
    );
    assert_eq!(report.items[1].status, ImportStatus::Existing);
    assert_eq!(
        report.items[1].skill.as_ref().unwrap().source,
        initial.items[1].skill.as_ref().unwrap().source
    );
}

#[test]
fn url_download_follows_saved_ref_after_default_branch_switch() {
    let (remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "a/b");
    let report = run(&imports, &discovery, &repo, store.clone());
    let mut source = report.items[0]
        .skill
        .as_ref()
        .unwrap()
        .source
        .clone()
        .unwrap();
    write(remote.path(), "skills/one", "one", "main-update");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "main-update"]);
    git(remote.path(), &["checkout", "-b", "next"]);
    write(remote.path(), "skills/one", "one", "next-update");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "next-update"]);
    source.url = Some(url);
    let downloaded = download(&repo, &source, &ProxySettings::default(), &SilentObserver).unwrap();
    assert!(
        fs::read_to_string(downloaded.skill_root.join("SKILL.md"))
            .unwrap()
            .contains("main-update")
    );
    source.git_ref = None;
    let downloaded = download(&repo, &source, &ProxySettings::default(), &SilentObserver).unwrap();
    assert!(
        fs::read_to_string(downloaded.skill_root.join("SKILL.md"))
            .unwrap()
            .contains("next-update")
    );
}

#[test]
fn manual_update_checks_without_replacing_files_and_protects_local_changes() {
    use crate::core::library::update::{apply_update_with_download, check_update_with_download};
    use crate::core::model::{UpdateConflictPolicy, UpdateState};
    use crate::core::operations::test_context;
    use crate::core::sources::common::canonical_source;

    let (remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/blob/main/skills/one/SKILL.md",
    );
    let report = run(&imports, &discovery, &repo, store.clone());
    let skill = report.items[0].skill.as_ref().unwrap();
    let installed_source = skill.source.clone();
    let mut transport = canonical_source(installed_source.clone().unwrap()).unwrap();
    assert_eq!(transport.git_ref.as_deref(), Some("refs/heads/main"));
    transport.url = Some(url);
    let skill_md = repo
        .skill_path(&skill.root_relpath)
        .unwrap()
        .join("SKILL.md");
    let initial = fs::read_to_string(&skill_md).unwrap();
    write(remote.path(), "skills/one", "one", "remote update");
    git(remote.path(), &["add", "."]);
    git(remote.path(), &["commit", "-m", "update"]);
    let checked = check_update_with_download(&store, &skill.id, || {
        download(
            &repo,
            &transport,
            &ProxySettings::default(),
            &SilentObserver,
        )
    })
    .unwrap();
    assert_eq!(checked.update_state, UpdateState::Available);
    assert_eq!(fs::read_to_string(&skill_md).unwrap(), initial);
    fs::write(&skill_md, format!("{initial}\nlocal edit")).unwrap();
    let error = apply_update_with_download(
        &repo,
        &store,
        &skill.id,
        UpdateConflictPolicy::Reject,
        || panic!("local modification must be rejected before downloading"),
        &test_context(false),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::LocalModified);
    let updated = apply_update_with_download(
        &repo,
        &store,
        &skill.id,
        UpdateConflictPolicy::Overwrite,
        || {
            download(
                &repo,
                &transport,
                &ProxySettings::default(),
                &SilentObserver,
            )
        },
        &test_context(false),
    )
    .unwrap();
    assert!(
        fs::read_to_string(&skill_md)
            .unwrap()
            .contains("remote update")
    );
    assert_eq!(updated.source, installed_source);
    assert_eq!(updated.baseline_hash, updated.current_hash);
    assert_eq!(updated.installed_revision, checked.remote_revision);
}

#[test]
fn one_failed_database_commit_rolls_back_and_keeps_other_items() {
    use crate::core::model::OperationEventKind;
    let (_remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "a/b");
    let db = rusqlite::Connection::open(repo.database_path()).unwrap();
    db.execute_batch("CREATE TRIGGER fail_one BEFORE INSERT ON skills WHEN NEW.display_name = 'one' BEGIN SELECT RAISE(FAIL, 'test failure'); END;").unwrap();
    let queue = OperationQueue::new();
    let mut events = queue.subscribe();
    let batch = imports
        .prepare(
            &discovery.session_id,
            &discovery
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let (job_repo, job_store) = (repo.clone(), store.clone());
    let id = queue
        .enqueue(OperationKind::ImportUrl, move |context| {
            batch.run(&job_repo, &job_store, &ProxySettings::default(), &context)
        })
        .unwrap();
    let report: LocalImportReport =
        serde_json::from_value(wait_terminal(&queue, &id).result.unwrap()).unwrap();
    assert_eq!(report.items[0].status, ImportStatus::Failed);
    assert_eq!(
        report.items[0].error.as_ref().unwrap().code,
        ErrorCode::Database
    );
    assert_eq!(report.items[1].status, ImportStatus::Added);
    assert!(!repo.skill_path("one").unwrap().exists());
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert_eq!(imports.snapshot().unwrap().candidates.len(), 1);
    let mut changes = 0;
    while let Ok(event) = events.try_recv() {
        if event.kind == OperationEventKind::LibraryChanged {
            changes += 1;
        }
    }
    assert_eq!(changes, 1);
    db.execute_batch("DROP TRIGGER fail_one").unwrap();
    let retry = imports.snapshot().unwrap();
    assert_eq!(
        run(&imports, &retry, &repo, store).items[0].status,
        ImportStatus::Added
    );
    assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
}

#[test]
fn cancelling_a_batch_keeps_the_snapshot_and_candidates_for_retry() {
    use crate::core::operations::test_context;
    let (_remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "a/b");
    let batch = imports
        .prepare(
            &discovery.session_id,
            &discovery
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let report = batch
        .run(
            &repo,
            &store,
            &ProxySettings::default(),
            &test_context(true),
        )
        .unwrap();
    assert!(
        report
            .items
            .iter()
            .all(|item| item.status == ImportStatus::Cancelled)
    );
    assert_eq!(imports.snapshot().unwrap().candidates.len(), 2);
    assert!(store.list_skills().unwrap().is_empty());
    assert!(
        run(&imports, &discovery, &repo, store)
            .items
            .iter()
            .all(|item| item.status == ImportStatus::Added)
    );
}

#[test]
fn unsafe_candidate_does_not_block_other_selected_skills() {
    let (remote, url) = fixture();
    // 直接写入 Git 索引，Windows 无需创建符号链接。
    fs::write(remote.path().join("link-target"), "../../outside").unwrap();
    let oid = git(remote.path(), &["hash-object", "-w", "link-target"]);
    git(
        remote.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{oid},skills/one/link"),
        ],
    );
    git(remote.path(), &["commit", "-m", "unsafe candidate"]);
    let (_temp, repo, store) = setup();
    let imports = UrlImports::default();
    let discovery = discover(&imports, &repo, &store, &url, "a/b");
    let report = run(&imports, &discovery, &repo, store.clone());
    assert_eq!(report.items[0].status, ImportStatus::Failed);
    assert_eq!(
        report.items[0].error.as_ref().unwrap().code,
        ErrorCode::UnsafePath
    );
    assert_eq!(report.items[1].status, ImportStatus::Added);
    assert_eq!(store.list_skills().unwrap().len(), 1);
}

#[test]
fn same_content_preserves_a_local_skills_absent_source() {
    use crate::core::local_import::session::LocalImports;
    use crate::core::operations::test_context;
    let (remote, url) = fixture();
    let (_temp, repo, store) = setup();
    let local = LocalImports::default();
    let local_view = local
        .add_paths(
            &repo,
            &[remote
                .path()
                .join("skills/one")
                .to_string_lossy()
                .to_string()],
        )
        .unwrap();
    let local_report = local
        .prepare(&[local_view.candidates[0].id.clone()])
        .unwrap()
        .run(&repo, &store, &test_context(false))
        .unwrap();
    let imports = UrlImports::default();
    let discovery = discover(
        &imports,
        &repo,
        &store,
        &url,
        "https://github.com/a/b/blob/main/skills/one/SKILL.md",
    );
    let report = run(&imports, &discovery, &repo, store);
    assert_eq!(report.items[0].status, ImportStatus::Existing);
    assert_eq!(
        report.items[0].skill.as_ref().unwrap().id,
        local_report.items[0].skill.as_ref().unwrap().id
    );
    assert!(report.items[0].skill.as_ref().unwrap().source.is_none());
}
