use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::{DownloadObserver, DownloadProgress, DownloadedSkill};
use crate::core::central_repo::resolve_within;
use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::metadata::{discover_single, parse_skill_md_content};
use crate::core::model::{ProxyMode, ProxySettings, SkillSource};
use crate::core::operations::cancelled_error;

const TEMP_DIR_PREFIX: &str = "skills-manager-nsh-github-";
const MAX_SKILL_MD_BYTES: usize = 1024 * 1024;
const MAX_CANDIDATES: usize = 256;
const MAX_GIT_STDERR_BYTES: usize = 8_000;
const MAX_GIT_STDERR_LINE_BYTES: usize = 4_096;
const GIT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROGRESS_SCALE: u64 = 100;
const CLONE_PROGRESS: (u64, u64) = (0, 40);
const CHECKOUT_PROGRESS: (u64, u64) = (50, 100);

struct SkillTreeEntry {
    /// The repository-relative directory of a SKILL.md; empty for the repository root.
    root: String,
    oid: String,
}

pub fn download(
    source: &SkillSource,
    proxy: &ProxySettings,
    staging_dir: &Path,
    observer: &dyn DownloadObserver,
) -> Result<DownloadedSkill> {
    let url = source
        .url
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("GitHub source URL is required"))?;
    let subpath = source
        .subpath
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("GitHub source subpath is required"))?;
    download_repository(url, subpath, proxy, staging_dir, observer)
}

fn download_repository(
    url: &str,
    subpath: &str,
    proxy: &ProxySettings,
    staging_dir: &Path,
    observer: &dyn DownloadObserver,
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
            OsString::from("--progress"),
            OsString::from("--"),
            OsString::from(url),
            checkout.as_os_str().to_owned(),
        ],
        proxy,
        true,
        Some(CLONE_PROGRESS),
        observer,
    )?;

    let skill_root = resolve_skill_root(&checkout, subpath, proxy, observer)?;
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
            None,
            observer,
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
            None,
            observer,
        )?;
    }
    run_git(
        [
            OsString::from("-C"),
            checkout.as_os_str().to_owned(),
            OsString::from("checkout"),
            OsString::from("--detach"),
            OsString::from("--progress"),
        ],
        proxy,
        false,
        Some(CHECKOUT_PROGRESS),
        observer,
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
        None,
        observer,
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
fn resolve_skill_root(
    checkout: &Path,
    subpath: &str,
    proxy: &ProxySettings,
    observer: &dyn DownloadObserver,
) -> Result<String> {
    let entries = list_skill_candidates(checkout, proxy, observer)?;
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
    let names = read_candidate_names(checkout, &entries, proxy, observer)?;

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
fn list_skill_candidates(
    checkout: &Path,
    proxy: &ProxySettings,
    observer: &dyn DownloadObserver,
) -> Result<Vec<SkillTreeEntry>> {
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
        None,
        observer,
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
    observer: &dyn DownloadObserver,
) -> Result<Vec<Option<String>>> {
    observer.check_cancelled()?;
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
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| git_error("git did not provide stdin", false))?;
    let oids: Vec<String> = entries.iter().map(|entry| entry.oid.clone()).collect();
    // A blob-filtered clone fetches the blobs this asks for over the network,
    // and stdout must keep draining while that happens.
    let writer = thread::spawn(move || {
        for oid in oids {
            if writeln!(stdin, "{oid}").is_err() {
                break;
            }
        }
    });
    let output = wait_git_child(&mut child, None, observer);
    let _ = writer.join();
    let output = output?;
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
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
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

/// Runs git and returns what it printed; a non-zero exit becomes an error.
fn run_git(
    args: impl IntoIterator<Item = OsString>,
    proxy: &ProxySettings,
    retryable: bool,
    progress_span: Option<(u64, u64)>,
    observer: &dyn DownloadObserver,
) -> Result<Output> {
    observer.check_cancelled()?;
    let mut child = git_command(proxy)?
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| git_error(format!("failed to run git: {error}"), false))?;
    let output = wait_git_child(&mut child, progress_span, observer)?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail: String = stderr.trim().chars().take(MAX_GIT_STDERR_BYTES).collect();
    Err(git_error(
        if detail.is_empty() {
            format!("git exited with status {}", output.status)
        } else {
            format!("git failed: {detail}")
        },
        retryable,
    ))
}

/// Reads both pipes in worker threads while git runs. The loop forwards git's
/// transfer progress and kills the whole process tree as soon as the operation
/// is cancelled, so `git-remote-https` does not linger.
fn wait_git_child(
    child: &mut Child,
    progress_span: Option<(u64, u64)>,
    observer: &dyn DownloadObserver,
) -> Result<Output> {
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let (progress_sender, progress_receiver) = mpsc::channel::<GitProgress>();
    let stderr_reader = thread::spawn(move || read_git_stderr(stderr, progress_sender));

    let mut cancelled = false;
    let mut status: Option<ExitStatus> = None;
    let mut bytes: Option<u64> = None;
    while status.is_none() {
        while let Ok(update) = progress_receiver.try_recv() {
            if update.bytes.is_some() {
                bytes = update.bytes;
            }
            if let Err(error) = report_git_progress(observer, progress_span, update, bytes) {
                terminate_tree(child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(error);
            }
        }
        match child.try_wait() {
            Ok(Some(exit)) => status = Some(exit),
            Ok(None) => {}
            Err(error) => {
                terminate_tree(child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(git_error(format!("failed to wait for git: {error}"), false));
            }
        }
        if status.is_none() && observer.check_cancelled().is_err() {
            cancelled = true;
            terminate_tree(child);
            break;
        }
        if status.is_none() {
            thread::sleep(GIT_POLL_INTERVAL);
        }
    }

    let stdout = stdout_reader
        .join()
        .map_err(|_| AppError::internal("git stdout reader panicked"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| AppError::internal("git stderr reader panicked"))?;
    if cancelled {
        return Err(cancelled_error());
    }
    let status = status.expect("the loop only ends with an exit status or a cancellation");
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn report_git_progress(
    observer: &dyn DownloadObserver,
    span: Option<(u64, u64)>,
    update: GitProgress,
    bytes: Option<u64>,
) -> Result<()> {
    let Some((start, end)) = span else {
        return Ok(());
    };
    let width = end.saturating_sub(start);
    // Files are written after the transfer finishes, so it keeps the tail of
    // the span and the percentage never moves backwards inside one command.
    let (from, to) = match update.kind {
        GitProgressKind::Transfer => (start, start + width * 9 / 10),
        GitProgressKind::Checkout => (start + width * 9 / 10, end),
    };
    let overall = from
        + update
            .current
            .min(update.total)
            .saturating_mul(to.saturating_sub(from))
            .checked_div(update.total)
            .unwrap_or(0);
    observer.report(DownloadProgress {
        current: overall.min(end),
        total: Some(PROGRESS_SCALE),
        bytes,
        bytes_total: None,
    })
}

fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = reader.read_to_end(&mut bytes);
    bytes
}

/// One progress line git rewrites in place with `\r` (or ends with `\n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitProgressKind {
    Transfer,
    Checkout,
}

#[derive(Debug, Clone, Copy)]
struct GitProgress {
    current: u64,
    total: u64,
    kind: GitProgressKind,
    /// The transferred amount git reports next to the ratio, in bytes.
    bytes: Option<u64>,
}

/// Collects stderr for error messages and forwards every transfer progress
/// line, splitting on `\r` as well as `\n` because git redraws them in place.
fn read_git_stderr(mut reader: ChildStderr, sender: mpsc::Sender<GitProgress>) -> Vec<u8> {
    let mut collected = Vec::new();
    let mut line = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 4096];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => break,
        };
        for byte in &buffer[..read] {
            if matches!(*byte, b'\n' | b'\r') {
                if !line.is_empty() {
                    if let Ok(text) = std::str::from_utf8(&line)
                        && let Some(update) = parse_git_progress(text)
                    {
                        let _ = sender.send(update);
                    }
                    if !truncated && collected.len() + line.len() < MAX_GIT_STDERR_BYTES {
                        collected.extend_from_slice(&line);
                        collected.push(b'\n');
                    }
                    line.clear();
                    truncated = false;
                }
            } else if line.len() < MAX_GIT_STDERR_LINE_BYTES {
                line.push(*byte);
            } else {
                truncated = true;
            }
        }
    }
    if !truncated && !line.is_empty() && collected.len() + line.len() < MAX_GIT_STDERR_BYTES {
        collected.extend_from_slice(&line);
    }
    collected
}

