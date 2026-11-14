use std::sync::atomic::{AtomicBool, Ordering};

use super::common::*;
use crate::core::library::inspect::list_skills;
use crate::core::library::install::{InstallOutcome, install_with_download};
use crate::core::model::{OperationKind, OperationSnapshot, OperationState};
use crate::core::operations::OperationQueue;
use crate::core::test_support::wait_terminal;

fn run_install(
    repo: &CentralRepo,
    store: &Arc<Store>,
    source: SkillSource,
    downloaded: DownloadedSkill,
) -> OperationSnapshot {
    let queue = OperationQueue::new();
    let job_repo = repo.clone();
    let job_store = store.clone();
    let id = queue
        .enqueue(OperationKind::Install, move |context| {
            let (skill, unmanaged_copy) =
                install_with_download(&job_repo, &job_store, source, || Ok(downloaded), &context)?;
            Ok(InstallOutcome {
                skill,
                unmanaged_copy,
            })
        })
        .unwrap();
    wait_terminal(&queue, &id)
}

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
    let skill: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(skill.root_relpath, "demo");
    assert_eq!(skill.local_state, LocalState::Clean);
    assert_eq!(skill.installed_revision, Some("a".repeat(40)));
    assert_eq!(
        skill.source.as_ref().unwrap().digest.as_deref(),
        Some("sha256:test")
    );
    assert_eq!(skill.baseline_hash, skill.current_hash);
    let path = repo.skill_path("demo").unwrap();
    assert!(path.join("SKILL.md").is_file());
    assert_eq!(store.skill(&skill.id).unwrap(), Some(skill));
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
    assert_eq!(error.details.unwrap()["skillId"], "existing");
    let content = fs::read_to_string(skill_path(&repo, "demo").join("SKILL.md")).unwrap();
    assert!(content.contains("original"));
    assert_eq!(store.skill("existing").unwrap(), Some(existing));
}

#[test]
fn adopts_an_unmanaged_copy_with_identical_content() {
    let (_temp, repo, store) = setup();
    let orphan = repo.skills_root().join("demo");
    write_skill(&orphan, "demo", "same");

    let snapshot = run_install(
        &repo,
        &store,
        source("owner/repo"),
        downloaded(&repo, "demo", "same"),
    );

    assert_eq!(snapshot.state, OperationState::Succeeded);
    let value = snapshot.result.unwrap();
    assert!(value.get("unmanaged_copy").is_none());
    let skill: InstalledSkill = serde_json::from_value(value).unwrap();
    assert_eq!(skill.root_relpath, "demo");
    assert_eq!(skill.local_state, LocalState::Clean);
    assert_eq!(skill.baseline_hash, Some(content_hash(&orphan).unwrap()));
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert_eq!(store.skill(&skill.id).unwrap(), Some(skill));
}

#[test]
fn installs_with_a_suffix_when_an_unmanaged_copy_differs() {
    let (_temp, repo, store) = setup();
    let orphan = repo.skills_root().join("demo");
    write_skill(&orphan, "demo", "local copy");

    let snapshot = run_install(
        &repo,
        &store,
        source("owner/repo"),
        downloaded(&repo, "demo", "from market"),
    );

    assert_eq!(snapshot.state, OperationState::Succeeded);
    let value = snapshot.result.unwrap();
    assert_eq!(
        value.get("unmanaged_copy").and_then(|copy| copy.as_str()),
        Some("demo")
    );
    let skill: InstalledSkill = serde_json::from_value(value).unwrap();
    assert_eq!(skill.root_relpath, "demo-2");
    let orphan_content = fs::read_to_string(orphan.join("SKILL.md")).unwrap();
    assert!(orphan_content.contains("local copy"));
    let installed_content =
        fs::read_to_string(skill_path(&repo, "demo-2").join("SKILL.md")).unwrap();
    assert!(installed_content.contains("from market"));
}

#[test]
fn reinstalls_a_lineage_whose_local_files_were_deleted() {
    let (_temp, repo, store) = setup();
    let existing = add_existing(
        &repo,
        &store,
        "existing",
        source("owner/repo"),
        "demo",
        "original",
    );
    fs::remove_dir_all(skill_path(&repo, "demo")).unwrap();

    let snapshot = run_install(
        &repo,
        &store,
        source("owner/repo"),
        downloaded(&repo, "demo", "restored"),
    );

    assert_eq!(snapshot.state, OperationState::Succeeded);
    let restored: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_ne!(restored.id, existing.id);
    assert_eq!(restored.root_relpath, "demo");
    assert_eq!(restored.local_state, LocalState::Clean);
    assert_eq!(store.skill("existing").unwrap(), None);
    assert_eq!(store.skill(&restored.id).unwrap(), Some(restored.clone()));
    let content = fs::read_to_string(skill_path(&repo, "demo").join("SKILL.md")).unwrap();
    assert!(content.contains("restored"));
}

#[test]
fn listing_reports_deleted_skills_as_missing() {
    let (_temp, repo, store) = setup();
    let existing = add_existing(
        &repo,
        &store,
        "existing",
        source("owner/repo"),
        "demo",
        "original",
    );
    fs::remove_dir_all(skill_path(&repo, "demo")).unwrap();

    let skills = list_skills(&repo, &store).unwrap();

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].local_state, LocalState::Missing);
    assert_eq!(skills[0].current_hash, existing.current_hash);
    assert_eq!(
        store.skill("existing").unwrap().unwrap().local_state,
        LocalState::Missing
    );
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
    let reused: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(reused, existing);
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert!(!repo.skill_path("demo-2").unwrap().exists());
}

#[test]
fn same_name_with_different_content_uses_a_numeric_suffix() {
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
    let skill: InstalledSkill = serde_json::from_value(snapshot.result.unwrap()).unwrap();
    assert_eq!(skill.root_relpath, "demo-2");
    assert!(
        repo.skill_path("demo-2")
            .unwrap()
            .join("SKILL.md")
            .is_file()
    );
    assert_eq!(store.list_skills().unwrap().len(), 2);
}

#[test]
fn an_untracked_existing_target_is_never_overwritten() {
    let (_temp, repo, store) = setup();
    let target = repo.skill_path("demo").unwrap();
    write_skill(&target, "demo", "user files");
    fs::write(target.join("sentinel.txt"), "keep").unwrap();

    let snapshot = run_install(
        &repo,
        &store,
        source("owner/repo"),
        downloaded(&repo, "demo", "download"),
    );

    assert_eq!(snapshot.state, OperationState::Succeeded);
    let value = snapshot.result.unwrap();
    assert_eq!(
        value.get("unmanaged_copy").and_then(|copy| copy.as_str()),
        Some("demo")
    );
    let skill: InstalledSkill = serde_json::from_value(value).unwrap();
    assert_eq!(skill.root_relpath, "demo-2");
    assert_eq!(
        fs::read_to_string(target.join("sentinel.txt")).unwrap(),
        "keep"
    );
    assert_eq!(store.list_skills().unwrap().len(), 1);
}

#[test]
fn database_failure_rolls_the_moved_directory_back_to_staging() {
    let (_temp, repo, store) = setup();
    let connection = rusqlite::Connection::open(repo.database_path()).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_install BEFORE INSERT ON skills
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
    assert!(!repo.skill_path("demo").unwrap().exists());
    assert!(store.list_skills().unwrap().is_empty());
    assert_eq!(fs::read_dir(repo.staging_dir()).unwrap().count(), 0);
}
