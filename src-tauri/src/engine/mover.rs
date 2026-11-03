#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::database::transactions::{self, JournalMetadata};
use crate::database::Database;
use crate::engine::naming::collision_candidate;
use crate::engine::paths::{
    contains_excluded_dir_component, is_protected_location, normalize_absolute_path, ProtectedRoots,
};
use crate::errors::AppError;
use crate::fsops::{CancelToken, FileFingerprint, FsBackend, SameVolumeMoveOutcome};
use crate::models::{JournalState, OperationType, SourceFingerprintDto};

/// Optional synchronous failpoint hook used by crash-recovery tests (`AGENTS.md` §13.2).
pub type FailpointFn = Arc<dyn Fn(&str) + Send + Sync>;

const MIN_CROSS_VOLUME_MARGIN_BYTES: u64 = 64 * 1024 * 1024;
const FAT32_MAX_FILE_SIZE: u64 = 4_294_967_295;
const MAX_COLLISION_RETRIES: u32 = 10_000;

/// Inputs required to execute a single forward move through [`safe_move`].
pub struct SafeMoveRequest<'a> {
    pub batch_id: &'a str,
    pub seq: usize,
    pub src_path: &'a Path,
    pub dest_dir: &'a Path,
    pub original_filename: &'a str,
    pub expected_src_fingerprint: Option<&'a SourceFingerprintDto>,
    pub rule_id: Option<i64>,
    pub watched_root: Option<&'a Path>,
}

/// Result of a completed [`safe_move`].
#[derive(Debug, Clone)]
pub struct SafeMoveOutcome {
    pub final_destination: PathBuf,
    pub cross_volume: bool,
    pub sha256: Option<String>,
    pub file_size: u64,
    pub move_log_id: i64,
    pub journal_id: i64,
}

/// Result of a completed [`safe_restore_file`].
#[derive(Debug, Clone)]
pub struct SafeRestoreOutcome {
    pub cross_volume: bool,
    pub restored_fingerprint: SourceFingerprintDto,
}

pub fn trigger_failpoint(failpoint: Option<&FailpointFn>, name: &str) {
    if let Some(hook) = failpoint {
        hook(name);
    }
}

/// Converts [`FileFingerprint`] to IPC/journal DTO.
#[must_use]
pub fn fp_to_dto(fp: &FileFingerprint) -> SourceFingerprintDto {
    SourceFingerprintDto {
        volume_serial: fp.volume_serial,
        file_id: fp.file_id.clone(),
        size: fp.size,
        modified_epoch_ms: fp.modified_epoch_ms,
    }
}

/// Compares a stored fingerprint against a freshly queried filesystem fingerprint.
#[must_use]
pub fn fingerprints_equal(expected: &SourceFingerprintDto, actual: &FileFingerprint) -> bool {
    expected.size == actual.size
        && expected.modified_epoch_ms == actual.modified_epoch_ms
        && expected.volume_serial == actual.volume_serial
        && expected.file_id == actual.file_id
}

/// Verifies source and destination paths are valid, non-protected, and have no
/// reparse-point components in their parent chain below `root` (§4.6.3).
pub fn verify_safe_paths(
    src_path: &Path,
    dest_dir: &Path,
    watched_root: Option<&Path>,
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
) -> Result<(), AppError> {
    let src_str = src_path.to_str().ok_or_else(|| AppError::PathValidation {
        path: src_path.display().to_string(),
        reason: "source path is not valid Unicode".to_owned(),
    })?;
    let src_norm = normalize_absolute_path(src_str)?;

    let dst_str = dest_dir.to_str().ok_or_else(|| AppError::PathValidation {
        path: dest_dir.display().to_string(),
        reason: "destination path is not valid Unicode".to_owned(),
    })?;
    let dst_norm = normalize_absolute_path(dst_str)?;

    if is_protected_location(&src_norm, roots) || contains_excluded_dir_component(&src_norm) {
        return Err(AppError::PathValidation {
            path: src_norm.display,
            reason: "source path lies inside a protected or excluded location".to_owned(),
        });
    }

    if is_protected_location(&dst_norm, roots) || contains_excluded_dir_component(&dst_norm) {
        return Err(AppError::PathValidation {
            path: dst_norm.display,
            reason: "destination path lies inside a protected or excluded location".to_owned(),
        });
    }

    check_ancestors_no_reparse(src_path, watched_root, fs)?;
    check_ancestors_no_reparse(dest_dir, watched_root, fs)?;

    Ok(())
}

