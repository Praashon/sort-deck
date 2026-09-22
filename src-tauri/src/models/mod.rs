#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Health status of a watched folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename_all = "lowercase")]
pub enum FolderStatus {
    Ok,
    Unavailable,
    Ignored,
    Invalid,
}

impl FolderStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
            Self::Ignored => "ignored",
            Self::Invalid => "invalid",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "ok" => Some(Self::Ok),
            "unavailable" => Some(Self::Unavailable),
            "ignored" => Some(Self::Ignored),
            "invalid" => Some(Self::Invalid),
            _ => None,
        }
    }
}

/// Persisted watched folder configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct WatchedFolder {
    #[ts(type = "number")]
    pub id: i64,
    pub path: String,
    pub path_key: String,
    pub enabled: bool,
    pub auto_sort: bool,
    pub recursive: bool,
    pub max_depth: u32,
    pub min_file_age_secs: u32,
    pub status: FolderStatus,
    #[ts(type = "number | null")]
    pub auto_sort_enabled_at: Option<i64>,
    #[ts(type = "number | null")]
    pub last_reconciled_at: Option<i64>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// Request payload for registering a new watched folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct AddWatchedFolderRequest {
    pub path: String,
    pub recursive: Option<bool>,
    pub max_depth: Option<u32>,
    pub min_file_age_secs: Option<u32>,
    pub allow_network_root: Option<bool>,
}

/// Request payload for updating an existing watched folder's preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct UpdateWatchedFolderRequest {
    #[ts(type = "number")]
    pub id: i64,
    pub enabled: bool,
    pub auto_sort: bool,
    pub recursive: bool,
    pub max_depth: u32,
    pub min_file_age_secs: u32,
}

/// Persisted sorting rule with normalized extension list and optional smart filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Rule {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub destination: String,
    pub extensions: Vec<String>,
    pub enabled: bool,
    pub priority: u32,
    pub is_default: bool,
    pub default_key: Option<String>,
    pub name_contains: Option<String>,
    #[ts(type = "number | null")]
    pub min_size_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub max_size_bytes: Option<u64>,
    pub min_age_days: Option<u32>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// Request payload for creating a custom sorting rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct CreateRuleRequest {
    pub name: String,
    pub destination: String,
    pub extensions: Vec<String>,
    pub enabled: bool,
    #[serde(default)]
    pub name_contains: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub min_size_bytes: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_size_bytes: Option<u64>,
    #[serde(default)]
    pub min_age_days: Option<u32>,
}

/// Request payload for updating an existing sorting rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct UpdateRuleRequest {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub destination: String,
    pub extensions: Vec<String>,
    pub enabled: bool,
    #[serde(default)]
    pub name_contains: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub min_size_bytes: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_size_bytes: Option<u64>,
    #[serde(default)]
    pub min_age_days: Option<u32>,
}

/// Request payload for backend-owned dense priority reordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct ReorderRulesRequest {
    #[ts(type = "number[]")]
    pub ordered_rule_ids: Vec<i64>,
}

/// Non-blocking warning indicating multiple enabled rules claim the same extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ExtensionOverlapWarning {
    pub extension: String,
    #[ts(type = "number")]
    pub winner_rule_id: i64,
    pub winner_rule_name: String,
    #[ts(type = "number[]")]
    pub shadowed_rule_ids: Vec<i64>,
}

/// Combined rules listing and overlap warnings returned to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RulesListResponse {
    pub rules: Vec<Rule>,
    pub overlaps: Vec<ExtensionOverlapWarning>,
}

/// Diagnostic summary of the local SQLite database state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DatabaseStatusInfo {
    pub schema_version: u32,
    #[ts(type = "number")]
    pub config_revision: u64,
    pub db_path: String,
    #[ts(type = "number")]
    pub db_size_bytes: u64,
    pub recovered_from_corruption: bool,
    pub corrupt_backup_path: Option<String>,
}

/// Typed skip reasons emitted by the scanner, planner, and revalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum SkipReason {
    NoMatchingRule,
    RuleDisabled,
    ProtectedLocation,
    ExcludedDirectory,
    ProjectRoot,
    SortIgnore,
    HiddenAttribute,
    SystemAttribute,
    ReparsePoint,
    CloudPlaceholder,
    CloudManagedCrossVolume,
    TemporaryExtension,
    SpecialFile,
    OfficeLockFile,
    FileInUse,
    UnstableFile,
    TooRecent,
    AlreadyInDestination,
    DestinationInvalid,
    DestinationUnavailable,
    PathTooLong,
    InvalidName,
    NonUnicodePath,
    ReadOnlyCrossVolume,
    InsufficientSpace,
    FilesystemLimit,
    SourceVanished,
    SourceChanged,
    PermissionDenied,
    RestoredByUndo,
    DepthLimit,
    ScanLimit,
}

