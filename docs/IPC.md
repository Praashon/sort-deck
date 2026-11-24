# SortDeck IPC Contract

## Type Generation

Rust IPC payload structs derive `ts_rs::TS` and export deterministically into `src/types/generated/` via:

```powershell
npm run gen:types
```

Hand-editing files in `src/types/generated/` is forbidden; CI verifies `git diff --exit-code src/types/generated`.

## Commands

| Command                          | Arguments                             | Returns               | Notes                                                                                            |
| -------------------------------- | ------------------------------------- | --------------------- | ------------------------------------------------------------------------------------------------ |
| `get_app_info`                   | None                                  | `AppInfo`             | App version, identifier, schema version, and `SORTDECK_DATA_DIR` override flag.                  |
| `get_database_status`            | None                                  | `DatabaseStatusInfo`  | Current schema version, `configRevision`, and startup corruption recovery status.                |
| `get_settings`                   | None                                  | `AppSettings`         | Returns persisted v1 settings.                                                                   |
| `update_settings`                | `settings: AppSettings`               | `AppSettings`         | Validates scan depth (`1..=32`) and retention (`1..=36500` or `null`).                           |
| `list_watched_folders`           | None                                  | `WatchedFolder[]`     | Lists all watched folders ordered by ID.                                                         |
| `add_watched_folder`             | `request: AddWatchedFolderRequest`    | `WatchedFolder`       | Validates path policy, reparse points, project roots, and overlap; defaults `autoSort = false`.  |
| `update_watched_folder`          | `request: UpdateWatchedFolderRequest` | `WatchedFolder`       | Updates folder flags/limits and records `autoSortEnabledAt` when enabled.                        |
| `remove_watched_folder`          | `id: number`                          | `void`                | Deletes folder config without touching disk files.                                               |
| `list_rules`                     | None                                  | `RulesListResponse`   | Rules ordered by `(priority ASC, id ASC)` plus `overlaps` warnings.                              |
| `create_rule`                    | `request: CreateRuleRequest`          | `Rule`                | Validates name, normalized extensions, and destination.                                          |
| `update_rule`                    | `request: UpdateRuleRequest`          | `Rule`                | Validates and updates rule and extensions.                                                       |
| `delete_rule`                    | `id: number`                          | `void`                | Deletes rule and re-densifies priorities `1..=N`.                                                |
| `reorder_rules`                  | `request: ReorderRulesRequest`        | `RulesListResponse`   | Expects exact permutation of all existing rule IDs; assigns priorities `1..=N`.                  |
| `restore_default_rules`          | None                                  | `RulesListResponse`   | Re-creates/resets v2 default rules without deleting custom rules (`defaults_version = 2`).       |
| `create_dry_run_plan`            | `folderId: number`                    | `PlanSummary`         | Scans folder read-only, reserves collision names, and stores plan in backend memory.             |
| `get_plan_page`                  | `request: PlanPageRequest`            | `PlanPageResponse`    | Returns paginated, filtered operations for a stored plan (`limit <= 500`).                       |
| `discard_plan`                   | `planId: string`                      | `void`                | Evicts a stored dry-run plan from backend memory.                                                |
| `execute_plan`                   | `request: ExecutePlanRequest`         | `ExecutionResult`     | Revalidates and executes selected plan operations with write-ahead journaling.                   |
| `cancel_batch`                   | `batchId: string`                     | `boolean`             | Signals cooperative cancellation for an active batch.                                            |
| `undo_batch`                     | `request: UndoRequest`                | `UndoResult`          | Restores a batch or selected items in reverse order with identity checks and suppressions.       |
| `list_batches`                   | `request: BatchListRequest`           | `BatchListResponse`   | Paginated history of batches ordered newest-first.                                               |
| `get_batch_detail`               | `batchId: string`                     | `BatchDetailResponse` | Returns batch summary and ordered move log entries.                                              |
| `search_move_logs`               | `query: string, limit?: number`       | `MoveLogItem[]`       | Case-insensitive substring search across `originalPath` and `newPath` in move history.           |
| `analyze_folder_storage`         | `folderId: number`                    | `FolderStorageReport` | Read-only folder scan reporting space by category, 90+ day files, largest files, and duplicates. |
| `list_recovery_items`            | None                                  | `RecoveryEntry[]`     | Lists journal rows awaiting resolution in the Recovery Center.                                   |
| `acknowledge_recovery_item`      | `journalId: number`                   | `void`                | Marks a `recovery_required` row acknowledged without touching user files.                        |
| `retry_recovery`                 | None                                  | `RecoverySummary`     | Re-runs recovery verification across open/recovery-required journal rows.                        |
| `reveal_move_log_in_explorer`    | `moveLogId: number`                   | `void`                | Selects the validated move-log target file in Windows Explorer via `SHOpenFolderAndSelectItems`. |
| `open_logs_folder`               | None                                  | `void`                | Opens SortDeck's local `logs/` directory in Windows Explorer.                                    |
| `get_suggested_downloads_folder` | None                                  | `string \| null`      | Returns `%USERPROFILE%\Downloads` if present, non-protected, and not yet watched.                |
| `export_redacted_diagnostics`    | None                                  | `RedactedDiagnostics` | Returns redacted app/schema/config counts with zero user paths or filenames.                     |

