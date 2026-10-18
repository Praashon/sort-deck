#![forbid(unsafe_code)]

use std::path::Path;

use uuid::Uuid;

use crate::database::transactions::{
    self, JournalMetadata, JournalRow, DEFAULT_RESTORE_SUPPRESSION_TTL_MS,
};
use crate::database::Database;
use crate::engine::mover;
use crate::engine::paths::{is_protected_location, normalize_absolute_path, ProtectedRoots};
use crate::errors::AppError;
use crate::fsops::{CancelToken, FsBackend};
use crate::models::{BatchKind, BatchStatus, JournalState, OperationType, RecoverySummary};

/// Executes startup/manual crash recovery implementing the full Â§5.4
/// matrix based strictly on verifiable on-disk evidence.
///
/// Idempotent: running recovery twice yields zero additional mutations.
pub fn run_recovery(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    include_recovery_required: bool,
) -> Result<RecoverySummary, AppError> {
    let open_entries = db.with_writer(|conn| {
        transactions::list_open_journal_entries(conn, include_recovery_required)
    })?;

    let stale_running_batches =
        db.with_writer(|conn| transactions::list_running_batch_ids(conn))?;

    if open_entries.is_empty() && stale_running_batches.is_empty() {
        return Ok(RecoverySummary {
            recovery_batch_id: None,
            inspected_entries: 0,
            finalized_completed: 0,
            finalized_undone: 0,
            marked_failed: 0,
            recovery_required: 0,
            cleaned_temps: 0,
        });
    }

    let recovery_batch_id = Uuid::new_v4().to_string();
    let inspected_entries = u64::try_from(open_entries.len()).unwrap_or(u64::MAX);

    if !open_entries.is_empty() {
        db.with_writer(|conn| {
            transactions::create_batch(
                conn,
                &recovery_batch_id,
                BatchKind::Recovery,
                "recovery",
                None,
                None,
                inspected_entries,
            )
        })?;
    }

    let mut summary = RecoverySummary {
        recovery_batch_id: if open_entries.is_empty() {
            None
        } else {
            Some(recovery_batch_id.clone())
        },
        inspected_entries,
        finalized_completed: 0,
        finalized_undone: 0,
        marked_failed: 0,
        recovery_required: 0,
        cleaned_temps: 0,
    };

    let cancel = CancelToken::new();

    for entry in &open_entries {
        match recover_single_entry(db, fs, roots, entry, &recovery_batch_id, &cancel) {
            RecoveryOutcome::Completed => summary.finalized_completed += 1,
            RecoveryOutcome::Undone => summary.finalized_undone += 1,
            RecoveryOutcome::Failed { cleaned_temp } => {
                summary.marked_failed += 1;
                if cleaned_temp {
                    summary.cleaned_temps += 1;
                }
            }
            RecoveryOutcome::NeedsHumanReview => summary.recovery_required += 1,
        }
    }

    if !open_entries.is_empty() {
        let rec_status = if summary.recovery_required > 0 {
            BatchStatus::RecoveryRequired
        } else {
            BatchStatus::Completed
        };
        db.with_writer(|conn| {
            transactions::finalize_batch(
                conn,
                &recovery_batch_id,
                rec_status,
                inspected_entries,
                summary.finalized_completed + summary.finalized_undone,
                summary.marked_failed,
                0,
                0,
            )
        })?;
    }

    for batch_id in stale_running_batches {
        db.with_writer(|conn| transactions::reconcile_interrupted_batch(conn, &batch_id))?;
    }

    Ok(summary)
}

enum RecoveryOutcome {
    Completed,
    Undone,
    Failed { cleaned_temp: bool },
    NeedsHumanReview,
}

