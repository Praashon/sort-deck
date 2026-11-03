#![forbid(unsafe_code)]

use std::path::Path;

use uuid::Uuid;

use crate::database::transactions::{
    self, JournalMetadata, MoveLogRecord, DEFAULT_RESTORE_SUPPRESSION_TTL_MS,
};
use crate::database::Database;
use crate::engine::mover::{self, FailpointFn};
use crate::engine::paths::{
    contains_excluded_dir_component, is_protected_location, normalize_absolute_path, ProtectedRoots,
};
use crate::errors::AppError;
use crate::fsops::{CancelToken, FsBackend};
use crate::models::{
    BatchKind, BatchStatus, JournalState, OperationType, UndoItemOutcome, UndoRequest, UndoResult,
};

/// Executes batch-level or item-level undo (`AGENTS.md` §5.5, INV-10, INV-13).
pub fn execute_undo(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    req: &UndoRequest,
    cancel: &CancelToken,
    failpoint: Option<&FailpointFn>,
) -> Result<UndoResult, AppError> {
    if req.batch_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "batchId must not be empty".to_owned(),
        });
    }

    let (parent_batch, records) = db.with_writer(|conn| {
        let batch = transactions::get_batch_summary(conn, &req.batch_id)?;
        if batch.status == BatchStatus::Running {
            return Err(AppError::Conflict {
                message: "cannot undo a batch while it is still running".to_owned(),
            });
        }
        let records = transactions::list_move_logs_for_undo_desc(
            conn,
            &req.batch_id,
            req.item_ids.as_deref(),
        )?;
        Ok((batch, records))
    })?;

    if records.is_empty() {
        return Err(AppError::InvalidInput {
            message: "no eligible completed move logs found to undo".to_owned(),
        });
    }

    let undo_batch_id = Uuid::new_v4().to_string();
    let total_items = u64::try_from(records.len()).unwrap_or(u64::MAX);

    db.with_writer(|conn| {
        transactions::create_batch(
            conn,
            &undo_batch_id,
            BatchKind::Undo,
            &parent_batch.source_folder,
            parent_batch.source_folder_id,
            Some(&parent_batch.id),
            total_items,
        )
    })?;

    let mut restored_count: u64 = 0;
    let mut skipped_count: u64 = 0;
    let mut failed_count: u64 = 0;
    let mut recovery_required_count: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut items = Vec::with_capacity(records.len());

    let watched_root_path = Path::new(&parent_batch.source_folder);

    for (seq, rec) in records.iter().enumerate() {
        if cancel.is_cancelled() {
            break;
        }

        let outcome = restore_single_item(
            db,
            fs,
            roots,
            &undo_batch_id,
            seq,
            rec,
            watched_root_path,
            cancel,
            failpoint,
        );

        match outcome {
            SingleUndoOutcome::Restored(item, bytes) => {
                restored_count += 1;
                total_bytes = total_bytes.saturating_add(bytes);
                items.push(item);
            }
            SingleUndoOutcome::Skipped(item) => {
                skipped_count += 1;
                items.push(item);
            }
            SingleUndoOutcome::Failed(item, needs_recovery) => {
                failed_count += 1;
                if needs_recovery {
                    recovery_required_count += 1;
                }
                items.push(item);
            }
        }
    }

    // Remove directories SortDeck created for the original batch ONLY if empty (INV-10, §5.5 #10).
    let removed_empty_dirs = cleanup_empty_created_dirs(db, fs, &parent_batch.id);

    let batch_status = if cancel.is_cancelled() {
        BatchStatus::Cancelled
    } else if recovery_required_count > 0 {
        BatchStatus::RecoveryRequired
    } else if restored_count > 0 && skipped_count == 0 && failed_count == 0 {
        BatchStatus::Completed
    } else if restored_count > 0 {
        BatchStatus::Partial
    } else {
        BatchStatus::Failed
    };

    db.with_writer(|conn| {
        transactions::finalize_batch(
            conn,
            &undo_batch_id,
            batch_status,
            total_items,
            restored_count,
            failed_count,
            skipped_count,
            total_bytes,
        )
    })?;

    Ok(UndoResult {
        undo_batch_id,
        parent_batch_id: parent_batch.id,
        status: batch_status,
        restored_count,
        skipped_count,
        failed_count,
        removed_empty_dirs,
        items,
    })
}

enum SingleUndoOutcome {
    Restored(UndoItemOutcome, u64),
    Skipped(UndoItemOutcome),
    Failed(UndoItemOutcome, bool),
}