/// Complete list of all 32 [`SkipReason`] variants for exhaustive validation tests.
pub const ALL_SKIP_REASONS: [SkipReason; 32] = [
    SkipReason::NoMatchingRule,
    SkipReason::RuleDisabled,
    SkipReason::ProtectedLocation,
    SkipReason::ExcludedDirectory,
    SkipReason::ProjectRoot,
    SkipReason::SortIgnore,
    SkipReason::HiddenAttribute,
    SkipReason::SystemAttribute,
    SkipReason::ReparsePoint,
    SkipReason::CloudPlaceholder,
    SkipReason::CloudManagedCrossVolume,
    SkipReason::TemporaryExtension,
    SkipReason::SpecialFile,
    SkipReason::OfficeLockFile,
    SkipReason::FileInUse,
    SkipReason::UnstableFile,
    SkipReason::TooRecent,
    SkipReason::AlreadyInDestination,
    SkipReason::DestinationInvalid,
    SkipReason::DestinationUnavailable,
    SkipReason::PathTooLong,
    SkipReason::InvalidName,
    SkipReason::NonUnicodePath,
    SkipReason::ReadOnlyCrossVolume,
    SkipReason::InsufficientSpace,
    SkipReason::FilesystemLimit,
    SkipReason::SourceVanished,
    SkipReason::SourceChanged,
    SkipReason::PermissionDenied,
    SkipReason::RestoredByUndo,
    SkipReason::DepthLimit,
    SkipReason::ScanLimit,
];

/// Status of an operation inside a dry-run plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum OperationStatus {
    Eligible,
    Skipped,
    ConflictRenamed,
    Warning,
}

/// Source identity snapshot carried inside a dry-run plan for execution revalidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SourceFingerprintDto {
    #[ts(type = "number | null")]
    pub volume_serial: Option<u64>,
    pub file_id: Option<String>,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number | null")]
    pub modified_epoch_ms: Option<i64>,
}

/// Single proposed operation or skipped file inside a backend-owned dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PlannedOperation {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: Option<String>,
    pub original_filename: String,
    pub proposed_filename: Option<String>,
    #[ts(type = "number")]
    pub file_size: u64,
    pub source_fingerprint: SourceFingerprintDto,
    #[ts(type = "number | null")]
    pub matched_rule_id: Option<i64>,
    pub matched_rule_name: Option<String>,
    pub operation_status: OperationStatus,
    pub skip_reason: Option<SkipReason>,
    pub cross_volume_estimate: bool,
    pub warnings: Vec<String>,
}

/// Count of skipped files grouped by [`SkipReason`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SkipReasonCount {
    pub reason: SkipReason,
    #[ts(type = "number")]
    pub count: u64,
}

/// Summary of proposed moves grouped by destination directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DestinationGroupSummary {
    pub destination_dir: String,
    #[ts(type = "number")]
    pub file_count: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
}

/// High-level summary of a backend-owned dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PlanSummary {
    pub plan_id: String,
    #[ts(type = "number")]
    pub folder_id: i64,
    pub folder_path: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub expires_at: i64,
    #[ts(type = "number")]
    pub config_revision: u64,
    #[ts(type = "number")]
    pub total_discovered: u64,
    #[ts(type = "number")]
    pub eligible_count: u64,
    #[ts(type = "number")]
    pub conflict_renamed_count: u64,
    #[ts(type = "number")]
    pub skipped_count: u64,
    #[ts(type = "number")]
    pub total_bytes_to_move: u64,
    #[ts(type = "number")]
    pub cross_volume_count: u64,
    pub skip_reason_counts: Vec<SkipReasonCount>,
    pub destination_groups: Vec<DestinationGroupSummary>,
    pub warnings: Vec<String>,
    pub sortignore_detected: bool,
}

/// Paginated query parameters for inspecting operations inside a dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct PlanPageRequest {
    pub plan_id: String,
    pub offset: u32,
    pub limit: u32,
    pub status_filter: Option<OperationStatus>,
    pub skip_reason_filter: Option<SkipReason>,
    pub search: Option<String>,
}

/// Paginated slice of [`PlannedOperation`] rows from a stored dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PlanPageResponse {
    pub plan_id: String,
    #[ts(type = "number")]
    pub total_matching: u64,
    pub offset: u32,
    pub limit: u32,
    pub items: Vec<PlannedOperation>,
}

/// Per-operation rejection recorded during plan execution revalidation (INV-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct OperationRevalidationFailure {
    pub operation_id: String,
    pub reason: SkipReason,
    pub detail: String,
}

/// Outcome of revalidating a stored dry-run plan before execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PlanRevalidationReport {
    pub plan_id: String,
    pub valid_operation_ids: Vec<String>,
    pub rejected: Vec<OperationRevalidationFailure>,
}

