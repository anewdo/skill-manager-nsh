use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use super::DownloadedSkill;
use crate::core::central_repo::resolve_within;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::metadata::discover_single;
use crate::core::model::{ProxyMode, ProxySettings, SkillSource};

const TEMP_DIR_PREFIX: &str = "skills-manager-nsh-github-";

pub fn download(
    source: &SkillSource,
    proxy: &ProxySettings,
    staging_dir: &Path,
) -> Result<DownloadedSkill> {
    let url = source
        .url
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("GitHub source URL is required"))?;
    let subpath = source
        .subpath
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("GitHub source subpath is required"))?;
    download_repository(url, subpath, proxy, staging_dir)
}

fn download_repository(
    url: &str,
    subpath: &str,
    proxy: &ProxySettings,
    staging_dir: &Path,
) -> Result<DownloadedSkill> {
    http::custom_proxy_url(proxy)?;
    let temp_dir = tempfile::Builder::new()
        .prefix(TEMP_DIR_PREFIX)
        .tempdir_in(staging_dir)?;
    let checkout = temp_dir.path().join("checkout");

    run_git(
        [
            OsString::from("clone"),
            OsString::from("--depth"),
            OsString::from("1"),
            OsString::from("--filter=blob:none"),
            OsString::from("--sparse"),
            OsString::from("--no-checkout"),
            OsString::from("--"),
            OsString::from(url),
            checkout.as_os_str().to_owned(),
        ],
        proxy,
        true,
    )?;
    run_git(
        [
            OsString::from("-C"),
            checkout.as_os_str().to_owned(),
            OsString::from("sparse-checkout"),
            OsString::from("set"),
            OsString::from("--cone"),
            OsString::from("--"),
            OsString::from(subpath),
        ],
        proxy,
        false,
    )?;
    run_git(
        [
            OsString::from("-C"),
            checkout.as_os_str().to_owned(),
            OsString::from("checkout"),
            OsString::from("--detach"),
        ],
        proxy,
        false,
    )?;
    let output = run_git(
        [
            OsString::from("-C"),
            checkout.as_os_str().to_owned(),
            OsString::from("rev-parse"),
            OsString::from("HEAD"),
        ],
        proxy,
        false,
    )?;
    let revision = String::from_utf8(output.stdout)
        .map_err(|_| git_error("git returned a non-UTF-8 revision", false))?
        .trim()
        .to_owned();
    if !matches!(revision.len(), 40 | 64)
        || !revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(git_error("git returned an invalid HEAD revision", false));
    }

    let target = resolve_within(&checkout, subpath)?;
    let candidate = discover_single(&target)?;
    DownloadedSkill::from_candidate(temp_dir, &checkout, candidate, Some(revision), None)
}

fn run_git(
    args: impl IntoIterator<Item = OsString>,
    proxy: &ProxySettings,
    retryable: bool,
) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "http.timeout=60",
            "-c",
            "http.lowSpeedLimit=1024",
            "-c",
            "http.lowSpeedTime=30",
        ])
        .env("GIT_TERMINAL_PROMPT", "0");
    match proxy.mode {
        ProxyMode::System => {}
        ProxyMode::None => {
            command.arg("-c").arg("http.proxy=");
            for variable in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ] {
                command.env_remove(variable);
            }
            command.env("NO_PROXY", "*").env("no_proxy", "*");
        }
        ProxyMode::Custom => {
            let proxy_url = http::custom_proxy_url(proxy)?
                .expect("custom proxy mode requires a validated proxy URL");
            command.arg("-c").arg(format!("http.proxy={proxy_url}"));
        }
    }
    let output = command
        .args(args)
        .output()
        .map_err(|error| git_error(format!("failed to run git: {error}"), false))?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail: String = stderr.trim().chars().take(8_000).collect();
    Err(git_error(
        if detail.is_empty() {
            format!("git exited with status {}", output.status)
        } else {
            format!("git failed: {detail}")
        },
        retryable,
    ))
}

fn git_error(message: impl Into<String>, retryable: bool) -> AppError {
    AppError::new(ErrorCode::Git, message).retryable(retryable)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn git(repository: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_skill(repository: &Path, relative: &str, name: &str) {
        let root = repository.join(relative);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn sparse_checkout_resolves_only_the_requested_monorepo_target() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        Command::new("git")
            .arg("init")
            .arg(repository.path())
            .output()
            .unwrap();
        git(
            repository.path(),
            &["config", "user.email", "tests@example.com"],
        );
        git(repository.path(), &["config", "user.name", "Tests"]);
        write_skill(repository.path(), "skills/one", "one");
        write_skill(repository.path(), "skills/two", "two");
        git(repository.path(), &["add", "."]);
        git(repository.path(), &["commit", "-m", "skills"]);

        let downloaded = download_repository(
            repository.path().to_str().unwrap(),
            "skills/one",
            &ProxySettings::default(),
            staging.path(),
        )
        .unwrap();

        assert_eq!(downloaded.relative_root, "skills/one");
        assert_eq!(downloaded.metadata.name, "one");
        assert_eq!(downloaded.revision.as_ref().unwrap().len(), 40);
        assert!(!downloaded.skill_root.join("../two/SKILL.md").is_file());
    }
}