fn recover_single_entry(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    entry: &JournalRow,
    recovery_batch_id: &str,
    cancel: &CancelToken,
) -> RecoveryOutcome {
    match entry.state {
        JournalState::Planned => recover_planned_same_volume(db, fs, entry),
        JournalState::Copying => recover_copying(db, fs, entry),
        JournalState::Verified => recover_verified(db, fs, entry, cancel),
        JournalState::Committed => recover_committed(db, fs, entry, cancel),
        JournalState::UndoPending => {
            recover_undo_pending(db, fs, roots, entry, recovery_batch_id, cancel)
        }
        JournalState::RecoveryRequired => {
            if entry.operation_type == OperationType::MoveCrossVolume
                && entry.metadata.detail.as_deref() == Some("source_remains_after_verified_copy")
            {
                recover_committed(db, fs, entry, cancel)
            } else {
                RecoveryOutcome::NeedsHumanReview
            }
        }
        JournalState::Completed => RecoveryOutcome::Completed,
        JournalState::Undone => RecoveryOutcome::Undone,
        JournalState::Failed => RecoveryOutcome::Failed {
            cleaned_temp: false,
        },
    }
}

fn recover_planned_same_volume(
    db: &Database,
    fs: &dyn FsBackend,
    entry: &JournalRow,
) -> RecoveryOutcome {
    let src = Path::new(&entry.source_path);
    let dst = Path::new(&entry.destination_path);
    let src_exists = fs.exists(src).unwrap_or(false);
    let dst_exists = fs.exists(dst).unwrap_or(false);

    if src_exists && !dst_exists {
        mark_failed(db, entry, "interrupted_before_move");
        return RecoveryOutcome::Failed {
            cleaned_temp: false,
        };
    }

    if !src_exists && dst_exists {
        if let Ok(dst_fp) = fs.fingerprint(dst) {
            let id_matches = match entry.metadata.source_fingerprint.as_ref() {
                Some(expected) => {
                    if let (Some(exp_id), Some(act_id)) =
                        (expected.file_id.as_deref(), dst_fp.file_id.as_deref())
                    {
                        exp_id == act_id && dst_fp.size == expected.size
                    } else {
                        dst_fp.size == expected.size
                    }
                }
                None => false,
            };

            if id_matches {
                let dst_dto = mover::fp_to_dto(&dst_fp);
                let mut meta = entry.metadata.clone();
                meta.dest_fingerprint = Some(dst_dto.clone());
                if db
                    .with_writer(|conn| {
                        transactions::complete_move_atomically(
                            conn,
                            entry.id,
                            &entry.batch_id,
                            &entry.source_path,
                            &entry.destination_path,
                            false,
                            None,
                            dst_fp.size,
                            &dst_dto,
                            meta.rule_id,
                            &meta,
                        )
                    })
                    .is_ok()
                {
                    return RecoveryOutcome::Completed;
                }
            }
        }
    }

    mark_recovery_required(db, entry, "ambiguous_planned_state");
    RecoveryOutcome::NeedsHumanReview
}

fn recover_copying(db: &Database, fs: &dyn FsBackend, entry: &JournalRow) -> RecoveryOutcome {
    let src = Path::new(&entry.source_path);
    if !fs.exists(src).unwrap_or(false) {
        mark_recovery_required(db, entry, "source_absent_during_copying");
        return RecoveryOutcome::NeedsHumanReview;
    }

    if let Some(ref temp_str) = entry.temporary_path {
        let temp = Path::new(temp_str);
        if fs.exists(temp).unwrap_or(false) {
            match mover::cleanup_owned_temp(fs, temp, entry.metadata.temp_file_id.as_deref()) {
                Ok(cleaned) => {
                    mark_failed(db, entry, "interrupted_copy");
                    return RecoveryOutcome::Failed {
                        cleaned_temp: cleaned,
                    };
                }
                Err(_) => {
                    mark_recovery_required(db, entry, "temp_cleanup_failed_during_recovery");
                    return RecoveryOutcome::NeedsHumanReview;
                }
            }
        }
    }

    mark_failed(db, entry, "interrupted_copy");
    RecoveryOutcome::Failed {
        cleaned_temp: false,
    }
}

