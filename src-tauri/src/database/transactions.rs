#![forbid(unsafe_code)]

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::database::queries::now_epoch_ms;
use crate::errors::AppError;
use crate::models::{
    BatchDetailResponse, BatchKind, BatchListRequest, BatchListResponse, BatchStatus, BatchSummary,
    JournalState, MoveLogItem, OperationType, RecoveryEntry, SourceFingerprintDto,
};

/// Default TTL (24 hours) for `restore_suppressions` rows preventing undo -> auto-sort loops (Â§5.6).
pub const DEFAULT_RESTORE_SUPPRESSION_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// Structured metadata persisted inside `operation_journal.fingerprint_json` so
/// crash recovery acts only on verifiable facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalMetadata {
    pub source_fingerprint: Option<SourceFingerprintDto>,
    pub dest_fingerprint: Option<SourceFingerprintDto>,
    pub sha256: Option<String>,
    pub temp_file_id: Option<String>,
    pub rule_id: Option<i64>,
    pub move_log_id: Option<i64>,
    pub watched_root: Option<String>,
    pub detail: Option<String>,
}

/// Full persisted `operation_journal` row.
#[derive(Debug, Clone)]
pub struct JournalRow {
    pub id: i64,
    pub batch_id: String,
    pub source_path: String,
    pub destination_path: String,
    pub temporary_path: Option<String>,
    pub operation_type: OperationType,
    pub state: JournalState,
    pub metadata: JournalMetadata,
    pub error_code: Option<i32>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Full `move_logs` row including identity fields used by undo verification (Â§5.5).
#[derive(Debug, Clone)]
pub struct MoveLogRecord {
    pub item: MoveLogItem,
    pub volume_serial: Option<u64>,
    pub file_id: Option<String>,
    pub modified_at: Option<i64>,
}

/// Creates a new batch row in `running` state.
pub fn create_batch(
    conn: &Connection,
    batch_id: &str,
    kind: BatchKind,
    source_folder: &str,
    source_folder_id: Option<i64>,
    parent_batch_id: Option<&str>,
    total_files: u64,
) -> Result<(), AppError> {
    let now_ms = now_epoch_ms();
    conn.execute(
        "INSERT INTO batches
 (id, kind, source_folder, source_folder_id, parent_batch_id, status, total_files,
 successful_moves, failed_moves, skipped_files, total_bytes, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6, 0, 0, 0, 0, ?7)",
        params![
            batch_id,
            kind.as_str(),
            source_folder,
            source_folder_id,
            parent_batch_id,
            i64::try_from(total_files).unwrap_or(i64::MAX),
            now_ms
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert batch".to_owned(),
        message: err.to_string(),
    })?;
    Ok(())
}

/// Finalizes a batch's terminal status and aggregate counters.
#[allow(clippy::too_many_arguments)]
pub fn finalize_batch(
    conn: &Connection,
    batch_id: &str,
    status: BatchStatus,
    total_files: u64,
    successful_moves: u64,
    failed_moves: u64,
    skipped_files: u64,
    total_bytes: u64,
) -> Result<(), AppError> {
    let now_ms = now_epoch_ms();
    conn.execute(
        "UPDATE batches
 SET status = ?1, total_files = ?2, successful_moves = ?3, failed_moves = ?4,
 skipped_files = ?5, total_bytes = ?6, completed_at = ?7
 WHERE id = ?8",
        params![
            status.as_str(),
            i64::try_from(total_files).unwrap_or(i64::MAX),
            i64::try_from(successful_moves).unwrap_or(i64::MAX),
            i64::try_from(failed_moves).unwrap_or(i64::MAX),
            i64::try_from(skipped_files).unwrap_or(i64::MAX),
            i64::try_from(total_bytes).unwrap_or(i64::MAX),
            now_ms,
            batch_id
        ],
    )
    .map_err(|err| AppError::Database {
        context: "finalize batch".to_owned(),
        message: err.to_string(),
    })?;
    Ok(())
}

/// Inserts a durable write-ahead journal row BEFORE any filesystem call.
#[allow(clippy::too_many_arguments)]
pub fn insert_journal_entry(
    conn: &Connection,
    batch_id: &str,
    source_path: &str,
    destination_path: &str,
    temporary_path: Option<&str>,
    operation_type: OperationType,
    state: JournalState,
    metadata: &JournalMetadata,
) -> Result<i64, AppError> {
    let now_ms = now_epoch_ms();
    let meta_json = serde_json::to_string(metadata).map_err(|err| AppError::Internal {
        message: format!("serialize journal metadata: {err}"),
    })?;

    conn.execute(
        "INSERT INTO operation_journal
 (batch_id, source_path, destination_path, temporary_path, operation_type, state,
 fingerprint_json, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            batch_id,
            source_path,
            destination_path,
            temporary_path,
            operation_type.as_str(),
            state.as_str(),
            meta_json,
            now_ms
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert operation_journal row".to_owned(),
        message: err.to_string(),
    })?;

    Ok(conn.last_insert_rowid())
}

/// Validates and persists a state transition on an existing `operation_journal` row.
#[allow(clippy::too_many_arguments)]
pub fn transition_journal_entry(
    conn: &Connection,
    journal_id: i64,
    new_operation_type: OperationType,
    next_state: JournalState,
    destination_path: Option<&str>,
    temporary_path: Option<&str>,
    metadata: Option<&JournalMetadata>,
    error_code: Option<i32>,
    error_message: Option<&str>,
) -> Result<(), AppError> {
    let (current_state_str, current_dest, current_temp, current_meta_json): (
        String,
        String,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT state, destination_path, temporary_path, fingerprint_json
 FROM operation_journal WHERE id = ?1",
            params![journal_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|err| AppError::Database {
            context: "load journal row for transition".to_owned(),
            message: err.to_string(),
        })?;

    let current_state =
        JournalState::from_db(&current_state_str).ok_or_else(|| AppError::Database {
            context: "parse journal state".to_owned(),
            message: format!("unknown journal state '{current_state_str}'"),
        })?;

    if !current_state.can_transition(new_operation_type, next_state) {
        return Err(AppError::Internal {
            message: format!(
                "illegal journal state transition: {} -> {} ({})",
                current_state.as_str(),
                next_state.as_str(),
                new_operation_type.as_str()
            ),
        });
    }

    let final_dest = destination_path.unwrap_or(&current_dest);
    let final_temp = temporary_path.or(current_temp.as_deref());
    let final_meta_json = match metadata {
        Some(m) => Some(serde_json::to_string(m).map_err(|err| AppError::Internal {
            message: format!("serialize journal metadata: {err}"),
        })?),
        None => current_meta_json,
    };

    let now_ms = now_epoch_ms();
    conn.execute(
        "UPDATE operation_journal
 SET operation_type = ?1, state = ?2, destination_path = ?3, temporary_path = ?4,
 fingerprint_json = ?5, error_code = ?6, error_message = ?7, updated_at = ?8
 WHERE id = ?9",
        params![
            new_operation_type.as_str(),
            next_state.as_str(),
            final_dest,
            final_temp,
            final_meta_json,
            error_code,
            error_message,
            now_ms,
            journal_id
        ],
    )
    .map_err(|err| AppError::Database {
        context: "update operation_journal state".to_owned(),
        message: err.to_string(),
    })?;

    Ok(())
}

/// Atomically inserts a `move_logs` record and transitions `operation_journal`
/// to `completed` in a single SQLite transaction.
#[allow(clippy::too_many_arguments)]
pub fn complete_move_atomically(
    conn: &mut Connection,
    journal_id: i64,
    batch_id: &str,
    original_path: &str,
    new_path: &str,
    cross_volume: bool,
    sha256: Option<&str>,
    file_size: u64,
    dest_fingerprint: &SourceFingerprintDto,
    rule_id: Option<i64>,
    metadata: &JournalMetadata,
) -> Result<i64, AppError> {
    let now_ms = now_epoch_ms();
    let meta_json = serde_json::to_string(metadata).map_err(|err| AppError::Internal {
        message: format!("serialize completed journal metadata: {err}"),
    })?;

    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin complete_move_atomically".to_owned(),
        message: err.to_string(),
    })?;

