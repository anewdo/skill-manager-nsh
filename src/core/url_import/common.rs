use serde::Serialize;

use crate::core::error::AppError;
use crate::core::model::{InstalledSkill, SkillSource};

#[derive(Debug, Clone, Serialize)]
pub struct UrlCandidate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: SkillSource,
    pub existing: Option<InstalledSkill>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UrlDiscovery {
    pub session_id: String,
    pub repository: String,
    pub git_ref: String,
    pub revision: String,
    pub candidates: Vec<UrlCandidate>,
    pub issues: Vec<UrlDiscoveryIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UrlDiscoveryIssue {
    pub path: String,
    pub error: AppError,
}