/// Kind of execution batch recorded in `batches`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename_all = "lowercase")]
pub enum BatchKind {
    Manual,
    Auto,
    Undo,
    Recovery,
}

impl BatchKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
            Self::Undo => "undo",
            Self::Recovery => "recovery",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "manual" => Some(Self::Manual),
            "auto" => Some(Self::Auto),
            "undo" => Some(Self::Undo),
            "recovery" => Some(Self::Recovery),
            _ => None,
        }
    }
}

/// Batch completion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum BatchStatus {
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
    RecoveryRequired,
}

impl BatchStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::RecoveryRequired => "recovery_required",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "recovery_required" => Some(Self::RecoveryRequired),
            _ => None,
        }
    }
}

/// Journal operation classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum OperationType {
    MoveSameVolume,
    MoveCrossVolume,
    UndoMove,
    CreateDir,
}

impl OperationType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MoveSameVolume => "move_same_volume",
            Self::MoveCrossVolume => "move_cross_volume",
            Self::UndoMove => "undo_move",
            Self::CreateDir => "create_dir",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "move_same_volume" => Some(Self::MoveSameVolume),
            "move_cross_volume" => Some(Self::MoveCrossVolume),
            "undo_move" => Some(Self::UndoMove),
            "create_dir" => Some(Self::CreateDir),
            _ => None,
        }
    }
}

/// Write-ahead operation journal state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum JournalState {
    Planned,
    Copying,
    Verified,
    Committed,
    Completed,
    UndoPending,
    Undone,
    Failed,
    RecoveryRequired,
}

impl JournalState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Copying => "copying",
            Self::Verified => "verified",
            Self::Committed => "committed",
            Self::Completed => "completed",
            Self::UndoPending => "undo_pending",
            Self::Undone => "undone",
            Self::Failed => "failed",
            Self::RecoveryRequired => "recovery_required",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "planned" => Some(Self::Planned),
            "copying" => Some(Self::Copying),
            "verified" => Some(Self::Verified),
            "committed" => Some(Self::Committed),
            "completed" => Some(Self::Completed),
            "undo_pending" => Some(Self::UndoPending),
            "undone" => Some(Self::Undone),
            "failed" => Some(Self::Failed),
            "recovery_required" => Some(Self::RecoveryRequired),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Undone | Self::Failed)
    }

    /// Validates allowed state transitionsÂ§5.4.
    #[must_use]
    pub const fn can_transition(self, op_type: OperationType, next: Self) -> bool {
        if self.is_terminal() {
            return false;
        }
        matches!(
            (self, op_type, next),
            (
                Self::Planned,
                OperationType::MoveSameVolume,
                Self::Completed | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::Planned,
                OperationType::MoveCrossVolume,
                Self::Copying | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::Copying,
                OperationType::MoveCrossVolume,
                Self::Verified | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::Verified,
                OperationType::MoveCrossVolume,
                Self::Committed | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::Committed,
                OperationType::MoveCrossVolume,
                Self::Completed | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::UndoPending,
                OperationType::UndoMove,
                Self::Undone | Self::Failed | Self::RecoveryRequired,
            ) | (
                Self::RecoveryRequired,
                _,
                Self::Completed | Self::Undone | Self::Failed | Self::RecoveryRequired,
            )
        )
    }
}

/// Request payload to execute a validated backend-owned dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct ExecutePlanRequest {
    pub plan_id: String,
    pub deselected_operation_ids: Vec<String>,
}

/// Per-operation outcome reported by plan execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct OperationExecutionOutcome {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: Option<String>,
    pub success: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub recovery_required: bool,
}

/// Aggregate result of executing a dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub batch_id: String,
    pub status: BatchStatus,
    #[ts(type = "number")]
    pub total_planned: u64,
    #[ts(type = "number")]
    pub succeeded: u64,
    #[ts(type = "number")]
    pub failed: u64,
    #[ts(type = "number")]
    pub skipped: u64,
    #[ts(type = "number")]
    pub recovery_required_count: u64,
    pub outcomes: Vec<OperationExecutionOutcome>,
}

/// Request payload for batch-level or item-level undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct UndoRequest {
    pub batch_id: String,
    #[ts(type = "number[] | null")]
    pub item_ids: Option<Vec<i64>>,
}

/// Per-item outcome returned by undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UndoItemOutcome {
    #[ts(type = "number")]
    pub move_log_id: i64,
    pub original_path: String,
    pub current_path: String,
    pub restored: bool,
    pub skip_reason: Option<String>,
    pub error_message: Option<String>,
}

/// Aggregate result of an undo operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UndoResult {
    pub undo_batch_id: String,
    pub parent_batch_id: String,
    pub status: BatchStatus,
    #[ts(type = "number")]
    pub restored_count: u64,
    #[ts(type = "number")]
    pub skipped_count: u64,
    #[ts(type = "number")]
    pub failed_count: u64,
    #[ts(type = "number")]
    pub removed_empty_dirs: u64,
    pub items: Vec<UndoItemOutcome>,
}

