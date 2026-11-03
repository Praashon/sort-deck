use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use sortdeck_lib::database::transactions::{self, JournalMetadata};
use sortdeck_lib::database::Database;
use sortdeck_lib::engine::mover::FailpointFn;
use sortdeck_lib::engine::paths::ProtectedRoots;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::faulty::{FaultConfig, FaultyFsBackend};
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard};
use sortdeck_lib::fsops::{CancelToken, FsBackend};
use sortdeck_lib::models::{
    AddWatchedFolderRequest, BatchKind, ExecutePlanRequest, JournalState, OperationType,
    SourceFingerprintDto, UndoRequest,
};
use sortdeck_lib::recovery;
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

fn spawn_crash_child(sandbox_dir: &Path, scenario: &str) -> Result<Option<i32>, AppError> {
    let exe = std::env::current_exe().map_err(|e| AppError::from_io("current_exe", e))?;
    let output = Command::new(exe)
        .args(["--exact", "crash_child_runner", "--ignored", "--nocapture"])
        .env("SORTDECK_CRASH_DIR", sandbox_dir)
        .env("SORTDECK_CRASH_SCENARIO", scenario)
        .output()
        .map_err(|e| AppError::from_io("spawn crash child", e))?;
    Ok(output.status.code())
}

#[test]
#[ignore]
fn crash_child_runner() {
    let dir = std::path::PathBuf::from(std::env::var("SORTDECK_CRASH_DIR").unwrap());
    let scenario = std::env::var("SORTDECK_CRASH_SCENARIO").unwrap();

    let data_dir = dir.join("sortdeck-data");
    let downloads = dir.join("Downloads");

    let db = Database::open_in_dir(&data_dir).unwrap();
    let protected =
        ProtectedRoots::from_explicit(vec![data_dir.clone(), data_dir.join("logs")], None);

    let cross_vol = matches!(
        scenario.as_str(),
        "copying_with_temp" | "verified_resumable" | "committed_source_present"
    );
    let faulty = Arc::new(FaultyFsBackend::new(
        SandboxFsBackend::new(LocalFsBackend),
        FaultConfig {
            force_cross_volume: cross_vol,
            ..FaultConfig::default()
        },
    ));
    let fs_dyn: Arc<dyn FsBackend> = faulty;

    let target_fp = match scenario.as_str() {
        "planned_before_move" => "after_journal_planned_same_volume",
        "planned_after_move" => "after_same_volume_move",
        "copying_with_temp" => "after_copy_before_verify",
        "verified_resumable" => "after_journal_verified",
        "committed_source_present" => "after_journal_committed",
        "undo_pending" => "after_journal_undo_pending",
        other => panic!("unknown crash scenario {other}"),
    };

    if scenario == "undo_pending" {
        let state_no_fp = AppState::new(db, protected.clone(), fs_dyn.clone());
        let folder = services::folders::add_watched_folder(
            &state_no_fp,
            AddWatchedFolderRequest {
                path: downloads.to_string_lossy().into_owned(),
                recursive: Some(false),
                max_depth: Some(1),
                min_file_age_secs: Some(5),
                allow_network_root: None,
            },
        )
        .unwrap();
        let sum = services::planner::create_dry_run_plan(&state_no_fp, folder.id).unwrap();
        let exec = services::execution::execute_plan(
            &state_no_fp,
            ExecutePlanRequest {
                plan_id: sum.plan_id,
                deselected_operation_ids: vec![],
            },
        )
        .unwrap();
        let hook: FailpointFn = Arc::new(move |fp| {
            if fp == target_fp {
                std::process::exit(99);
            }
        });
        let state_fp = state_no_fp.with_failpoint(hook);
        let _ = services::execution::undo_batch(
            &state_fp,
            UndoRequest {
                batch_id: exec.batch_id,
                item_ids: None,
            },
        );
    } else {
        let hook: FailpointFn = Arc::new(move |fp| {
            if fp == target_fp {
                std::process::exit(99);
            }
        });
        let state = AppState::new(db, protected, fs_dyn).with_failpoint(hook);
        let folder = services::folders::add_watched_folder(
            &state,
            AddWatchedFolderRequest {
                path: downloads.to_string_lossy().into_owned(),
                recursive: Some(false),
                max_depth: Some(1),
                min_file_age_secs: Some(5),
                allow_network_root: None,
            },
        )
        .unwrap();
        let sum = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
        let _ = services::execution::execute_plan(
            &state,
            ExecutePlanRequest {
                plan_id: sum.plan_id,
                deselected_operation_ids: vec![],
            },
        );
    }
}

