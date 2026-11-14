use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{github, skills_sh, well_known};
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, Result};
use crate::core::metadata::{SkillCandidate, SkillMetadata, relative_path};
use crate::core::model::{ProxySettings, SkillSource, SourceType};
use crate::core::operations::OperationContext;

#[derive(Debug)]
pub struct DownloadedSkill {
    _temp_dir: TempDir,
    pub skill_root: PathBuf,
    pub relative_root: String,
    pub metadata: SkillMetadata,
    pub revision: Option<String>,
    pub digest: Option<String>,
    pub git_identity: Option<github::common::GitIdentity>,
}

/// Bytes or entries transferred so far and, when the transport knows it, the
/// total: progress is a fraction of `total` whenever it is present. `bytes`
/// carries the transferred amount even when `current` counts objects instead.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub bytes: Option<u64>,
    pub bytes_total: Option<u64>,
    pub detail: Option<String>,
}

/// Watches a running download: `check_cancelled` lets each transfer loop stop
/// as soon as the user cancels, `report` publishes the transferred amount.
pub trait DownloadObserver {
    fn check_cancelled(&self) -> Result<()>;
    fn report(&self, progress: DownloadProgress) -> Result<()>;
}

impl DownloadObserver for OperationContext {
    fn check_cancelled(&self) -> Result<()> {
        OperationContext::check_cancelled(self)
    }

    fn report(&self, progress: DownloadProgress) -> Result<()> {
        self.progress(crate::core::model::OperationProgress {
            phase: "downloading".to_string(),
            current: Some(progress.current),
            total: progress.total,
            bytes: progress.bytes,
            bytes_total: progress.bytes_total,
            detail: progress.detail,
            cancellable: true,
        })
    }
}

/// A download nothing listens to, such as the marketplace preview.
pub struct SilentObserver;

impl DownloadObserver for SilentObserver {
    fn check_cancelled(&self) -> Result<()> {
        Ok(())
    }

    fn report(&self, _progress: DownloadProgress) -> Result<()> {
        Ok(())
    }
}

pub fn download(
    repo: &CentralRepo,
    source: &SkillSource,
    proxy: &ProxySettings,
    observer: &dyn DownloadObserver,
) -> Result<DownloadedSkill> {
    match source.source_type {
        SourceType::GitHub => {
            github::repository::download(source, proxy, &repo.staging_dir(), observer)
        }
        SourceType::WellKnown => well_known::download(source, proxy, &repo.staging_dir(), observer),
    }
}

impl DownloadedSkill {
    pub(crate) fn from_candidate(
        temp_dir: TempDir,
        relative_base: &Path,
        candidate: SkillCandidate,
        revision: Option<String>,
        digest: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            _temp_dir: temp_dir,
            relative_root: relative_path(relative_base, &candidate.root)?,
            skill_root: candidate.root,
            metadata: candidate.metadata,
            revision,
            digest,
            git_identity: None,
        })
    }
}

pub(crate) fn canonical_source(source: SkillSource) -> Result<SkillSource> {
    match source.source_type {
        SourceType::GitHub => {
            let repository = source
                .repository
                .as_deref()
                .ok_or_else(|| AppError::invalid_input("GitHub source repository is required"))?;
            let mut canonical = match source.subpath.as_deref() {
                Some(subpath) => {
                    github::input::source(repository, source.git_ref.as_deref(), subpath)?
                }
                None if source.git_ref.is_none() => skills_sh::marketplace_source(
                    repository,
                    source.marketplace_skill_id.as_deref().ok_or_else(|| {
                        AppError::invalid_input("marketplace skill ID is required")
                    })?,
                )?,
                None => return Err(AppError::invalid_input("GitHub source subpath is required")),
            };
            canonical.marketplace_skill_id = source.marketplace_skill_id;
            if source.source_key != canonical.source_key {
                return Err(AppError::invalid_input(
                    "GitHub source key does not match its repository and subpath",
                ));
            }
            Ok(canonical)
        }
        SourceType::WellKnown => well_known::canonical_source(&source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;

    #[test]
    fn canonicalizes_transport_fields_without_trusting_client_urls() {
        let source = SkillSource {
            source_type: SourceType::GitHub,
            marketplace_skill_id: None,
            source_key: "github:owner/repo/nested/skill".to_string(),
            repository: Some("Owner/Repo.git".to_string()),
            git_ref: None,
            subpath: Some("nested/skill".to_string()),
            url: Some("https://attacker.invalid/owner/repo.git".to_string()),
            digest: None,
        };
        let canonical = canonical_source(source).unwrap();
        assert_eq!(
            canonical.url.as_deref(),
            Some("https://github.com/owner/repo.git")
        );
        assert_eq!(canonical.repository.as_deref(), Some("owner/repo"));
    }

    #[test]
    fn rejects_mismatched_lineage_and_unqualified_git_refs() {
        let mut source = skills_sh::marketplace_source("owner/repo", "skill").unwrap();
        source.source_key = "github:owner/repo/other".to_string();
        assert!(canonical_source(source).is_err());

        let mut source = skills_sh::marketplace_source("owner/repo", "skill").unwrap();
        source.git_ref = Some("other-branch".to_string());
        assert_eq!(
            canonical_source(source).unwrap_err().code,
            ErrorCode::InvalidInput
        );

        let mut source = skills_sh::marketplace_source("example.com", "skill").unwrap();
        source.git_ref = Some("refs/heads/main".to_string());
        assert_eq!(
            canonical_source(source).unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }
}