fn check_ancestors_no_reparse(
    path: &Path,
    stop_root: Option<&Path>,
    fs: &dyn FsBackend,
) -> Result<(), AppError> {
    let stop_norm = stop_root
        .and_then(|r| r.to_str())
        .and_then(|s| normalize_absolute_path(s).ok());

    for ancestor in path.ancestors() {
        let Some(anc_str) = ancestor.to_str() else {
            break;
        };
        let Ok(anc_norm) = normalize_absolute_path(anc_str) else {
            break;
        };

        if let Some(ref root) = stop_norm {
            if anc_norm.key.len() < root.key.len() {
                break;
            }
        }

        match fs.exists(ancestor) {
            Ok(true) => {
                let attrs = fs.attributes(ancestor)?;
                if attrs.is_reparse_point {
                    return Err(AppError::PathValidation {
                        path: anc_norm.display,
                        reason: "reparse point detected along operation path".to_owned(),
                    });
                }
            }
            Ok(false) => {}
            Err(AppError::SandboxViolation { .. }) => break,
            Err(err) => return Err(err),
        }

        if let Some(ref root) = stop_norm {
            if anc_norm.key == root.key {
                break;
            }
        }
    }
    Ok(())
}

/// Removes an incomplete temp file ONLY when path naming (`.sortdeck-*.sdpart`)
/// and optional recorded `file_id` confirm ownership (`AGENTS.md` §4.2.4).
pub fn cleanup_owned_temp(
    fs: &dyn FsBackend,
    temp_path: &Path,
    expected_file_id: Option<&str>,
) -> Result<bool, AppError> {
    let is_sortdeck_name = temp_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with(".sortdeck-") && name.ends_with(".sdpart"));
    if !is_sortdeck_name {
        return Err(AppError::Internal {
            message: format!(
                "refused temp cleanup on non-SortDeck filename '{}'",
                temp_path.display()
            ),
        });
    }

    if !fs.exists(temp_path)? {
        return Ok(false);
    }

    if let Some(expected_id) = expected_file_id {
        let fp = fs.fingerprint(temp_path)?;
        if fp.file_id.as_deref() != Some(expected_id) {
            return Err(AppError::RecoveryRequired {
                detail: format!(
                    "temp file '{}' file_id mismatch during cleanup",
                    temp_path.display()
                ),
            });
        }
    }

    fs.delete_file(temp_path)?;
    Ok(true)
}

