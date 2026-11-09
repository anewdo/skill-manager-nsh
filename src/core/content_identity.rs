use std::borrow::Cow;
use std::fs;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::error::{AppError, ErrorCode, Result};

const IGNORED_NAMES: &[&str] = &[".git", ".DS_Store", "Thumbs.db"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEntry {
    pub relative_path: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSummary {
    pub hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
}

pub fn content_hash(root: &Path) -> Result<String> {
    Ok(content_summary(root)?.hash)
}

pub fn content_summary(root: &Path) -> Result<ContentSummary> {
    let entries = content_entries(root)?;
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    for entry in &entries {
        write_frame(&mut hasher, entry.relative_path.as_bytes());
        let content = fs::read(&entry.path)?;
        total_bytes += content.len() as u64;
        let normalized = normalize_content(&content);
        write_frame(&mut hasher, normalized.as_ref());
    }
    Ok(ContentSummary {
        hash: hex::encode(hasher.finalize()),
        file_count: entries.len() as u64,
        total_bytes,
    })
}

pub fn files_identical(left: &Path, right: &Path) -> Result<bool> {
    let left_entries = content_entries(left)?;
    let right_entries = content_entries(right)?;
    if left_entries.len() != right_entries.len()
        || left_entries
            .iter()
            .zip(&right_entries)
            .any(|(left, right)| left.relative_path != right.relative_path)
    {
        return Ok(false);
    }

    for (left, right) in left_entries.iter().zip(&right_entries) {
        let left_content = fs::read(&left.path)?;
        let right_content = fs::read(&right.path)?;
        if normalize_content(&left_content) != normalize_content(&right_content) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn content_entries(root: &Path) -> Result<Vec<ContentEntry>> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::unsafe_path(format!(
            "content root must be a real directory: {}",
            root.display()
        )));
    }

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !IGNORED_NAMES.contains(&entry.file_name().to_string_lossy().as_ref())
        });
    let mut entries = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(AppError::unsafe_path(format!(
                "symbolic links are not allowed: {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(AppError::unsafe_path(format!(
                "special files are not allowed: {}",
                entry.path().display()
            )));
        }
        entries.push(ContentEntry {
            relative_path: relative_path(root, entry.path())?,
            path: entry.into_path(),
        });
    }
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(entries)
}

fn normalize_content(content: &[u8]) -> Cow<'_, [u8]> {
    if content.contains(&0) {
        return Cow::Borrowed(content);
    }
    let Ok(text) = std::str::from_utf8(content) else {
        return Cow::Borrowed(content);
    };
    let needs_crlf_normalization = text.contains("\r\n");
    let needs_trailing_newline_normalization = text.ends_with('\n');
    if !needs_crlf_normalization && !needs_trailing_newline_normalization {
        return Cow::Borrowed(content);
    }
    let mut normalized = if needs_crlf_normalization {
        text.replace("\r\n", "\n")
    } else {
        text.to_owned()
    };
    if normalized.ends_with('\n') {
        normalized.pop();
    }
    Cow::Owned(normalized.into_bytes())
}

fn write_frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|error| AppError::unsafe_path(error.to_string()))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| AppError::unsafe_path("content path is not valid UTF-8"))?,
            ),
            _ => return Err(AppError::unsafe_path("content path escapes its root")),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, content: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn identical_trees_have_deterministic_hashes() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        for root in [left.path(), right.path()] {
            write(
                root,
                "SKILL.md",
                b"---\nname: demo\ndescription: Demo\n---\n",
            );
            write(root, "assets/.hidden", b"hidden");
            write(root, "assets/empty.bin", b"");
        }

        assert_eq!(
            content_hash(left.path()).unwrap(),
            content_hash(right.path()).unwrap()
        );
        assert!(files_identical(left.path(), right.path()).unwrap());
    }

    #[test]
    fn text_eol_and_one_trailing_newline_do_not_change_identity() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(left.path(), "SKILL.md", b"first\r\nsecond\r\n");
        write(right.path(), "SKILL.md", b"first\nsecond");

        assert_eq!(
            content_hash(left.path()).unwrap(),
            content_hash(right.path()).unwrap()
        );
        assert!(files_identical(left.path(), right.path()).unwrap());
    }

    #[test]
    fn extra_blank_lines_and_real_text_edits_still_change_identity() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(left.path(), "SKILL.md", b"first\n\n");
        write(right.path(), "SKILL.md", b"first");
        assert_ne!(
            content_hash(left.path()).unwrap(),
            content_hash(right.path()).unwrap()
        );

        write(left.path(), "SKILL.md", b"first\nsecond");
        write(right.path(), "SKILL.md", b"first\nchanged");
        assert!(!files_identical(left.path(), right.path()).unwrap());
    }

    #[test]
    fn binary_eol_bytes_are_never_normalized() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(left.path(), "blob.bin", b"\xff\r\n");
        write(right.path(), "blob.bin", b"\xff\n");

        assert_ne!(
            content_hash(left.path()).unwrap(),
            content_hash(right.path()).unwrap()
        );
        assert!(!files_identical(left.path(), right.path()).unwrap());
    }

    #[test]
    fn additions_deletions_renames_and_empty_files_change_identity() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "SKILL.md", b"skill");
        let baseline = content_hash(root.path()).unwrap();

        write(root.path(), "empty.txt", b"");
        let with_empty = content_hash(root.path()).unwrap();
        assert_ne!(baseline, with_empty);

        fs::rename(
            root.path().join("empty.txt"),
            root.path().join("renamed.txt"),
        )
        .unwrap();
        assert_ne!(with_empty, content_hash(root.path()).unwrap());

        fs::remove_file(root.path().join("renamed.txt")).unwrap();
        assert_eq!(baseline, content_hash(root.path()).unwrap());

        write(root.path(), "SKILL.md", b"changed");
        assert_ne!(baseline, content_hash(root.path()).unwrap());
    }

    #[test]
    fn only_explicit_system_files_are_excluded() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "SKILL.md", b"skill");
        let baseline = content_hash(root.path()).unwrap();
        for ignored in [".git/config", ".DS_Store", "nested/Thumbs.db"] {
            write(root.path(), ignored, b"ignored");
        }
        assert_eq!(baseline, content_hash(root.path()).unwrap());

        write(root.path(), ".gitignore", b"target/");
        assert_ne!(baseline, content_hash(root.path()).unwrap());
    }

    #[test]
    fn path_and_content_fields_are_length_framed() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(left.path(), "ab", b"c");
        write(right.path(), "a", b"bc");

        assert_ne!(
            content_hash(left.path()).unwrap(),
            content_hash(right.path()).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn identity_rejects_symbolic_links() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();

        assert_eq!(
            content_hash(root.path()).unwrap_err().code,
            ErrorCode::UnsafePath
        );
    }
}
