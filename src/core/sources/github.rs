use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use super::DownloadedSkill;
use crate::core::central_repo::resolve_within;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::metadata::{discover_single, parse_skill_md_content};
use crate::core::model::{ProxyMode, ProxySettings, SkillSource};

const TEMP_DIR_PREFIX: &str = "skills-manager-nsh-github-";
const MAX_SKILL_MD_BYTES: usize = 1024 * 1024;
const MAX_CANDIDATES: usize = 256;

struct SkillTreeEntry {
    /// The repository-relative directory of a SKILL.md; empty for the repository root.
    root: String,
    oid: String,
}

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

    let skill_root = resolve_skill_root(&checkout, subpath, proxy)?;
    if skill_root.is_empty() {
        run_git(
            [
                OsString::from("-C"),
                checkout.as_os_str().to_owned(),
                OsString::from("sparse-checkout"),
                OsString::from("disable"),
            ],
            proxy,
            false,
        )?;
    } else {
        run_git(
            [
                OsString::from("-C"),
                checkout.as_os_str().to_owned(),
                OsString::from("sparse-checkout"),
                OsString::from("set"),
                OsString::from("--cone"),
                OsString::from("--"),
                OsString::from(&skill_root),
            ],
            proxy,
            false,
        )?;
    }
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

    let target = if skill_root.is_empty() {
        checkout.clone()
    } else {
        resolve_within(&checkout, &skill_root)?
    };
    let candidate = discover_single(&target)?;
    DownloadedSkill::from_candidate(temp_dir, &checkout, candidate, Some(revision), None)
}

/// Finds the skill a marketplace entry names: skills.sh identifies a GitHub skill
/// by its name, which is its SKILL.md `name`, not its path in the repository.
fn resolve_skill_root(checkout: &Path, subpath: &str, proxy: &ProxySettings) -> Result<String> {
    let entries = list_skill_candidates(checkout, proxy)?;
    if entries.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidSkill,
            "the repository contains no SKILL.md",
        ));
    }
    if entries.len() > MAX_CANDIDATES {
        return Err(AppError::new(
            ErrorCode::AmbiguousSkill,
            format!("the repository contains more than {MAX_CANDIDATES} SKILL.md candidates"),
        ));
    }
    let names = read_candidate_names(checkout, &entries, proxy)?;

    let mut named: Vec<String> = entries
        .iter()
        .zip(&names)
        .filter(|(_, name)| name.as_deref() == Some(subpath))
        .map(|(entry, _)| entry.root.clone())
        .collect();
    named.sort();
    named.dedup();
    match named.len() {
        1 => return Ok(named.remove(0)),
        0 => {}
        _ => return Err(ambiguous_skill(subpath, &named)),
    }

    let mut matching: Vec<String> = entries
        .iter()
        .filter(|entry| entry.root == subpath || entry.root.rsplit('/').next() == Some(subpath))
        .map(|entry| entry.root.clone())
        .collect();
    matching.sort();
    matching.dedup();
    match matching.len() {
        1 => Ok(matching.remove(0)),
        0 => Err(AppError::new(
            ErrorCode::InvalidSkill,
            format!("the repository contains no skill named {subpath}"),
        )),
        _ => Err(ambiguous_skill(subpath, &matching)),
    }
}

fn ambiguous_skill(subpath: &str, roots: &[String]) -> AppError {
    AppError::new(
        ErrorCode::AmbiguousSkill,
        format!("multiple SKILL.md candidates match {subpath}"),
    )
    .with_details(serde_json::json!({ "candidates": roots }))
}

/// Lists every SKILL.md in the checked out revision without fetching file contents.
fn list_skill_candidates(checkout: &Path, proxy: &ProxySettings) -> Result<Vec<SkillTreeEntry>> {
    let output = run_git(
        [
            OsString::from("-C"),
            checkout.as_os_str().to_owned(),
            OsString::from("ls-tree"),
            OsString::from("-r"),
            OsString::from("-z"),
            OsString::from("HEAD"),
        ],
        proxy,
        false,
    )?;

    let mut entries = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let Ok(path) = std::str::from_utf8(&record[tab + 1..]) else {
            continue;
        };
        let Some(root) = path.strip_suffix("SKILL.md") else {
            continue;
        };
        if !root.is_empty() && !root.ends_with('/') {
            continue;
        }
        let Ok(header) = std::str::from_utf8(&record[..tab]) else {
            continue;
        };
        let mut fields = header.split(' ');
        let _mode = fields.next();
        if fields.next() != Some("blob") {
            continue;
        }
        let Some(oid) = fields.next() else {
            continue;
        };
        if !matches!(oid.len(), 40 | 64) {
            continue;
        }
        entries.push(SkillTreeEntry {
            root: root.strip_suffix('/').unwrap_or("").to_string(),
            oid: oid.to_string(),
        });
    }
    Ok(entries)
}

