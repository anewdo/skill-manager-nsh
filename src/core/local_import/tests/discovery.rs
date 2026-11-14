use super::common::*;

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
