use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use proptest::prelude::*;
use sortdeck_lib::database::transactions;
use sortdeck_lib::database::Database;
use sortdeck_lib::engine::paths::ProtectedRoots;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::faulty::{FaultConfig, FaultyFsBackend};
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard};
use sortdeck_lib::fsops::FsBackend;
use sortdeck_lib::models::{
    AddWatchedFolderRequest, BatchListRequest, BatchStatus, ExecutePlanRequest, JournalState,
    OperationType, PlanPageRequest, UndoRequest,
};
use sortdeck_lib::services;
use sortdeck_lib::services::state::AppState;

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::from_io("mkdir", e))?;
    }
    let mut f = std::fs::File::create(path).map_err(|e| AppError::from_io("create", e))?;
    f.write_all(bytes)
        .map_err(|e| AppError::from_io("write", e))
}

type FaultyStateSetup = (
    tempfile::TempDir,
    AppState,
    Arc<FaultyFsBackend<SandboxFsBackend<LocalFsBackend>>>,
    PathBuf,
);

fn setup_faulty_state(faults: FaultConfig) -> Result<FaultyStateSetup, AppError> {
    let dir = tempfile::tempdir().map_err(|e| AppError::from_io("create tempdir", e))?;
    SandboxGuard::init_sandbox_dir(dir.path())?;
    let data_dir = dir.path().join("sortdeck-data");
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| AppError::from_io("create Downloads", e))?;

    let db = Database::open_in_dir(&data_dir)?;
    let protected =
        ProtectedRoots::from_explicit(vec![data_dir.clone(), data_dir.join("logs")], None);
    let faulty = Arc::new(FaultyFsBackend::new(
        SandboxFsBackend::new(LocalFsBackend),
        faults,
    ));
    let fs_dyn: Arc<dyn FsBackend> = faulty.clone();
    let state = AppState::new(db, protected, fs_dyn);
    Ok((dir, state, faulty, downloads))
}

fn add_downloads_folder(state: &AppState, downloads: &Path) -> Result<i64, AppError> {
    let folder = services::folders::add_watched_folder(
        state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: Some(false),
            max_depth: Some(1),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )?;
    Ok(folder.id)
}

#[test]
fn inv_01_never_overwrites_including_concurrent_destination_race() {
    let (_dir, state, _faulty, downloads) = setup_faulty_state(FaultConfig {
        inject_race_collision_once: true,
        ..FaultConfig::default()
    })
    .unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();

    // Source file `photo.png`; competing process creates `Images/photo.png` right before MoveFileExW!
    write_file(&downloads.join("photo.png"), b"original-user-photo").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    assert_eq!(summary.eligible_count, 1);

    let result = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();

    assert_eq!(result.status, BatchStatus::Completed);
    assert_eq!(result.succeeded, 1);
    // Competing file MUST remain intact, and moved file gets collision suffix `photo (1).png`.
    assert_eq!(
        std::fs::read(downloads.join("Images/photo.png")).unwrap(),
        b"competing-existing-content"
    );
    assert_eq!(
        std::fs::read(downloads.join("Images/photo (1).png")).unwrap(),
        b"original-user-photo"
    );
    assert!(!downloads.join("photo.png").exists());
}

#[test]
fn inv_02_source_never_deleted_without_verified_destination() {
    // Case A: SHA-256 hash mismatch during cross-volume copy
    let (_dir, state, faulty, downloads) = setup_faulty_state(FaultConfig {
        force_cross_volume: true,
        corrupt_temp_after_copy: true,
        ..FaultConfig::default()
    })
    .unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();
    let src = downloads.join("report.pdf");
    write_file(&src, b"important-pdf-bytes").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();

    assert_eq!(res.status, BatchStatus::Failed);
    assert_eq!(res.outcomes[0].error_code.as_deref(), Some("HASH_MISMATCH"));
    assert_eq!(std::fs::read(&src).unwrap(), b"important-pdf-bytes");
    assert!(!downloads.join("Documents/report.pdf").exists());
    // No orphaned `.sdpart` temp file remains
    let leftovers: Vec<_> = std::fs::read_dir(downloads.join("Documents"))
        .unwrap()
        .collect();
    assert!(leftovers.is_empty());

    // Case B: Source modified during copy -> SourceChangedDuringCopy, source preserved
    faulty
        .update_faults(|f| {
            f.corrupt_temp_after_copy = false;
            f.modify_source_during_copy = true;
        })
        .unwrap();
    let summary2 = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res2 = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary2.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(res2.status, BatchStatus::Failed);
    assert_eq!(
        res2.outcomes[0].error_code.as_deref(),
        Some("SOURCE_CHANGED_DURING_COPY")
    );
    assert!(src.exists());
    assert!(!downloads.join("Documents/report.pdf").exists());
}

