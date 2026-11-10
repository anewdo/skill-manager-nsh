use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{DownloadedSkill, SilentObserver, download, skills_sh, well_known};
use crate::core::central_repo::CentralRepo;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata::parse_skill_md_content;
use crate::core::model::{ProxySettings, SkillSource, SourceType};
use crate::core::store::Store;

const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;

/// A preview only helps the user decide whether to install, so it stays useful
/// for a short while after the dialog is closed. Memory-only entries avoid
/// leaving downloaded content behind for a transient read.
const PREVIEW_TTL: Duration = Duration::from_mins(3);
const MAX_PREVIEW_ENTRIES: usize = 5;

/// Recently loaded previews, kept for one process only and evicted by LRU.
pub struct PreviewCache {
    ttl: Duration,
    max_entries: usize,
    entries: Mutex<HashMap<String, CachedPreview>>,
}

struct CachedPreview {
    preview: SkillPreview,
    stored_at: Instant,
    last_accessed: Instant,
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewCache {
    pub fn new() -> Self {
        Self::with_limits(PREVIEW_TTL, MAX_PREVIEW_ENTRIES)
    }

    fn with_limits(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, key: &str) -> Option<SkillPreview> {
        let mut entries = lock(&self.entries);
        purge_expired(&mut entries, self.ttl);
        let entry = entries.get_mut(key)?;
        entry.last_accessed = Instant::now();
        Some(entry.preview.clone())
    }

    fn insert(&self, key: String, preview: SkillPreview) {
        let mut entries = lock(&self.entries);
        purge_expired(&mut entries, self.ttl);
        let now = Instant::now();
        entries.insert(
            key,
            CachedPreview {
                preview,
                stored_at: now,
                last_accessed: now,
            },
        );
        while entries.len() > self.max_entries {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
    }

    pub fn clear(&self) -> u64 {
        let mut entries = lock(&self.entries);
        let cleared = entries.len() as u64;
        entries.clear();
        cleared
    }
}

fn purge_expired(entries: &mut HashMap<String, CachedPreview>, ttl: Duration) {
    entries.retain(|_, entry| entry.stored_at.elapsed() < ttl);
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

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
    cache: &PreviewCache,
    store: &Store,
    repo: &CentralRepo,
    source: SkillSource,
    proxy: &ProxySettings,
) -> Result<SkillPreview> {
    let source = canonical_source(source)?;
    if let Some(preview) = local_preview(store, repo, &source)? {
        return Ok(preview);
    }
    cached_or_fetch(cache, source, |source| {
        let downloaded = download(repo, source, proxy, &SilentObserver)?;
        from_downloaded(source.clone(), downloaded)
    })
}

/// An installed skill already exists locally, so its preview is read from the
/// application repository and never triggers another remote request.
fn local_preview(
    store: &Store,
    repo: &CentralRepo,
    source: &SkillSource,
) -> Result<Option<SkillPreview>> {
    let Some(installation) = store.marketplace_installation_by_source(&source.source_key)? else {
        return Ok(None);
    };
    let root = repo.installation_path(installation.category, &installation.root_relpath)?;
    let skill_md = read_skill_md(&root)?;
    let metadata = parse_skill_md_content(&skill_md, &root.join("SKILL.md").display().to_string())?;
    let digest = installation
        .source
        .as_ref()
        .and_then(|installed| installed.digest.clone());
    let stored_source = installation.source.unwrap_or_else(|| source.clone());
    Ok(Some(SkillPreview {
        source: stored_source,
        relative_root: installation.root_relpath,
        name: metadata.name,
        description: metadata.description,
        skill_md,
        revision: installation.installed_revision,
        digest,
    }))
}

fn cached_or_fetch(
    cache: &PreviewCache,
    source: SkillSource,
    fetch: impl FnOnce(&SkillSource) -> Result<SkillPreview>,
) -> Result<SkillPreview> {
    if let Some(cached) = cache.get(&source.source_key) {
        return Ok(cached);
    }
    let preview = fetch(&source)?;
    cache.insert(source.source_key.clone(), preview.clone());
    Ok(preview)
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
    let skill_md = read_skill_md(&downloaded.skill_root)?;
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

fn read_skill_md(skill_root: &Path) -> Result<String> {
    let path = skill_root.join("SKILL.md");
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
        let candidate = skill_candidate(download.path());
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
        let temp_path = download.path().to_path_buf();
        let candidate = skill_candidate(download.path());
        let downloaded =
            DownloadedSkill::from_candidate(download, &temp_path, candidate, None, None).unwrap();
        assert_eq!(
            read_skill_md(&downloaded.skill_root).unwrap_err().code,
            ErrorCode::InvalidSkill
        );
    }

    use std::cell::Cell;

    use crate::core::model::{Category, Installation, LocalState, UpdateState};

    fn skill_source(name: &str) -> SkillSource {
        skills_sh::marketplace_source("owner/repo", name).unwrap()
    }

    fn skill_candidate(root: &Path) -> SkillCandidate {
        SkillCandidate {
            root: root.to_path_buf(),
            relative_root: ".".to_string(),
            metadata: SkillMetadata {
                name: "demo".to_string(),
                description: "Demo skill".to_string(),
            },
        }
    }

    fn sample_preview(name: &str) -> SkillPreview {
        SkillPreview {
            source: skill_source(name),
            relative_root: format!("skills/{name}"),
            name: name.to_string(),
            description: format!("{name} skill"),
            skill_md: format!("# {name}\n"),
            revision: Some("a".repeat(40)),
            digest: None,
        }
    }

    #[test]
    fn reuses_a_fresh_preview_instead_of_downloading_again() {
        let cache = PreviewCache::new();
        let calls = Cell::new(0);

        let first = cached_or_fetch(&cache, skill_source("one"), |_| {
            calls.set(calls.get() + 1);
            Ok(sample_preview("one"))
        })
        .unwrap();
        let second = cached_or_fetch(&cache, skill_source("one"), |_| {
            calls.set(calls.get() + 1);
            Ok(sample_preview("one"))
        })
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn expired_previews_are_fetched_again() {
        let cache = PreviewCache::with_limits(Duration::ZERO, 32);
        let calls = Cell::new(0);

        for _ in 0..2 {
            cached_or_fetch(&cache, skill_source("one"), |_| {
                calls.set(calls.get() + 1);
                Ok(sample_preview("one"))
            })
            .unwrap();
        }

        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn evicts_the_least_recently_used_preview_when_full() {
        let cache = PreviewCache::with_limits(Duration::from_secs(60), 2);
        cached_or_fetch(&cache, skill_source("one"), |_| Ok(sample_preview("one"))).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        cached_or_fetch(&cache, skill_source("two"), |_| Ok(sample_preview("two"))).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        cached_or_fetch(&cache, skill_source("one"), |_| unreachable!()).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        cached_or_fetch(&cache, skill_source("three"), |_| {
            Ok(sample_preview("three"))
        })
        .unwrap();

        assert!(cache.get(&skill_source("one").source_key).is_some());
        assert!(cache.get(&skill_source("two").source_key).is_none());
        assert!(cache.get(&skill_source("three").source_key).is_some());
    }

    #[test]
    fn failed_previews_are_not_cached() {
        let cache = PreviewCache::new();

        let error = cached_or_fetch(&cache, skill_source("one"), |_| {
            Err(AppError::new(ErrorCode::Network, "offline"))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Network);

        let calls = Cell::new(0);
        let preview = cached_or_fetch(&cache, skill_source("one"), |_| {
            calls.set(calls.get() + 1);
            Ok(sample_preview("one"))
        })
        .unwrap();
        assert_eq!(preview.name, "one");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn clearing_reports_and_removes_every_cached_preview() {
        let cache = PreviewCache::new();
        cached_or_fetch(&cache, skill_source("one"), |_| Ok(sample_preview("one"))).unwrap();
        cached_or_fetch(&cache, skill_source("two"), |_| Ok(sample_preview("two"))).unwrap();

        assert_eq!(cache.clear(), 2);
        assert_eq!(cache.clear(), 0);
        assert!(cache.get(&skill_source("one").source_key).is_none());
    }

    #[test]
    fn installed_skills_are_previewed_from_the_local_repository() {
        let temp = tempfile::tempdir().unwrap();
        let repo = CentralRepo::new(temp.path().join("repo"));
        repo.ensure().unwrap();
        let root = repo.skills_root(Category::Marketplace).join("demo");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("SKILL.md"),
            "---\nname: demo\ndescription: Local demo\n---\n# Local\n",
        )
        .unwrap();

        let store = Store::in_memory().unwrap();
        let source = skill_source("demo");
        let now = 1;
        store
            .insert_installation(&Installation {
                id: "one".to_string(),
                category: Category::Marketplace,
                display_name: "demo".to_string(),
                root_relpath: "demo".to_string(),
                root_key: "demo".to_string(),
                source: Some(source.clone()),
                installed_revision: Some("b".repeat(40)),
                remote_revision: None,
                baseline_hash: Some("hash".to_string()),
                current_hash: Some("hash".to_string()),
                local_state: LocalState::Clean,
                update_state: UpdateState::Unknown,
                update_checked_at: None,
                created_at: now,
                updated_at: now,
                last_error: None,
            })
            .unwrap();

        let preview = load(
            &PreviewCache::new(),
            &store,
            &repo,
            source.clone(),
            &ProxySettings::default(),
        )
        .unwrap();

        assert_eq!(preview.name, "demo");
        assert_eq!(preview.description, "Local demo");
        assert!(preview.skill_md.ends_with("# Local\n"));
        assert_eq!(preview.relative_root, "demo");
        assert_eq!(preview.revision, Some("b".repeat(40)));
        assert_eq!(preview.source, source);
    }

    #[test]
    fn uninstalled_skills_have_no_local_preview() {
        let temp = tempfile::tempdir().unwrap();
        let repo = CentralRepo::new(temp.path().join("repo"));
        repo.ensure().unwrap();

        let preview = local_preview(
            &Store::in_memory().unwrap(),
            &repo,
            &skill_source("missing"),
        )
        .unwrap();

        assert!(preview.is_none());
    }
}
