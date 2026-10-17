#![forbid(unsafe_code)]

use std::path::{Component, Path, PathBuf};

use crate::errors::AppError;
use crate::fsops::{
    CancelToken, FileAttributes, FileFingerprint, FsBackend, FsEntry, SameVolumeMoveOutcome,
    VolumeInfo,
};

/// Sentinel filename marking an authorized filesystem test sandbox (AGENTS.md §13.1).
pub const SANDBOX_SENTINEL: &str = ".sortdeck-test-sandbox";

/// Guards test filesystem backends against touching any path outside an
/// explicitly initialized sandbox containing `.sortdeck-test-sandbox`.
pub struct SandboxGuard;

impl SandboxGuard {
    /// Seeds [`SANDBOX_SENTINEL`] inside an existing directory to mark it as an
    /// authorized filesystem test sandbox.
    pub fn init_sandbox_dir(root: &Path) -> Result<(), AppError> {
        let sentinel_path = root.join(SANDBOX_SENTINEL);
        // Confined to sandbox initialization inside fsops/sandbox.rs per AGENTS.md §2.6.
        #[allow(clippy::disallowed_methods)]
        std::fs::write(&sentinel_path, b"sortdeck-test-sandbox\n")
            .map_err(|err| AppError::from_io("write sandbox sentinel", err))
    }

    /// Validates that `path` is absolute, contains no `..` traversal, and lies
    /// within a directory tree containing [`SANDBOX_SENTINEL`].
    pub fn validate_path(path: &Path) -> Result<PathBuf, AppError> {
        if !path.is_absolute() {
            return Err(AppError::SandboxViolation {
                path: path.display().to_string(),
                reason: "path must be absolute inside test sandbox".to_owned(),
            });
        }

        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AppError::SandboxViolation {
                path: path.display().to_string(),
                reason: "parent directory traversal ('..') is forbidden in sandbox paths"
                    .to_owned(),
            });
        }

        let Some(sandbox_root) = find_sentinel_root(path) else {
            return Err(AppError::SandboxViolation {
                path: path.display().to_string(),
                reason: format!("no ancestor directory contains {SANDBOX_SENTINEL}"),
            });
        };

        // Confined canonicalization inside fsops/sandbox.rs per AGENTS.md §2.6
        // to defeat symlink/junction escapes out of the sandbox root.
        #[allow(clippy::disallowed_methods)]
        let canonical_root = std::fs::canonicalize(&sandbox_root)
            .map_err(|err| AppError::from_io("canonicalize sandbox root", err))?;

        let existing_ancestor =
            find_deepest_existing_ancestor(path).ok_or_else(|| AppError::SandboxViolation {
                path: path.display().to_string(),
                reason: "no existing ancestor found for path".to_owned(),
            })?;

        #[allow(clippy::disallowed_methods)]
        let canonical_ancestor = std::fs::canonicalize(&existing_ancestor)
            .map_err(|err| AppError::from_io("canonicalize sandbox path ancestor", err))?;

        if !canonical_ancestor.starts_with(&canonical_root) {
            return Err(AppError::SandboxViolation {
                path: path.display().to_string(),
                reason: "canonical ancestor escapes sandbox root".to_owned(),
            });
        }

        Ok(sandbox_root)
    }
}

fn find_sentinel_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };

    while let Some(dir) = current {
        if dir.join(SANDBOX_SENTINEL).is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }

    None
}

fn find_deepest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// Wrapper around any [`FsBackend`] that enforces [`SandboxGuard::validate_path`]
/// on every path parameter before delegating.
pub struct SandboxFsBackend<B: FsBackend> {
    inner: B,
}

impl<B: FsBackend> SandboxFsBackend<B> {
    #[must_use]
    pub const fn new(inner: B) -> Self {
        Self { inner }
    }
}

