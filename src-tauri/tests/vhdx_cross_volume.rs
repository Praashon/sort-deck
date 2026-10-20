use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sortdeck_lib::database::Database;
use sortdeck_lib::engine::paths::ProtectedRoots;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard};
use sortdeck_lib::fsops::FsBackend;
use sortdeck_lib::models::{
    AddWatchedFolderRequest, BatchStatus, CreateRuleRequest, ExecutePlanRequest, UndoRequest,
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

#[test]
#[ignore]
fn real_vhdx_cross_volume_move_ads_and_undo() {
    let second_vol_env = std::env::var("SORTDECK_SECOND_VOLUME_DIR").expect(
        "set SORTDECK_SECOND_VOLUME_DIR to a mounted VHDX folder containing .sortdeck-test-sandbox",
    );
    let second_vol = PathBuf::from(second_vol_env);
    SandboxGuard::init_sandbox_dir(&second_vol).unwrap();

    let dir = tempfile::tempdir().unwrap();
    SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
    let data_dir = dir.path().join("sortdeck-data");
    let downloads = dir.path().join("Downloads");
    let dest_dir = second_vol.join("SortedImages");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let db = Database::open_in_dir(&data_dir).unwrap();
    let protected =
        ProtectedRoots::from_explicit(vec![data_dir.clone(), data_dir.join("logs")], None);
    let fs: Arc<dyn FsBackend> = Arc::new(SandboxFsBackend::new(LocalFsBackend));
    let state = AppState::new(db, protected, fs);

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

    services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "CrossVol Images".to_owned(),
            destination: dest_dir.to_string_lossy().into_owned(),
            extensions: vec!["png".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap();
    // Move custom rule to priority 1
    let rules = services::rules::list_rules(&state).unwrap().rules;
    let mut ids: Vec<i64> = rules.iter().map(|r| r.id).collect();
    let last = ids.pop().unwrap();
    ids.insert(0, last);
    services::rules::reorder_rules(
        &state,
        sortdeck_lib::models::ReorderRulesRequest {
            ordered_rule_ids: ids,
        },
    )
    .unwrap();

    let src_file = downloads.join("日本語_downloaded.png");
    write_file(&src_file, b"cross-volume-vhdx-payload").unwrap();
    // Attach Zone.Identifier ADS to verify Mark-of-the-Web survives CopyFileExW!
    let ads_path = PathBuf::from(format!("{}:Zone.Identifier", src_file.display()));
    write_file(&ads_path, b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(summary.cross_volume_count, 1);

    let exec = services::execution::execute_plan(
        &state,
        ExecutePlanRequest {
            plan_id: summary.plan_id,
            deselected_operation_ids: vec![],
        },
    )
    .unwrap();
    assert_eq!(exec.status, BatchStatus::Completed);

    let moved_file = dest_dir.join("日本語_downloaded.png");
    assert!(!src_file.exists());
    assert_eq!(
        std::fs::read(&moved_file).unwrap(),
        b"cross-volume-vhdx-payload"
    );
    let moved_ads = PathBuf::from(format!("{}:Zone.Identifier", moved_file.display()));
    assert!(std::fs::read_to_string(&moved_ads)
        .unwrap()
        .contains("ZoneId=3"));

    let undo = services::execution::undo_batch(
        &state,
        UndoRequest {
            batch_id: exec.batch_id,
            item_ids: None,
        },
    )
    .unwrap();
    assert_eq!(undo.status, BatchStatus::Completed);
    assert_eq!(
        std::fs::read(&src_file).unwrap(),
        b"cross-volume-vhdx-payload"
    );
    assert!(!moved_file.exists());
}
