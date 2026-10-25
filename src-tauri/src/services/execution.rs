#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::path::Path;

use uuid::Uuid;

use crate::database::{queries, transactions};
use crate::engine::mover::{self, SafeMoveRequest};
use crate::engine::paths::normalize_absolute_path;
use crate::engine::planner;
use crate::engine::scanner::resolve_destination_dir;
use crate::engine::undo;
use crate::errors::AppError;
use crate::fsops::CancelToken;
use crate::models::{
    BatchKind, BatchStatus, ExecutePlanRequest, ExecutionResult, OperationExecutionOutcome, Rule,
    UndoRequest, UndoResult,
};
use crate::services::state::AppState;

/// Revalidates and executes a backend-owned dry-run plan.
pub fn execute_plan(
    state: &AppState,
    req: ExecutePlanRequest,
) -> Result<ExecutionResult, AppError> {
    if req.plan_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "planId must not be empty".to_owned(),
        });
    }

    let plan = {
        let store = state.plans.lock().map_err(|_| AppError::Internal {
            message: "plan store mutex poisoned".to_owned(),
        })?;
        store
            .get(&req.plan_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound {
                resource: format!("plan {}", req.plan_id),
                os_error_code: None,
            })?
    };

    let (folder, rules, current_rev) = state.db.with_writer(|conn| {
        let folder = queries::get_watched_folder(conn, plan.summary.folder_id)?;
        let rules = queries::list_rules(conn)?;
        let rev = queries::get_config_revision(conn)?;
        Ok((folder, rules, rev))
    })?;

    let now_ms = queries::now_epoch_ms();
    let revalidation = planner::revalidate_plan(
        &plan,
        &req.deselected_operation_ids,
        current_rev,
        &folder,
        &rules,
        &state.protected_roots,
        state.fs.as_ref(),
        now_ms,
    )?;

    let valid_ids: HashSet<&str> = revalidation
        .valid_operation_ids
        .iter()
        .map(String::as_str)
        .collect();
    let rules_by_id: HashMap<i64, &Rule> = rules.iter().map(|r| (r.id, r)).collect();
    let folder_norm = normalize_absolute_path(&folder.path)?;
    let watched_root = Path::new(&folder.path);

    let batch_id = Uuid::new_v4().to_string();
    let cancel = CancelToken::new();
    state.active_jobs.register(&batch_id, cancel.clone())?;

    let total_planned = u64::try_from(valid_ids.len() + revalidation.rejected.len()).unwrap_or(0);

    state.db.with_writer(|conn| {
        transactions::create_batch(
            conn,
            &batch_id,
            BatchKind::Manual,
            &folder.path,
            Some(folder.id),
            None,
            total_planned,
        )
    })?;

    let mut outcomes = Vec::with_capacity(valid_ids.len() + revalidation.rejected.len());
    let mut succeeded: u64 = 0;
    let mut failed: u64 = 0;
    let mut skipped: u64 = u64::try_from(revalidation.rejected.len()).unwrap_or(0);
    let mut recovery_required_count: u64 = 0;
    let mut total_bytes: u64 = 0;

    for rej in &revalidation.rejected {
        outcomes.push(OperationExecutionOutcome {
            operation_id: rej.operation_id.clone(),
            source_path: String::new(),
            destination_path: None,
            success: false,
            error_code: Some(format!("{:?}", rej.reason)),
            error_message: Some(rej.detail.clone()),
            recovery_required: false,
        });
    }

    for (seq, op) in plan
        .operations
        .iter()
        .filter(|op| valid_ids.contains(op.operation_id.as_str()))
        .enumerate()
    {
        if cancel.is_cancelled() {
            break;
        }

        let Some(rule_id) = op.matched_rule_id else {
            skipped += 1;
            continue;
        };
        let Some(rule) = rules_by_id.get(&rule_id) else {
            skipped += 1;
            continue;
        };
        let Some(dest_dir_norm) = resolve_destination_dir(&folder_norm, &rule.destination) else {
            failed += 1;
            outcomes.push(OperationExecutionOutcome {
                operation_id: op.operation_id.clone(),
                source_path: op.source_path.clone(),
                destination_path: op.destination_path.clone(),
                success: false,
                error_code: Some("PATH_VALIDATION".to_owned()),
                error_message: Some("unable to resolve destination directory".to_owned()),
                recovery_required: false,
            });
            continue;
        };

        let src_norm = match normalize_absolute_path(&op.source_path) {
            Ok(n) => n,
            Err(err) => {
                failed += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: false,
                });
                continue;
            }
        };

        let _in_flight_guard = match state.in_flight.try_acquire(&src_norm.key) {
            Ok(guard) => guard,
            Err(err) => {
                failed += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: false,
                });
                continue;
            }
        };

        let move_req = SafeMoveRequest {
            batch_id: &batch_id,
            seq,
            src_path: Path::new(&op.source_path),
            dest_dir: &dest_dir_norm.path,
            original_filename: &op.original_filename,
            expected_src_fingerprint: Some(&op.source_fingerprint),
            rule_id: Some(rule.id),
            watched_root: Some(watched_root),
        };

        match mover::safe_move(
            &state.db,
            state.fs.as_ref(),
            &state.protected_roots,
            &move_req,
            &cancel,
            state.failpoint.as_ref(),
        ) {
            Ok(res) => {
                succeeded += 1;
                total_bytes = total_bytes.saturating_add(res.file_size);
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: Some(res.final_destination.to_string_lossy().into_owned()),
                    success: true,
                    error_code: None,
                    error_message: None,
                    recovery_required: false,
                });
            }
            Err(err @ AppError::DiskFull { .. }) => {
                failed += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: false,
                });
                break;
            }
            Err(err @ AppError::Cancelled) => {
                failed += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: false,
                });
                break;
            }
            Err(err @ AppError::RecoveryRequired { .. }) => {
                failed += 1;
                recovery_required_count += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: true,
                });
            }
            Err(err) => {
                failed += 1;
                outcomes.push(OperationExecutionOutcome {
                    operation_id: op.operation_id.clone(),
                    source_path: op.source_path.clone(),
                    destination_path: None,
                    success: false,
                    error_code: Some(err.code().to_owned()),
                    error_message: Some(err.to_string()),
                    recovery_required: false,
                });
            }
        }
    }

    state.active_jobs.unregister(&batch_id);

    let status = if cancel.is_cancelled() {
        BatchStatus::Cancelled
    } else if recovery_required_count > 0 {
        BatchStatus::RecoveryRequired
    } else if succeeded > 0 && (failed > 0 || !revalidation.rejected.is_empty()) {
        BatchStatus::Partial
    } else if succeeded == 0 && (failed > 0 || !revalidation.rejected.is_empty()) {
        BatchStatus::Failed
    } else {
        BatchStatus::Completed
    };

    state.db.with_writer(|conn| {
        transactions::finalize_batch(
            conn,
            &batch_id,
            status,
            total_planned,
            succeeded,
            failed,
            skipped,
            total_bytes,
        )
    })?;

    if let Ok(mut store) = state.plans.lock() {
        store.remove(&req.plan_id);
    }

    Ok(ExecutionResult {
        batch_id,
        status,
        total_planned,
        succeeded,
        failed,
        skipped,
        recovery_required_count,
        outcomes,
    })
}

/// Signals cancellation for a running execution batch.
pub fn cancel_batch(state: &AppState, batch_id: &str) -> Result<bool, AppError> {
    if batch_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "batchId must not be empty".to_owned(),
        });
    }
    state.active_jobs.cancel(batch_id)
}

/// Undoes a whole batch or selected items inside a batch.
pub fn undo_batch(state: &AppState, req: UndoRequest) -> Result<UndoResult, AppError> {
    let cancel = CancelToken::new();
    undo::execute_undo(
        &state.db,
        state.fs.as_ref(),
        &state.protected_roots,
        &req,
        &cancel,
        state.failpoint.as_ref(),
    )
}
