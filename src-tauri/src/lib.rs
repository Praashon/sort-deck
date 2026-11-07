pub mod commands;
pub mod config;
pub mod database;
pub mod engine;
pub mod errors;
pub mod fsops;
pub mod jobs;
pub mod logging;
pub mod models;
#[allow(unsafe_code)]
pub mod platform;
pub mod recovery;
pub mod services;

use std::sync::Arc;

use tauri::Manager;

use crate::database::Database;
use crate::engine::paths::ProtectedRoots;
use crate::fsops::local::LocalFsBackend;
use crate::services::state::AppState;

/// Entry point invoked by `main.rs`.
pub fn run() {
    logging::install_panic_hook();

    let data_dir = match config::resolve_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("SortDeck data directory resolution failed: {err}");
            return;
        }
    };

    let _log_guard = logging::init_logging(&data_dir.join("logs"))
        .map_err(|err| {
            eprintln!("SortDeck logging initialization warning: {err}");
            err
        })
        .ok();

    let db = match Database::open_in_dir(&data_dir) {
        Ok(db) => db,
        Err(err) => {
            tracing::error!(error = %err, "failed to initialize SQLite database");
            eprintln!("SortDeck database initialization failed: {err}");
            return;
        }
    };

    let protected_roots = ProtectedRoots::from_system_env(&data_dir);
    let fs_backend = Arc::new(LocalFsBackend);

    // Run crash recovery before any watcher or job starts (§5.4, §8.3).
    match recovery::run_recovery(&db, fs_backend.as_ref(), &protected_roots, false) {
        Ok(summary) if summary.inspected_entries > 0 => {
            tracing::info!(
                inspected = summary.inspected_entries,
                completed = summary.finalized_completed,
                undone = summary.finalized_undone,
                failed = summary.marked_failed,
                recovery_required = summary.recovery_required,
                "startup recovery completed"
            );
        }
        Ok(_) => {}
        Err(err) => {
            tracing::error!(error = %err, "startup recovery failed");
        }
    }

    let app_state = AppState::new(db, protected_roots, fs_backend);

    let result = tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_database_status,
            commands::get_settings,
            commands::update_settings,
            commands::list_watched_folders,
            commands::add_watched_folder,
            commands::update_watched_folder,
            commands::remove_watched_folder,
            commands::list_rules,
            commands::create_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::reorder_rules,
            commands::restore_default_rules,
            commands::create_dry_run_plan,
            commands::get_plan_page,
            commands::discard_plan,
            commands::execute_plan,
            commands::cancel_batch,
            commands::undo_batch,
            commands::list_batches,
            commands::get_batch_detail,
            commands::list_recovery_items,
            commands::acknowledge_recovery_item,
            commands::retry_recovery,
            commands::reveal_move_log_in_explorer,
            commands::open_logs_folder,
            commands::get_suggested_downloads_folder,
            commands::export_redacted_diagnostics,
            commands::search_move_logs,
            commands::analyze_folder_storage,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(error = %err, "tauri application exited with error");
        eprintln!("SortDeck failed to start: {err}");
    }
}

#[cfg(test)]
mod ts_bindings {
    use ts_rs::TS;

    use crate::config::{AppInfo, AppSettings, ThemeMode};
    use crate::errors::AppErrorPayload;
    use crate::models::{
        AddWatchedFolderRequest, BatchDetailResponse, BatchKind, BatchListRequest,
        BatchListResponse, BatchStatus, BatchSummary, CreateRuleRequest, DatabaseStatusInfo,
        DestinationGroupSummary, DuplicateGroup, ExecutePlanRequest, ExecutionResult,
        ExtensionOverlapWarning, FolderStatus, FolderStorageReport, JournalState, LargeFileEntry,
        MoveLogItem, OperationExecutionOutcome, OperationRevalidationFailure, OperationStatus,
        OperationType, PlanPageRequest, PlanPageResponse, PlanRevalidationReport, PlanSummary,
        PlannedOperation, RecoveryEntry, RecoverySummary, RedactedDiagnostics, ReorderRulesRequest,
        Rule, RulesListResponse, SkipReason, SkipReasonCount, SourceFingerprintDto,
        StorageCategoryBreakdown, UndoItemOutcome, UndoRequest, UndoResult, UpdateRuleRequest,
        UpdateWatchedFolderRequest, WatchedFolder,
    };