fn parse_git_progress(line: &str) -> Option<GitProgress> {
    let (label, rest) = line.split_once(':')?;
    let kind = match label.trim() {
        "Receiving objects" => GitProgressKind::Transfer,
        "Updating files" | "Checking out files" => GitProgressKind::Checkout,
        _ => return None,
    };
    let open = rest.find('(')?;
    let close = open + rest[open..].find(')')?;
    let (current, total) = rest[open + 1..close].split_once('/')?;
    let current = current.trim().parse::<u64>().ok()?;
    let total = total.trim().parse::<u64>().ok()?;
    if total == 0 {
        return None;
    }
    Some(GitProgress {
        current,
        total,
        kind,
        bytes: parse_git_bytes(&rest[close + 1..]),
    })
}

/// Reads git's `, 5.60 MiB` suffix: the ratio above counts objects, this is
/// what actually crossed the wire.
fn parse_git_bytes(rest: &str) -> Option<u64> {
    let amount = rest.split('|').next()?.trim();
    let amount = amount.trim_start_matches(',').trim();
    let (number, unit) = amount.split_once(char::is_whitespace)?;
    let number = number.trim().parse::<f64>().ok()?;
    let factor = match unit.trim() {
        "bytes" | "byte" | "B" => 1,
        "KiB" | "KB" => 1024,
        "MiB" | "MB" => 1024 * 1024,
        "GiB" | "GB" => 1024 * 1024 * 1024,
        "TiB" | "TB" => 1024_u64.pow(4),
        _ => return None,
    };
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    Some((number * factor as f64) as u64)
}

/// `git` leaves transport helpers behind; killing the process tree stops them
/// from holding files in the temporary checkout on Windows.
fn terminate_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // `taskkill` can miss a direct child that exited between the two
        // calls; killing it as well guarantees its pipes close.
        let _ = child.kill();
    }
    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn git_error(message: impl Into<String>, retryable: bool) -> AppError {
    AppError::new(ErrorCode::Git, message).retryable(retryable)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::core::sources::SilentObserver;

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
            &SilentObserver,
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
            &SilentObserver,
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
            &SilentObserver,
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
            &SilentObserver,
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
            &SilentObserver,
        )
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidSkill);
    }

    #[test]
    fn parses_git_transfer_progress() {
        let receiving =
            parse_git_progress("Receiving objects:  45% (450/1001), 1.20 MiB | 2.00 MiB/s")
                .unwrap();
        assert_eq!((receiving.current, receiving.total), (450, 1001));
        let updating = parse_git_progress("Updating files: 100% (10/10), done.").unwrap();
        assert_eq!((updating.current, updating.total), (10, 10));
        assert!(parse_git_progress("remote: Enumerating objects: 10, done.").is_none());
        assert!(parse_git_progress("Receiving objects:  45%").is_none());
        assert!(parse_git_progress("Receiving objects: 0% (0/0)").is_none());
    }
}