/// Reads the frontmatter name of each candidate in one `git cat-file` run.
fn read_candidate_names(
    checkout: &Path,
    entries: &[SkillTreeEntry],
    proxy: &ProxySettings,
) -> Result<Vec<Option<String>>> {
    let mut command = git_command(proxy)?;
    command
        .arg("-C")
        .arg(checkout)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| git_error(format!("failed to run git: {error}"), false))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| git_error("git did not provide stdin", false))?;
        for entry in entries {
            writeln!(stdin, "{}", entry.oid)
                .map_err(|error| git_error(format!("failed to write to git: {error}"), false))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| git_error(format!("failed to read from git: {error}"), false))?;
    if !output.status.success() {
        return Err(git_error(
            format!(
                "git failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            false,
        ));
    }
    Ok(parse_candidate_names(&output.stdout, entries.len()))
}

fn parse_candidate_names(output: &[u8], count: usize) -> Vec<Option<String>> {
    let mut names = Vec::with_capacity(count);
    let mut cursor = 0;
    while names.len() < count && cursor < output.len() {
        let Some(line_end) = output[cursor..].iter().position(|byte| *byte == b'\n') else {
            break;
        };
        let Ok(header) = std::str::from_utf8(&output[cursor..cursor + line_end]) else {
            break;
        };
        cursor += line_end + 1;
        let mut fields = header.split(' ');
        let _oid = fields.next();
        let Some("blob") = fields.next() else {
            names.push(None);
            continue;
        };
        let Some(size) = fields.next().and_then(|size| size.parse::<usize>().ok()) else {
            break;
        };
        if cursor + size > output.len() {
            break;
        }
        let content = &output[cursor..cursor + size];
        cursor += size + 1;
        let name = if size <= MAX_SKILL_MD_BYTES {
            std::str::from_utf8(content)
                .ok()
                .and_then(|content| parse_skill_md_content(content, "SKILL.md").ok())
                .map(|metadata| metadata.name)
        } else {
            None
        };
        names.push(name);
    }
    names
}

fn git_command(proxy: &ProxySettings) -> Result<Command> {
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
    Ok(command)
}

fn run_git(
    args: impl IntoIterator<Item = OsString>,
    proxy: &ProxySettings,
    retryable: bool,
) -> Result<Output> {
    let output = git_command(proxy)?
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

    fn init_repository(repository: &Path) {
        Command::new("git")
            .arg("init")
            .arg(repository)
            .output()
            .unwrap();
        git(repository, &["config", "user.email", "tests@example.com"]);
        git(repository, &["config", "user.name", "Tests"]);
    }

    fn commit(repository: &Path) {
        git(repository, &["add", "."]);
        git(repository, &["commit", "-m", "skills"]);
    }

    #[test]
    fn sparse_checkout_resolves_only_the_requested_monorepo_target() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        init_repository(repository.path());
        write_skill(repository.path(), "skills/one", "one");
        write_skill(repository.path(), "skills/two", "two");
        commit(repository.path());

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

    #[test]
    fn resolves_a_marketplace_skill_name_to_its_differently_named_directory() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        init_repository(repository.path());
        write_skill(repository.path(), "skills/find-skills", "find-skills");
        write_skill(repository.path(), "skills/other", "other");
        commit(repository.path());

        let downloaded = download_repository(
            repository.path().to_str().unwrap(),
            "find-skills",
            &ProxySettings::default(),
            staging.path(),
        )
        .unwrap();

        assert_eq!(downloaded.relative_root, "skills/find-skills");
        assert_eq!(downloaded.metadata.name, "find-skills");
        assert!(!downloaded.skill_root.join("../other/SKILL.md").is_file());
    }

    #[test]
    fn resolves_a_skill_declared_at_the_repository_root() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        init_repository(repository.path());
        write_skill(repository.path(), "", "root-skill");
        commit(repository.path());

        let downloaded = download_repository(
            repository.path().to_str().unwrap(),
            "root-skill",
            &ProxySettings::default(),
            staging.path(),
        )
        .unwrap();

        assert_eq!(downloaded.relative_root, ".");
        assert_eq!(downloaded.metadata.name, "root-skill");
    }

    #[test]
    fn reports_every_candidate_sharing_the_requested_skill_name() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        init_repository(repository.path());
        write_skill(repository.path(), "a/duplicate", "duplicate");
        write_skill(repository.path(), "b/duplicate", "duplicate");
        commit(repository.path());

        let error = download_repository(
            repository.path().to_str().unwrap(),
            "duplicate",
            &ProxySettings::default(),
            staging.path(),
        )
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::AmbiguousSkill);
        assert_eq!(
            error.details.unwrap()["candidates"],
            serde_json::json!(["a/duplicate", "b/duplicate"])
        );
    }

    #[test]
    fn rejects_repositories_without_the_requested_skill() {
        let repository = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        init_repository(repository.path());
        write_skill(repository.path(), "skills/present", "present");
        commit(repository.path());

        let error = download_repository(
            repository.path().to_str().unwrap(),
            "absent",
            &ProxySettings::default(),
            staging.path(),
        )
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidSkill);
    }
}