impl<B: FsBackend> FsBackend for SandboxFsBackend<B> {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.exists(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.canonicalize(path)
    }

    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.attributes(path)
    }

    fn fingerprint(&self, path: &Path) -> Result<FileFingerprint, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.fingerprint(path)
    }

    fn read_dir(&self, dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        SandboxGuard::validate_path(dir)?;
        self.inner.read_dir(dir)
    }

    fn move_no_overwrite(&self, src: &Path, dst: &Path) -> Result<SameVolumeMoveOutcome, AppError> {
        SandboxGuard::validate_path(src)?;
        SandboxGuard::validate_path(dst)?;
        self.inner.move_no_overwrite(src, dst)
    }

    fn copy_to_temp(
        &self,
        src: &Path,
        temp_dst: &Path,
        cancel: &CancelToken,
    ) -> Result<(), AppError> {
        SandboxGuard::validate_path(src)?;
        SandboxGuard::validate_path(temp_dst)?;
        self.inner.copy_to_temp(src, temp_dst, cancel)
    }

    fn flush_file(&self, path: &Path) -> Result<(), AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.flush_file(path)
    }

    fn hash_sha256(&self, path: &Path, cancel: &CancelToken) -> Result<String, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.hash_sha256(path, cancel)
    }

    fn delete_file(&self, path: &Path) -> Result<(), AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.delete_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<Vec<PathBuf>, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.create_dir_all(path)
    }

    fn remove_empty_dir(&self, path: &Path) -> Result<(), AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.remove_empty_dir(path)
    }

    fn volume_info(&self, path: &Path) -> Result<VolumeInfo, AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.volume_info(path)
    }

    fn check_access_for_move(&self, path: &Path) -> Result<(), AppError> {
        SandboxGuard::validate_path(path)?;
        self.inner.check_access_for_move(path)
    }
}

#[cfg(test)]
pub struct TestSandbox {
    dir: tempfile::TempDir,
}

#[cfg(test)]
impl TestSandbox {
    /// Creates a fresh temporary sandbox containing [`SANDBOX_SENTINEL`].
    pub fn new() -> Result<Self, AppError> {
        let dir =
            tempfile::tempdir().map_err(|err| AppError::from_io("create test sandbox", err))?;
        SandboxGuard::init_sandbox_dir(dir.path())?;
        Ok(Self { dir })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    #[must_use]
    pub fn child(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }
}

/// Minimal in-memory/stub backend used in Phase 0 sandbox guard tests before
/// the Phase 3 Win32 `FsBackend` implementation is added.
#[derive(Debug, Default)]
pub struct StubFsBackend;

impl FsBackend for StubFsBackend {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        Ok(path.exists())
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        Ok(path.to_path_buf())
    }

    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError> {
        Ok(FileAttributes {
            is_dir: path.is_dir(),
            ..FileAttributes::default()
        })
    }

    fn fingerprint(&self, _path: &Path) -> Result<FileFingerprint, AppError> {
        Ok(FileFingerprint {
            volume_serial: None,
            file_id: None,
            size: 0,
            modified_epoch_ms: None,
        })
    }

    fn read_dir(&self, _dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        Ok(Vec::new())
    }

    fn move_no_overwrite(
        &self,
        _src: &Path,
        _dst: &Path,
    ) -> Result<SameVolumeMoveOutcome, AppError> {
        Ok(SameVolumeMoveOutcome::Moved)
    }

    fn copy_to_temp(
        &self,
        _src: &Path,
        _temp_dst: &Path,
        _cancel: &CancelToken,
    ) -> Result<(), AppError> {
        Ok(())
    }

    fn flush_file(&self, _path: &Path) -> Result<(), AppError> {
        Ok(())
    }

    fn hash_sha256(&self, _path: &Path, _cancel: &CancelToken) -> Result<String, AppError> {
        Ok(String::new())
    }

    fn delete_file(&self, _path: &Path) -> Result<(), AppError> {
        Ok(())
    }

    fn create_dir_all(&self, _path: &Path) -> Result<Vec<PathBuf>, AppError> {
        Ok(Vec::new())
    }

    fn remove_empty_dir(&self, _path: &Path) -> Result<(), AppError> {
        Ok(())
    }

    fn volume_info(&self, _path: &Path) -> Result<VolumeInfo, AppError> {
        Ok(VolumeInfo {
            volume_serial: None,
            filesystem_name: "NTFS".to_owned(),
            supports_file_ids: true,
            supports_ads: true,
            max_component_len: 255,
            available_bytes: None,
        })
    }

    fn check_access_for_move(&self, _path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}
