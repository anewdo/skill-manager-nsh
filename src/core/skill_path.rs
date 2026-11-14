use std::path::{Component, Path};

use super::error::{AppError, ErrorCode, Result};

pub(super) const MAX_PATH_BYTES: usize = 512;
pub(super) const MAX_PATH_DEPTH: usize = 16;
const MAX_TOTAL_PATH_BYTES: usize = 64 * 1024;
const MAX_TOTAL_COMPONENTS: usize = 16 * 1024;

pub(crate) fn safe_relative_path(path: &str) -> Result<&Path> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path.split('/').count() > MAX_PATH_DEPTH
        || path.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || part.ends_with(['.', ' '])
                || is_windows_device(part)
        })
    {
        return Err(AppError::unsafe_path("invalid skill file path"));
    }
    let path = Path::new(path);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::unsafe_path("invalid skill file path"));
    }
    Ok(path)
}

fn is_windows_device(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        || (stem.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
        }) && matches!(
            stem.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        ))
}

#[derive(Default)]
pub(crate) struct PathBudget {
    bytes: usize,
    components: usize,
}

impl PathBudget {
    pub(crate) fn add(&mut self, path: &str) -> Result<()> {
        safe_relative_path(path)?;
        self.bytes += path.len();
        self.components += path.split('/').count();
        if self.bytes > MAX_TOTAL_PATH_BYTES || self.components > MAX_TOTAL_COMPONENTS {
            return Err(AppError::new(
                ErrorCode::InvalidSkill,
                "skill exceeds total path budget",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_windows_special_names_but_preserves_unicode() {
        for name in [
            "NUL.md",
            "con",
            "COM1.txt",
            "LPT9",
            "COM¹.md",
            "file.",
            "file ",
            "nested/aux.txt",
            "nul\0.txt",
            "a\nb",
        ] {
            assert!(safe_relative_path(name).is_err(), "{name:?}");
        }
        assert_eq!(
            safe_relative_path("参考/é.md").unwrap(),
            Path::new("参考/é.md")
        );
    }
}