/// Executes a single safe move (`AGENTS.md` §4.2) using write-ahead journaling
/// and OS no-overwrite primitives (`FsBackend`).
pub fn safe_move(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    req: &SafeMoveRequest<'_>,
    cancel: &CancelToken,
    failpoint: Option<&FailpointFn>,
) -> Result<SafeMoveOutcome, AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    verify_safe_paths(req.src_path, req.dest_dir, req.watched_root, roots, fs)?;

    if !fs.exists(req.src_path)? {
        return Err(AppError::NotFound {
            resource: req.src_path.display().to_string(),
            os_error_code: None,
        });
    }

    let src_attrs = fs.attributes(req.src_path)?;
    if src_attrs.is_dir || src_attrs.is_reparse_point {
        return Err(AppError::PathValidation {
            path: req.src_path.display().to_string(),
            reason: "cannot move directory or reparse point".to_owned(),
        });
    }

    let src_fp = fs.fingerprint(req.src_path)?;
    if let Some(expected) = req.expected_src_fingerprint {
        if !fingerprints_equal(expected, &src_fp) {
            return Err(AppError::Conflict {
                message: format!(
                    "source '{}' changed since plan creation",
                    req.src_path.display()
                ),
            });
        }
    }
    let src_fp_dto = fp_to_dto(&src_fp);

    let created_dirs = fs.create_dir_all(req.dest_dir)?;
    if !created_dirs.is_empty() {
        let dir_strings: Vec<String> = created_dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        db.with_writer(|conn| {
            transactions::record_created_directories(conn, req.batch_id, &dir_strings)
        })?;
    }

    let initial_candidate = req.dest_dir.join(req.original_filename);
    let src_str = req.src_path.to_string_lossy().into_owned();
    let initial_dst_str = initial_candidate.to_string_lossy().into_owned();

    let mut meta = JournalMetadata {
        source_fingerprint: Some(src_fp_dto.clone()),
        rule_id: req.rule_id,
        watched_root: req.watched_root.map(|p| p.to_string_lossy().into_owned()),
        ..JournalMetadata::default()
    };

    // Write-ahead journal row BEFORE any move/copy call (§5.2).
    let journal_id = db.with_writer(|conn| {
        transactions::insert_journal_entry(
            conn,
            req.batch_id,
            &src_str,
            &initial_dst_str,
            None,
            OperationType::MoveSameVolume,
            JournalState::Planned,
            &meta,
        )
    })?;

    trigger_failpoint(failpoint, "after_journal_planned_same_volume");

    // Step 1: Attempt same-volume no-overwrite move across bounded collision candidates (§4.2.1).
    let mut cross_volume_needed = false;
    let mut final_same_vol_dst: Option<PathBuf> = None;

    for attempt in 0..=MAX_COLLISION_RETRIES {
        if cancel.is_cancelled() {
            record_journal_failure(
                db,
                journal_id,
                OperationType::MoveSameVolume,
                &AppError::Cancelled,
            );
            return Err(AppError::Cancelled);
        }

        let candidate_name = collision_candidate(req.original_filename, attempt);
        if candidate_name.encode_utf16().count() > 255 {
            let err = AppError::PathValidation {
                path: candidate_name,
                reason: "collision candidate exceeds 255 UTF-16 code units".to_owned(),
            };
            record_journal_failure(db, journal_id, OperationType::MoveSameVolume, &err);
            return Err(err);
        }

        let candidate_dst = req.dest_dir.join(&candidate_name);
        match fs.move_no_overwrite(req.src_path, &candidate_dst) {
            Ok(SameVolumeMoveOutcome::Moved) => {
                final_same_vol_dst = Some(candidate_dst);
                break;
            }
            Ok(SameVolumeMoveOutcome::AlreadyExists) => {
                continue;
            }
            Ok(SameVolumeMoveOutcome::CrossVolumeRequired) => {
                cross_volume_needed = true;
                break;
            }
            Err(err) => {
                record_journal_failure(db, journal_id, OperationType::MoveSameVolume, &err);
                return Err(err);
            }
        }
    }

    if let Some(dst_path) = final_same_vol_dst {
        trigger_failpoint(failpoint, "after_same_volume_move");

        let dst_fp = match fs.fingerprint(&dst_path) {
            Ok(fp) => fp,
            Err(err) => {
                record_journal_recovery_required(
                    db,
                    journal_id,
                    OperationType::MoveSameVolume,
                    &meta,
                    &err.to_string(),
                );
                return Err(err);
            }
        };

        if dst_fp.size != src_fp.size
            || (src_fp.file_id.is_some()
                && dst_fp.file_id.is_some()
                && dst_fp.file_id != src_fp.file_id)
        {
            let mismatch = AppError::RecoveryRequired {
                detail: "post-move destination identity mismatch".to_owned(),
            };
            record_journal_recovery_required(
                db,
                journal_id,
                OperationType::MoveSameVolume,
                &meta,
                "post-move destination identity mismatch",
            );
            return Err(mismatch);
        }

        let dst_fp_dto = fp_to_dto(&dst_fp);
        meta.dest_fingerprint = Some(dst_fp_dto.clone());
        let dst_str = dst_path.to_string_lossy().into_owned();
        let move_log_id = db.with_writer(|conn| {
            transactions::complete_move_atomically(
                conn,
                journal_id,
                req.batch_id,
                &src_str,
                &dst_str,
                false,
                None,
                dst_fp.size,
                &dst_fp_dto,
                req.rule_id,
                &meta,
            )
        })?;

        return Ok(SafeMoveOutcome {
            final_destination: dst_path,
            cross_volume: false,
            sha256: None,
            file_size: dst_fp.size,
            move_log_id,
            journal_id,
        });
    }

    if !cross_volume_needed {
        let exhausted = AppError::AlreadyExists {
            path: initial_dst_str,
            os_error_code: None,
        };
        record_journal_failure(db, journal_id, OperationType::MoveSameVolume, &exhausted);
        return Err(exhausted);
    }

    // Step 2: Cross-volume move state machine (§4.2.3).
    execute_cross_volume_move(
        db, fs, journal_id, req, src_attrs, &src_fp, &mut meta, cancel, failpoint,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_cross_volume_move(
    db: &Database,
    fs: &dyn FsBackend,
    journal_id: i64,
    req: &SafeMoveRequest<'_>,
    src_attrs: crate::fsops::FileAttributes,
    src_fp: &FileFingerprint,
    meta: &mut JournalMetadata,
    cancel: &CancelToken,
    failpoint: Option<&FailpointFn>,
) -> Result<SafeMoveOutcome, AppError> {
    if src_attrs.is_readonly {
        let err = AppError::PermissionDenied {
            resource: req.src_path.display().to_string(),
            os_error_code: None,
        };
        record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, &err);
        return Err(err);
    }
    if src_attrs.is_cloud_managed || src_attrs.is_encrypted {
        let err = AppError::CloudFileError {
            path: req.src_path.display().to_string(),
            os_error_code: None,
        };
        record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, &err);
        return Err(err);
    }

    if let Ok(vol) = fs.volume_info(req.dest_dir) {
        if vol.filesystem_name.eq_ignore_ascii_case("FAT32") && src_fp.size > FAT32_MAX_FILE_SIZE {
            let err = AppError::PathValidation {
                path: req.src_path.display().to_string(),
                reason: "file exceeds FAT32 4 GiB limit".to_owned(),
            };
            record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, &err);
            return Err(err);
        }
        if let Some(avail) = vol.available_bytes {
            let margin = MIN_CROSS_VOLUME_MARGIN_BYTES.max(src_fp.size / 100);
            if avail < src_fp.size.saturating_add(margin) {
                let err = AppError::DiskFull {
                    path: req.dest_dir.display().to_string(),
                    os_error_code: Some(112),
                };
                record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, &err);
                return Err(err);
            }
        }
    }

    let temp_name = format!(".sortdeck-{}-{}.sdpart", req.batch_id, req.seq);
    let temp_path = req.dest_dir.join(&temp_name);
    let temp_str = temp_path.to_string_lossy().into_owned();
    let src_str = req.src_path.to_string_lossy().into_owned();

    db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            journal_id,
            OperationType::MoveCrossVolume,
            JournalState::Copying,
            None,
            Some(&temp_str),
            Some(meta),
            None,
            None,
        )
    })?;

    trigger_failpoint(failpoint, "after_journal_copying_before_copy");

    if let Err(err) = fs.copy_to_temp(req.src_path, &temp_path, cancel) {
        handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
        return Err(err);
    }

    if let Ok(temp_fp) = fs.fingerprint(&temp_path) {
        meta.temp_file_id = temp_fp.file_id;
    }

    trigger_failpoint(failpoint, "after_copy_before_verify");

    if let Err(err) = fs.flush_file(&temp_path) {
        handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
        return Err(err);
    }

    // Re-check source fingerprint before hashing so concurrent source edits are
    // classified as SourceChangedDuringCopy rather than HashMismatch (§4.2.3 #7).
    let src_fp_after_copy = match fs.fingerprint(req.src_path) {
        Ok(fp) => fp,
        Err(err) => {
            handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
            return Err(err);
        }
    };
    if !fingerprints_equal(&fp_to_dto(src_fp), &src_fp_after_copy) {
        let err = AppError::SourceChangedDuringCopy {
            path: src_str.clone(),
        };
        handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
        return Err(err);
    }

    // Verify streaming SHA-256 of both source and temporary copy (§4.2.3 #6).
    let src_hash = match fs.hash_sha256(req.src_path, cancel) {
        Ok(h) => h,
        Err(err) => {
            handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
            return Err(err);
        }
    };
    let temp_hash = match fs.hash_sha256(&temp_path, cancel) {
        Ok(h) => h,
        Err(err) => {
            handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
            return Err(err);
        }
    };
    let temp_fp_after = match fs.fingerprint(&temp_path) {
        Ok(fp) => fp,
        Err(err) => {
            handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
            return Err(err);
        }
    };

    // Re-check source fingerprint after hashing as well before blaming hash mismatch (§4.2.3 #7).
    let src_fp_after_hash = match fs.fingerprint(req.src_path) {
        Ok(fp) => fp,
        Err(err) => {
            handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
            return Err(err);
        }
    };
    if !fingerprints_equal(&fp_to_dto(src_fp), &src_fp_after_hash) {
        let err = AppError::SourceChangedDuringCopy {
            path: src_str.clone(),
        };
        handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
        return Err(err);
    }

    if temp_fp_after.size != src_fp.size || src_hash != temp_hash {
        let err = AppError::HashMismatch {
            path: req.src_path.display().to_string(),
        };
        handle_copy_failure(db, fs, journal_id, &temp_path, meta, &err);
        return Err(err);
    }

    meta.sha256 = Some(src_hash.clone());
    meta.temp_file_id = temp_fp_after.file_id.clone();

    db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            journal_id,
            OperationType::MoveCrossVolume,
            JournalState::Verified,
            None,
            Some(&temp_str),
            Some(meta),
            None,
            None,
        )
    })?;

    trigger_failpoint(failpoint, "after_journal_verified");

    // Commit temp -> final collision-safe name via same-volume MoveFileExW (§4.2.3 #9).
    let final_dst = commit_temp_no_overwrite(fs, &temp_path, req.dest_dir, req.original_filename)
        .inspect_err(|err| {
        let temp_id = meta.temp_file_id.clone();
        if let Err(clean_err) = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref()) {
            record_journal_recovery_required(
                db,
                journal_id,
                OperationType::MoveCrossVolume,
                meta,
                &clean_err.to_string(),
            );
        } else {
            record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, err);
        }
    })?;

    let final_dst_str = final_dst.to_string_lossy().into_owned();
    let dst_fp = fs.fingerprint(&final_dst)?;
    let dst_fp_dto = fp_to_dto(&dst_fp);
    meta.dest_fingerprint = Some(dst_fp_dto.clone());

    if dst_fp.size != src_fp.size {
        let err = AppError::RecoveryRequired {
            detail: "committed destination size mismatch".to_owned(),
        };
        record_journal_recovery_required(
            db,
            journal_id,
            OperationType::MoveCrossVolume,
            meta,
            "committed destination size mismatch",
        );
        return Err(err);
    }

    db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            journal_id,
            OperationType::MoveCrossVolume,
            JournalState::Committed,
            Some(&final_dst_str),
            Some(&temp_str),
            Some(meta),
            None,
            None,
        )
    })?;

    trigger_failpoint(failpoint, "after_journal_committed");

    // Delete original source ONLY NOW (§4.2.3 #11, §4.2.5, INV-02, INV-03).
    if let Err(del_err) = fs.delete_file(req.src_path) {
        meta.detail = Some("source_remains_after_verified_copy".to_owned());
        record_journal_recovery_required(
            db,
            journal_id,
            OperationType::MoveCrossVolume,
            meta,
            "source_remains_after_verified_copy",
        );
        return Err(AppError::RecoveryRequired {
            detail: format!("source_remains_after_verified_copy: {del_err}"),
        });
    }

    let move_log_id = db.with_writer(|conn| {
        transactions::complete_move_atomically(
            conn,
            journal_id,
            req.batch_id,
            &src_str,
            &final_dst_str,
            true,
            Some(&src_hash),
            dst_fp.size,
            &dst_fp_dto,
            req.rule_id,
            meta,
        )
    })?;

    Ok(SafeMoveOutcome {
        final_destination: final_dst,
        cross_volume: true,
        sha256: Some(src_hash),
        file_size: dst_fp.size,
        move_log_id,
        journal_id,
    })
}

