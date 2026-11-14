use std::collections::HashSet;
use std::fs::{OpenOptions, create_dir_all};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use super::error::{AppError, ErrorCode, Result};
use super::skill_path::{PathBudget, safe_relative_path};
use super::sources::common::DownloadObserver;

const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;

pub(crate) fn extract_zip(
    bytes: &[u8],
    destination: &Path,
    observer: &dyn DownloadObserver,
) -> Result<()> {
    extract_zip_with_limits(
        bytes,
        destination,
        MAX_EXTRACTED_BYTES,
        MAX_ARCHIVE_ENTRIES,
        observer,
    )
}

fn extract_zip_with_limits(
    bytes: &[u8],
    destination: &Path,
    max_bytes: u64,
    max_entries: usize,
    observer: &dyn DownloadObserver,
) -> Result<()> {
    reject_duplicate_archive_entries(bytes)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() > max_entries {
        return Err(invalid_skill("archive contains too many entries"));
    }

    let mut declared_bytes = 0u64;
    let mut paths = PathBudget::default();
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        observer.check_cancelled()?;
        let entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().trim_end_matches('/');
        paths.add(name)?;
        if !seen.insert(name.to_owned()) {
            return Err(invalid_skill("archive contains duplicate paths"));
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                return Err(AppError::unsafe_path(
                    "archive contains a link or special file",
                ));
            }
        }
        declared_bytes = declared_bytes
            .checked_add(entry.size())
            .ok_or_else(|| invalid_skill("archive size overflow"))?;
        if declared_bytes > max_bytes {
            return Err(invalid_skill("archive exceeds extraction size limit"));
        }
    }

    let mut remaining = max_bytes;
    for index in 0..archive.len() {
        observer.check_cancelled()?;
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let relative = safe_relative_path(entry.name().trim_end_matches('/'))?;
        let target = destination.join(relative);
        if entry.is_dir() {
            create_dir_all(target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let written = copy_with_cancel(&mut entry, &mut file, remaining, observer)?;
        remaining -= written;
        let mut extra = [0u8; 1];
        if entry.read(&mut extra)? != 0 {
            return Err(invalid_skill("archive exceeds extraction size limit"));
        }
    }
    Ok(())
}

/// Copies one archive entry block by block, so cancelling does not wait for a
/// whole file to be written.
fn copy_with_cancel(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
    observer: &dyn DownloadObserver,
) -> Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = limit;
    let mut written = 0u64;
    while remaining > 0 {
        observer.check_cancelled()?;
        let want = buffer.len().min(remaining as usize);
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        remaining -= read as u64;
        written += read as u64;
    }
    Ok(written)
}