fn recover_verified(
    db: &Database,
    fs: &dyn FsBackend,
    entry: &JournalRow,
    cancel: &CancelToken,
) -> RecoveryOutcome {
    let src = Path::new(&entry.source_path);
    let src_exists = fs.exists(src).unwrap_or(false);

    if let (Some(temp_str), Some(recorded_hash), Some(expected_src_fp)) = (
        entry.temporary_path.as_ref(),
        entry.metadata.sha256.as_ref(),
        entry.metadata.source_fingerprint.as_ref(),
    ) {
        let temp = Path::new(temp_str);
        if src_exists && fs.exists(temp).unwrap_or(false) {
            let src_unchanged = fs
                .fingerprint(src)
                .map(|fp| mover::fingerprints_equal(expected_src_fp, &fp))
                .unwrap_or(false);
            let temp_hash_ok = fs
                .hash_sha256(temp, cancel)
                .map(|h| h == *recorded_hash)
                .unwrap_or(false);

            if src_unchanged && temp_hash_ok {
                let dst = Path::new(&entry.destination_path);
                if let (Some(dest_dir), Some(file_name)) =
                    (dst.parent(), dst.file_name().and_then(|n| n.to_str()))
                {
                    if let Ok(final_dst) =
                        mover::commit_temp_no_overwrite(fs, temp, dest_dir, file_name)
                    {
                        let final_dst_str = final_dst.to_string_lossy().into_owned();
                        if let Ok(dst_fp) = fs.fingerprint(&final_dst) {
                            let dst_dto = mover::fp_to_dto(&dst_fp);
                            let mut meta = entry.metadata.clone();
                            meta.dest_fingerprint = Some(dst_dto.clone());
                            if db
                                .with_writer(|conn| {
                                    transactions::transition_journal_entry(
                                        conn,
                                        entry.id,
                                        OperationType::MoveCrossVolume,
                                        JournalState::Committed,
                                        Some(&final_dst_str),
                                        Some(temp_str),
                                        Some(&meta),
                                        None,
                                        None,
                                    )
                                })
                                .is_ok()
                            {
                                if fs.delete_file(src).is_ok()
                                    && db
                                        .with_writer(|conn| {
                                            transactions::complete_move_atomically(
                                                conn,
                                                entry.id,
                                                &entry.batch_id,
                                                &entry.source_path,
                                                &final_dst_str,
                                                true,
                                                Some(recorded_hash),
                                                dst_fp.size,
                                                &dst_dto,
                                                meta.rule_id,
                                                &meta,
                                            )
                                        })
                                        .is_ok()
                                {
                                    return RecoveryOutcome::Completed;
                                }
                                meta.detail = Some("source_remains_after_verified_copy".to_owned());
                                mark_recovery_required_with_meta(
                                    db,
                                    entry,
                                    &meta,
                                    "source_remains_after_verified_copy",
                                );
                                return RecoveryOutcome::NeedsHumanReview;
                            }
                        }
                    }
                }
            }
        }
    }

    // Cannot safely resume: discard app-owned temp if source is intact.
    if src_exists {
        if let Some(ref temp_str) = entry.temporary_path {
            let temp = Path::new(temp_str);
            if fs.exists(temp).unwrap_or(false) {
                if let Ok(cleaned) =
                    mover::cleanup_owned_temp(fs, temp, entry.metadata.temp_file_id.as_deref())
                {
                    mark_failed(db, entry, "interrupted_verified_discarded");
                    return RecoveryOutcome::Failed {
                        cleaned_temp: cleaned,
                    };
                }
                mark_recovery_required(db, entry, "verified_temp_cleanup_failed");
                return RecoveryOutcome::NeedsHumanReview;
            }
        }
        mark_failed(db, entry, "interrupted_verified_discarded");
        return RecoveryOutcome::Failed {
            cleaned_temp: false,
        };
    }

    mark_recovery_required(db, entry, "verified_source_missing");
    RecoveryOutcome::NeedsHumanReview
}

