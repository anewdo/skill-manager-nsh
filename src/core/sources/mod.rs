use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::central_repo::CentralRepo;
use super::error::Result;
use super::metadata::{SkillCandidate, SkillMetadata, relative_path};
use super::model::{ProxySettings, SkillSource, SourceType};
use super::operations::OperationContext;

pub mod github;
pub mod preview;
pub mod skills_sh;
pub mod well_known;

#[derive(Debug)]
pub struct DownloadedSkill {
    _temp_dir: TempDir,
    pub skill_root: PathBuf,
    pub relative_root: String,
    pub metadata: SkillMetadata,
    pub revision: Option<String>,
    pub digest: Option<String>,
}

/// Bytes or entries transferred so far and, when the transport knows it, the
/// total: progress is a fraction of `total` whenever it is present. `bytes`
/// carries the transferred amount even when `current` counts objects instead.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub bytes: Option<u64>,
    pub bytes_total: Option<u64>,
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
        self.progress(super::model::OperationProgress {
            phase: "downloading".to_string(),
            current: Some(progress.current),
            total: progress.total,
            bytes: progress.bytes,
            bytes_total: progress.bytes_total,
            detail: None,
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
        SourceType::GitHub => github::download(source, proxy, &repo.staging_dir(), observer),
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
        })
    }
}