    let (current_state_str, op_type_str): (String, String) = tx
        .query_row(
            "SELECT state, operation_type FROM operation_journal WHERE id = ?1",
            params![journal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|err| AppError::Database {
            context: "read journal state for completion".to_owned(),
            message: err.to_string(),
        })?;

    let current_state =
        JournalState::from_db(&current_state_str).ok_or_else(|| AppError::Database {
            context: "parse journal state".to_owned(),
            message: format!("unknown journal state '{current_state_str}'"),
        })?;
    let op_type = OperationType::from_db(&op_type_str).ok_or_else(|| AppError::Database {
        context: "parse operation_type".to_owned(),
        message: format!("unknown operation_type '{op_type_str}'"),
    })?;

    if !current_state.can_transition(op_type, JournalState::Completed) {
        return Err(AppError::Internal {
            message: format!(
                "illegal completion transition from {} ({})",
                current_state.as_str(),
                op_type.as_str()
            ),
        });
    }

    tx.execute(
        "INSERT INTO move_logs
 (batch_id, original_path, new_path, status, cross_volume, sha256, file_size,
 volume_serial, file_id, modified_at, rule_id, moved_at, undone)
 VALUES (?1, ?2, ?3, 'completed', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)",
        params![
            batch_id,
            original_path,
            new_path,
            i64::from(cross_volume),
            sha256,
            i64::try_from(file_size).unwrap_or(i64::MAX),
            dest_fingerprint
                .volume_serial
                .and_then(|v| i64::try_from(v).ok()),
            dest_fingerprint.file_id,
            dest_fingerprint.modified_epoch_ms,
            rule_id,
            now_ms
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert move_logs row".to_owned(),
        message: err.to_string(),
    })?;

    let move_log_id = tx.last_insert_rowid();

    tx.execute(
        "UPDATE operation_journal
 SET state = 'completed', destination_path = ?1, fingerprint_json = ?2, updated_at = ?3
 WHERE id = ?4",
        params![new_path, meta_json, now_ms, journal_id],
    )
    .map_err(|err| AppError::Database {
        context: "mark operation_journal completed".to_owned(),
        message: err.to_string(),
    })?;

    tx.commit().map_err(|err| AppError::Database {
        context: "commit complete_move_atomically".to_owned(),
        message: err.to_string(),
    })?;

    Ok(move_log_id)
}

/// Atomically updates `move_logs` to `undone`, updates `operation_journal` to `undone`,
/// and records a `restore_suppressions` row to prevent undo -> auto-sort loops (Â§5.5, Â§5.6).
#[allow(clippy::too_many_arguments)]
pub fn complete_undo_atomically(
    conn: &mut Connection,
    move_log_id: i64,
    journal_id: i64,
    undo_batch_id: &str,
    restored_path_key: &str,
    restored_fp: &SourceFingerprintDto,
    undo_note: Option<&str>,
    suppression_ttl_ms: i64,
) -> Result<(), AppError> {
    let now_ms = now_epoch_ms();
    let expires_at = now_ms.saturating_add(suppression_ttl_ms);

    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin complete_undo_atomically".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "UPDATE move_logs
 SET status = 'undone', undone = 1, undone_at = ?1, undo_batch_id = ?2, undo_note = ?3
 WHERE id = ?4",
        params![now_ms, undo_batch_id, undo_note, move_log_id],
    )
    .map_err(|err| AppError::Database {
        context: "mark move_log undone".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "UPDATE operation_journal SET state = 'undone', updated_at = ?1 WHERE id = ?2",
        params![now_ms, journal_id],
    )
    .map_err(|err| AppError::Database {
        context: "mark operation_journal undone".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "INSERT INTO restore_suppressions
 (path_key, volume_serial, file_id, file_size, modified_at, expires_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            restored_path_key,
            restored_fp
                .volume_serial
                .and_then(|v| i64::try_from(v).ok()),
            restored_fp.file_id,
            i64::try_from(restored_fp.size).unwrap_or(i64::MAX),
            restored_fp.modified_epoch_ms,
            expires_at
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert restore_suppression".to_owned(),
        message: err.to_string(),
    })?;

    tx.commit().map_err(|err| AppError::Database {
        context: "commit complete_undo_atomically".to_owned(),
        message: err.to_string(),
    })?;

    Ok(())
}

/// Marks a `move_logs` item as `undo_failed` with a reason note.
pub fn mark_move_log_undo_failed(
    conn: &Connection,
    move_log_id: i64,
    note: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE move_logs SET status = 'undo_failed', undo_note = ?1 WHERE id = ?2",
        params![note, move_log_id],
    )
    .map_err(|err| AppError::Database {
        context: "mark move_log undo_failed".to_owned(),
        message: err.to_string(),
    })?;
    Ok(())
}

/// Records directories created by SortDeck during a batch for safe empty-only undo cleanup (Â§4.8).
pub fn record_created_directories(
    conn: &Connection,
    batch_id: &str,
    dirs: &[String],
) -> Result<(), AppError> {
    if dirs.is_empty() {
        return Ok(());
    }
    let now_ms = now_epoch_ms();
    for dir in dirs {
        conn.execute(
            "INSERT INTO created_directories (batch_id, path, created_at) VALUES (?1, ?2, ?3)",
            params![batch_id, dir, now_ms],
        )
        .map_err(|err| AppError::Database {
            context: "insert created_directory".to_owned(),
            message: err.to_string(),
        })?;
    }
    Ok(())
}

/// Lists directories created by `batch_id` in reverse creation order (`id DESC`)
/// so child directories are removed before parent directories.
pub fn list_created_directories_desc(
    conn: &Connection,
    batch_id: &str,
) -> Result<Vec<(i64, String)>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, path FROM created_directories WHERE batch_id = ?1 ORDER BY id DESC")
        .map_err(|err| AppError::Database {
            context: "prepare list_created_directories_desc".to_owned(),
            message: err.to_string(),
        })?;
    let rows = stmt
        .query_map(params![batch_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|err| AppError::Database {
            context: "query created_directories".to_owned(),
            message: err.to_string(),
        })?;

    let mut out = Vec::new();
    for row_res in rows {
        out.push(row_res.map_err(|err| AppError::Database {
            context: "map created_directory row".to_owned(),
            message: err.to_string(),
        })?);
    }
    Ok(out)
}

/// Deletes a `created_directories` tracking row after the empty directory has been removed.
pub fn delete_created_directory_record(conn: &Connection, id: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM created_directories WHERE id = ?1", params![id])
        .map_err(|err| AppError::Database {
            context: "delete created_directory record".to_owned(),
            message: err.to_string(),
        })?;
    Ok(())
}

/// Checks whether `path_key` has an active, matching restore suppression (INV-13).
pub fn has_active_restore_suppression(
    conn: &Connection,
    path_key: &str,
    file_size: u64,
    modified_at: Option<i64>,
    now_ms: i64,
) -> Result<bool, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT file_size, modified_at FROM restore_suppressions
 WHERE path_key = ?1 AND expires_at > ?2",
        )
        .map_err(|err| AppError::Database {
            context: "prepare restore_suppressions query".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map(params![path_key, now_ms], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .map_err(|err| AppError::Database {
            context: "query restore_suppressions".to_owned(),
            message: err.to_string(),
        })?;

    for row_res in rows {
        let (sup_size, sup_mtime) = row_res.map_err(|err| AppError::Database {
            context: "map restore_suppression row".to_owned(),
            message: err.to_string(),
        })?;
        let size_matches = sup_size
            .and_then(|s| u64::try_from(s).ok())
            .is_some_and(|s| s == file_size);
        let mtime_matches = sup_mtime == modified_at;
        if size_matches && mtime_matches {
            return Ok(true);
        }
    }

    Ok(false)
}

fn row_to_batch_summary(row: &rusqlite::Row<'_>) -> Result<BatchSummary, rusqlite::Error> {
    let kind_raw: String = row.get(1)?;
    let status_raw: String = row.get(5)?;
    Ok(BatchSummary {
        id: row.get(0)?,
        kind: BatchKind::from_db(&kind_raw).unwrap_or(BatchKind::Manual),
        source_folder: row.get(2)?,
        source_folder_id: row.get(3)?,
        parent_batch_id: row.get(4)?,
        status: BatchStatus::from_db(&status_raw).unwrap_or(BatchStatus::Failed),
        total_files: u64::try_from(row.get::<_, i64>(6)?.max(0)).unwrap_or(0),
        successful_moves: u64::try_from(row.get::<_, i64>(7)?.max(0)).unwrap_or(0),
        failed_moves: u64::try_from(row.get::<_, i64>(8)?.max(0)).unwrap_or(0),
        skipped_files: u64::try_from(row.get::<_, i64>(9)?.max(0)).unwrap_or(0),
        total_bytes: u64::try_from(row.get::<_, i64>(10)?.max(0)).unwrap_or(0),
        created_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

/// Returns a single batch summary by ID.
pub fn get_batch_summary(conn: &Connection, batch_id: &str) -> Result<BatchSummary, AppError> {
    conn.query_row(
        "SELECT id, kind, source_folder, source_folder_id, parent_batch_id, status,
 total_files, successful_moves, failed_moves, skipped_files, total_bytes,
 created_at, completed_at
 FROM batches WHERE id = ?1",
        params![batch_id],
        row_to_batch_summary,
    )
    .optional()
    .map_err(|err| AppError::Database {
        context: "get batch summary".to_owned(),
        message: err.to_string(),
    })?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("batch {batch_id}"),
        os_error_code: None,
    })
}

/// Paginated query over `batches` ordered newest-first.
pub fn list_batches_paginated(
    conn: &Connection,
    req: &BatchListRequest,
) -> Result<BatchListResponse, AppError> {
    let limit = req.limit.clamp(1, 200);
    let status_param = req.status_filter.map(|s| s.as_str().to_owned());

    let total: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM batches WHERE (?1 IS NULL OR status = ?1)",
            params![status_param],
            |row| Ok(u64::try_from(row.get::<_, i64>(0)?.max(0)).unwrap_or(0)),
        )
        .map_err(|err| AppError::Database {
            context: "count batches".to_owned(),
            message: err.to_string(),
        })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, kind, source_folder, source_folder_id, parent_batch_id, status,
 total_files, successful_moves, failed_moves, skipped_files, total_bytes,
 created_at, completed_at
 FROM batches
 WHERE (?1 IS NULL OR status = ?1)
 ORDER BY created_at DESC, rowid DESC
 LIMIT ?2 OFFSET ?3",
        )
        .map_err(|err| AppError::Database {
            context: "prepare list_batches_paginated".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map(
            params![status_param, i64::from(limit), i64::from(req.offset)],
            row_to_batch_summary,
        )
        .map_err(|err| AppError::Database {
            context: "query batches".to_owned(),
            message: err.to_string(),
        })?;

    let mut items = Vec::new();
    for row_res in rows {
        items.push(row_res.map_err(|err| AppError::Database {
            context: "map batch row".to_owned(),
            message: err.to_string(),
        })?);
    }

    Ok(BatchListResponse {
        total,
        offset: req.offset,
        limit,
        items,
    })
}

fn row_to_move_log_record(row: &rusqlite::Row<'_>) -> Result<MoveLogRecord, rusqlite::Error> {
    let item = MoveLogItem {
        id: row.get(0)?,
        batch_id: row.get(1)?,
        original_path: row.get(2)?,
        new_path: row.get(3)?,
        status: row.get(4)?,
        cross_volume: row.get::<_, i64>(5)? != 0,
        sha256: row.get(6)?,
        file_size: u64::try_from(row.get::<_, i64>(7)?.max(0)).unwrap_or(0),
        rule_id: row.get(11)?,
        moved_at: row.get(12)?,
        undone: row.get::<_, i64>(13)? != 0,
        undone_at: row.get(14)?,
        undo_batch_id: row.get(15)?,
        undo_note: row.get(16)?,
    };
    let volume_serial = row
        .get::<_, Option<i64>>(8)?
        .and_then(|v| u64::try_from(v).ok());
    let file_id = row.get(9)?;
    let modified_at = row.get(10)?;

    Ok(MoveLogRecord {
        item,
        volume_serial,
        file_id,
        modified_at,
    })
}

/// Returns a single `move_logs` row by primary key ID.
pub fn get_move_log_by_id(conn: &Connection, move_log_id: i64) -> Result<MoveLogRecord, AppError> {
    conn.query_row(
        "SELECT id, batch_id, original_path, new_path, status, cross_volume, sha256, file_size,
 volume_serial, file_id, modified_at, rule_id, moved_at, undone, undone_at,
 undo_batch_id, undo_note
 FROM move_logs WHERE id = ?1",
        params![move_log_id],
        row_to_move_log_record,
    )
    .optional()
    .map_err(|err| AppError::Database {
        context: "get move_log by id".to_owned(),
        message: err.to_string(),
    })?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("move log {move_log_id}"),
        os_error_code: None,
    })
}

/// Lists move logs for `batch_id` in execution order (`id ASC`).
pub fn get_batch_detail(
    conn: &Connection,
    batch_id: &str,
) -> Result<BatchDetailResponse, AppError> {
    let batch = get_batch_summary(conn, batch_id)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, batch_id, original_path, new_path, status, cross_volume, sha256, file_size,
 volume_serial, file_id, modified_at, rule_id, moved_at, undone, undone_at,
 undo_batch_id, undo_note
 FROM move_logs WHERE batch_id = ?1 ORDER BY id ASC",
        )
        .map_err(|err| AppError::Database {
            context: "prepare get_batch_detail".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map(params![batch_id], row_to_move_log_record)
        .map_err(|err| AppError::Database {
            context: "query move_logs".to_owned(),
            message: err.to_string(),
        })?;

    let mut items = Vec::new();
    for row_res in rows {
        items.push(
            row_res
                .map_err(|err| AppError::Database {
                    context: "map move_log row".to_owned(),
                    message: err.to_string(),
                })?
                .item,
        );
    }

    Ok(BatchDetailResponse { batch, items })
}

/// Searches `move_logs` across original and destination paths so users can locate moved
/// files quickly by partial filename. Escapes SQL LIKE wildcards cleanly.
pub fn search_move_logs(
    conn: &Connection,
    query: &str,
    limit: u32,
) -> Result<Vec<MoveLogItem>, AppError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let capped_limit = limit.clamp(1, 50);
    let escaped = trimmed
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    let mut stmt = conn
        .prepare(
            "SELECT id, batch_id, original_path, new_path, status, cross_volume, sha256, file_size,
 volume_serial, file_id, modified_at, rule_id, moved_at, undone, undone_at,
 undo_batch_id, undo_note
 FROM move_logs
 WHERE status = 'completed'
 AND (original_path LIKE ?1 ESCAPE '\\' OR new_path LIKE ?1 ESCAPE '\\')
 ORDER BY moved_at DESC, id DESC
 LIMIT ?2",
        )
        .map_err(|err| AppError::Database {
            context: "prepare search_move_logs".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map(
            params![pattern, i64::from(capped_limit)],
            row_to_move_log_record,
        )
        .map_err(|err| AppError::Database {
            context: "query search_move_logs".to_owned(),
            message: err.to_string(),
        })?;

    let mut items = Vec::new();
    for row_res in rows {
        items.push(
            row_res
                .map_err(|err| AppError::Database {
                    context: "map search move_log row".to_owned(),
                    message: err.to_string(),
                })?
                .item,
        );
    }
    Ok(items)
}

/// Lists eligible move logs for undo in **reverse execution order** (`id DESC`, Â§5.5 #3).
pub fn list_move_logs_for_undo_desc(
    conn: &Connection,
    batch_id: &str,
    item_ids: Option<&[i64]>,
) -> Result<Vec<MoveLogRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, batch_id, original_path, new_path, status, cross_volume, sha256, file_size,
 volume_serial, file_id, modified_at, rule_id, moved_at, undone, undone_at,
 undo_batch_id, undo_note
 FROM move_logs
 WHERE batch_id = ?1 AND undone = 0 AND status IN ('completed', 'undo_failed')
 ORDER BY id DESC",
        )
        .map_err(|err| AppError::Database {
            context: "prepare list_move_logs_for_undo_desc".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map(params![batch_id], row_to_move_log_record)
        .map_err(|err| AppError::Database {
            context: "query undoable move_logs".to_owned(),
            message: err.to_string(),
        })?;

    let mut records = Vec::new();
    for row_res in rows {
        let rec = row_res.map_err(|err| AppError::Database {
            context: "map undoable move_log".to_owned(),
            message: err.to_string(),
        })?;
        if let Some(filter_ids) = item_ids {
            if !filter_ids.contains(&rec.item.id) {
                continue;
            }
        }
        records.push(rec);
    }
    Ok(records)
}

fn row_to_journal_row(row: &rusqlite::Row<'_>) -> Result<JournalRow, rusqlite::Error> {
    let op_type_raw: String = row.get(5)?;
    let state_raw: String = row.get(6)?;
    let meta_json: Option<String> = row.get(7)?;
    let metadata = meta_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<JournalMetadata>(s).ok())
        .unwrap_or_default();

    Ok(JournalRow {
        id: row.get(0)?,
        batch_id: row.get(1)?,
        source_path: row.get(2)?,
        destination_path: row.get(3)?,
        temporary_path: row.get(4)?,
        operation_type: OperationType::from_db(&op_type_raw)
            .unwrap_or(OperationType::MoveSameVolume),
        state: JournalState::from_db(&state_raw).unwrap_or(JournalState::RecoveryRequired),
        metadata,
        error_code: row.get(8)?,
        error_message: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

/// Lists non-terminal journal rows for startup recovery (`idx_journal_open_state`).
pub fn list_open_journal_entries(
    conn: &Connection,
    include_recovery_required: bool,
) -> Result<Vec<JournalRow>, AppError> {
    let sql = if include_recovery_required {
        "SELECT id, batch_id, source_path, destination_path, temporary_path, operation_type, state,
 fingerprint_json, error_code, error_message, created_at, updated_at
 FROM operation_journal
 WHERE state NOT IN ('completed','undone','failed')
 ORDER BY id ASC"
    } else {
        "SELECT id, batch_id, source_path, destination_path, temporary_path, operation_type, state,
 fingerprint_json, error_code, error_message, created_at, updated_at
 FROM operation_journal
 WHERE state NOT IN ('completed','undone','failed','recovery_required')
 ORDER BY id ASC"
    };

    let mut stmt = conn.prepare(sql).map_err(|err| AppError::Database {
        context: "prepare list_open_journal_entries".to_owned(),
        message: err.to_string(),
    })?;

    let rows = stmt
        .query_map([], row_to_journal_row)
        .map_err(|err| AppError::Database {
            context: "query open journal entries".to_owned(),
            message: err.to_string(),
        })?;

    let mut out = Vec::new();
    for row_res in rows {
        out.push(row_res.map_err(|err| AppError::Database {
            context: "map journal row".to_owned(),
            message: err.to_string(),
        })?);
    }
    Ok(out)
}

/// Lists journal entries currently in `recovery_required` state for the Recovery Center.
pub fn list_recovery_required_entries(conn: &Connection) -> Result<Vec<RecoveryEntry>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, batch_id, source_path, destination_path, temporary_path, operation_type,
 state, fingerprint_json, error_code, error_message, created_at, updated_at
 FROM operation_journal
 WHERE state = 'recovery_required'
 ORDER BY updated_at DESC, id DESC",
        )
        .map_err(|err| AppError::Database {
            context: "prepare list_recovery_required_entries".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map([], row_to_journal_row)
        .map_err(|err| AppError::Database {
            context: "query recovery_required entries".to_owned(),
            message: err.to_string(),
        })?;

    let mut out = Vec::new();
    for row_res in rows {
        let j = row_res.map_err(|err| AppError::Database {
            context: "map recovery entry".to_owned(),
            message: err.to_string(),
        })?;
        out.push(RecoveryEntry {
            journal_id: j.id,
            batch_id: j.batch_id,
            operation_type: j.operation_type,
            state: j.state,
            source_path: j.source_path,
            destination_path: j.destination_path,
            temporary_path: j.temporary_path,
            error_code: j.error_code,
            error_message: j.error_message,
            detail: j.metadata.detail,
            updated_at: j.updated_at,
        });
    }
    Ok(out)
}

/// Marks a `recovery_required` journal row as acknowledged (`failed`) without touching disk files.
pub fn acknowledge_recovery_entry(conn: &Connection, journal_id: i64) -> Result<(), AppError> {
    let op_type_raw: Option<String> = conn
.query_row(
 "SELECT operation_type FROM operation_journal WHERE id = ?1 AND state = 'recovery_required'",
 params![journal_id],
 |row| row.get(0),
 )
.optional()
.map_err(|err| AppError::Database {
 context: "find recovery_required entry".to_owned(),
 message: err.to_string(),
 })?;

    let Some(op_raw) = op_type_raw else {
        return Err(AppError::NotFound {
            resource: format!("recovery entry {journal_id}"),
            os_error_code: None,
        });
    };

    let op_type = OperationType::from_db(&op_raw).unwrap_or(OperationType::MoveSameVolume);
    transition_journal_entry(
        conn,
        journal_id,
        op_type,
        JournalState::Failed,
        None,
        None,
        None,
        None,
        Some("acknowledged_by_user"),
    )
}

/// Lists IDs of batches still marked `running` (e.g. left open after an unclean crash).
pub fn list_running_batch_ids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id FROM batches WHERE status = 'running'")
        .map_err(|err| AppError::Database {
            context: "prepare list_running_batch_ids".to_owned(),
            message: err.to_string(),
        })?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| AppError::Database {
            context: "query running batches".to_owned(),
            message: err.to_string(),
        })?;
    let mut ids = Vec::new();
    for row_res in rows {
        ids.push(row_res.map_err(|err| AppError::Database {
            context: "map running batch id".to_owned(),
            message: err.to_string(),
        })?);
    }
    Ok(ids)
}

/// Recomputes and finalizes an interrupted `running` batch from its journal and move_log rows.
pub fn reconcile_interrupted_batch(conn: &Connection, batch_id: &str) -> Result<(), AppError> {
    let completed: u64 = conn
.query_row(
 "SELECT COUNT(*) FROM operation_journal WHERE batch_id = ?1 AND state IN ('completed','undone')",
 params![batch_id],
 |r| Ok(u64::try_from(r.get::<_, i64>(0)?.max(0)).unwrap_or(0)),
 )
.map_err(|err| AppError::Database {
 context: "count completed journal rows".to_owned(),
 message: err.to_string(),
 })?;

    let failed: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM operation_journal WHERE batch_id = ?1 AND state = 'failed'",
            params![batch_id],
            |r| Ok(u64::try_from(r.get::<_, i64>(0)?.max(0)).unwrap_or(0)),
        )
        .map_err(|err| AppError::Database {
            context: "count failed journal rows".to_owned(),
            message: err.to_string(),
        })?;

    let rec_req: u64 = conn
.query_row(
 "SELECT COUNT(*) FROM operation_journal WHERE batch_id = ?1 AND state = 'recovery_required'",
 params![batch_id],
 |r| Ok(u64::try_from(r.get::<_, i64>(0)?.max(0)).unwrap_or(0)),
 )
.map_err(|err| AppError::Database {
 context: "count recovery_required journal rows".to_owned(),
 message: err.to_string(),
 })?;

    let total_bytes: u64 = conn
        .query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM move_logs WHERE batch_id = ?1",
            params![batch_id],
            |r| Ok(u64::try_from(r.get::<_, i64>(0)?.max(0)).unwrap_or(0)),
        )
        .map_err(|err| AppError::Database {
            context: "sum move_logs bytes".to_owned(),
            message: err.to_string(),
        })?;

    let status = if rec_req > 0 {
        BatchStatus::RecoveryRequired
    } else if completed > 0 && failed > 0 {
        BatchStatus::Partial
    } else if completed == 0 && failed > 0 {
        BatchStatus::Failed
    } else {
        BatchStatus::Completed
    };

    finalize_batch(
        conn,
        batch_id,
        status,
        completed + failed + rec_req,
        completed,
        failed,
        0,
        total_bytes,
    )
}