fn recover_committed(
    db: &Database,
    fs: &dyn FsBackend,
    entry: &JournalRow,
    cancel: &CancelToken,
) -> RecoveryOutcome {
    let src = Path::new(&entry.source_path);
    let dst = Path::new(&entry.destination_path);
    let src_exists = fs.exists(src).unwrap_or(false);
    let dst_exists = fs.exists(dst).unwrap_or(false);

    let Some(ref recorded_hash) = entry.metadata.sha256 else {
        mark_recovery_required(db, entry, "committed_missing_recorded_hash");
        return RecoveryOutcome::NeedsHumanReview;
    };

    if !dst_exists {
        mark_recovery_required(db, entry, "committed_destination_missing");
        return RecoveryOutcome::NeedsHumanReview;
    }

    let Ok(dst_hash) = fs.hash_sha256(dst, cancel) else {
        mark_recovery_required(db, entry, "committed_destination_unreadable");
        return RecoveryOutcome::NeedsHumanReview;
    };
    if &dst_hash != recorded_hash {
        mark_recovery_required(db, entry, "committed_destination_hash_mismatch");
        return RecoveryOutcome::NeedsHumanReview;
    }

    let Ok(dst_fp) = fs.fingerprint(dst) else {
        mark_recovery_required(db, entry, "committed_destination_fingerprint_failed");
        return RecoveryOutcome::NeedsHumanReview;
    };
    let dst_dto = mover::fp_to_dto(&dst_fp);
    let mut meta = entry.metadata.clone();
    meta.dest_fingerprint = Some(dst_dto.clone());
    meta.detail = None;

    if !src_exists {
        if db
            .with_writer(|conn| {
                transactions::complete_move_atomically(
                    conn,
                    entry.id,
                    &entry.batch_id,
                    &entry.source_path,
                    &entry.destination_path,
                    true,
                    Some(recorded_hash),
                    dst_fp.size,
                    &dst_dto,
                    meta.rule_id,
                    &meta,
                )
            })
            .is_ok()
        {
            return RecoveryOutcome::Completed;
        }
        mark_recovery_required(db, entry, "committed_finalize_db_failed");
        return RecoveryOutcome::NeedsHumanReview;
    }

    // Both source and verified destination exist: delete source ONLY if source hash matches destination (Â§5.4).
    let Ok(src_hash) = fs.hash_sha256(src, cancel) else {
        mark_recovery_required(db, entry, "committed_source_unreadable");
        return RecoveryOutcome::NeedsHumanReview;
    };
    if src_hash != dst_hash {
        // Â§5.4: Destination present; source present but differs -> recovery_required, DO NOTHING DESTRUCTIVE.
        mark_recovery_required(db, entry, "committed_source_differs_from_destination");
        return RecoveryOutcome::NeedsHumanReview;
    }

    if fs.delete_file(src).is_err() {
        meta.detail = Some("source_remains_after_verified_copy".to_owned());
        mark_recovery_required_with_meta(db, entry, &meta, "source_remains_after_verified_copy");
        return RecoveryOutcome::NeedsHumanReview;
    }

    if db
        .with_writer(|conn| {
            transactions::complete_move_atomically(
                conn,
                entry.id,
                &entry.batch_id,
                &entry.source_path,
                &entry.destination_path,
                true,
                Some(recorded_hash),
                dst_fp.size,
                &dst_dto,
                meta.rule_id,
                &meta,
            )
        })
        .is_ok()
    {
        return RecoveryOutcome::Completed;
    }

    mark_recovery_required(db, entry, "committed_finalize_db_failed");
    RecoveryOutcome::NeedsHumanReview
}

