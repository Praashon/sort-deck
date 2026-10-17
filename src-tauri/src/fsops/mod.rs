#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::errors::AppError;

pub mod app_data;
pub mod faulty;
pub mod local;
pub mod sandbox;

/// Source identity snapshot used to detect modifications between planning and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub volume_serial: Option<u64>,
    pub file_id: Option<String>,
    pub size: u64,
    pub modified_epoch_ms: Option<i64>,
}

/// Normalized file attributes required by safety exclusion checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileAttributes {
    pub is_dir: bool,
    pub is_hidden: bool,
    pub is_system: bool,
    pub is_readonly: bool,
    pub is_reparse_point: bool,
    pub is_offline_placeholder: bool,
    pub is_cloud_managed: bool,
    pub is_encrypted: bool,
}

/// Filesystem capability snapshot queried per volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub volume_serial: Option<u64>,
    pub filesystem_name: String,
    pub supports_file_ids: bool,
    pub supports_ads: bool,
    pub max_component_len: u32,
    pub available_bytes: Option<u64>,
}

/// Directory entry yielded by `FsBackend::read_dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub path: PathBuf,
    pub file_name: String,
    pub attributes: FileAttributes,
    pub size: u64,
    pub modified_epoch_ms: Option<i64>,
    pub is_non_unicode: bool,
}

/// Streaming copy progress update passed to cancellation-aware callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

/// Cooperative cancellation signal checked between operations and during copies.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Outcome of a same-volume move attempt before cross-volume fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameVolumeMoveOutcome {
    Moved,
    AlreadyExists,
    CrossVolumeRequired,
}

/// Abstraction over all filesystem operations in SortDeck (AGENTS.md §3.1, INV-15).
///
/// Engine and service logic MUST interact with the filesystem strictly through
/// implementations of this trait so that tests can enforce sandbox isolation
/// and inject faults deterministically.
pub trait FsBackend: Send + Sync {
    fn exists(&self, path: &Path) -> Result<bool, AppError>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError>;
    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError>;
    fn fingerprint(&self, path: &Path) -> Result<FileFingerprint, AppError>;
    fn read_dir(&self, dir: &Path) -> Result<Vec<FsEntry>, AppError>;
    fn move_no_overwrite(&self, src: &Path, dst: &Path) -> Result<SameVolumeMoveOutcome, AppError>;
    fn copy_to_temp(
        &self,
        src: &Path,
        temp_dst: &Path,
        cancel: &CancelToken,
    ) -> Result<(), AppError>;
    fn flush_file(&self, path: &Path) -> Result<(), AppError>;
    fn hash_sha256(&self, path: &Path, cancel: &CancelToken) -> Result<String, AppError>;
    fn delete_file(&self, path: &Path) -> Result<(), AppError>;
    fn create_dir_all(&self, path: &Path) -> Result<Vec<PathBuf>, AppError>;
    fn remove_empty_dir(&self, path: &Path) -> Result<(), AppError>;
    fn volume_info(&self, path: &Path) -> Result<VolumeInfo, AppError>;
    fn check_access_for_move(&self, path: &Path) -> Result<(), AppError>;
}
