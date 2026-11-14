use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::input::{self, RepositoryInput};
use super::process::{run_git, run_git_with_input};
use super::repository::{list_skill_candidates, read_candidate_metadata};
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::metadata::SkillMetadata;
use crate::core::model::ProxySettings;
use crate::core::skill_path::safe_relative_path;
use crate::core::sources::common::DownloadObserver;

pub struct RepositorySnapshot {
    _temp: TempDir,
    pub checkout: PathBuf,
    pub reference: String,
    pub revision: String,
}

pub type DiscoveredMetadata = Vec<(String, Option<SkillMetadata>)>;

impl RepositorySnapshot {
    pub fn discover(
        staging: &Path,
        input: &RepositoryInput,
        url: &str,
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<(Self, DiscoveredMetadata)> {
        let remote = run_git(
            ["ls-remote", "--symref", "--", url].map(OsString::from),
            proxy,
            true,
            false,
            observer,
        )?;
        let remote = String::from_utf8(remote.stdout)
            .map_err(|_| AppError::new(ErrorCode::Git, "Git refs are not UTF-8"))?;
        let mut default = String::new();
        let mut refs = Vec::new();
        for line in remote.lines() {
            let Some((value, name)) = line.split_once('\t') else {
                continue;
            };
            if name == "HEAD" {
                if let Some(reference) = value.strip_prefix("ref: ") {
                    default = reference.to_string();
                }
            } else if (name.starts_with("refs/heads/") || name.starts_with("refs/tags/"))
                && !name.ends_with("^{}")
            {
                refs.push((name.to_string(), value.to_string()));
            }
        }
        if input.location.is_none() && default.is_empty() {
            return Err(
                AppError::new(ErrorCode::NotFound, "repository has no default branch")
                    .with_details(serde_json::json!({"reason":"reference_not_found"})),
            );
        }
        let (reference, root, single) = input::resolve(input, &refs, &default)?;
        let snapshot = Self::fetch(staging, url, &reference, proxy, observer)?;
        let scope = if single {
            Some(if root.is_empty() {
                "SKILL.md".to_string()
            } else {
                format!("{root}/SKILL.md")
            })
        } else if root.is_empty() {
            None
        } else {
            Some(root.clone())
        };
        let entries: Vec<_> =
            list_skill_candidates(&snapshot.checkout, scope.as_deref(), proxy, observer)?
                .into_iter()
                .filter(|entry| {
                    if single {
                        entry.root == root
                    } else {
                        root.is_empty()
                            || entry.root == root
                            || entry.root.starts_with(&format!("{root}/"))
                    }
                })
                .collect();
        if single && entries.is_empty() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "SKILL.md was not found at the requested path",
            )
            .with_details(serde_json::json!({"reason":"path_not_found"})));
        }
        if !root.is_empty() && !single {
            let output = snapshot
                .git(
                    &["cat-file", "-t", &format!("HEAD:{root}")],
                    proxy,
                    observer,
                )
                .map_err(|error| {
                    error.with_details(serde_json::json!({"reason":"path_not_found"}))
                })?;
            if output.trim() != "tree" {
                return Err(AppError::invalid_input(
                    "tree URL must point to a directory",
                ));
            }
        }
        let metadata = read_candidate_metadata(&snapshot.checkout, &entries, proxy, observer)?;
        let candidates = entries
            .into_iter()
            .zip(metadata)
            .map(|(entry, metadata)| (entry.root, metadata))
            .collect();
        Ok((snapshot, candidates))
    }

    pub fn fetch(
        staging: &Path,
        url: &str,
        reference: &str,
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<Self> {
        let temp = tempfile::Builder::new()
            .prefix("github-snapshot-")
            .tempdir_in(staging)?;
        let checkout = temp.path().join("checkout");
        run_git(
            [OsString::from("init"), checkout.as_os_str().to_owned()],
            proxy,
            false,
            false,
            observer,
        )?;
        let mut snapshot = Self {
            _temp: temp,
            checkout,
            reference: reference.to_string(),
            revision: String::new(),
        };
        snapshot.git(&["remote", "add", "origin", url], proxy, observer)?;
        snapshot.git(
            &["config", "remote.origin.promisor", "true"],
            proxy,
            observer,
        )?;
        snapshot.git(
            &["config", "remote.origin.partialclonefilter", "blob:none"],
            proxy,
            observer,
        )?;
        snapshot.git(
            &[
                "fetch",
                "--depth=1",
                "--filter=blob:none",
                "--no-tags",
                "--progress",
                "origin",
                reference,
            ],
            proxy,
            observer,
        )?;
        snapshot.revision = snapshot
            .git(&["rev-parse", "FETCH_HEAD^{commit}"], proxy, observer)?
            .trim()
            .to_string();
        snapshot.git(&["update-ref", "HEAD", &snapshot.revision], proxy, observer)?;
        Ok(snapshot)
    }

    pub fn validate_roots(
        &self,
        roots: &[String],
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<Vec<Result<()>>> {
        if roots.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.tree(roots, proxy, observer)?;
        Ok(roots
            .iter()
            .map(|root| validate_tree(&tree, root))
            .collect())
    }

    fn tree(
        &self,
        roots: &[String],
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<Vec<u8>> {
        let mut args = vec![
            OsString::from("-C"),
            self.checkout.as_os_str().to_owned(),
            OsString::from("--literal-pathspecs"),
            OsString::from("ls-tree"),
            OsString::from("-r"),
            OsString::from("-z"),
            OsString::from("HEAD"),
        ];
        if !roots.iter().any(String::is_empty) {
            args.push(OsString::from("--"));
            args.extend(roots.iter().map(OsString::from));
        }
        Ok(run_git(args, proxy, false, false, observer)?.stdout)
    }

    pub fn materialize(
        &self,
        roots: &[String],
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<()> {
        let tree = self.tree(roots, proxy, observer)?;
        for root in roots {
            validate_tree(&tree, root)?;
        }
        let tree = std::str::from_utf8(&tree)
            .map_err(|_| AppError::unsafe_path("repository paths must be UTF-8"))?;
        let selected: std::collections::BTreeSet<_> = tree
            .split('\0')
            .filter_map(|record| record.split_once('\t'))
            .filter_map(|(header, _)| header.split_whitespace().nth(2))
            .collect();
        let objects = self.git(
            &["rev-list", "--objects", "--missing=print", "HEAD"],
            proxy,
            observer,
        )?;
        let missing: Vec<_> = objects
            .lines()
            .filter_map(|line| line.strip_prefix('?'))
            .filter(|oid| selected.contains(oid))
            .collect();
        if !missing.is_empty() {
            run_git_with_input(
                [OsString::from("-C"), self.checkout.as_os_str().to_owned()]
                    .into_iter()
                    .chain(
                        [
                            "-c",
                            "fetch.negotiationAlgorithm=noop",
                            "fetch",
                            "--no-tags",
                            "--no-write-fetch-head",
                            "--recurse-submodules=no",
                            "--filter=blob:none",
                            "--progress",
                            "--stdin",
                            "origin",
                        ]
                        .map(OsString::from),
                    ),
                proxy,
                true,
                true,
                Some(format!("{}\n", missing.join("\n")).into_bytes()),
                observer,
            )?;
        }
        if roots.iter().any(String::is_empty) {
            self.git(&["sparse-checkout", "disable"], proxy, observer)?;
        } else {
            let patterns: Vec<_> = roots
                .iter()
                .map(|root| {
                    let mut pattern = String::from("/");
                    for character in root.chars() {
                        if matches!(character, '*' | '?' | '[' | ']' | '!' | '#') {
                            pattern.push('\\');
                        }
                        pattern.push(character);
                    }
                    pattern.push('/');
                    pattern
                })
                .collect();
            let mut args = vec!["sparse-checkout", "set", "--no-cone", "--"];
            args.extend(patterns.iter().map(String::as_str));
            self.git(&args, proxy, observer)?;
        }
        self.git(
            &["checkout", "--detach", "--force", &self.revision],
            proxy,
            observer,
        )?;
        Ok(())
    }

    fn git(
        &self,
        args: &[&str],
        proxy: &ProxySettings,
        observer: &dyn DownloadObserver,
    ) -> Result<String> {
        let args = [OsString::from("-C"), self.checkout.as_os_str().to_owned()]
            .into_iter()
            .chain(args.iter().map(OsString::from));
        let output = run_git(args, proxy, true, true, observer)?;
        String::from_utf8(output.stdout)
            .map_err(|_| AppError::new(ErrorCode::Git, "Git output is not UTF-8"))
    }
}

fn validate_tree(tree: &[u8], root: &str) -> Result<()> {
    let mut seen = std::collections::BTreeMap::new();
    let mut budget = crate::core::skill_path::PathBudget::default();
    let mut count = 0;
    let mut has_metadata = false;
    for record in tree
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| AppError::unsafe_path("invalid Git tree"))?;
        let (header, path) = (&record[..tab], &record[tab + 1..]);
        let relative = if root.is_empty() {
            path
        } else if let Some(path) = path.strip_prefix(format!("{root}/").as_bytes()) {
            path
        } else {
            continue;
        };
        let path = std::str::from_utf8(path)
            .map_err(|_| AppError::unsafe_path("repository paths must be UTF-8"))?;
        let relative = std::str::from_utf8(relative)
            .map_err(|_| AppError::unsafe_path("repository paths must be UTF-8"))?;
        safe_relative_path(path)?;
        budget.add(relative)?;
        if !header.starts_with(b"100644 blob ") && !header.starts_with(b"100755 blob ") {
            return Err(AppError::unsafe_path(
                "links, submodules and special files are not allowed",
            ));
        }
        if relative == "SKILL.md" {
            has_metadata = true;
        }
        count += 1;
        if count > 4096 {
            return Err(AppError::new(
                ErrorCode::InvalidSkill,
                "skill exceeds the 4096 file count limit",
            ));
        }
        let mut prefix = String::new();
        let parts: Vec<_> = relative.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if part.eq_ignore_ascii_case(".git") {
                return Err(AppError::unsafe_path("Git metadata paths are not allowed"));
            }
            let directory = index + 1 < parts.len();
            let key = prefix.to_lowercase();
            if let Some((original, was_directory)) = seen.get(&key) {
                if original != &prefix || !directory || !was_directory {
                    return Err(AppError::unsafe_path("duplicate repository path"));
                }
            } else {
                seen.insert(key, (prefix.clone(), directory));
            }
        }
    }
    if !has_metadata {
        return Err(AppError::new(
            ErrorCode::NotFound,
            "the selected skill directory no longer exists",
        )
        .with_details(serde_json::json!({"reason":"path_not_found"})));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_utf8_paths_only_reject_the_affected_candidate() {
        let tree = b"100644 blob oid\tskills/one/SKILL.md\0\
            100644 blob oid\tskills/one/\xff\0\
            100644 blob oid\tskills/two/SKILL.md\0";
        assert_eq!(
            validate_tree(tree, "skills/one").unwrap_err().code,
            ErrorCode::UnsafePath
        );
        assert!(validate_tree(tree, "skills/two").is_ok());
    }
}
