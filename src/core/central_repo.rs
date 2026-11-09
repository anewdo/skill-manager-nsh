use std::fs;
use std::path::{Component, Path, PathBuf};

use super::error::{AppError, Result};
use super::model::Category;

#[derive(Debug, Clone)]
pub struct CentralRepo {
    root: PathBuf,
}

impl CentralRepo {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_location() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::internal("cannot determine the user home directory"))?;
        Ok(Self::new(home.join(".skills-manager-nsh")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join("skills-manager.db")
    }

    pub fn skills_root(&self, category: Category) -> PathBuf {
        self.root.join("skills").join(category.as_str())
    }

    pub fn git_cache_dir(&self) -> PathBuf {
        self.root.join("cache").join("git")
    }

    pub fn market_cache_dir(&self) -> PathBuf {
        self.root.join("cache").join("market")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn ensure(&self) -> Result<()> {
        for path in [
            self.root.clone(),
            self.skills_root(Category::Marketplace),
            self.skills_root(Category::Local),
            self.git_cache_dir(),
            self.market_cache_dir(),
            self.staging_dir(),
            self.logs_dir(),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    pub fn installation_path(&self, category: Category, relative: &str) -> Result<PathBuf> {
        resolve_within(&self.skills_root(category), relative)
    }

    pub fn staging_path(&self, relative: &str) -> Result<PathBuf> {
        resolve_within(&self.staging_dir(), relative)
    }
}

pub fn resolve_within(base: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() {
        return Err(AppError::unsafe_path("relative path cannot be empty"));
    }

    if relative
        .split(['/', '\\'])
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(AppError::unsafe_path(format!(
            "unsafe relative path: {relative}"
        )));
    }

    let path = Path::new(relative);
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => names.push(name),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(AppError::unsafe_path(format!(
                    "unsafe relative path: {relative}"
                )));
            }
        }
    }

    if names.is_empty() {
        return Err(AppError::unsafe_path("relative path cannot be empty"));
    }

    let mut resolved = base.to_path_buf();
    for name in names {
        resolved.push(name);
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::unsafe_path(format!(
                    "symbolic links are not allowed: {}",
                    resolved.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_only_the_independent_repository_layout() {
        let temp = tempfile::tempdir().unwrap();
        let sentinel = temp.path().join(".skills-manager");
        fs::write(&sentinel, b"do not touch").unwrap();
        let repo = CentralRepo::new(temp.path().join(".skills-manager-nsh"));

        repo.ensure().unwrap();

        assert!(repo.database_path().ends_with("skills-manager.db"));
        assert!(repo.skills_root(Category::Marketplace).is_dir());
        assert!(repo.skills_root(Category::Local).is_dir());
        assert!(repo.git_cache_dir().is_dir());
        assert!(repo.market_cache_dir().is_dir());
        assert!(repo.staging_dir().is_dir());
        assert!(repo.logs_dir().is_dir());
        assert!(!repo.root().join("trash").exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"do not touch");
    }

    #[test]
    fn rejects_absolute_and_traversing_paths() {
        let temp = tempfile::tempdir().unwrap();
        for path in ["../escape", "a/../../escape", ".", "a/./b", "/absolute"] {
            assert!(
                resolve_within(temp.path(), path).is_err(),
                "accepted {path}"
            );
        }
        #[cfg(windows)]
        assert!(resolve_within(temp.path(), r"C:\absolute").is_err());
    }

    #[test]
    fn resolves_normal_nested_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = resolve_within(temp.path(), "@owner/repo/skill").unwrap();
        assert_eq!(path, temp.path().join("@owner/repo/skill"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_ancestors() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("linked")).unwrap();

        let error = resolve_within(temp.path(), "linked/skill").unwrap_err();
        assert_eq!(error.code, super::super::error::ErrorCode::UnsafePath);
    }
}
