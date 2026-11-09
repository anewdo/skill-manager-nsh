use std::fs::File;
use std::io::Read;

use serde::{Deserialize, Serialize};

use super::{DownloadedSkill, download, skills_sh, well_known};
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{ProxySettings, SkillSource, SourceType};

const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillPreview {
    pub source: SkillSource,
    pub relative_root: String,
    pub name: String,
    pub description: String,
    pub skill_md: String,
    pub revision: Option<String>,
    pub digest: Option<String>,
}

pub fn load(
    repo: &CentralRepo,
    source: SkillSource,
    proxy: &ProxySettings,
) -> Result<SkillPreview> {
    let source = canonical_source(source)?;
    let downloaded = download(repo, &source, proxy)?;
    from_downloaded(source, downloaded)
}

pub(crate) fn canonical_source(source: SkillSource) -> Result<SkillSource> {
    if source.git_ref.is_some() {
        return Err(AppError::new(
            ErrorCode::Unsupported,
            "only the default Git branch is supported",
        ));
    }
    match source.source_type {
        SourceType::GitHub => {
            let repository = source
                .repository
                .as_deref()
                .ok_or_else(|| AppError::invalid_input("GitHub source repository is required"))?;
            let subpath = source
                .subpath
                .as_deref()
                .ok_or_else(|| AppError::invalid_input("GitHub source subpath is required"))?;
            let canonical = skills_sh::marketplace_source(repository, subpath)?;
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

fn from_downloaded(mut source: SkillSource, downloaded: DownloadedSkill) -> Result<SkillPreview> {
    let skill_md = read_skill_md(&downloaded)?;
    source.digest = downloaded.digest.clone();
    Ok(SkillPreview {
        source,
        relative_root: downloaded.relative_root,
        name: downloaded.metadata.name,
        description: downloaded.metadata.description,
        skill_md,
        revision: downloaded.revision,
        digest: downloaded.digest,
    })
}

fn read_skill_md(downloaded: &DownloadedSkill) -> Result<String> {
    let path = downloaded.skill_root.join("SKILL.md");
    let file = File::open(&path)?;
    if file.metadata()?.len() > MAX_SKILL_MD_BYTES {
        return Err(invalid_skill_md("SKILL.md exceeds the 1 MiB preview limit"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_SKILL_MD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SKILL_MD_BYTES {
        return Err(invalid_skill_md("SKILL.md exceeds the 1 MiB preview limit"));
    }
    String::from_utf8(bytes).map_err(|_| invalid_skill_md("SKILL.md is not UTF-8"))
}

fn invalid_skill_md(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidSkill, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metadata::SkillCandidate;
    use crate::core::metadata::SkillMetadata;

    #[test]
    fn canonicalizes_transport_fields_without_trusting_client_urls() {
        let source = SkillSource {
            source_type: SourceType::GitHub,
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
    fn rejects_mismatched_lineage_and_non_default_git_refs() {
        let mut source = skills_sh::marketplace_source("owner/repo", "skill").unwrap();
        source.source_key = "github:owner/repo/other".to_string();
        assert!(canonical_source(source).is_err());

        let mut source = skills_sh::marketplace_source("owner/repo", "skill").unwrap();
        source.git_ref = Some("other-branch".to_string());
        assert_eq!(
            canonical_source(source).unwrap_err().code,
            ErrorCode::Unsupported
        );
    }

    #[test]
    fn preview_reads_metadata_and_cleans_the_private_download() {
        let staging = tempfile::tempdir().unwrap();
        let download = tempfile::tempdir_in(staging.path()).unwrap();
        let temp_path = download.path().to_path_buf();
        std::fs::write(
            download.path().join("SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\n# Demo\n",
        )
        .unwrap();
        let candidate = SkillCandidate {
            root: download.path().to_path_buf(),
            relative_root: ".".to_string(),
            metadata: SkillMetadata {
                name: "demo".to_string(),
                description: "Demo skill".to_string(),
            },
        };
        let downloaded = DownloadedSkill::from_candidate(
            download,
            &temp_path,
            candidate,
            Some("a".repeat(40)),
            None,
        )
        .unwrap();
        let source = skills_sh::marketplace_source("owner/repo", "skill").unwrap();

        let preview = from_downloaded(source, downloaded).unwrap();

        assert_eq!(preview.name, "demo");
        assert!(preview.skill_md.ends_with("# Demo\n"));
        assert_eq!(preview.relative_root, ".");
        assert!(!temp_path.exists());
        assert_eq!(std::fs::read_dir(staging.path()).unwrap().count(), 0);
    }

    #[test]
    fn preview_rejects_oversized_skill_documents() {
        let download = tempfile::tempdir().unwrap();
        std::fs::write(
            download.path().join("SKILL.md"),
            vec![b'a'; MAX_SKILL_MD_BYTES as usize + 1],
        )
        .unwrap();
        let candidate = SkillCandidate {
            root: download.path().to_path_buf(),
            relative_root: ".".to_string(),
            metadata: SkillMetadata {
                name: "demo".to_string(),
                description: "Demo skill".to_string(),
            },
        };
        let temp_path = download.path().to_path_buf();
        let downloaded =
            DownloadedSkill::from_candidate(download, &temp_path, candidate, None, None).unwrap();
        assert_eq!(
            read_skill_md(&downloaded).unwrap_err().code,
            ErrorCode::InvalidSkill
        );
    }
}
