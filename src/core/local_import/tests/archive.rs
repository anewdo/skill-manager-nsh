use super::common::*;

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