/// Summary row for a batch in History.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: String,
    pub kind: BatchKind,
    pub source_folder: String,
    #[ts(type = "number | null")]
    pub source_folder_id: Option<i64>,
    pub parent_batch_id: Option<String>,
    pub status: BatchStatus,
    #[ts(type = "number")]
    pub total_files: u64,
    #[ts(type = "number")]
    pub successful_moves: u64,
    #[ts(type = "number")]
    pub failed_moves: u64,
    #[ts(type = "number")]
    pub skipped_files: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
}

/// Persisted move log entry exposed in batch history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MoveLogItem {
    #[ts(type = "number")]
    pub id: i64,
    pub batch_id: String,
    pub original_path: String,
    pub new_path: String,
    pub status: String,
    pub cross_volume: bool,
    pub sha256: Option<String>,
    #[ts(type = "number")]
    pub file_size: u64,
    #[ts(type = "number | null")]
    pub rule_id: Option<i64>,
    #[ts(type = "number")]
    pub moved_at: i64,
    pub undone: bool,
    #[ts(type = "number | null")]
    pub undone_at: Option<i64>,
    pub undo_batch_id: Option<String>,
    pub undo_note: Option<String>,
}

/// Detailed view of a batch and its move logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BatchDetailResponse {
    pub batch: BatchSummary,
    pub items: Vec<MoveLogItem>,
}

/// Paginated query parameters for History batches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct BatchListRequest {
    pub offset: u32,
    pub limit: u32,
    pub status_filter: Option<BatchStatus>,
}

/// Paginated batch history response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BatchListResponse {
    #[ts(type = "number")]
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
    pub items: Vec<BatchSummary>,
}

/// Journal entry requiring user review in the Recovery Center.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RecoveryEntry {
    #[ts(type = "number")]
    pub journal_id: i64,
    pub batch_id: String,
    pub operation_type: OperationType,
    pub state: JournalState,
    pub source_path: String,
    pub destination_path: String,
    pub temporary_path: Option<String>,
    pub error_code: Option<i32>,
    pub error_message: Option<String>,
    pub detail: Option<String>,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// Summary returned after startup or manual recovery runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RecoverySummary {
    pub recovery_batch_id: Option<String>,
    #[ts(type = "number")]
    pub inspected_entries: u64,
    #[ts(type = "number")]
    pub finalized_completed: u64,
    #[ts(type = "number")]
    pub finalized_undone: u64,
    #[ts(type = "number")]
    pub marked_failed: u64,
    #[ts(type = "number")]
    pub recovery_required: u64,
    #[ts(type = "number")]
    pub cleaned_temps: u64,
}

/// Redacted local diagnostics snapshot exported from Settings.
/// Deliberately excludes usernames, watched folder paths, and file names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RedactedDiagnostics {
    pub app_version: String,
    pub identifier: String,
    pub os: String,
    pub arch: String,
    pub schema_version: u32,
    #[ts(type = "number")]
    pub config_revision: u64,
    pub recovered_from_corruption: bool,
    pub settings: crate::config::AppSettings,
    #[ts(type = "number")]
    pub watched_folder_count: u64,
    #[ts(type = "number")]
    pub total_rule_count: u64,
    #[ts(type = "number")]
    pub enabled_rule_count: u64,
    #[ts(type = "number")]
    pub batch_count: u64,
    #[ts(type = "number")]
    pub recovery_required_count: u64,
    #[ts(type = "number")]
    pub generated_at: i64,
}

/// File type category total inside a read-only folder storage report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StorageCategoryBreakdown {
    pub name: String,
    #[ts(type = "number")]
    pub file_count: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
}

/// Large file entry surfaced in a folder storage report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct LargeFileEntry {
    pub path: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number | null")]
    pub modified_epoch_ms: Option<i64>,
    pub matched_rule_name: Option<String>,
}

/// Group of identical files verified by SHA-256 hash inside a watched folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub sha256: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number")]
    pub wasted_bytes: u64,
    pub paths: Vec<String>,
}

/// Read-only storage, old-file, and duplicate summary for a watched folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FolderStorageReport {
    #[ts(type = "number")]
    pub folder_id: i64,
    pub folder_path: String,
    #[ts(type = "number")]
    pub total_files: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub old_files_count: u64,
    #[ts(type = "number")]
    pub old_files_bytes: u64,
    pub categories: Vec<StorageCategoryBreakdown>,
    pub largest_files: Vec<LargeFileEntry>,
    pub duplicate_groups: Vec<DuplicateGroup>,
    #[ts(type = "number")]
    pub scanned_at: i64,
}