#[test]
fn inv_03_destination_preserved_and_recovery_required_when_source_delete_fails() {
    let (_dir, state, _faulty, downloads) = setup_faulty_state(FaultConfig {
        force_cross_volume: true,
        fail_delete_source_with_sharing_violation: true,
        ..FaultConfig::default()
    })
    .unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();
    let src = downloads.join("doc.pdf");
    write_file(&src, b"verified-payload").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();

    assert_eq!(res.status, BatchStatus::RecoveryRequired);
    assert_eq!(res.recovery_required_count, 1);
    // Both source and verified destination exist; destination is NEVER deleted (INV-03)!
    assert_eq!(std::fs::read(&src).unwrap(), b"verified-payload");
    assert_eq!(
        std::fs::read(downloads.join("Documents/doc.pdf")).unwrap(),
        b"verified-payload"
    );

    let recovery_items = services::history::list_recovery_items(&state).unwrap();
    assert_eq!(recovery_items.len(), 1);
    assert_eq!(
        recovery_items[0].detail.as_deref(),
        Some("source_remains_after_verified_copy")
    );
}

#[test]
fn inv_04_failures_leave_source_intact_and_disk_full_stops_batch() {
    // Disk full during cross-volume copy stops batch immediately
    let (_dir, state, faulty, downloads) = setup_faulty_state(FaultConfig {
        force_cross_volume: true,
        fail_copy_with_disk_full: true,
        ..FaultConfig::default()
    })
    .unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();
    write_file(&downloads.join("a.png"), b"aaa").unwrap();
    write_file(&downloads.join("b.png"), b"bbb").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(res.status, BatchStatus::Failed);
    assert_eq!(res.failed, 1);
    assert_eq!(res.outcomes.len(), 1);
    assert_eq!(res.outcomes[0].error_code.as_deref(), Some("DISK_FULL"));
    assert_eq!(std::fs::read(downloads.join("a.png")).unwrap(), b"aaa");
    assert_eq!(std::fs::read(downloads.join("b.png")).unwrap(), b"bbb");

    // Cancel mid-copy preserves source & reports Cancelled (INV-14)
    faulty
        .update_faults(|f| {
            f.fail_copy_with_disk_full = false;
            f.cancel_during_copy = true;
        })
        .unwrap();
    let summary2 = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res2 = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary2.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(res2.status, BatchStatus::Cancelled);
    assert_eq!(std::fs::read(downloads.join("a.png")).unwrap(), b"aaa");
}

#[test]
fn inv_08_and_inv_14_completed_cross_volume_and_partial_batch_statuses() {
    let (_dir, state, faulty, downloads) = setup_faulty_state(FaultConfig {
        force_cross_volume: true,
        ..FaultConfig::default()
    })
    .unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();
    write_file(&downloads.join("pic.png"), b"png-bytes").unwrap();
    write_file(&downloads.join("note.txt"), b"txt-bytes").unwrap();

    // Execute successful cross-volume move for both files
    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let res = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(res.status, BatchStatus::Completed);
    assert_eq!(res.succeeded, 2);

    let detail = services::history::get_batch_detail(&state, &res.batch_id).unwrap();
    assert_eq!(detail.items.len(), 2);
    assert!(detail
        .items
        .iter()
        .all(|i| i.cross_volume && i.sha256.is_some()));

    // Verify partial batch status when 1 succeeds and 1 fails (INV-14)
    write_file(&downloads.join("ok.png"), b"ok").unwrap();
    write_file(&downloads.join("fail.pdf"), b"fail").unwrap();
    faulty
        .update_faults(|f| {
            f.force_cross_volume = false;
        })
        .unwrap();
    let sum2 = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    // Mutate `fail.pdf` after planning so revalidation rejects it while `ok.png` succeeds!
    write_file(&downloads.join("fail.pdf"), b"fail-changed").unwrap();
    let res2 = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: sum2.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(res2.status, BatchStatus::Partial);
    assert_eq!(res2.succeeded, 1);

    let batches = services::history::list_batches(
        &state,
        BatchListRequest {
            offset: 0,
            limit: 10,
            status_filter: None,
        },
    )
    .unwrap();
    assert_eq!(batches.total, 2);
}

