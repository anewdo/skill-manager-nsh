use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::central_repo::CentralRepo;
use super::error::Result;
use super::metadata::{SkillCandidate, SkillMetadata, relative_path};
use super::model::{ProxySettings, SkillSource, SourceType};

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

pub fn download(
    repo: &CentralRepo,
    source: &SkillSource,
    proxy: &ProxySettings,
) -> Result<DownloadedSkill> {
    match source.source_type {
        SourceType::GitHub => github::download(source, proxy, &repo.staging_dir()),
        SourceType::WellKnown => well_known::download(source, proxy, &repo.staging_dir()),
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
