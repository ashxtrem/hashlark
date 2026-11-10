// SPDX-License-Identifier: GPL-3.0-or-later

//! Which folders the client may read and write (plan §2), and the default
//! download folders.

use std::path::{Component, Path, PathBuf};

use hashlark_core::{Error, Result};

/// Folders the client may use for downloads, the watched folder and
/// creating torrents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PathPolicy {
    /// Desktop and Android: any absolute path.
    #[default]
    Unrestricted,
    /// Server: only paths inside one of these folders.
    Roots(Vec<PathBuf>),
}

impl PathPolicy {
    /// Checks `path` and returns it with `.` parts removed.
    ///
    /// The path must be absolute and free of `..`. With [`Roots`](Self::Roots),
    /// it must also lie inside a root once symlinks are resolved: the deepest
    /// existing ancestor is canonicalized, so a link inside a root that
    /// points outside it is rejected.
    pub fn check(&self, path: &Path) -> Result<PathBuf> {
        if !path.is_absolute() {
            return Err(Error::Invalid(format!(
                "`{}` is not an absolute path",
                path.display()
            )));
        }
        if path.components().any(|c| c == Component::ParentDir) {
            return Err(Error::Invalid(format!(
                "`{}` must not contain `..`",
                path.display()
            )));
        }
        let clean: PathBuf = path
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect();
        let Self::Roots(roots) = self else {
            return Ok(clean);
        };
        let resolved = resolve(&clean)?;
        let inside = roots.iter().any(|root| {
            let root = if root.is_absolute() {
                resolve(root).ok()
            } else {
                None
            };
            root.is_some_and(|root| resolved.starts_with(root))
        });
        if inside {
            Ok(clean)
        } else {
            Err(Error::Invalid(format!(
                "`{}` is outside the allowed folders",
                path.display()
            )))
        }
    }

    /// True when `path` passes [`check`](Self::check).
    pub fn allows(&self, path: &Path) -> bool {
        self.check(path).is_ok()
    }
}

/// Canonicalizes the deepest existing ancestor of an absolute, `..`-free
/// path and appends the rest.
fn resolve(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut rest = Vec::new();
    loop {
        match existing.canonicalize() {
            Ok(real) => {
                let mut out = real;
                for part in rest.iter().rev() {
                    out.push(part);
                }
                return Ok(out);
            }
            Err(_) => {
                let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
                    return Err(Error::Invalid(format!(
                        "`{}` can't be resolved",
                        path.display()
                    )));
                };
                rest.push(name.to_owned());
                existing = parent;
            }
        }
    }
}

/// Desktop default download folder: `<OS Downloads>/Hashlark`.
pub fn desktop_download_dir() -> Option<PathBuf> {
    directories::UserDirs::new()?
        .download_dir()
        .map(|d| d.join("Hashlark"))
}

/// Server default download folder: `<data>/downloads`.
pub fn server_download_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("downloads")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(dir: &Path) -> PathPolicy {
        PathPolicy::Roots(vec![dir.join("root")])
    }

    #[test]
    fn unrestricted_takes_any_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = PathPolicy::Unrestricted;
        assert!(p.allows(&dir.path().join("x").join("y")));
        assert!(!p.allows(Path::new("relative/dir")));
        assert!(!p.allows(&dir.path().join("a").join("..").join("b")));
    }

    #[test]
    fn roots_accept_inside_and_reject_outside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("root").join("sub")).unwrap();
        std::fs::create_dir_all(dir.path().join("other")).unwrap();
        let p = roots(dir.path());
        assert!(p.allows(&dir.path().join("root")));
        assert!(p.allows(&dir.path().join("root").join("sub")));
        // Not yet existing folders inside a root are fine.
        assert!(p.allows(&dir.path().join("root").join("new").join("deeper")));
        assert!(!p.allows(&dir.path().join("other")));
        assert!(!p.allows(dir.path()));
        // A sibling whose name starts with the root's name is outside.
        assert!(!p.allows(&dir.path().join("rootkit")));
    }

    #[test]
    fn roots_reject_traversal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("root")).unwrap();
        let p = roots(dir.path());
        let sneaky = dir.path().join("root").join("..").join("other");
        let err = p.check(&sneaky).unwrap_err();
        assert!(matches!(err, Error::Invalid(m) if m.contains("..")));
        assert!(!p.allows(&dir.path().join("root").join("a").join("..").join("..")));
        assert!(p.allows(&dir.path().join("root").join(".").join("a")));
    }

    #[test]
    fn roots_reject_symlink_escape() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let link = root.join("escape");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, &link);
        if let Err(e) = made {
            // Windows needs Developer Mode or admin rights for symlinks.
            eprintln!("skipping symlink escape test: {e}");
            return;
        }
        let p = roots(dir.path());
        assert!(!p.allows(&link));
        assert!(!p.allows(&link.join("new-folder")));
        assert!(p.allows(&root.join("plain")));
    }

    #[test]
    fn default_folders() {
        let data = Path::new("/data");
        assert_eq!(
            server_download_dir(data),
            Path::new("/data").join("downloads")
        );
        if let Some(d) = desktop_download_dir() {
            assert!(d.ends_with("Hashlark"));
        }
    }
}
