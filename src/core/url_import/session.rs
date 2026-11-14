use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::common::{UrlCandidate, UrlDiscovery, UrlDiscoveryIssue};
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::library::install::commit_prepared;
use crate::core::local_import::common::{ImportStatus, LocalImportItem, LocalImportReport};
use crate::core::local_import::staging::stage_skill;
use crate::core::metadata::SkillMetadata;
use crate::core::model::{OperationProgress, ProxySettings};
use crate::core::operations::OperationContext;
use crate::core::sources::common::{DownloadObserver, DownloadProgress};
use crate::core::sources::github::{input, snapshot::RepositorySnapshot};
use crate::core::store::Store;

struct Session {
    snapshot: Arc<RepositorySnapshot>,
    view: UrlDiscovery,
}

#[derive(Clone, Default)]
pub struct UrlImports(Arc<Mutex<(u64, Option<Session>)>>);

pub struct DiscoveryRequest {
    generation: u64,
    previous: Option<Session>,
}

struct DiscoveryObserver<'a> {
    imports: &'a UrlImports,
    generation: u64,
}

impl DownloadObserver for DiscoveryObserver<'_> {
    fn check_cancelled(&self) -> Result<()> {
        let state = self
            .imports
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.0 == self.generation {
            Ok(())
        } else {
            Err(expired())
        }
    }

    fn report(&self, _progress: DownloadProgress) -> Result<()> {
        self.check_cancelled()
    }
}

pub struct UrlImportBatch {
    imports: UrlImports,
    session_id: String,
    snapshot: Arc<RepositorySnapshot>,
    candidates: Vec<UrlCandidate>,
}

impl UrlImports {
    pub fn begin(&self) -> DiscoveryRequest {
        let (generation, previous) = {
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.0 += 1;
            (state.0, state.1.take())
        };
        DiscoveryRequest {
            generation,
            previous,
        }
    }

    pub fn discard(&self) {
        drop(self.begin());
    }

    pub fn discover(
        &self,
        repo: &CentralRepo,
        store: &Store,
        raw: &str,
        proxy: &ProxySettings,
        request: DiscoveryRequest,
    ) -> Result<UrlDiscovery> {
        let input = input::parse(raw)?;
        let url = format!("https://github.com/{}.git", input.repository);
        self.discover_requested(repo, store, &input, &url, proxy, request)
    }

    #[cfg(test)]
    pub(crate) fn discover_repository(
        &self,
        repo: &CentralRepo,
        store: &Store,
        input: &input::RepositoryInput,
        url: &str,
        proxy: &ProxySettings,
    ) -> Result<UrlDiscovery> {
        self.discover_requested(repo, store, input, url, proxy, self.begin())
    }

    fn discover_requested(
        &self,
        repo: &CentralRepo,
        store: &Store,
        input: &input::RepositoryInput,
        url: &str,
        proxy: &ProxySettings,
        request: DiscoveryRequest,
    ) -> Result<UrlDiscovery> {
        let DiscoveryRequest {
            generation,
            previous,
        } = request;
        drop(previous);
        let (snapshot, metadata) = RepositorySnapshot::discover(
            &repo.staging_dir(),
            input,
            url,
            proxy,
            &DiscoveryObserver {
                imports: self,
                generation,
            },
        )?;
        let mut candidates = Vec::new();
        let mut issues = Vec::new();
        for (path, metadata) in metadata {
            let Some(metadata) = metadata else {
                issues.push(UrlDiscoveryIssue {
                    path,
                    error: AppError::new(
                        ErrorCode::InvalidSkill,
                        "SKILL.md must be a regular UTF-8 file with valid frontmatter within 1 MiB",
                    ),
                });
                continue;
            };
            let source = match input::source(&input.repository, Some(&snapshot.reference), &path) {
                Ok(source) => source,
                Err(error) => {
                    issues.push(UrlDiscoveryIssue { path, error });
                    continue;
                }
            };
            candidates.push(UrlCandidate {
                id: Uuid::new_v4().to_string(),
                name: metadata.name,
                description: metadata.description,
                path,
                existing: store.skill_by_source(&source.source_key)?,
                source,
            });
        }
        let view = UrlDiscovery {
            session_id: Uuid::new_v4().to_string(),
            repository: input.repository.clone(),
            git_ref: snapshot.reference.clone(),
            revision: snapshot.revision.clone(),
            candidates,
            issues,
        };
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.0 != generation {
            return Err(expired());
        }
        state.1 = if view.candidates.is_empty() {
            None
        } else {
            Some(Session {
                snapshot: Arc::new(snapshot),
                view: view.clone(),
            })
        };
        Ok(view)
    }

