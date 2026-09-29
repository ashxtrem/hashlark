// SPDX-License-Identifier: GPL-3.0-or-later

//! Locations of Hashlark's data on disk.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Environment variable that overrides the data directory.
pub const DATA_DIR_ENV: &str = "HASHLARK_DATA_DIR";

const DB_FILE_NAME: &str = "hashlark.db";

/// Resolved on-disk locations.
#[derive(Debug, Clone)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    /// Resolves paths in priority order: `explicit`, then
    /// `$HASHLARK_DATA_DIR`, then the OS default
    /// (e.g. `%APPDATA%\Hashlark\data` on Windows).
    pub fn resolve(explicit: Option<&Path>) -> Result<Self> {
        let data_dir = match explicit {
            Some(dir) => dir.to_path_buf(),
            None => match std::env::var_os(DATA_DIR_ENV) {
                Some(dir) if !dir.is_empty() => PathBuf::from(dir),
                _ => directories::ProjectDirs::from("", "", "Hashlark")
                    .ok_or_else(|| Error::Config("could not determine a home directory".into()))?
                    .data_dir()
                    .to_path_buf(),
            },
        };
        Ok(Self { data_dir })
    }

    /// Uses `dir` as the data directory as-is.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(DB_FILE_NAME)
    }

    /// Creates the data directory if it does not exist.
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_dir_wins() {
        let paths = AppPaths::resolve(Some(Path::new("custom"))).unwrap();
        assert_eq!(paths.data_dir(), Path::new("custom"));
        assert_eq!(paths.db_path(), Path::new("custom").join("hashlark.db"));
    }

    #[test]
    fn os_default_is_under_hashlark_folder() {
        let dirs = directories::ProjectDirs::from("", "", "Hashlark").unwrap();
        let dir = dirs.data_dir();
        assert!(
            // Linux uses a lowercase folder name (`~/.local/share/hashlark`).
            dir.components()
                .any(|c| c.as_os_str().eq_ignore_ascii_case("Hashlark")),
            "unexpected default data dir: {}",
            dir.display()
        );
        #[cfg(windows)]
        assert!(dir.ends_with("Hashlark/data"), "{}", dir.display());
    }
}
