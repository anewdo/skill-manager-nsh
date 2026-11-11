use super::*;

#[test]
fn detail_reports_clean_modified_and_missing_local_state() {
    let (_temp, repo, store) = setup();
    let skill = add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "original",
    );
    let baseline = skill.baseline_hash.clone();

    let detail = skill_detail(&repo, &store, "one").unwrap();
    assert_eq!(detail.local.state, LocalState::Clean);
    assert_eq!(detail.skill.local_state, LocalState::Clean);
    assert_eq!(detail.local.metadata.as_ref().unwrap().name, "demo");
    assert!(
        detail
            .local
            .skill_md
            .as_ref()
            .unwrap()
            .contains("description: Demo skill")
    );
    assert_eq!(detail.local.file_count, Some(1));
    assert!(detail.local.total_bytes.unwrap() > 0);
    assert!(detail.local.path.ends_with("demo"));

    let path = skill_path(&repo, "demo");
    fs::write(
        path.join("SKILL.md"),
        "---\nname: demo\ndescription: Demo skill\n---\nchanged",
    )
    .unwrap();
    let detail = skill_detail(&repo, &store, "one").unwrap();
    assert_eq!(detail.local.state, LocalState::Modified);
    assert_ne!(detail.skill.current_hash, baseline);
    assert_eq!(detail.skill.baseline_hash, baseline);
    assert_eq!(detail.skill.local_state, LocalState::Modified);

    fs::remove_dir_all(&path).unwrap();
    let detail = skill_detail(&repo, &store, "one").unwrap();
    assert_eq!(detail.local.state, LocalState::Missing);
    assert!(detail.local.skill_md.is_none());
    assert!(detail.local.metadata.is_none());
    assert_eq!(detail.local.file_count, None);
    assert_eq!(detail.skill.current_hash, None);
    assert_eq!(detail.skill.baseline_hash, baseline);
    assert_eq!(detail.skill.local_state, LocalState::Missing);
}

#[test]
fn detail_rejects_paths_that_escape_the_central_repository() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "original",
    );
    store
        .update_skill_rename("one", "demo", "../escape", "../escape", 2)
        .unwrap();

    let error = skill_detail(&repo, &store, "one").unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsafePath);
}

#[cfg(unix)]
#[test]
fn detail_rejects_symbolic_linked_skill_roots() {
    use std::os::unix::fs::symlink;

    let (temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "original",
    );
    let path = skill_path(&repo, "demo");
    let outside = temp.path().join("outside");
    write_skill(&outside, "demo", "outside");
    fs::remove_dir_all(&path).unwrap();
    symlink(&outside, &path).unwrap();

    let error = skill_detail(&repo, &store, "one").unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsafePath);
}

#[test]
fn detail_limits_the_skill_document_size() {
    let (_temp, repo, store) = setup();
    add_existing(
        &repo,
        &store,
        "one",
        source("owner/repo"),
        "demo",
        "original",
    );
    let path = skill_path(&repo, "demo");
    fs::write(
        path.join("SKILL.md"),
        vec![b'a'; MAX_LOCAL_SKILL_MD_BYTES as usize + 1],
    )
    .unwrap();

    let detail = skill_detail(&repo, &store, "one").unwrap();
    assert_eq!(detail.local.state, LocalState::Modified);
    assert!(detail.local.skill_md.is_none());
    assert!(detail.local.metadata.is_none());
    assert_eq!(
        detail.local.skill_md_error.unwrap().code,
        ErrorCode::InvalidSkill
    );
}