## Error Payload (`AppErrorPayload`)

Errors returned over IPC serialize to `{ code, messageKey, details? }`.

| Code                         | `messageKey`                        | Meaning                                                           |
| ---------------------------- | ----------------------------------- | ----------------------------------------------------------------- |
| `INVALID_INPUT`              | `errors.INVALID_INPUT`              | Invalid command or configuration input                            |
| `PATH_VALIDATION`            | `errors.PATH_VALIDATION`            | Path failed safety or format validation                           |
| `SANDBOX_VIOLATION`          | `errors.SANDBOX_VIOLATION`          | Test operation attempted outside `.sortdeck-test-sandbox`         |
| `NOT_FOUND`                  | `errors.NOT_FOUND`                  | Requested resource missing                                        |
| `PERMISSION_DENIED`          | `errors.PERMISSION_DENIED`          | OS denied permission                                              |
| `ALREADY_EXISTS`             | `errors.ALREADY_EXISTS`             | Target already exists                                             |
| `CONFLICT`                   | `errors.CONFLICT`                   | Overlapping watched folder or conflicting destination             |
| `DATABASE_DOWNGRADE`         | `errors.DATABASE_DOWNGRADE`         | Database schema is newer than supported by this app version       |
| `DATABASE_ERROR`             | `errors.DATABASE_ERROR`             | Local SQLite error                                                |
| `PLAN_EXPIRED`               | `errors.PLAN_EXPIRED`               | Dry-run plan exceeded its TTL and must be regenerated             |
| `PLAN_INVALIDATED`           | `errors.PLAN_INVALIDATED`           | Rules, folder settings, or config revision changed after creation |
| `CANCELLED`                  | `errors.CANCELLED`                  | Operation cancelled via cancellation token                        |
| `FILE_IN_USE`                | `errors.FILE_IN_USE`                | File is open or locked (`ERROR_SHARING_VIOLATION` / `LOCK`)       |
| `DISK_FULL`                  | `errors.DISK_FULL`                  | Destination volume out of space (`ERROR_DISK_FULL`)               |
| `CLOUD_FILE_ERROR`           | `errors.CLOUD_FILE_ERROR`           | Cloud provider file error (`ERROR_CANT_ACCESS_FILE`)              |
| `NETWORK_UNAVAILABLE`        | `errors.NETWORK_UNAVAILABLE`        | Network share/volume unreachable                                  |
| `HASH_MISMATCH`              | `errors.HASH_MISMATCH`              | Cross-volume SHA-256 verification failed; source preserved        |
| `SOURCE_CHANGED_DURING_COPY` | `errors.SOURCE_CHANGED_DURING_COPY` | Source modified during copy; temp removed, source kept            |
| `RECOVERY_REQUIRED`          | `errors.RECOVERY_REQUIRED`          | Operation requires user review in the Recovery Center             |
| `IO_ERROR`                   | `errors.IO_ERROR`                   | Filesystem or I/O failure                                         |
| `INTERNAL_ERROR`             | `errors.INTERNAL_ERROR`             | Unexpected internal failure                                       |

## `SkipReason` Codes

`NoMatchingRule`, `RuleDisabled`, `ProtectedLocation`, `ExcludedDirectory`, `ProjectRoot`, `SortIgnore`, `HiddenAttribute`, `SystemAttribute`, `ReparsePoint`, `CloudPlaceholder`, `CloudManagedCrossVolume`, `TemporaryExtension`, `SpecialFile`, `OfficeLockFile`, `FileInUse`, `UnstableFile`, `TooRecent`, `AlreadyInDestination`, `DestinationInvalid`, `DestinationUnavailable`, `PathTooLong`, `InvalidName`, `NonUnicodePath`, `ReadOnlyCrossVolume`, `InsufficientSpace`, `FilesystemLimit`, `SourceVanished`, `SourceChanged`, `PermissionDenied`, `RestoredByUndo`, `DepthLimit`, `ScanLimit`.