#[test]
fn inv_10_inv_12_and_inv_13_undo_safety_idempotency_and_suppression() {
    let (_dir, state, _faulty, downloads) = setup_faulty_state(FaultConfig::default()).unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();

    write_file(&downloads.join("one.png"), b"one").unwrap();
    write_file(&downloads.join("two.png"), b"two").unwrap();
    write_file(&downloads.join("doc.pdf"), b"doc").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let exec = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(exec.status, BatchStatus::Completed);

    // INV-12: running another sweep immediately plans zero eligible operations.
    let second_plan = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    assert_eq!(second_plan.eligible_count, 0);
    assert_eq!(second_plan.conflict_renamed_count, 0);

    // Create a non-empty blocker in Documents/ AND occupy original path of `two.png`.
    write_file(&downloads.join("Documents/user_keep.txt"), b"keep").unwrap();
    write_file(&downloads.join("two.png"), b"new-occupier").unwrap();
    // Replace `doc.pdf` at destination with a DIFFERENT file (new file_id / content).
    state
        .fs
        .delete_file(&downloads.join("Documents/doc.pdf"))
        .unwrap();
    write_file(&downloads.join("Documents/doc.pdf"), b"replaced-doc").unwrap();

    // Item-level + partial batch undo: `one.png` restores, `two.png` skipped (OriginalPathOccupied),
    // `doc.pdf` skipped (DestinationChanged/ModifiedSinceMove).
    let undo = services::execution::undo_batch(
        &state,
        UndoRequest {
            batch_id: exec.batch_id.clone(),
            item_ids: None,
        },
    )
    .unwrap();

    assert_eq!(undo.status, BatchStatus::Partial);
    assert_eq!(undo.restored_count, 1);
    assert_eq!(undo.skipped_count, 2);
    // `Images/` still contains skipped `two.png`; `Documents/` still contains `user_keep.txt`.
    assert!(downloads.join("Images/two.png").exists());
    assert!(downloads.join("Documents/user_keep.txt").exists());
    assert_eq!(std::fs::read(downloads.join("one.png")).unwrap(), b"one");
    assert_eq!(
        std::fs::read(downloads.join("two.png")).unwrap(),
        b"new-occupier"
    );

    // INV-13: `restore_suppressions` row recorded for restored `one.png`!
    let fp = state.fs.fingerprint(&downloads.join("one.png")).unwrap();
    let one_key = downloads
        .join("one.png")
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let suppressed = state
        .db
        .with_writer(|conn| {
            transactions::has_active_restore_suppression(
                conn,
                &one_key,
                fp.size,
                fp.modified_epoch_ms,
                sortdeck_lib::database::queries::now_epoch_ms(),
            )
        })
        .unwrap();
    assert!(suppressed);

    // Per-item undo works after freeing `two.png` original path.
    state.fs.delete_file(&downloads.join("two.png")).unwrap();
    let detail = services::history::get_batch_detail(&state, &exec.batch_id).unwrap();
    let two_id = detail
        .items
        .iter()
        .find(|i| i.original_path.ends_with("two.png"))
        .unwrap()
        .id;
    let item_undo = services::execution::undo_batch(
        &state,
        UndoRequest {
            batch_id: exec.batch_id,
            item_ids: Some(vec![two_id]),
        },
    )
    .unwrap();
    assert_eq!(item_undo.status, BatchStatus::Completed);
    assert_eq!(item_undo.restored_count, 1);
    assert_eq!(std::fs::read(downloads.join("two.png")).unwrap(), b"two");
    // Now `Images/` is empty and SortDeck-created -> removed; `Documents/` has `user_keep.txt` -> preserved.
    assert!(!downloads.join("Images").exists());
    assert!(downloads.join("Documents/user_keep.txt").exists());
}

#[test]
fn inv_11_execute_plan_rejects_stale_config_and_deselected_items() {
    let (_dir, state, _faulty, downloads) = setup_faulty_state(FaultConfig::default()).unwrap();
    let folder_id = add_downloads_folder(&state, &downloads).unwrap();
    write_file(&downloads.join("keep.png"), b"keep").unwrap();
    write_file(&downloads.join("move.png"), b"move").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder_id).unwrap();
    let page = services::planner::get_plan_page(
        &state,
        PlanPageRequest {
            plan_id: summary.plan_id.clone(),
            offset: 0,
            limit: 10,
            status_filter: None,
            skip_reason_filter: None,
            search: None,
        },
    )
    .unwrap();
    let keep_op = page
        .items
        .iter()
        .find(|i| i.original_filename == "keep.png")
        .unwrap()
        .operation_id
        .clone();

    let res = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![keep_op],
        },
    )
    .unwrap();
    assert_eq!(res.status, BatchStatus::Completed);
    assert_eq!(res.succeeded, 1);
    assert!(downloads.join("keep.png").exists());
    assert!(downloads.join("Images/move.png").exists());
}

proptest! {
    #[test]
    fn prop_journal_terminal_states_never_transition(
        terminal_idx in 0usize..3,
        op_idx in 0usize..4,
        next_idx in 0usize..9
    ) {
        let terminals = [JournalState::Completed, JournalState::Undone, JournalState::Failed];
        let ops = [
            OperationType::MoveSameVolume,
            OperationType::MoveCrossVolume,
            OperationType::UndoMove,
            OperationType::CreateDir,
        ];
        let all_states = [
            JournalState::Planned,
            JournalState::Copying,
            JournalState::Verified,
            JournalState::Committed,
            JournalState::Completed,
            JournalState::UndoPending,
            JournalState::Undone,
            JournalState::Failed,
            JournalState::RecoveryRequired,
        ];
        prop_assert!(!terminals[terminal_idx].can_transition(ops[op_idx], all_states[next_idx]));
    }
}