/// Restores a previously moved file from `current_path` back to exact `original_path`
/// without overwriting any existing file (`AGENTS.md` §5.5 #8, INV-10).
pub fn safe_restore_file(
    fs: &dyn FsBackend,
    batch_id: &str,
    seq: usize,
    current_path: &Path,
    original_path: &Path,
    cancel: &CancelToken,
) -> Result<SafeRestoreOutcome, AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    match fs.move_no_overwrite(current_path, original_path)? {
        SameVolumeMoveOutcome::Moved => {
            let fp = fs.fingerprint(original_path)?;
            Ok(SafeRestoreOutcome {
                cross_volume: false,
                restored_fingerprint: fp_to_dto(&fp),
            })
        }
        SameVolumeMoveOutcome::AlreadyExists => Err(AppError::AlreadyExists {
            path: original_path.display().to_string(),
            os_error_code: None,
        }),
        SameVolumeMoveOutcome::CrossVolumeRequired => {
            let parent = original_path
                .parent()
                .ok_or_else(|| AppError::PathValidation {
                    path: original_path.display().to_string(),
                    reason: "original restore path has no parent".to_owned(),
                })?;
            let temp_path = parent.join(format!(".sortdeck-{batch_id}-undo-{seq}.sdpart"));
            let src_fp = fs.fingerprint(current_path)?;

            if let Err(err) = fs.copy_to_temp(current_path, &temp_path, cancel) {
                let _ = cleanup_owned_temp(fs, &temp_path, None);
                return Err(err);
            }
            let temp_id = fs.fingerprint(&temp_path).ok().and_then(|f| f.file_id);

            if let Err(err) = fs.flush_file(&temp_path) {
                let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                return Err(err);
            }

            let src_hash = match fs.hash_sha256(current_path, cancel) {
                Ok(h) => h,
                Err(err) => {
                    let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                    return Err(err);
                }
            };
            let temp_hash = match fs.hash_sha256(&temp_path, cancel) {
                Ok(h) => h,
                Err(err) => {
                    let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                    return Err(err);
                }
            };
            let temp_fp = fs.fingerprint(&temp_path)?;
            if temp_fp.size != src_fp.size || src_hash != temp_hash {
                let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                return Err(AppError::HashMismatch {
                    path: current_path.display().to_string(),
                });
            }

            match fs.move_no_overwrite(&temp_path, original_path)? {
                SameVolumeMoveOutcome::Moved => {}
                SameVolumeMoveOutcome::AlreadyExists => {
                    let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                    return Err(AppError::AlreadyExists {
                        path: original_path.display().to_string(),
                        os_error_code: None,
                    });
                }
                SameVolumeMoveOutcome::CrossVolumeRequired => {
                    let _ = cleanup_owned_temp(fs, &temp_path, temp_id.as_deref());
                    return Err(AppError::Internal {
                        message: "temp commit unexpectedly reported cross-volume".to_owned(),
                    });
                }
            }

            fs.delete_file(current_path)?;
            let restored_fp = fs.fingerprint(original_path)?;
            Ok(SafeRestoreOutcome {
                cross_volume: true,
                restored_fingerprint: fp_to_dto(&restored_fp),
            })
        }
    }
}