fn recover_undo_pending(
    db: &Database,
    fs: &dyn FsBackend,
    roots: &ProtectedRoots,
    entry: &JournalRow,
    recovery_batch_id: &str,
    cancel: &CancelToken,
) -> RecoveryOutcome {
    let Some(move_log_id) = entry.metadata.move_log_id else {
        mark_recovery_required(db, entry, "undo_missing_move_log_id");
        return RecoveryOutcome::NeedsHumanReview;
    };

    // Revalidate target original path (`entry.destination_path`) as untrusted (Â§5.5 #6).
    let Ok(orig_norm) = normalize_absolute_path(&entry.destination_path) else {
        mark_failed(db, entry, "undo_invalid_destination");
        return RecoveryOutcome::Failed {
            cleaned_temp: false,
        };
    };
    if is_protected_location(&orig_norm, roots) {
        mark_failed(db, entry, "undo_protected_destination");
        return RecoveryOutcome::Failed {
            cleaned_temp: false,
        };
    }

    // In `undo_move` journal rows: `source_path` = moved file (`new_path`), `destination_path` = `original_path`.
    let moved_src = Path::new(&entry.source_path);
    let orig_dst = Path::new(&entry.destination_path);
    let moved_exists = fs.exists(moved_src).unwrap_or(false);
    let orig_exists = fs.exists(orig_dst).unwrap_or(false);

    if !moved_exists && orig_exists {
        if let Ok(restored_fp) = fs.fingerprint(orig_dst) {
            let matches_expected = entry
                .metadata
                .source_fingerprint
                .as_ref()
                .map(|expected| expected.size == restored_fp.size)
                .unwrap_or(true);
            if matches_expected
                && db
                    .with_writer(|conn| {
                        transactions::complete_undo_atomically(
                            conn,
                            move_log_id,
                            entry.id,
                            &entry.batch_id,
                            &orig_norm.key,
                            &mover::fp_to_dto(&restored_fp),
                            Some("completed_by_startup_recovery"),
                            DEFAULT_RESTORE_SUPPRESSION_TTL_MS,
                        )
                    })
                    .is_ok()
            {
                return RecoveryOutcome::Undone;
            }
        }
        mark_recovery_required(db, entry, "undo_ambiguous_restored_state");
        return RecoveryOutcome::NeedsHumanReview;
    }

    if moved_exists && !orig_exists {
        let watched_root = entry.metadata.watched_root.as_deref().map(Path::new);
        if let Some(parent) = orig_dst.parent() {
            if mover::verify_safe_paths(moved_src, parent, watched_root, roots, fs).is_err() {
                mark_failed(db, entry, "undo_unsafe_destination");
                return RecoveryOutcome::Failed {
                    cleaned_temp: false,
                };
            }
        }
        match mover::safe_restore_file(
            fs,
            recovery_batch_id,
            entry.id as usize,
            moved_src,
            orig_dst,
            cancel,
        ) {
            Ok(res) => {
                if db
                    .with_writer(|conn| {
                        transactions::complete_undo_atomically(
                            conn,
                            move_log_id,
                            entry.id,
                            &entry.batch_id,
                            &orig_norm.key,
                            &res.restored_fingerprint,
                            Some("resumed_by_startup_recovery"),
                            DEFAULT_RESTORE_SUPPRESSION_TTL_MS,
                        )
                    })
                    .is_ok()
                {
                    return RecoveryOutcome::Undone;
                }
            }
            Err(AppError::AlreadyExists { .. }) => {
                mark_failed(db, entry, "conflict");
                return RecoveryOutcome::Failed {
                    cleaned_temp: false,
                };
            }
            Err(_) => {}
        }
        mark_recovery_required(db, entry, "undo_resume_failed");
        return RecoveryOutcome::NeedsHumanReview;
    }

    if moved_exists && orig_exists {
        mark_failed(db, entry, "conflict");
        return RecoveryOutcome::Failed {
            cleaned_temp: false,
        };
    }

    mark_recovery_required(db, entry, "undo_both_paths_missing");
    RecoveryOutcome::NeedsHumanReview
}

fn mark_failed(db: &Database, entry: &JournalRow, reason: &str) {
    let _ = db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            entry.id,
            entry.operation_type,
            JournalState::Failed,
            None,
            None,
            None,
            None,
            Some(reason),
        )
    });
}

fn mark_recovery_required(db: &Database, entry: &JournalRow, detail: &str) {
    let mut meta = entry.metadata.clone();
    meta.detail = Some(detail.to_owned());
    mark_recovery_required_with_meta(db, entry, &meta, detail);
}

fn mark_recovery_required_with_meta(
    db: &Database,
    entry: &JournalRow,
    meta: &JournalMetadata,
    detail: &str,
) {
    let _ = db.with_writer(|conn| {
        transactions::transition_journal_entry(
            conn,
            entry.id,
            entry.operation_type,
            JournalState::RecoveryRequired,
            None,
            None,
            Some(meta),
            None,
            Some(detail),
        )
    });
}
