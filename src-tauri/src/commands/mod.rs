#![forbid(unsafe_code)]

use crate::config::{AppInfo, AppSettings};
use crate::errors::AppError;
use crate::models::{
    AddWatchedFolderRequest, BatchDetailResponse, BatchListRequest, BatchListResponse,
    CreateRuleRequest, DatabaseStatusInfo, ExecutePlanRequest, ExecutionResult,
    FolderStorageReport, MoveLogItem, PlanPageRequest, PlanPageResponse, PlanSummary,
    RecoveryEntry, RecoverySummary, RedactedDiagnostics, ReorderRulesRequest, Rule,
    RulesListResponse, UndoRequest, UndoResult, UpdateRuleRequest, UpdateWatchedFolderRequest,
    WatchedFolder,
};
use crate::services;
use crate::services::state::AppState;

/// Thin IPC handler returning baseline application metadata.
#[tauri::command]
pub fn get_app_info() -> Result<AppInfo, AppError> {
    Ok(services::get_app_info())
}

#[tauri::command]
pub fn get_database_status(
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseStatusInfo, AppError> {
    services::get_database_status(&state)
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, AppError> {
    services::settings::get_settings(&state)
}

#[tauri::command]
pub fn update_settings(
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    services::settings::update_settings(&state, settings)
}

#[tauri::command]
pub fn list_watched_folders(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<WatchedFolder>, AppError> {
    services::folders::list_watched_folders(&state)
}

#[tauri::command]
pub fn add_watched_folder(
    state: tauri::State<'_, AppState>,
    request: AddWatchedFolderRequest,
) -> Result<WatchedFolder, AppError> {
    services::folders::add_watched_folder(&state, request)
}

#[tauri::command]
pub fn update_watched_folder(
    state: tauri::State<'_, AppState>,
    request: UpdateWatchedFolderRequest,
) -> Result<WatchedFolder, AppError> {
    services::folders::update_watched_folder(&state, request)
}

#[tauri::command]
pub fn remove_watched_folder(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    services::folders::remove_watched_folder(&state, id)
}

#[tauri::command]
pub fn list_rules(state: tauri::State<'_, AppState>) -> Result<RulesListResponse, AppError> {
    services::rules::list_rules(&state)
}

#[tauri::command]
pub fn create_rule(
    state: tauri::State<'_, AppState>,
    request: CreateRuleRequest,
) -> Result<Rule, AppError> {
    services::rules::create_rule(&state, request)
}

#[tauri::command]
pub fn update_rule(
    state: tauri::State<'_, AppState>,
    request: UpdateRuleRequest,
) -> Result<Rule, AppError> {
    services::rules::update_rule(&state, request)
}

#[tauri::command]
pub fn delete_rule(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    services::rules::delete_rule(&state, id)
}

#[tauri::command]
pub fn reorder_rules(
    state: tauri::State<'_, AppState>,
    request: ReorderRulesRequest,
) -> Result<RulesListResponse, AppError> {
    services::rules::reorder_rules(&state, request)
}

#[tauri::command]
pub fn restore_default_rules(
    state: tauri::State<'_, AppState>,
) -> Result<RulesListResponse, AppError> {
    services::rules::restore_default_rules(&state)
}

#[tauri::command]
pub fn create_dry_run_plan(
    state: tauri::State<'_, AppState>,
    folder_id: i64,
) -> Result<PlanSummary, AppError> {
    services::planner::create_dry_run_plan(&state, folder_id)
}

#[tauri::command]
pub fn get_plan_page(
    state: tauri::State<'_, AppState>,
    request: PlanPageRequest,
) -> Result<PlanPageResponse, AppError> {
    services::planner::get_plan_page(&state, request)
}

#[tauri::command]
pub fn discard_plan(state: tauri::State<'_, AppState>, plan_id: String) -> Result<(), AppError> {
    services::planner::discard_plan(&state, &plan_id)
}

#[tauri::command]
pub fn execute_plan(
    state: tauri::State<'_, AppState>,
    request: ExecutePlanRequest,
) -> Result<ExecutionResult, AppError> {
    services::execution::execute_plan(&state, request)
}

#[tauri::command]
pub fn cancel_batch(state: tauri::State<'_, AppState>, batch_id: String) -> Result<bool, AppError> {
    services::execution::cancel_batch(&state, &batch_id)
}

#[tauri::command]
pub fn undo_batch(
    state: tauri::State<'_, AppState>,
    request: UndoRequest,
) -> Result<UndoResult, AppError> {
    services::execution::undo_batch(&state, request)
}

#[tauri::command]
pub fn list_batches(
    state: tauri::State<'_, AppState>,
    request: BatchListRequest,
) -> Result<BatchListResponse, AppError> {
    services::history::list_batches(&state, request)
}

#[tauri::command]
pub fn get_batch_detail(
    state: tauri::State<'_, AppState>,
    batch_id: String,
) -> Result<BatchDetailResponse, AppError> {
    services::history::get_batch_detail(&state, &batch_id)
}

#[tauri::command]
pub fn list_recovery_items(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RecoveryEntry>, AppError> {
    services::history::list_recovery_items(&state)
}

#[tauri::command]
pub fn acknowledge_recovery_item(
    state: tauri::State<'_, AppState>,
    journal_id: i64,
) -> Result<(), AppError> {
    services::history::acknowledge_recovery_item(&state, journal_id)
}

#[tauri::command]
pub fn retry_recovery(state: tauri::State<'_, AppState>) -> Result<RecoverySummary, AppError> {
    services::history::retry_recovery(&state)
}

#[tauri::command]
pub fn reveal_move_log_in_explorer(
    state: tauri::State<'_, AppState>,
    move_log_id: i64,
) -> Result<(), AppError> {
    services::history::reveal_move_log_in_explorer(&state, move_log_id)
}

#[tauri::command]
pub fn open_logs_folder(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    services::open_logs_folder(&state)
}

#[tauri::command]
pub fn get_suggested_downloads_folder(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, AppError> {
    services::get_suggested_downloads_folder(&state)
}

#[tauri::command]
pub fn export_redacted_diagnostics(
    state: tauri::State<'_, AppState>,
) -> Result<RedactedDiagnostics, AppError> {
    services::export_redacted_diagnostics(&state)
}

#[tauri::command]
pub fn search_move_logs(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<MoveLogItem>, AppError> {
    services::history::search_move_logs(&state, &query, limit.unwrap_or(20))
}

#[tauri::command]
pub fn analyze_folder_storage(
    state: tauri::State<'_, AppState>,
    folder_id: i64,
) -> Result<FolderStorageReport, AppError> {
    services::planner::analyze_folder_storage(&state, folder_id)
}
