use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

use super::process::run_git;
use super::repository::{prepare_download, resolve_skill_root};
use super::snapshot::RepositorySnapshot;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::model::{ProxySettings, SkillSource};
use crate::core::sources::common::{DownloadObserver, DownloadedSkill};

#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub url: String,
    pub reference: String,
    pub revision: String,
}

pub fn resolve_target(
    url: &str,
    reference: Option<&str>,
    proxy: &ProxySettings,
    observer: &dyn DownloadObserver,
) -> Result<RemoteTarget> {
    let requested = reference.unwrap_or("HEAD");
    let output = run_git(
        [
            "ls-remote",
            "--symref",
            "--",
            url,
            requested,
            &format!("{requested}^{{}}"),
        ]
        .map(OsString::from),
        proxy,
        true,
        false,
        observer,
    )?;
    let output = String::from_utf8(output.stdout)
        .map_err(|_| AppError::new(ErrorCode::Git, "Git refs are not UTF-8"))?;
    let mut resolved = requested.to_string();
    let mut revision = None;
    let mut peeled = None;
    for line in output.lines() {
        let Some((value, name)) = line.split_once('\t') else {
            continue;
        };
        if name == requested {
            if let Some(symbolic) = value.strip_prefix("ref: ") {
                resolved = symbolic.to_string();
            } else {
                revision = Some(value.to_string());
            }
        } else if name == format!("{requested}^{{}}") {
            peeled = Some(value.to_string());
        }
    }
    let revision = peeled.or(revision).ok_or_else(|| {
        AppError::new(ErrorCode::NotFound, "remote reference was not found")
            .with_details(serde_json::json!({"reason":"reference_not_found"}))
    })?;
    Ok(RemoteTarget {
        url: url.to_string(),
        reference: resolved,
        revision,
    })
}

// 测试在受控仓库边界观测获取行为，生产入口始终使用规范化的托管来源。
pub trait GitUpdateTransport {
    fn resolve(
        &mut self,
        source: &SkillSource,
        observer: &dyn DownloadObserver,
    ) -> Result<RemoteTarget>;
    fn fetch(
        &mut self,
        target: &RemoteTarget,
        observer: &dyn DownloadObserver,
    ) -> Result<RepositorySnapshot>;
    fn download(
        &mut self,
        snapshot: &RepositorySnapshot,
        root: &str,
        observer: &dyn DownloadObserver,
    ) -> Result<DownloadedSkill>;
}

pub struct GitRepositories<'a> {
    pub staging: &'a Path,
    pub proxy: &'a ProxySettings,
}

impl GitUpdateTransport for GitRepositories<'_> {
    fn resolve(
        &mut self,
        source: &SkillSource,
        observer: &dyn DownloadObserver,
    ) -> Result<RemoteTarget> {
        resolve_target(
            source
                .url
                .as_deref()
                .expect("canonical Git source has a URL"),
            source.git_ref.as_deref(),
            self.proxy,
            observer,
        )
    }

    fn fetch(
        &mut self,
        target: &RemoteTarget,
        observer: &dyn DownloadObserver,
    ) -> Result<RepositorySnapshot> {
        let mut snapshot = RepositorySnapshot::fetch(
            self.staging,
            &target.url,
            &target.revision,
            self.proxy,
            observer,
        )?;
        snapshot.reference = target.reference.clone();
        Ok(snapshot)
    }

    fn download(
        &mut self,
        snapshot: &RepositorySnapshot,
        root: &str,
        observer: &dyn DownloadObserver,
    ) -> Result<DownloadedSkill> {
        prepare_download(snapshot, root, self.staging, self.proxy, observer)
    }
}

#[derive(Default)]
pub struct UpdateSnapshots {
    targets: BTreeMap<(String, Option<String>), Result<RemoteTarget>>,
    snapshots: BTreeMap<(String, String), Result<RepositorySnapshot>>,
}

impl UpdateSnapshots {
    pub fn target(
        &mut self,
        source: &SkillSource,
        transport: &mut impl GitUpdateTransport,
        observer: &dyn DownloadObserver,
    ) -> Result<RemoteTarget> {
        let key = (source.url.clone().unwrap(), source.git_ref.clone());
        if let Some(target) = self.targets.get(&key) {
            return target.clone();
        }
        let target = transport.resolve(source, observer).map(|target| {
            self.targets
                .values()
                .filter_map(|target| target.as_ref().ok())
                .find(|existing| {
                    existing.url == target.url && existing.reference == target.reference
                })
                .cloned()
                .unwrap_or(target)
        });
        self.targets.insert(key, target.clone());
        target
    }

    pub fn snapshot(
        &mut self,
        target: &RemoteTarget,
        transport: &mut impl GitUpdateTransport,
        observer: &dyn DownloadObserver,
    ) -> Result<&RepositorySnapshot> {
        self.snapshots
            .entry((target.url.clone(), target.reference.clone()))
            .or_insert_with(|| transport.fetch(target, observer))
            .as_ref()
            .map_err(Clone::clone)
    }
}

pub fn skill_root(
    snapshot: &RepositorySnapshot,
    source: &SkillSource,
    proxy: &ProxySettings,
    observer: &dyn DownloadObserver,
) -> Result<String> {
    if let Some(root) = &source.subpath {
        return Ok(root.clone());
    }
    resolve_skill_root(
        &snapshot.checkout,
        source
            .marketplace_skill_id
            .as_deref()
            .ok_or_else(|| AppError::invalid_input("marketplace skill ID is required"))?,
        proxy,
        observer,
    )
}