#[allow(clippy::too_many_arguments)]
fn restore_single_item(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    undo_batch_id: &str,
    seq: usize,
    rec: &MoveLogRecord,
    watched_root: &Path,
    cancel: &CancelToken,
    failpoint: Option<&FailpointFn>,
) -> SingleUndoOutcome {
    let orig_path = Path::new(&rec.item.original_path);
    let curr_path = Path::new(&rec.item.new_path);

    // Step 1: Re-validate DB-stored original_path as untrusted input (§5.5 #6).
    let orig_norm = match normalize_absolute_path(&rec.item.original_path) {
        Ok(norm) => norm,
        Err(err) => {
            return record_undo_skip(
                db,
                rec,
                "DestinationInvalid",
                &format!("original path rejected: {err}"),
            );
        }
    };

    if is_protected_location(&orig_norm, roots) || contains_excluded_dir_component(&orig_norm) {
        return record_undo_skip(
            db,
            rec,
            "ProtectedLocation",
            "refused restore into protected or excluded location",
        );
    }

    if normalize_absolute_path(&rec.item.new_path).is_err() {
        return record_undo_skip(db, rec, "InvalidName", "current path failed validation");
    }

    // Step 2: Check original watched root still exists (§5.5 #7).
    if !watched_root.as_os_str().is_empty() && !fs.exists(watched_root).unwrap_or(false) {
        return record_undo_skip(
            db,
            rec,
            "OriginalFolderMissing",
            "original watched folder no longer exists",
        );
    }

    // Step 3: Verify file at `new_path` exists and is still the same file (§5.5 #4).
    if !fs.exists(curr_path).unwrap_or(false) {
        return record_undo_skip(
            db,
            rec,
            "SourceVanished",
            "moved file no longer exists at destination",
        );
    }

    if let Err(reason) = verify_moved_file_identity(fs, curr_path, rec, cancel) {
        return record_undo_skip(db, rec, &reason.0, &reason.1);
    }

    // Step 4: Check original_path is free (never overwrite, INV-10, §5.5 #5).
    if fs.exists(orig_path).unwrap_or(true) {
        return record_undo_skip(
            db,
            rec,
            "OriginalPathOccupied",
            "a file already exists at the original path",
        );
    }

    // Step 5: Re-create missing intermediate parent directories inside watched root safely (§5.5 #7).
    if let Some(orig_parent) = orig_path.parent() {
        if let Err(err) =
            mover::verify_safe_paths(curr_path, orig_parent, Some(watched_root), roots, fs)
        {
            return record_undo_skip(db, rec, "ProtectedLocation", &err.to_string());
        }
        if let Err(err) = fs.create_dir_all(orig_parent) {
            return SingleUndoOutcome::Failed(
                UndoItemOutcome {
                    move_log_id: rec.item.id,
                    original_path: rec.item.original_path.clone(),
                    current_path: rec.item.new_path.clone(),
                    restored: false,
                    skip_reason: Some("DestinationInvalid".to_owned()),
                    error_message: Some(err.to_string()),
                },
                false,
            );
        }
    }

    // Step 6: Write-ahead journal `undo_pending` BEFORE filesystem restore (§5.2, §5.5 #2).
    let curr_fp_dto = fs
        .fingerprint(curr_path)
        .ok()
        .map(|fp| mover::fp_to_dto(&fp));
    let meta = JournalMetadata {
        source_fingerprint: curr_fp_dto,
        sha256: rec.item.sha256.clone(),
        rule_id: rec.item.rule_id,
        move_log_id: Some(rec.item.id),
        watched_root: Some(watched_root.to_string_lossy().into_owned()),
        ..JournalMetadata::default()
    };

    let journal_id = match db.with_writer(|conn| {
        transactions::insert_journal_entry(
            conn,
            undo_batch_id,
            &rec.item.new_path,
            &rec.item.original_path,
            None,
            OperationType::UndoMove,
            JournalState::UndoPending,
            &meta,
        )
    }) {
        Ok(id) => id,
        Err(err) => {
            return SingleUndoOutcome::Failed(
                UndoItemOutcome {
                    move_log_id: rec.item.id,
                    original_path: rec.item.original_path.clone(),
                    current_path: rec.item.new_path.clone(),
                    restored: false,
                    skip_reason: None,
                    error_message: Some(err.to_string()),
                },
                false,
            );
        }
    };

    mover::trigger_failpoint(failpoint, "after_journal_undo_pending");

    // Step 7: Restore using the no-overwrite safe_restore_file primitive (§5.5 #8).
    match mover::safe_restore_file(fs, undo_batch_id, seq, curr_path, orig_path, cancel) {
        Ok(res) => {
            let db_res = db.with_writer(|conn| {
                transactions::complete_undo_atomically(
                    conn,
                    rec.item.id,
                    journal_id,
                    undo_batch_id,
                    &orig_norm.key,
                    &res.restored_fingerprint,
                    None,
                    DEFAULT_RESTORE_SUPPRESSION_TTL_MS,
                )
            });

            if let Err(err) = db_res {
                return SingleUndoOutcome::Failed(
                    UndoItemOutcome {
                        move_log_id: rec.item.id,
                        original_path: rec.item.original_path.clone(),
                        current_path: rec.item.new_path.clone(),
                        restored: false,
                        skip_reason: None,
                        error_message: Some(err.to_string()),
                    },
                    true,
                );
            }

            SingleUndoOutcome::Restored(
                UndoItemOutcome {
                    move_log_id: rec.item.id,
                    original_path: rec.item.original_path.clone(),
                    current_path: rec.item.new_path.clone(),
                    restored: true,
                    skip_reason: None,
                    error_message: None,
                },
                rec.item.file_size,
            )
        }
        Err(AppError::AlreadyExists { .. }) => {
            let _ = db.with_writer(|conn| {
                transactions::transition_journal_entry(
                    conn,
                    journal_id,
                    OperationType::UndoMove,
                    JournalState::Failed,
                    None,
                    None,
                    None,
                    None,
                    Some("OriginalPathOccupied"),
                )?;
                transactions::mark_move_log_undo_failed(conn, rec.item.id, "OriginalPathOccupied")
            });
            SingleUndoOutcome::Skipped(UndoItemOutcome {
                move_log_id: rec.item.id,
                original_path: rec.item.original_path.clone(),
                current_path: rec.item.new_path.clone(),
                restored: false,
                skip_reason: Some("OriginalPathOccupied".to_owned()),
                error_message: Some("original path occupied during restore".to_owned()),
            })
        }
        Err(err) => {
            let msg = err.to_string();
            let _ = db.with_writer(|conn| {
                transactions::transition_journal_entry(
                    conn,
                    journal_id,
                    OperationType::UndoMove,
                    JournalState::Failed,
                    None,
                    None,
                    None,
                    err.os_error_code(),
                    Some(&msg),
                )?;
                transactions::mark_move_log_undo_failed(conn, rec.item.id, &msg)
            });
            SingleUndoOutcome::Failed(
                UndoItemOutcome {
                    move_log_id: rec.item.id,
                    original_path: rec.item.original_path.clone(),
                    current_path: rec.item.new_path.clone(),
                    restored: false,
                    skip_reason: None,
                    error_message: Some(msg),
                },
                false,
            )
        }
    }
}

