use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::error::{AppError, ErrorCode, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCandidate {
    pub root: PathBuf,
    pub relative_root: String,
    pub metadata: SkillMetadata,
}

#[derive(Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

pub fn parse_skill_md(skill_root: &Path) -> Result<SkillMetadata> {
    let path = skill_root.join("SKILL.md");
    let bytes = fs::read(&path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidSkill,
            format!("cannot read {}: {error}", path.display()),
        )
    })?;
    let content = String::from_utf8(bytes).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidSkill,
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    parse_skill_md_content(&content, &path.display().to_string())
}

pub fn parse_skill_md_content(content: &str, label: &str) -> Result<SkillMetadata> {
    let yaml = frontmatter(content).ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidSkill,
            format!("{label} has no complete YAML frontmatter"),
        )
    })?;
    let frontmatter: Frontmatter = serde_yaml::from_str(yaml).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidSkill,
            format!("{label} has invalid YAML frontmatter: {error}"),
        )
    })?;
    let name = frontmatter.name.unwrap_or_default().trim().to_owned();
    let description = frontmatter
        .description
        .unwrap_or_default()
        .trim()
        .to_owned();
    validate_name(&name)?;
    if description.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidSkill,
            "SKILL.md description is required",
        ));
    }
    if description.chars().count() > 1024 {
        return Err(AppError::new(
            ErrorCode::InvalidSkill,
            "SKILL.md description exceeds 1024 characters",
        ));
    }
    Ok(SkillMetadata { name, description })
}

pub fn discover_candidates(root: &Path) -> Result<Vec<SkillCandidate>> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::unsafe_path(format!(
            "Skill discovery root must be a real directory: {}",
            root.display()
        )));
    }

    let mut candidates = Vec::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || entry.file_name().to_string_lossy() != ".git");
    for entry in walker {
        let entry = entry.map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(AppError::unsafe_path(format!(
                "symbolic links are not allowed: {}",
                entry.path().display()
            )));
        }
        if !file_type.is_dir() && !file_type.is_file() {
            return Err(AppError::unsafe_path(format!(
                "special files are not allowed: {}",
                entry.path().display()
            )));
        }
        if !file_type.is_file() || entry.file_name().to_string_lossy() != "SKILL.md" {
            continue;
        }
        let Some(skill_root) = entry.path().parent() else {
            continue;
        };
        let Ok(metadata) = parse_skill_md(skill_root) else {
            continue;
        };
        candidates.push(SkillCandidate {
            root: skill_root.to_path_buf(),
            relative_root: relative_path(root, skill_root)?,
            metadata,
        });
    }
    candidates.sort_by(|left, right| left.relative_root.cmp(&right.relative_root));
    Ok(candidates)
}

pub fn discover_single(root: &Path) -> Result<SkillCandidate> {
    let candidates = discover_candidates(root)?;
    match candidates.len() {
        0 => Err(AppError::new(
            ErrorCode::InvalidSkill,
            format!("no valid SKILL.md candidate under {}", root.display()),
        )),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => Err(AppError::new(
            ErrorCode::AmbiguousSkill,
            "multiple valid SKILL.md candidates found",
        )
        .with_details(serde_json::json!({
            "candidates": candidates.iter().map(|candidate| &candidate.relative_root).collect::<Vec<_>>()
        }))),
    }
}

fn frontmatter(content: &str) -> Option<&str> {
    if content.lines().next()?.trim_end() != "---" {
        return None;
    }
    let start = content.find('\n')? + 1;
    let remainder = &content[start..];
    let mut offset = 0;
    for line in remainder.split_inclusive('\n') {
        if line.trim_end().is_empty() {
            offset += line.len();
            continue;
        }
        if line.trim_end() == "---" {
            return Some(&remainder[..offset]);
        }
        offset += line.len();
    }
    if remainder[offset..].trim_end() == "---" {
        return Some(&remainder[..offset]);
    }
    None
}

fn validate_name(name: &str) -> Result<()> {
    let length = name.chars().count();
    let valid = (1..=64).contains(&length)
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && name
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && !name.contains("--");
    if valid {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidSkill,
            "SKILL.md name must be 1-64 lowercase alphanumeric characters or single hyphens",
        ))
    }
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|error| AppError::unsafe_path(error.to_string()))?;
    if relative.as_os_str().is_empty() {
        return Ok(".".to_owned());
    }
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| AppError::unsafe_path("Skill path is not valid UTF-8"))?,
            ),
            _ => {
                return Err(AppError::unsafe_path(
                    "Skill path escapes its discovery root",
                ));
            }
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(root: &Path, name: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: A useful skill\n---\n# Body\n"),
        )
        .unwrap();
    }

    #[test]
    fn parses_required_frontmatter_without_requiring_resource_directories() {
        let temp = tempfile::tempdir().unwrap();
        write_skill(temp.path(), "directory-independent-name");

        let metadata = parse_skill_md(temp.path()).unwrap();
        assert_eq!(metadata.name, "directory-independent-name");
        assert_eq!(metadata.description, "A useful skill");
        assert!(!temp.path().join("scripts").exists());
    }

    #[test]
    fn rejects_missing_or_malformed_required_fields() {
        let temp = tempfile::tempdir().unwrap();
        for content in [
            "# No frontmatter",
            "---\nname: valid-name\n---\n",
            "---\ndescription: Missing name\n---\n",
            "---\nname: Invalid_Name\ndescription: Invalid name\n---\n",
            "---\nname: double--hyphen\ndescription: Invalid name\n---\n",
        ] {
            fs::write(temp.path().join("SKILL.md"), content).unwrap();
            assert!(parse_skill_md(temp.path()).is_err(), "accepted {content}");
        }
    }

    #[test]
    fn discovers_valid_nested_candidates_and_skips_invalid_ones() {
        let temp = tempfile::tempdir().unwrap();
        write_skill(&temp.path().join("z/last"), "last-skill");
        write_skill(&temp.path().join("a/first"), "first-skill");
        fs::create_dir_all(temp.path().join("invalid")).unwrap();
        fs::write(temp.path().join("invalid/SKILL.md"), "invalid").unwrap();

        let candidates = discover_candidates(temp.path()).unwrap();
        let names: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate.relative_root.as_str())
            .collect();
        assert_eq!(names, ["a/first", "z/last"]);
    }

    #[test]
    fn single_candidate_reports_empty_and_ambiguous_results() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            discover_single(empty.path()).unwrap_err().code,
            ErrorCode::InvalidSkill
        );

        write_skill(&empty.path().join("one"), "one");
        write_skill(&empty.path().join("two"), "two");
        let error = discover_single(empty.path()).unwrap_err();
        assert_eq!(error.code, ErrorCode::AmbiguousSkill);
        assert_eq!(
            error.details.unwrap()["candidates"],
            serde_json::json!(["one", "two"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn discovery_rejects_symbolic_links() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("linked")).unwrap();

        assert_eq!(
            discover_candidates(temp.path()).unwrap_err().code,
            ErrorCode::UnsafePath
        );
    }
}