/// Commits `temp_path` inside `dest_dir` to a race-safe final filename without overwriting.
pub fn commit_temp_no_overwrite(
    fs: &dyn FsBackend,
    temp_path: &Path,
    dest_dir: &Path,
    original_filename: &str,
) -> Result<PathBuf, AppError> {
    for attempt in 0..=MAX_COLLISION_RETRIES {
        let candidate_name = collision_candidate(original_filename, attempt);
        if candidate_name.encode_utf16().count() > 255 {
            return Err(AppError::PathValidation {
                path: candidate_name,
                reason: "collision candidate exceeds 255 UTF-16 code units".to_owned(),
            });
        }
        let candidate_dst = dest_dir.join(&candidate_name);
        match fs.move_no_overwrite(temp_path, &candidate_dst)? {
            SameVolumeMoveOutcome::Moved => return Ok(candidate_dst),
            SameVolumeMoveOutcome::AlreadyExists => continue,
            SameVolumeMoveOutcome::CrossVolumeRequired => {
                return Err(AppError::Internal {
                    message: "temp commit inside destination directory reported cross-volume"
                        .to_owned(),
                });
            }
        }
    }
    Err(AppError::AlreadyExists {
        path: dest_dir.join(original_filename).display().to_string(),
        os_error_code: None,
    })
}