#[test]
fn inv_09_crash_recovery_matrix_via_child_process() {
    // 1. planned (same-volume), source present, destination absent -> failed(interrupted_before_move)
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/a.png");
        write_file(&src, b"payload-a").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "planned_before_move").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.marked_failed, 1);
        assert!(src.exists());
        assert!(!dir.path().join("Downloads/Images/a.png").exists());
        // Idempotent second run
        assert_eq!(
            recovery::run_recovery(&db, &fs, &roots, false)
                .unwrap()
                .inspected_entries,
            0
        );
    }

    // 2. planned (same-volume), source absent, destination present with matching file ID -> completed + move_logs
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/b.png");
        write_file(&src, b"payload-b").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "planned_after_move").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.finalized_completed, 1);
        assert!(!src.exists());
        assert_eq!(
            std::fs::read(dir.path().join("Downloads/Images/b.png")).unwrap(),
            b"payload-b"
        );
        let move_count: i64 = db
            .with_writer(|conn| {
                conn.query_row("SELECT COUNT(*) FROM move_logs", [], |r| r.get(0))
                    .map_err(|e| AppError::Database {
                        context: "count".to_owned(),
                        message: e.to_string(),
                    })
            })
            .unwrap();
        assert_eq!(move_count, 1);
    }

    // 3. copying, source present, app-owned temp present -> delete temp, mark failed
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/c.png");
        write_file(&src, b"payload-c").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "copying_with_temp").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.marked_failed, 1);
        assert_eq!(rec.cleaned_temps, 1);
        assert_eq!(std::fs::read(&src).unwrap(), b"payload-c");
        assert!(std::fs::read_dir(dir.path().join("Downloads/Images"))
            .unwrap()
            .next()
            .is_none());
    }

    // 4. verified, source unchanged & temp hash matches -> resumes commit -> deletes source -> completed!
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/d.png");
        write_file(&src, b"payload-d").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "verified_resumable").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.finalized_completed, 1);
        assert!(!src.exists());
        assert_eq!(
            std::fs::read(dir.path().join("Downloads/Images/d.png")).unwrap(),
            b"payload-d"
        );
    }

    // 5. committed, destination verified == source, source present -> deletes source, completes!
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/e.png");
        write_file(&src, b"payload-e").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "committed_source_present").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.finalized_completed, 1);
        assert!(!src.exists());
        assert_eq!(
            std::fs::read(dir.path().join("Downloads/Images/e.png")).unwrap(),
            b"payload-e"
        );
    }

    // 6. undo_pending interrupted before restore -> recovery resumes safe restore!
    {
        let dir = tempfile::tempdir().unwrap();
        SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
        let src = dir.path().join("Downloads/u.png");
        write_file(&src, b"payload-u").unwrap();

        assert_eq!(
            spawn_crash_child(dir.path(), "undo_pending").unwrap(),
            Some(99)
        );

        let data_dir = dir.path().join("sortdeck-data");
        let db = Database::open_in_dir(&data_dir).unwrap();
        let fs = SandboxFsBackend::new(LocalFsBackend);
        let roots = ProtectedRoots::from_explicit(vec![data_dir], None);

        let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
        assert_eq!(rec.finalized_undone, 1);
        assert_eq!(std::fs::read(&src).unwrap(), b"payload-u");
        assert!(!dir.path().join("Downloads/Images/u.png").exists());
    }
}