    pub fn prepare(&self, session_id: &str, ids: &[String]) -> Result<UrlImportBatch> {
        if ids.is_empty() {
            return Err(AppError::invalid_input("select at least one candidate"));
        }
        let state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = state
            .1
            .as_ref()
            .filter(|session| session.view.session_id == session_id)
            .ok_or_else(expired)?;
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            candidates.push(
                session
                    .view
                    .candidates
                    .iter()
                    .find(|candidate| &candidate.id == id)
                    .ok_or_else(expired)?
                    .clone(),
            );
        }
        Ok(UrlImportBatch {
            imports: self.clone(),
            session_id: session_id.to_string(),
            snapshot: session.snapshot.clone(),
            candidates,
        })
    }

    pub fn snapshot(&self) -> Option<UrlDiscovery> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1
            .as_ref()
            .map(|session| session.view.clone())
    }

    fn forget(&self, session_id: &str, ids: &BTreeSet<String>) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(session) = &mut state.1
            && session.view.session_id == session_id
        {
            session
                .view
                .candidates
                .retain(|candidate| !ids.contains(&candidate.id));
            if session.view.candidates.is_empty() {
                state.1 = None;
            }
        }
    }
}

impl UrlImportBatch {
    pub fn run(
        self,
        repo: &CentralRepo,
        store: &Store,
        proxy: &ProxySettings,
        context: &OperationContext,
    ) -> Result<LocalImportReport> {
        let mut roots = Vec::new();
        for candidate in &self.candidates {
            if store
                .skill_by_source(&candidate.source.source_key)?
                .is_none()
            {
                roots.push(candidate.path.clone());
            }
        }
        let mut failures = std::collections::BTreeMap::new();
        let mut ready = Vec::new();
        let validation = self.snapshot.validate_roots(&roots, proxy, context);
        match validation {
            Ok(results) => {
                for (root, result) in roots.into_iter().zip(results) {
                    match result {
                        Ok(()) => ready.push(root),
                        Err(error) => {
                            failures.insert(root, error);
                        }
                    }
                }
            }
            Err(error) => {
                for root in roots {
                    failures.insert(root, error.clone());
                }
            }
        }
        if !ready.is_empty()
            && let Err(error) = self.snapshot.materialize(&ready, proxy, context)
        {
            for root in ready {
                failures.insert(root, error.clone());
            }
        }
        let mut report = LocalImportReport { items: Vec::new() };
        let mut completed = BTreeSet::new();
        for (index, candidate) in self.candidates.iter().enumerate() {
            let progress = |phase: &str, cancellable| OperationProgress {
                phase: phase.to_string(),
                current: Some(index as u64 + 1),
                total: Some(self.candidates.len() as u64),
                bytes: None,
                bytes_total: None,
                detail: Some(candidate.name.clone()),
                cancellable,
            };
            let result = (|| {
                context.progress(progress("validating", true))?;
                if let Some(existing) = store.skill_by_source(&candidate.source.source_key)? {
                    return Ok((existing, true));
                }
                if let Some(error) = failures.get(&candidate.path) {
                    return Err(error.clone());
                }
                let mut prepared = stage_skill(
                    &repo.staging_dir(),
                    &self.snapshot.checkout.join(&candidate.path),
                    &SkillMetadata {
                        name: candidate.name.clone(),
                        description: candidate.description.clone(),
                    },
                    context,
                )?;
                prepared.revision = Some(self.snapshot.revision.clone());
                let outcome = commit_prepared(
                    repo,
                    store,
                    &prepared,
                    Some(candidate.source.clone()),
                    context,
                    progress("installing", true),
                )?;
                Ok((outcome.skill, outcome.reused))
            })();
            let (status, skill, error) = match result {
                Ok((skill, reused)) => {
                    completed.insert(candidate.id.clone());
                    (
                        if reused {
                            ImportStatus::Existing
                        } else {
                            ImportStatus::Added
                        },
                        Some(skill),
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
                candidate_id: candidate.id.clone(),
                name: candidate.name.clone(),
                path: candidate.path.clone(),
                status,
                skill,
                error,
            });
        }
        self.imports.forget(&self.session_id, &completed);
        Ok(report)
    }
}

fn expired() -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        "URL discovery expired; discover and select again",
    )
    .with_details(serde_json::json!({ "reason": "discovery_expired" }))
}