fn verify_moved_file_identity(
    fs: &dyn FsBackend,
    curr_path: &Path,
    rec: &MoveLogRecord,
    cancel: &CancelToken,
) -> Result<(), (String, String)> {
    let curr_fp = fs.fingerprint(curr_path).map_err(|err| {
        (
            "PermissionDenied".to_owned(),
            format!("unable to inspect moved file: {err}"),
        )
    })?;

    if !rec.item.cross_volume {
        if let (Some(exp_vol), Some(exp_id)) = (rec.volume_serial, rec.file_id.as_deref()) {
            if curr_fp.volume_serial == Some(exp_vol) && curr_fp.file_id.as_deref() == Some(exp_id)
            {
                return Ok(());
            }
            return Err((
                "DestinationChanged".to_owned(),
                "destination file identity (volume/file_id) no longer matches moved file"
                    .to_owned(),
            ));
        }
    }

    if let Some(ref expected_hash) = rec.item.sha256 {
        let actual_hash = fs.hash_sha256(curr_path, cancel).map_err(|err| {
            (
                "PermissionDenied".to_owned(),
                format!("unable to hash moved file for undo verification: {err}"),
            )
        })?;
        if &actual_hash == expected_hash {
            return Ok(());
        }
        return Err((
            "ModifiedSinceMove".to_owned(),
            "destination file SHA-256 differs from recorded move hash".to_owned(),
        ));
    }

    if curr_fp.size == rec.item.file_size && curr_fp.modified_epoch_ms == rec.modified_at {
        return Ok(());
    }

    Err((
        "ModifiedSinceMove".to_owned(),
        "destination file size or mtime changed since move".to_owned(),
    ))
}

fn record_undo_skip(
    db: &Database,
    rec: &MoveLogRecord,
    reason: &str,
    detail: &str,
) -> SingleUndoOutcome {
    let _ = db.with_writer(|conn| {
        transactions::mark_move_log_undo_failed(conn, rec.item.id, &format!("{reason}: {detail}"))
    });
    SingleUndoOutcome::Skipped(UndoItemOutcome {
        move_log_id: rec.item.id,
        original_path: rec.item.original_path.clone(),
        current_path: rec.item.new_path.clone(),
        restored: false,
        skip_reason: Some(reason.to_owned()),
        error_message: Some(detail.to_owned()),
    })
}

fn cleanup_empty_created_dirs(db: &Database, fs: &dyn FsBackend, parent_batch_id: &str) -> u64 {
    let Ok(dirs) =
        db.with_writer(|conn| transactions::list_created_directories_desc(conn, parent_batch_id))
    else {
        return 0;
    };

    let mut removed: u64 = 0;
    for (dir_id, dir_path_str) in dirs {
        let dir_path = Path::new(&dir_path_str);
        if !fs.exists(dir_path).unwrap_or(false) {
            let _ =
                db.with_writer(|conn| transactions::delete_created_directory_record(conn, dir_id));
            continue;
        }
        let is_empty = fs
            .read_dir(dir_path)
            .map(|entries| entries.is_empty())
            .unwrap_or(false);
        if is_empty && fs.remove_empty_dir(dir_path).is_ok() {
            removed += 1;
            let _ =
                db.with_writer(|conn| transactions::delete_created_directory_record(conn, dir_id));
        }
    }
    removed
}
