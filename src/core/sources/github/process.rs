use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::core::error::{AppError, ErrorCode, Result};
use crate::core::http;
use crate::core::model::{ProxyMode, ProxySettings};
use crate::core::operations::cancelled_error;
use crate::core::sources::common::{DownloadObserver, DownloadProgress};

const MAX_GIT_STDERR_BYTES: usize = 8_000;
const MAX_GIT_STDERR_LINE_BYTES: usize = 4_096;
const GIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) fn git_command(proxy: &ProxySettings) -> Result<Command> {
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
        .env("GCM_INTERACTIVE", "never")
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
pub(super) fn run_git(
    args: impl IntoIterator<Item = OsString>,
    proxy: &ProxySettings,
    retryable: bool,
    report_progress: bool,
    observer: &dyn DownloadObserver,
) -> Result<Output> {
    run_git_with_input(args, proxy, retryable, report_progress, None, observer)
}

pub(super) fn run_git_with_input(
    args: impl IntoIterator<Item = OsString>,
    proxy: &ProxySettings,
    retryable: bool,
    report_progress: bool,
    input: Option<Vec<u8>>,
    observer: &dyn DownloadObserver,
) -> Result<Output> {
    observer.check_cancelled()?;
    if report_progress {
        observer.report(DownloadProgress {
            current: 0,
            total: None,
            bytes: None,
            bytes_total: None,
            detail: Some("starting git".to_string()),
        })?;
    }
    let mut child = git_command(proxy)?
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| git_error(format!("failed to run git: {error}"), false))?;
    let writer = input.map(|input| {
        let mut stdin = child.stdin.take().expect("stdin was piped");
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        })
    });
    let output = wait_git_child(&mut child, report_progress, observer);
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let output = output?;
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
pub(super) fn wait_git_child(
    child: &mut Child,
    report_progress: bool,
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
            if let Err(error) = report_git_progress(observer, report_progress, update, bytes) {
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

    if status.is_some_and(|exit| !exit.success()) {
        terminate_tree(child);
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
    while let Ok(update) = progress_receiver.try_recv() {
        if update.bytes.is_some() {
            bytes = update.bytes;
        }
        report_git_progress(observer, report_progress, update, bytes)?;
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
    report_progress: bool,
    update: GitProgress,
    bytes: Option<u64>,
) -> Result<()> {
    if !report_progress {
        return Ok(());
    }
    observer.report(DownloadProgress {
        current: bytes.unwrap_or(0),
        total: None,
        bytes,
        bytes_total: None,
        detail: Some(format!(
            "{} ({}/{})",
            match update.kind {
                GitProgressKind::Transfer => "Receiving objects",
                GitProgressKind::Checkout => "Writing files",
            },
            update.current,
            update.total
        )),
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

pub(super) fn git_error(message: impl Into<String>, retryable: bool) -> AppError {
    let message = message.into();
    let lower = message.to_ascii_lowercase();
    let reason = if lower.contains("429") || lower.contains("rate limit") {
        "rate_limited"
    } else if lower.contains("repository not found")
        || lower.contains("does not appear to be a git repository")
    {
        "repository_not_found"
    } else if lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("403")
        || lower.contains("401")
    {
        "authentication_required"
    } else if lower.contains("couldn't find remote ref")
        || lower.contains("not a valid object name")
    {
        "reference_not_found"
    } else {
        "network"
    };
    AppError::new(ErrorCode::Git, message)
        .retryable(
            retryable
                && !matches!(
                    reason,
                    "authentication_required" | "repository_not_found" | "reference_not_found"
                ),
        )
        .with_details(serde_json::json!({ "reason": reason }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_failures_keep_distinct_reasons_and_retryability() {
        for (message, reason, retryable) in [
            ("HTTP 429 rate limit", "rate_limited", true),
            ("Authentication failed", "authentication_required", false),
            (
                "Repository not found; could not read Username",
                "repository_not_found",
                false,
            ),
            (
                "couldn't find remote ref refs/heads/missing",
                "reference_not_found",
                false,
            ),
            ("Could not resolve host", "network", true),
        ] {
            let error = git_error(message, true);
            assert_eq!(error.details.unwrap()["reason"], reason);
            assert_eq!(error.retryable, retryable);
        }
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
