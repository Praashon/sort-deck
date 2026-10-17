#![forbid(unsafe_code)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::errors::AppError;
use crate::fsops::{
    CancelToken, FileAttributes, FileFingerprint, FsBackend, FsEntry, SameVolumeMoveOutcome,
    VolumeInfo,
};

const HASH_BUFFER_BYTES: usize = 1024 * 1024;

#[cfg(windows)]
const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
#[cfg(windows)]
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
#[cfg(windows)]
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
#[cfg(windows)]
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
#[cfg(windows)]
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
#[cfg(windows)]
const FILE_ATTRIBUTE_ENCRYPTED: u32 = 0x0000_4000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_PINNED: u32 = 0x0008_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_UNPINNED: u32 = 0x0010_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// Local filesystem implementation of [`FsBackend`] backed by Win32 primitives
/// in `platform/windows` (`AGENTS.md` §3.1, §4.1.1).
#[derive(Debug, Default)]
pub struct LocalFsBackend;

impl LocalFsBackend {
    fn map_metadata_attributes(meta: &std::fs::Metadata) -> FileAttributes {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            let raw = meta.file_attributes();
            FileAttributes {
                is_dir: (raw & FILE_ATTRIBUTE_DIRECTORY) != 0 || meta.is_dir(),
                is_hidden: (raw & FILE_ATTRIBUTE_HIDDEN) != 0,
                is_system: (raw & FILE_ATTRIBUTE_SYSTEM) != 0,
                is_readonly: (raw & FILE_ATTRIBUTE_READONLY) != 0 || meta.permissions().readonly(),
                is_reparse_point: (raw & FILE_ATTRIBUTE_REPARSE_POINT) != 0
                    || meta.file_type().is_symlink(),
                is_offline_placeholder: (raw
                    & (FILE_ATTRIBUTE_OFFLINE
                        | FILE_ATTRIBUTE_RECALL_ON_OPEN
                        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
                    != 0,
                is_cloud_managed: (raw
                    & (FILE_ATTRIBUTE_PINNED
                        | FILE_ATTRIBUTE_UNPINNED
                        | FILE_ATTRIBUTE_OFFLINE
                        | FILE_ATTRIBUTE_RECALL_ON_OPEN
                        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
                    != 0,
                is_encrypted: (raw & FILE_ATTRIBUTE_ENCRYPTED) != 0,
            }
        }
        #[cfg(not(windows))]
        {
            FileAttributes {
                is_dir: meta.is_dir(),
                is_hidden: false,
                is_system: false,
                is_readonly: meta.permissions().readonly(),
                is_reparse_point: meta.file_type().is_symlink(),
                is_offline_placeholder: false,
                is_cloud_managed: false,
                is_encrypted: false,
            }
        }
    }

    fn modified_ms(meta: &std::fs::Metadata) -> Option<i64> {
        meta.modified()
            .ok()
            .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
            .and_then(|dur| i64::try_from(dur.as_millis()).ok())
    }
}

impl FsBackend for LocalFsBackend {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        Ok(std::fs::symlink_metadata(path).is_ok())
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        // Confined to fsops/local.rs per AGENTS.md §2.6; callers strip `\\?\` for UI display.
        #[allow(clippy::disallowed_methods)]
        std::fs::canonicalize(path).map_err(|err| AppError::from_io("canonicalize path", err))
    }

    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError> {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|err| AppError::from_io("read file attributes", err))?;
        Ok(Self::map_metadata_attributes(&meta))
    }

    fn fingerprint(&self, path: &Path) -> Result<FileFingerprint, AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::get_file_identity(path)
        }
        #[cfg(not(windows))]
        {
            let meta = std::fs::symlink_metadata(path)
                .map_err(|err| AppError::from_io("read file fingerprint", err))?;
            Ok(FileFingerprint {
                volume_serial: None,
                file_id: None,
                size: meta.len(),
                modified_epoch_ms: Self::modified_ms(&meta),
            })
        }
    }

    fn read_dir(&self, dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        let entries =
            std::fs::read_dir(dir).map_err(|err| AppError::from_io("read directory", err))?;
        let mut results = Vec::new();
        for entry_res in entries {
            let entry = entry_res.map_err(|err| AppError::from_io("iterate directory", err))?;
            let os_name = entry.file_name();
            let (file_name, is_non_unicode) = match os_name.to_str() {
                Some(valid) => (valid.to_owned(), false),
                None => (os_name.to_string_lossy().into_owned(), true),
            };
            let entry_path = entry.path();
            let meta = std::fs::symlink_metadata(&entry_path)
                .map_err(|err| AppError::from_io("read entry metadata", err))?;
            results.push(FsEntry {
                path: entry_path,
                file_name,
                attributes: Self::map_metadata_attributes(&meta),
                size: meta.len(),
                modified_epoch_ms: Self::modified_ms(&meta),
                is_non_unicode,
            });
        }
        Ok(results)
    }

    fn move_no_overwrite(&self, src: &Path, dst: &Path) -> Result<SameVolumeMoveOutcome, AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::move_file_no_overwrite(src, dst)
        }
        #[cfg(not(windows))]
        {
            let _ = (src, dst);
            Err(AppError::Internal {
                message: "same-volume no-overwrite move is only implemented for Windows".to_owned(),
            })
        }
    }

    fn copy_to_temp(
        &self,
        src: &Path,
        temp_dst: &Path,
        cancel: &CancelToken,
    ) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::copy_file_no_overwrite(src, temp_dst, cancel)
        }
        #[cfg(not(windows))]
        {
            let _ = (src, temp_dst, cancel);
            Err(AppError::Internal {
                message: "copy_to_temp is only implemented for Windows".to_owned(),
            })
        }
    }

    fn flush_file(&self, path: &Path) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::flush_file_buffers(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(())
        }
    }

    fn hash_sha256(&self, path: &Path, cancel: &CancelToken) -> Result<String, AppError> {
        let mut file = std::fs::File::open(path)
            .map_err(|err| AppError::from_io("open file for hash", err))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; HASH_BUFFER_BYTES];

        loop {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            let read = file
                .read(&mut buffer)
                .map_err(|err| AppError::from_io("read file chunk for hash", err))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }

        let digest = hasher.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        Ok(hex)
    }

    fn delete_file(&self, path: &Path) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::delete_file_strict(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(AppError::Internal {
                message: "delete_file is only implemented for Windows".to_owned(),
            })
        }
    }

    fn create_dir_all(&self, path: &Path) -> Result<Vec<PathBuf>, AppError> {
        let mut missing_chain = Vec::new();
        let mut current = Some(path);
        while let Some(dir) = current {
            if self.exists(dir)? {
                break;
            }
            missing_chain.push(dir.to_path_buf());
            current = dir.parent();
        }

        missing_chain.reverse();
        let mut created = Vec::with_capacity(missing_chain.len());
        for dir in missing_chain {
            match std::fs::create_dir(&dir) {
                Ok(()) => created.push(dir),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(AppError::from_io(
                        format!("create directory '{}'", dir.display()),
                        err,
                    ))
                }
            }
        }
        Ok(created)
    }

    fn remove_empty_dir(&self, path: &Path) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::remove_empty_dir_strict(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(AppError::Internal {
                message: "remove_empty_dir is only implemented for Windows".to_owned(),
            })
        }
    }

    fn volume_info(&self, path: &Path) -> Result<VolumeInfo, AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::get_volume_info(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(VolumeInfo {
                volume_serial: None,
                filesystem_name: "NTFS".to_owned(),
                supports_file_ids: true,
                supports_ads: true,
                max_component_len: 255,
                available_bytes: None,
            })
        }
    }

    fn check_access_for_move(&self, path: &Path) -> Result<(), AppError> {
        #[cfg(windows)]
        {
            crate::platform::windows::check_access_for_move(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(())
        }
    }
}
