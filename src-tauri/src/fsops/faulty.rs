#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::errors::AppError;
use crate::fsops::{
    CancelToken, FileAttributes, FileFingerprint, FsBackend, FsEntry, SameVolumeMoveOutcome,
    VolumeInfo,
};

/// Declarative fault-injection configuration for testing `safe_move`, undo,
/// and recovery failure paths (`AGENTS.md` §13.2).
#[derive(Debug, Clone, Default)]
pub struct FaultConfig {
    pub force_cross_volume: bool,
    pub inject_race_collision_once: bool,
    pub corrupt_temp_after_copy: bool,
    pub modify_source_during_copy: bool,
    pub fail_delete_source_with_sharing_violation: bool,
    pub fail_copy_with_disk_full: bool,
    pub cancel_during_copy: bool,
    pub fail_move_with_access_denied: bool,
    pub fail_move_with_sharing_violation: bool,
}

/// Fault-injecting wrapper around any [`FsBackend`] (usually `SandboxFsBackend<LocalFsBackend>`).
pub struct FaultyFsBackend<B: FsBackend> {
    inner: B,
    faults: Mutex<FaultConfig>,
}

impl<B: FsBackend> FaultyFsBackend<B> {
    #[must_use]
    pub fn new(inner: B, faults: FaultConfig) -> Self {
        Self {
            inner,
            faults: Mutex::new(faults),
        }
    }

    pub fn update_faults<F: FnOnce(&mut FaultConfig)>(&self, f: F) -> Result<(), AppError> {
        let mut guard = self.faults.lock().map_err(|_| AppError::Internal {
            message: "fault config mutex poisoned".to_owned(),
        })?;
        f(&mut guard);
        Ok(())
    }

    fn snapshot_faults(&self) -> Result<FaultConfig, AppError> {
        self.faults
            .lock()
            .map(|g| g.clone())
            .map_err(|_| AppError::Internal {
                message: "fault config mutex poisoned".to_owned(),
            })
    }
}

fn is_sortdeck_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with(".sortdeck-") && name.ends_with(".sdpart"))
}

impl<B: FsBackend> FsBackend for FaultyFsBackend<B> {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        self.inner.exists(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        self.inner.canonicalize(path)
    }

    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError> {
        self.inner.attributes(path)
    }

    fn fingerprint(&self, path: &Path) -> Result<FileFingerprint, AppError> {
        self.inner.fingerprint(path)
    }

    fn read_dir(&self, dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        self.inner.read_dir(dir)
    }

    fn move_no_overwrite(&self, src: &Path, dst: &Path) -> Result<SameVolumeMoveOutcome, AppError> {
        let (force_cross, inject_race, fail_access, fail_sharing) = {
            let mut cfg = self.faults.lock().map_err(|_| AppError::Internal {
                message: "fault config mutex poisoned".to_owned(),
            })?;
            let race = if cfg.inject_race_collision_once {
                cfg.inject_race_collision_once = false;
                true
            } else {
                false
            };
            (
                cfg.force_cross_volume,
                race,
                cfg.fail_move_with_access_denied,
                cfg.fail_move_with_sharing_violation,
            )
        };

        if fail_access {
            return Err(AppError::PermissionDenied {
                resource: src.display().to_string(),
                os_error_code: Some(5),
            });
        }
        if fail_sharing {
            return Err(AppError::FileInUse {
                path: src.display().to_string(),
                os_error_code: Some(32),
            });
        }
        if inject_race && !self.inner.exists(dst)? {
            // Simulate a competing process creating `dst` right before MoveFileExW runs (INV-01).
            let mut competing = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dst)
                .map_err(|err| AppError::from_io("inject competing destination file", err))?;
            competing
                .write_all(b"competing-existing-content")
                .map_err(|err| AppError::from_io("write competing file", err))?;
        }

        if force_cross && !is_sortdeck_temp(src) {
            return Ok(SameVolumeMoveOutcome::CrossVolumeRequired);
        }

        self.inner.move_no_overwrite(src, dst)
    }

    fn copy_to_temp(
        &self,
        src: &Path,
        temp_dst: &Path,
        cancel: &CancelToken,
    ) -> Result<(), AppError> {
        let cfg = self.snapshot_faults()?;
        if cfg.fail_copy_with_disk_full {
            return Err(AppError::DiskFull {
                path: temp_dst.display().to_string(),
                os_error_code: Some(112),
            });
        }
        if cfg.cancel_during_copy {
            cancel.cancel();
            return Err(AppError::Cancelled);
        }

        self.inner.copy_to_temp(src, temp_dst, cancel)?;

        if cfg.corrupt_temp_after_copy {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(temp_dst)
                .map_err(|err| AppError::from_io("corrupt temp copy", err))?;
            f.write_all(b"CORRUPT")
                .map_err(|err| AppError::from_io("append corruption bytes", err))?;
        }

        if cfg.modify_source_during_copy {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(src)
                .map_err(|err| AppError::from_io("modify source during copy", err))?;
            f.write_all(b"-MODIFIED")
                .map_err(|err| AppError::from_io("append source mutation", err))?;
        }

        Ok(())
    }

    fn flush_file(&self, path: &Path) -> Result<(), AppError> {
        self.inner.flush_file(path)
    }

    fn hash_sha256(&self, path: &Path, cancel: &CancelToken) -> Result<String, AppError> {
        self.inner.hash_sha256(path, cancel)
    }

    fn delete_file(&self, path: &Path) -> Result<(), AppError> {
        let cfg = self.snapshot_faults()?;
        if cfg.fail_delete_source_with_sharing_violation {
            return Err(AppError::FileInUse {
                path: path.display().to_string(),
                os_error_code: Some(32),
            });
        }
        self.inner.delete_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<Vec<PathBuf>, AppError> {
        self.inner.create_dir_all(path)
    }

    fn remove_empty_dir(&self, path: &Path) -> Result<(), AppError> {
        self.inner.remove_empty_dir(path)
    }

    fn volume_info(&self, path: &Path) -> Result<VolumeInfo, AppError> {
        self.inner.volume_info(path)
    }

    fn check_access_for_move(&self, path: &Path) -> Result<(), AppError> {
        self.inner.check_access_for_move(path)
    }
}