fn reject_duplicate_archive_entries(bytes: &[u8]) -> Result<()> {
    const EOCD_LEN: usize = 22;
    const CENTRAL_HEADER_LEN: usize = 46;
    let Some(eocd) = (0..=bytes.len().saturating_sub(EOCD_LEN))
        .rev()
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(b"PK\x05\x06")
                && u16_at(bytes, *offset + 20).is_some_and(|comment_len| {
                    *offset + EOCD_LEN + comment_len as usize == bytes.len()
                })
        })
    else {
        return Ok(());
    };
    let entries = u16_at(bytes, eocd + 10)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?;
    let central_size = u32_at(bytes, eocd + 12)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?
        as usize;
    let central_offset = u32_at(bytes, eocd + 16)
        .ok_or_else(|| invalid_skill("archive has a truncated directory"))?
        as usize;
    if u16_at(bytes, eocd + 4) != Some(0)
        || u16_at(bytes, eocd + 6) != Some(0)
        || u16_at(bytes, eocd + 8) != Some(entries)
        || entries == u16::MAX
        || central_size == u32::MAX as usize
        || central_offset == u32::MAX as usize
    {
        return Err(invalid_skill(
            "multi-disk and ZIP64 skill archives are not supported",
        ));
    }
    let central_end = central_offset
        .checked_add(central_size)
        .filter(|end| *end <= eocd)
        .ok_or_else(|| invalid_skill("archive directory is out of bounds"))?;
    let mut cursor = central_offset;
    let mut seen = HashSet::new();
    for _ in 0..entries {
        if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") {
            return Err(invalid_skill("archive has an invalid central directory"));
        }
        let name_len = u16_at(bytes, cursor + 28)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let extra_len = u16_at(bytes, cursor + 30)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let comment_len = u16_at(bytes, cursor + 32)
            .ok_or_else(|| invalid_skill("archive has a truncated entry"))?
            as usize;
        let name_start = cursor + CENTRAL_HEADER_LEN;
        let name_end = name_start
            .checked_add(name_len)
            .filter(|end| *end <= central_end)
            .ok_or_else(|| invalid_skill("archive entry name is out of bounds"))?;
        let mut name = &bytes[name_start..name_end];
        while name.last() == Some(&b'/') {
            name = &name[..name.len() - 1];
        }
        if !seen.insert(name.to_vec()) {
            return Err(invalid_skill("archive contains duplicate paths"));
        }
        cursor = name_end
            .checked_add(extra_len)
            .and_then(|end| end.checked_add(comment_len))
            .filter(|end| *end <= central_end)
            .ok_or_else(|| invalid_skill("archive entry is out of bounds"))?;
    }
    Ok(())
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn invalid_skill(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidSkill, message)
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    invalid_skill(format!("invalid skill archive: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill_path::{MAX_PATH_BYTES, MAX_PATH_DEPTH};
    use crate::core::sources::common::SilentObserver;
    use zip::write::SimpleFileOptions;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn extracts_valid_bundle_at_exact_byte_limit() {
        let bytes = archive(&[("SKILL.md", b"skill"), ("references/guide.md", b"guide")]);
        let temp = tempfile::tempdir().unwrap();
        extract_zip_with_limits(&bytes, temp.path(), 10, 2, &SilentObserver).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("references/guide.md")).unwrap(),
            b"guide"
        );
    }

    #[test]
    fn rejects_traversal_links_and_duplicates_before_writing() {
        for name in [
            "../escaped",
            "/escaped",
            "nested/../../escaped",
            "C:/escaped",
            "nested\\escaped",
            "./escaped",
        ] {
            let bytes = archive(&[("SKILL.md", b"skill"), (name, b"bad")]);
            let temp = tempfile::tempdir().unwrap();
            assert!(
                extract_zip(&bytes, temp.path(), &SilentObserver).is_err(),
                "{name}"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }

        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink("linked", "../escaped", SimpleFileOptions::default())
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);

        let mut bytes = archive(&[("SKILL.md", b"one"), ("OTHER.md", b"two")]);
        for index in 0..=bytes.len() - b"OTHER.md".len() {
            if &bytes[index..index + b"OTHER.md".len()] == b"OTHER.md" {
                bytes[index..index + b"OTHER.md".len()].copy_from_slice(b"SKILL.md");
            }
        }
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn rejects_expansion_entry_count_and_path_budgets() {
        let bytes = archive(&[
            ("SKILL.md", &[b'a'; 4096]),
            ("references/guide.md", &[b'b'; 4096]),
        ]);
        for (max_bytes, max_entries) in [(8191, 2), (8192, 1)] {
            let temp = tempfile::tempdir().unwrap();
            assert!(
                extract_zip_with_limits(
                    &bytes,
                    temp.path(),
                    max_bytes,
                    max_entries,
                    &SilentObserver
                )
                .is_err()
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }

        let long = "a".repeat(MAX_PATH_BYTES + 1);
        let deep = format!("{}SKILL.md", "a/".repeat(MAX_PATH_DEPTH));
        for name in [long, deep] {
            let bytes = archive(&[("SKILL.md", b"skill"), (&name, b"bad")]);
            let temp = tempfile::tempdir().unwrap();
            assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn refuses_existing_files_and_file_directory_collisions() {
        let bytes = archive(&[("SKILL.md", b"second")]);
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("SKILL.md"), b"first").unwrap();
        assert!(extract_zip(&bytes, temp.path(), &SilentObserver).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("SKILL.md")).unwrap(),
            b"first"
        );

        for entries in [
            vec![("reference", b"a".as_slice()), ("reference/guide.md", b"b")],
            vec![("reference/guide.md", b"a".as_slice()), ("reference", b"b")],
        ] {
            let temp = tempfile::tempdir().unwrap();
            assert!(extract_zip(&archive(&entries), temp.path(), &SilentObserver).is_err());
        }
    }

    #[test]
    fn understated_entry_size_cannot_write_past_the_limit() {
        let mut bytes = archive(&[("SKILL.md", &vec![b'a'; 1_000_000])]);
        let local = bytes.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
        bytes[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
        let temp = tempfile::tempdir().unwrap();
        assert!(extract_zip_with_limits(&bytes, temp.path(), 100, 10, &SilentObserver).is_err());
        let written = std::fs::metadata(temp.path().join("SKILL.md"))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        assert!(written <= 100);
    }
}