fn handle_copy_failure(
    db: &Database,
    fs: &dyn FsBackend,
    journal_id: i64,
    temp_path: &Path,
    meta: &mut JournalMetadata,
    err: &AppError,
) {
    let temp_id = meta.temp_file_id.clone();
    if let Err(clean_err) = cleanup_owned_temp(fs, temp_path, temp_id.as_deref()) {
        meta.detail = Some(format!("temp_cleanup_failed: {clean_err}"));
        record_journal_recovery_required(
            db,
            journal_id,
            OperationType::MoveCrossVolume,
            meta,
            &format!("copy failed ({err}) and temp cleanup failed ({clean_err})"),
        );
    } else {
        record_journal_failure(db, journal_id, OperationType::MoveCrossVolume, err);
    }
}

fn record_journal_failure(db: &Database, journal_id: i64, op_type: OperationType, err: &AppError) {
    let msg = err.to_string();
    let _ = db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            journal_id,
            op_type,
            JournalState::Failed,
            None,
            None,
            None,
            err.os_error_code(),
            Some(&msg),
        )
    });
}

fn record_journal_recovery_required(
    db: &Database,
    journal_id: i64,
    op_type: OperationType,
    meta: &JournalMetadata,
    detail: &str,
) {
    let _ = db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            journal_id,
            op_type,
            JournalState::RecoveryRequired,
            None,
            None,
            Some(meta),
            None,
            Some(detail),
        )
    });
}
