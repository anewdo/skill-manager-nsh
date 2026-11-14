use serde::{Deserialize, Serialize};

use crate::core::model::{InstalledSkill, SkillSource};
use crate::core::sources::common::{DownloadedSkill, canonical_source};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitIdentity {
    pub reference: String,
    pub root: String,
    pub revision: String,
    pub tree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitVerification {
    pub source: SkillSource,
    pub identity: GitIdentity,
    pub baseline_hash: Option<String>,
    pub content_hash: String,
}

impl GitVerification {
    pub fn from_download(
        skill: &InstalledSkill,
        downloaded: &DownloadedSkill,
        hash: &str,
    ) -> Option<Self> {
        Some(Self {
            source: canonical_source(skill.source.clone()?).ok()?,
            identity: downloaded.git_identity.clone()?,
            baseline_hash: skill.baseline_hash.clone(),
            content_hash: hash.to_string(),
        })
    }

    pub fn matches(&self, skill: &InstalledSkill, source: &SkillSource, reference: &str) -> bool {
        self.source == *source
            && self.baseline_hash == skill.baseline_hash
            && self.identity.reference == reference
            && source
                .subpath
                .as_ref()
                .is_none_or(|root| root == &self.identity.root)
    }
}
