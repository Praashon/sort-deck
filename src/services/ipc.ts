import { invoke } from '@tauri-apps/api/core';
import type {
  AddWatchedFolderRequest,
  AppInfo,
  AppSettings,
  BatchDetailResponse,
  BatchListRequest,
  BatchListResponse,
  CreateRuleRequest,
  DatabaseStatusInfo,
  ExecutePlanRequest,
  ExecutionResult,
  FolderStorageReport,
  MoveLogItem,
  PlanPageRequest,
  PlanPageResponse,
  PlanSummary,
  RecoveryEntry,
  RecoverySummary,
  RedactedDiagnostics,
  ReorderRulesRequest,
  Rule,
  RulesListResponse,
  UndoRequest,
  UndoResult,
  UpdateRuleRequest,
  UpdateWatchedFolderRequest,
  WatchedFolder,
} from '../types/generated';

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>('get_app_info');
}

export async function getDatabaseStatus(): Promise<DatabaseStatusInfo> {
  return invoke<DatabaseStatusInfo>('get_database_status');
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>('get_settings');
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>('update_settings', { settings });
}

export async function listWatchedFolders(): Promise<WatchedFolder[]> {
  return invoke<WatchedFolder[]>('list_watched_folders');
}

export async function addWatchedFolder(request: AddWatchedFolderRequest): Promise<WatchedFolder> {
  return invoke<WatchedFolder>('add_watched_folder', { request });
}

export async function updateWatchedFolder(
  request: UpdateWatchedFolderRequest,
): Promise<WatchedFolder> {
  return invoke<WatchedFolder>('update_watched_folder', { request });
}

export async function removeWatchedFolder(id: number): Promise<void> {
  return invoke<void>('remove_watched_folder', { id });
}

export async function listRules(): Promise<RulesListResponse> {
  return invoke<RulesListResponse>('list_rules');
}

export async function createRule(request: CreateRuleRequest): Promise<Rule> {
  return invoke<Rule>('create_rule', { request });
}

export async function updateRule(request: UpdateRuleRequest): Promise<Rule> {
  return invoke<Rule>('update_rule', { request });
}

export async function deleteRule(id: number): Promise<void> {
  return invoke<void>('delete_rule', { id });
}

export async function reorderRules(request: ReorderRulesRequest): Promise<RulesListResponse> {
  return invoke<RulesListResponse>('reorder_rules', { request });
}

export async function restoreDefaultRules(): Promise<RulesListResponse> {
  return invoke<RulesListResponse>('restore_default_rules');
}

export async function createDryRunPlan(folderId: number): Promise<PlanSummary> {
  return invoke<PlanSummary>('create_dry_run_plan', { folderId });
}

export async function getPlanPage(request: PlanPageRequest): Promise<PlanPageResponse> {
  return invoke<PlanPageResponse>('get_plan_page', { request });
}

export async function discardPlan(planId: string): Promise<void> {
  return invoke<void>('discard_plan', { planId });
}

export async function executePlan(request: ExecutePlanRequest): Promise<ExecutionResult> {
  return invoke<ExecutionResult>('execute_plan', { request });
}

export async function cancelBatch(batchId: string): Promise<boolean> {
  return invoke<boolean>('cancel_batch', { batchId });
}

export async function undoBatch(request: UndoRequest): Promise<UndoResult> {
  return invoke<UndoResult>('undo_batch', { request });
}

export async function listBatches(request: BatchListRequest): Promise<BatchListResponse> {
  return invoke<BatchListResponse>('list_batches', { request });
}

export async function getBatchDetail(batchId: string): Promise<BatchDetailResponse> {
  return invoke<BatchDetailResponse>('get_batch_detail', { batchId });
}

export async function listRecoveryItems(): Promise<RecoveryEntry[]> {
  return invoke<RecoveryEntry[]>('list_recovery_items');
}

export async function acknowledgeRecoveryItem(journalId: number): Promise<void> {
  return invoke<void>('acknowledge_recovery_item', { journalId });
}

export async function retryRecovery(): Promise<RecoverySummary> {
  return invoke<RecoverySummary>('retry_recovery');
}

export async function revealMoveLogInExplorer(moveLogId: number): Promise<void> {
  return invoke<void>('reveal_move_log_in_explorer', { moveLogId });
}

export async function openLogsFolder(): Promise<void> {
  return invoke<void>('open_logs_folder');
}

export async function getSuggestedDownloadsFolder(): Promise<string | null> {
  return invoke<string | null>('get_suggested_downloads_folder');
}

export async function exportRedactedDiagnostics(): Promise<RedactedDiagnostics> {
  return invoke<RedactedDiagnostics>('export_redacted_diagnostics');
}

export async function searchMoveLogs(query: string, limit = 20): Promise<MoveLogItem[]> {
  return invoke<MoveLogItem[]>('search_move_logs', { query, limit });
}

export async function analyzeFolderStorage(folderId: number): Promise<FolderStorageReport> {
  return invoke<FolderStorageReport>('analyze_folder_storage', { folderId });
}
