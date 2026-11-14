use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::common::{
    DiscoveryIssue, ImportStatus, LocalDiscovery, LocalImportItem, LocalImportReport, LocalSource,
    LocalSourceKind, PendingCandidate, Session, check_ancestors, display_path, source_id,
};
use super::discovery::discover_source;
use super::staging::stage_candidate;
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::library::install::commit_prepared;
use crate::core::model::OperationProgress;
use crate::core::operations::OperationContext;
use crate::core::store::Store;

#[derive(Clone, Default)]
pub struct LocalImports(Arc<Mutex<Session>>);

pub struct LocalImportBatch {
    session: LocalImports,
    candidates: Vec<PendingCandidate>,
}

impl LocalImports {
    pub fn snapshot(&self) -> LocalDiscovery {
        let session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snapshot(&session)
    }

    pub fn add_paths(&self, repo: &CentralRepo, inputs: &[String]) -> Result<LocalDiscovery> {
        if inputs.is_empty() {
            return Err(AppError::invalid_input(
                "at least one local path is required",
            ));
        }
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session.issues.clear();
        for input in inputs {
            let result = input_path(input).and_then(|path| {
                check_ancestors(&path)?;
                let path = path.canonicalize()?;
                let metadata = fs::symlink_metadata(&path)?;
                let kind = if metadata.is_dir() {
                    LocalSourceKind::Folder
                } else if metadata.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                {
                    LocalSourceKind::Zip
                } else {
                    return Err(AppError::invalid_input("choose a folder or a ZIP archive"));
                };
                let id = source_id(&path);
                let existing = session.sources.iter().position(|source| source.id == id);
                let old_digest =
                    existing.and_then(|index| session.sources[index].digest.as_deref());
                if let Some(source) = discover_source(repo, &path, kind, old_digest) {
                    for previous in &mut session.sources {
                        previous.candidates.retain(|old| {
                            !source
                                .candidates
                                .iter()
                                .any(|new| old.root == new.root && old.view.id != new.view.id)
                        });
                    }
                    if let Some(index) = existing {
                        session.sources[index] = source;
                    } else {
                        session.sources.push(source);
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                session.issues.push(DiscoveryIssue {
                    path: input.clone(),
                    error,
                });
            }
        }
        Ok(snapshot(&session))
    }

    pub fn rescan(&self, repo: &CentralRepo) -> LocalDiscovery {
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let folders: Vec<_> = session
            .sources
            .iter()
            .filter(|source| source.kind == LocalSourceKind::Folder)
            .map(|source| source.path.clone())
            .collect();
        session.sources.clear();
        session.issues.clear();
        for path in folders {
            if let Some(source) = discover_source(repo, &path, LocalSourceKind::Folder, None) {
                session.sources.push(source);
            }
        }
        snapshot(&session)
    }

    pub fn discard(&self) {
        let old = {
            let mut session = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *session)
        };
        drop(old);
    }

    pub fn prepare(&self, ids: &[String]) -> Result<LocalImportBatch> {
        if ids.is_empty() {
            return Err(AppError::invalid_input("select at least one candidate"));
        }
        let session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let candidate = session
                .sources
                .iter()
                .flat_map(|source| &source.candidates)
                .find(|candidate| &candidate.view.id == id)
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::Conflict,
                        "candidate expired; rescan and select again",
                    )
                })?;
            candidates.push(candidate.clone());
        }
        Ok(LocalImportBatch {
            session: self.clone(),
            candidates,
        })
    }

    fn forget(&self, id: &str) {
        let mut session = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for source in &mut session.sources {
            source
                .candidates
                .retain(|candidate| candidate.view.id != id);
        }
        session.sources.retain(|source| {
            source.kind == LocalSourceKind::Folder || !source.candidates.is_empty()
        });
    }
}

impl LocalImportBatch {
    pub fn run(
        self,
        repo: &CentralRepo,
        store: &Store,
        context: &OperationContext,
    ) -> Result<LocalImportReport> {
        let mut report = LocalImportReport { items: Vec::new() };
        let total = self.candidates.len() as u64;
        for (index, candidate) in self.candidates.into_iter().enumerate() {
            let progress = |phase, cancellable| OperationProgress {
                phase: String::from(phase),
                current: Some(index as u64 + 1),
                total: Some(total),
                bytes: None,
                bytes_total: None,
                detail: Some(candidate.view.name.clone()),
                cancellable,
            };
            let result = context.progress(progress("copying", true)).and_then(|_| {
                let prepared = stage_candidate(repo, &candidate, context)?;
                context.progress(progress("validating", true))?;
                commit_prepared(
                    repo,
                    store,
                    &prepared,
                    None,
                    context,
                    progress("installing", true),
                )
            });
            let (status, skill, error) = match result {
                Ok(outcome) => {
                    self.session.forget(&candidate.view.id);
                    (
                        if outcome.reused {
                            ImportStatus::Existing
                        } else {
                            ImportStatus::Added
                        },
                        Some(outcome.skill),
                        None,
                    )
                }
                Err(error) => (
                    if error.code == ErrorCode::Cancelled {
                        ImportStatus::Cancelled
                    } else {
                        ImportStatus::Failed
                    },
                    None,
                    Some(error),
                ),
            };
            report.items.push(LocalImportItem {
                candidate_id: candidate.view.id.clone(),
                name: candidate.view.name.clone(),
                path: candidate.view.path.clone(),
                status,
                skill,
                error,
            });
        }
        Ok(report)
    }
}

fn snapshot(session: &Session) -> LocalDiscovery {
    let mut seen = BTreeSet::new();
    LocalDiscovery {
        candidates: session
            .sources
            .iter()
            .flat_map(|source| &source.candidates)
            .filter(|candidate| seen.insert(candidate.view.id.clone()))
            .map(|candidate| candidate.view.clone())
            .collect(),
        sources: session
            .sources
            .iter()
            .map(|source| LocalSource {
                id: source.id.clone(),
                path: display_path(&source.path),
                kind: source.kind,
                status: if source.failed {
                    "failed"
                } else if !source.candidates.is_empty() {
                    "ready"
                } else if !source.issues.is_empty() {
                    "invalid"
                } else {
                    "empty"
                }
                .to_string(),
                candidate_count: source.candidates.len(),
            })
            .collect(),
        issues: session
            .issues
            .iter()
            .chain(session.sources.iter().flat_map(|source| &source.issues))
            .cloned()
            .collect(),
    }
}

fn input_path(input: &str) -> Result<PathBuf> {
    let path = if input.starts_with("file:") {
        reqwest::Url::parse(input)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| AppError::invalid_input("invalid local file URL"))?
    } else {
        PathBuf::from(input)
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(AppError::unsafe_path(
            "local source must be an absolute path without traversal",
        ));
    }
    Ok(path)
}