#[test]
fn inv_09_recovery_matrix_remaining_ambiguous_and_differs_branches() {
    let dir = tempfile::tempdir().unwrap();
    SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
    let data_dir = dir.path().join("sortdeck-data");
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir_all(downloads.join("Images")).unwrap();

    let db = Database::open_in_dir(&data_dir).unwrap();
    let fs = SandboxFsBackend::new(LocalFsBackend);
    let roots = ProtectedRoots::from_explicit(vec![data_dir], None);
    let cancel = CancelToken::new();

    // A: planned same-volume missing both src & dst -> recovery_required
    // B: copying with temp absent & src present -> failed
    // C: copying with src absent -> recovery_required
    // D: committed with src absent & verified dst -> completed
    // E: committed with src present & differs from dst -> recovery_required (NO deletion!)
    let b_src = downloads.join("b_src.png");
    let d_dst = downloads.join("Images/d_dst.png");
    let e_src = downloads.join("e_src.png");
    let e_dst = downloads.join("Images/e_dst.png");

    write_file(&b_src, b"b-src").unwrap();
    write_file(&d_dst, b"d-verified").unwrap();
    write_file(&e_src, b"e-modified-source").unwrap();
    write_file(&e_dst, b"e-committed-dest").unwrap();

    let d_hash = fs.hash_sha256(&d_dst, &cancel).unwrap();
    let e_hash = fs.hash_sha256(&e_dst, &cancel).unwrap();

    db.with_writer(|conn| {
        transactions::create_batch(
            conn,
            "seed-batch",
            BatchKind::Manual,
            downloads.to_str().unwrap(),
            None,
            None,
            5,
        )?;
        transactions::insert_journal_entry(
            conn,
            "seed-batch",
            downloads.join("missing.png").to_str().unwrap(),
            downloads.join("Images/missing.png").to_str().unwrap(),
            None,
            OperationType::MoveSameVolume,
            JournalState::Planned,
            &JournalMetadata::default(),
        )?;
        transactions::insert_journal_entry(
            conn,
            "seed-batch",
            b_src.to_str().unwrap(),
            downloads.join("Images/b_src.png").to_str().unwrap(),
            Some(
                downloads
                    .join("Images/.sortdeck-seed-1.sdpart")
                    .to_str()
                    .unwrap(),
            ),
            OperationType::MoveCrossVolume,
            JournalState::Copying,
            &JournalMetadata::default(),
        )?;
        transactions::insert_journal_entry(
            conn,
            "seed-batch",
            downloads.join("c_absent.png").to_str().unwrap(),
            downloads.join("Images/c_absent.png").to_str().unwrap(),
            None,
            OperationType::MoveCrossVolume,
            JournalState::Copying,
            &JournalMetadata::default(),
        )?;
        transactions::insert_journal_entry(
            conn,
            "seed-batch",
            downloads.join("d_src.png").to_str().unwrap(),
            d_dst.to_str().unwrap(),
            None,
            OperationType::MoveCrossVolume,
            JournalState::Committed,
            &JournalMetadata {
                sha256: Some(d_hash),
                source_fingerprint: Some(SourceFingerprintDto {
                    volume_serial: None,
                    file_id: None,
                    size: 10,
                    modified_epoch_ms: None,
                }),
                ..JournalMetadata::default()
            },
        )?;
        transactions::insert_journal_entry(
            conn,
            "seed-batch",
            e_src.to_str().unwrap(),
            e_dst.to_str().unwrap(),
            None,
            OperationType::MoveCrossVolume,
            JournalState::Committed,
            &JournalMetadata {
                sha256: Some(e_hash),
                ..JournalMetadata::default()
            },
        )?;
        Ok(())
    })
    .unwrap();

    let rec = recovery::run_recovery(&db, &fs, &roots, false).unwrap();
    assert_eq!(rec.inspected_entries, 5);
    assert_eq!(rec.finalized_completed, 1);
    assert_eq!(rec.marked_failed, 1);
    assert_eq!(rec.recovery_required, 3);

    // Crucial: `e_src` and `e_dst` must BOTH remain untouched!
    assert_eq!(std::fs::read(&e_src).unwrap(), b"e-modified-source");
    assert_eq!(std::fs::read(&e_dst).unwrap(), b"e-committed-dest");

    // Acknowledge one recovery item
    let items = db
        .with_writer(|conn| transactions::list_recovery_required_entries(conn))
        .unwrap();
    assert_eq!(items.len(), 3);
    db.with_writer(|conn| transactions::acknowledge_recovery_entry(conn, items[0].journal_id))
        .unwrap();
    let remaining = db
        .with_writer(|conn| transactions::list_recovery_required_entries(conn))
        .unwrap();
    assert_eq!(remaining.len(), 2);
}