    #[test]
    fn export_bindings() {
        let _temp_guard;
        let cfg = if std::env::var_os("TS_RS_EXPORT_DIR").is_some() {
            ts_rs::Config::from_env()
        } else {
            let dir = tempfile::tempdir().expect("create temp bindings dir");
            let cfg = ts_rs::Config::from_env().with_out_dir(dir.path());
            _temp_guard = Some(dir);
            cfg
        };
        AppErrorPayload::export_all(&cfg).expect("export AppErrorPayload");
        AppInfo::export_all(&cfg).expect("export AppInfo");
        AppSettings::export_all(&cfg).expect("export AppSettings");
        ThemeMode::export_all(&cfg).expect("export ThemeMode");
        FolderStatus::export_all(&cfg).expect("export FolderStatus");
        WatchedFolder::export_all(&cfg).expect("export WatchedFolder");
        AddWatchedFolderRequest::export_all(&cfg).expect("export AddWatchedFolderRequest");
        UpdateWatchedFolderRequest::export_all(&cfg).expect("export UpdateWatchedFolderRequest");
        Rule::export_all(&cfg).expect("export Rule");
        CreateRuleRequest::export_all(&cfg).expect("export CreateRuleRequest");
        UpdateRuleRequest::export_all(&cfg).expect("export UpdateRuleRequest");
        ReorderRulesRequest::export_all(&cfg).expect("export ReorderRulesRequest");
        ExtensionOverlapWarning::export_all(&cfg).expect("export ExtensionOverlapWarning");
        RulesListResponse::export_all(&cfg).expect("export RulesListResponse");
        DatabaseStatusInfo::export_all(&cfg).expect("export DatabaseStatusInfo");
        SkipReason::export_all(&cfg).expect("export SkipReason");
        OperationStatus::export_all(&cfg).expect("export OperationStatus");
        SourceFingerprintDto::export_all(&cfg).expect("export SourceFingerprintDto");
        PlannedOperation::export_all(&cfg).expect("export PlannedOperation");
        SkipReasonCount::export_all(&cfg).expect("export SkipReasonCount");
        DestinationGroupSummary::export_all(&cfg).expect("export DestinationGroupSummary");
        PlanSummary::export_all(&cfg).expect("export PlanSummary");
        PlanPageRequest::export_all(&cfg).expect("export PlanPageRequest");
        PlanPageResponse::export_all(&cfg).expect("export PlanPageResponse");
        OperationRevalidationFailure::export_all(&cfg)
            .expect("export OperationRevalidationFailure");
        PlanRevalidationReport::export_all(&cfg).expect("export PlanRevalidationReport");
        BatchKind::export_all(&cfg).expect("export BatchKind");
        BatchStatus::export_all(&cfg).expect("export BatchStatus");
        OperationType::export_all(&cfg).expect("export OperationType");
        JournalState::export_all(&cfg).expect("export JournalState");
        ExecutePlanRequest::export_all(&cfg).expect("export ExecutePlanRequest");
        OperationExecutionOutcome::export_all(&cfg).expect("export OperationExecutionOutcome");
        ExecutionResult::export_all(&cfg).expect("export ExecutionResult");
        UndoRequest::export_all(&cfg).expect("export UndoRequest");
        UndoItemOutcome::export_all(&cfg).expect("export UndoItemOutcome");
        UndoResult::export_all(&cfg).expect("export UndoResult");
        BatchSummary::export_all(&cfg).expect("export BatchSummary");
        MoveLogItem::export_all(&cfg).expect("export MoveLogItem");
        BatchDetailResponse::export_all(&cfg).expect("export BatchDetailResponse");
        BatchListRequest::export_all(&cfg).expect("export BatchListRequest");
        BatchListResponse::export_all(&cfg).expect("export BatchListResponse");
        RecoveryEntry::export_all(&cfg).expect("export RecoveryEntry");
        RecoverySummary::export_all(&cfg).expect("export RecoverySummary");
        RedactedDiagnostics::export_all(&cfg).expect("export RedactedDiagnostics");
        StorageCategoryBreakdown::export_all(&cfg).expect("export StorageCategoryBreakdown");
        LargeFileEntry::export_all(&cfg).expect("export LargeFileEntry");
        DuplicateGroup::export_all(&cfg).expect("export DuplicateGroup");
        FolderStorageReport::export_all(&cfg).expect("export FolderStorageReport");
    }

    #[test]
    fn windows_manifest_includes_common_controls_v6_and_safety_settings() {
        let manifest = include_str!("../windows/sortdeck.manifest");
        assert!(manifest.contains("Microsoft.Windows.Common-Controls"));
        assert!(manifest.contains(r#"version="6.0.0.0""#));
        assert!(manifest.contains(r#"level="asInvoker""#));
        assert!(manifest.contains("<longPathAware"));
        assert!(manifest.contains("PerMonitorV2"));
    }
}
