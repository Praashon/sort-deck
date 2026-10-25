#![forbid(unsafe_code)]

use std::path::Path;

use crate::database::transactions;
use crate::engine::paths;
use crate::errors::AppError;
use crate::models::{
    BatchDetailResponse, BatchListRequest, BatchListResponse, MoveLogItem, RecoveryEntry,
    RecoverySummary,
};
use crate::recovery;
use crate::services::state::AppState;

/// Returns paginated operation batches newest-first.
pub fn list_batches(
    state: &AppState,
    req: BatchListRequest,
) -> Result<BatchListResponse, AppError> {
    state
        .db
        .with_writer(|conn| transactions::list_batches_paginated(conn, &req))
}

/// Returns batch metadata and individual move logs in execution order.
pub fn get_batch_detail(state: &AppState, batch_id: &str) -> Result<BatchDetailResponse, AppError> {
    if batch_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "batchId must not be empty".to_owned(),
        });
    }
    state
        .db
        .with_writer(|conn| transactions::get_batch_detail(conn, batch_id))
}

/// Searches persisted move logs across source and destination paths by partial query.
pub fn search_move_logs(
    state: &AppState,
    query: &str,
    limit: u32,
) -> Result<Vec<MoveLogItem>, AppError> {
    state
        .db
        .with_writer(|conn| transactions::search_move_logs(conn, query, limit))
}

/// Lists journal entries currently awaiting human resolution in the Recovery Center.
pub fn list_recovery_items(state: &AppState) -> Result<Vec<RecoveryEntry>, AppError> {
    state
        .db
        .with_writer(|conn| transactions::list_recovery_required_entries(conn))
}

/// Marks a `recovery_required` journal row as acknowledged without deleting files.
pub fn acknowledge_recovery_item(state: &AppState, journal_id: i64) -> Result<(), AppError> {
    if journal_id <= 0 {
        return Err(AppError::InvalidInput {
            message: "journalId must be positive".to_owned(),
        });
    }
    state
        .db
        .with_writer(|conn| transactions::acknowledge_recovery_entry(conn, journal_id))
}

/// Re-runs verification/recovery including `recovery_required` rows where safe.
pub fn retry_recovery(state: &AppState) -> Result<RecoverySummary, AppError> {
    recovery::run_recovery(&state.db, state.fs.as_ref(), &state.protected_roots, true)
}

/// Reveals the file referenced by `move_log_id` in Windows Explorer without executing it.
pub fn reveal_move_log_in_explorer(state: &AppState, move_log_id: i64) -> Result<(), AppError> {
    if move_log_id <= 0 {
        return Err(AppError::InvalidInput {
            message: "moveLogId must be positive".to_owned(),
        });
    }

    let record = state
        .db
        .with_writer(|conn| transactions::get_move_log_by_id(conn, move_log_id))?;

    let target_raw = if record.item.undone {
        &record.item.original_path
    } else {
        &record.item.new_path
    };

    let normalized = paths::normalize_absolute_path(target_raw)?;
    if paths::is_protected_location(&normalized, &state.protected_roots)
        || paths::contains_excluded_dir_component(&normalized)
    {
        return Err(AppError::PermissionDenied {
            resource: normalized.display,
            os_error_code: None,
        });
    }

    let target_path = Path::new(&normalized.display);
    if !state.fs.exists(target_path)? {
        return Err(AppError::NotFound {
            resource: normalized.display,
            os_error_code: None,
        });
    }

    #[cfg(windows)]
    {
        crate::platform::windows::reveal_in_explorer(target_path)
    }
    #[cfg(not(windows))]
    {
        let _ = target_path;
        Err(AppError::Internal {
            message: "reveal_in_explorer is only supported on Windows".to_owned(),
        })
    }
}
