use std::collections::BTreeSet;

use super::error::{AppError, ErrorCode, Result};

const WINDOWS_RESERVED_BASENAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameKind {
    Same,
    CaseOnly,
    Regular,
}

pub fn allocate_marketplace_name(
    skill_name: &str,
    repository: &str,
    used_root_keys: &BTreeSet<String>,
) -> Result<String> {
    validate_component(skill_name)?;
    if !used_root_keys.contains(&root_key(skill_name)) {
        return Ok(skill_name.to_owned());
    }

    let scoped = scoped_name(repository, skill_name)?;
    if used_root_keys.contains(&root_key(&scoped)) {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!("installation name already exists: {scoped}"),
        ));
    }
    Ok(scoped)
}

pub fn scoped_name(repository: &str, skill_name: &str) -> Result<String> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() {
        return Err(AppError::invalid_input(format!(
            "repository must use owner/repo form: {repository}"
        )));
    }
    validate_component(owner)?;
    validate_component(repo)?;
    validate_component(skill_name)?;
    Ok(format!("@{owner}/{repo}/{skill_name}"))
}

pub fn validate_manual_name(name: &str) -> Result<()> {
    validate_component(name)
}

pub fn classify_rename(current: &str, new_name: &str) -> Result<RenameKind> {
    validate_manual_name(new_name)?;
    if current == new_name {
        return Ok(RenameKind::Same);
    }
    if current.to_lowercase() == new_name.to_lowercase() {
        return Ok(RenameKind::CaseOnly);
    }
    Ok(RenameKind::Regular)
}

pub fn root_key(relative_path: &str) -> String {
    let normalized = relative_path.replace('\\', "/");
    #[cfg(windows)]
    return normalized.to_lowercase();
    #[cfg(not(windows))]
    normalized
}

fn validate_component(name: &str) -> Result<()> {
    let invalid = name.is_empty()
        || name == "."
        || name == ".."
        || name != name.trim()
        || name.ends_with([' ', '.'])
        || name.encode_utf16().count() > 255
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        });
    if invalid {
        return Err(AppError::invalid_input(format!(
            "invalid installation name: {name:?}"
        )));
    }

    let basename = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if WINDOWS_RESERVED_BASENAMES.contains(&basename.as_str()) {
        return Err(AppError::invalid_input(format!(
            "installation name is reserved by Windows: {name}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn used(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| root_key(name)).collect()
    }

    #[test]
    fn allocates_plain_then_full_scoped_name_without_numeric_suffixes() {
        let names = used(&[]);
        assert_eq!(
            allocate_marketplace_name("demo", "owner/repo", &names).unwrap(),
            "demo"
        );

        let names = used(&["demo"]);
        assert_eq!(
            allocate_marketplace_name("demo", "owner/repo", &names).unwrap(),
            "@owner/repo/demo"
        );

        let names = used(&["demo", "@owner/repo/demo"]);
        let error = allocate_marketplace_name("demo", "owner/repo", &names).unwrap_err();
        assert_eq!(error.code, ErrorCode::Conflict);
    }

    #[test]
    fn scoped_name_is_a_real_relative_path() {
        assert_eq!(
            scoped_name("longbridge/nsh", "skill-name").unwrap(),
            "@longbridge/nsh/skill-name"
        );
        assert!(scoped_name("missing-repo", "skill-name").is_err());
        assert!(scoped_name("owner/repo/extra", "skill-name").is_err());
    }

    #[test]
    fn manual_rename_accepts_unicode_but_rejects_unsafe_windows_names() {
        for valid in ["技能-测试", "my skill (v2)", ".hidden", "name-1"] {
            assert!(validate_manual_name(valid).is_ok(), "rejected {valid}");
        }
        for invalid in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            "a:b",
            "a*b",
            "a?b",
            "a|b",
            "a\0b",
            " trailing",
            "trailing ",
            "trailing.",
            "CON",
            "nul.txt",
            "Com1",
        ] {
            assert!(
                validate_manual_name(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn distinguishes_case_only_renames() {
        assert_eq!(classify_rename("Skill", "Skill").unwrap(), RenameKind::Same);
        assert_eq!(
            classify_rename("Skill", "skill").unwrap(),
            RenameKind::CaseOnly
        );
        assert_eq!(
            classify_rename("Skill", "other").unwrap(),
            RenameKind::Regular
        );
    }

    #[test]
    fn root_key_normalizes_separators() {
        let key = root_key("@owner\\repo/Skill");
        assert!(!key.contains('\\'));
        #[cfg(windows)]
        assert_eq!(key, "@owner/repo/skill");
        #[cfg(not(windows))]
        assert_eq!(key, "@owner/repo/Skill");
    }
}
