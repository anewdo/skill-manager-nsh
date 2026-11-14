pub(super) use std::fs;
pub(super) use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
pub(super) use std::sync::Arc;

use tempfile::TempDir;
pub(super) use zip::write::SimpleFileOptions;

pub(super) use crate::core::central_repo::CentralRepo;
pub(super) use crate::core::error::ErrorCode;
pub(super) use crate::core::local_import::common::{
    ImportStatus, LocalDiscovery, LocalImportReport, LocalSourceKind, MAX_CANDIDATES, MAX_ENTRIES,
    MAX_SKILL_BYTES,
};
pub(super) use crate::core::local_import::session::LocalImports;
pub(super) use crate::core::model::{OperationKind, OperationState};
pub(super) use crate::core::operations::OperationQueue;
pub(super) use crate::core::store::Store;
pub(super) use crate::core::test_support::wait_terminal;

pub(super) fn setup() -> (TempDir, CentralRepo, Arc<Store>, LocalImports) {
    let temp = tempfile::tempdir().unwrap();
    let repo = CentralRepo::new(temp.path().join("central"));
    let store = Arc::new(Store::open(&repo).unwrap());
    (temp, repo, store, LocalImports::default())
}

pub(super) fn write_skill(root: &Path, name: &str, body: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: 本地导入示例\n---\n{body}"),
    )
    .unwrap();
}

pub(super) fn add(session: &LocalImports, repo: &CentralRepo, paths: &[&Path]) -> LocalDiscovery {
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

pub(super) fn import(
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

pub(super) fn ids(discovery: &LocalDiscovery) -> Vec<String> {
    discovery
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect()
}

pub(super) fn staged(repo: &CentralRepo) -> Vec<PathBuf> {
    fs::read_dir(repo.staging_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

pub(super) fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    fs::write(path, writer.finish().unwrap().into_inner()).unwrap();
}

pub(super) fn skill_bytes(name: &str) -> Vec<u8> {
    format!("---\nname: {name}\ndescription: ZIP 示例\n---\n内容").into_bytes()
}

pub(super) fn fail_insert(repo: &CentralRepo) -> rusqlite::Connection {
    let db = rusqlite::Connection::open(repo.database_path()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON skills WHEN NEW.display_name = 'bad' BEGIN SELECT RAISE(ABORT, 'forced import failure'); END;").unwrap();
    db
}

#[cfg(any(windows, unix))]
pub(super) fn directory_link(source: &Path, target: &Path) {
    #[cfg(windows)]
    junction::create(source, target).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, target).unwrap();
}
