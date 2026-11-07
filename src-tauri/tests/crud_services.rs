use std::path::PathBuf;
use std::sync::Arc;

use sortdeck_lib::config::AppSettings;
use sortdeck_lib::database::Database;
use sortdeck_lib::engine::paths::ProtectedRoots;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard};
use sortdeck_lib::models::{
    AddWatchedFolderRequest, CreateRuleRequest, ReorderRulesRequest, UpdateRuleRequest,
    UpdateWatchedFolderRequest,
};
use sortdeck_lib::services;
use sortdeck_lib::services::state::AppState;

fn setup_sandbox_state() -> Result<(tempfile::TempDir, AppState, PathBuf), AppError> {
    let dir = tempfile::tempdir().map_err(|e| AppError::from_io("create sandbox dir", e))?;
    SandboxGuard::init_sandbox_dir(dir.path())?;

    let data_dir = dir.path().join("sortdeck-data");
    let watched_root = dir.path().join("Downloads");
    std::fs::create_dir_all(&watched_root)
        .map_err(|e| AppError::from_io("create watched root", e))?;

    let db = Database::open_in_dir(&data_dir)?;
    let protected =
        ProtectedRoots::from_explicit(vec![data_dir.clone(), data_dir.join("logs")], None);
    let fs = Arc::new(SandboxFsBackend::new(LocalFsBackend));
    let state = AppState::new(db, protected, fs);

    Ok((dir, state, watched_root))
}

#[test]
fn watched_folder_crud_enforces_safety_and_overlap_rules() {
    let (sandbox, state, downloads) = setup_sandbox_state().unwrap();
    let nested = downloads.join("Subfolder");
    std::fs::create_dir_all(&nested).unwrap();

    // Add folder: starts enabled with auto_sort = false (no surprise moves).
    let added = services::folders::add_watched_folder(
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
    assert!(added.enabled);
    assert!(!added.auto_sort);
    assert!(added.auto_sort_enabled_at.is_none());

    // Duplicate and ancestor/descendant overlap must be rejected.
    let dup_err = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: None,
            max_depth: None,
            min_file_age_secs: None,
            allow_network_root: None,
        },
    )
    .unwrap_err();
    assert_eq!(dup_err.code(), "CONFLICT");

    let overlap_err = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: nested.to_string_lossy().into_owned(),
            recursive: None,
            max_depth: None,
            min_file_age_secs: None,
            allow_network_root: None,
        },
    )
    .unwrap_err();
    assert_eq!(overlap_err.code(), "CONFLICT");

    // Invalid max_depth rejected.
    let depth_err = services::folders::update_watched_folder(
        &state,
        UpdateWatchedFolderRequest {
            id: added.id,
            enabled: true,
            auto_sort: true,
            recursive: true,
            max_depth: 0,
            min_file_age_secs: 5,
        },
    )
    .unwrap_err();
    assert_eq!(depth_err.code(), "INVALID_INPUT");

    // Enabling auto_sort records auto_sort_enabled_at.
    let updated = services::folders::update_watched_folder(
        &state,
        UpdateWatchedFolderRequest {
            id: added.id,
            enabled: true,
            auto_sort: true,
            recursive: true,
            max_depth: 3,
            min_file_age_secs: 10,
        },
    )
    .unwrap();
    assert!(updated.auto_sort);
    assert!(updated.auto_sort_enabled_at.is_some());

    // Removing folder keeps files on disk intact.
    let sentinel_file = downloads.join("keep.txt");
    std::io::Write::write_all(
        &mut std::fs::File::create(&sentinel_file).unwrap(),
        b"important",
    )
    .unwrap();
    services::folders::remove_watched_folder(&state, added.id).unwrap();
    assert!(sentinel_file.exists());
    assert!(services::folders::list_watched_folders(&state)
        .unwrap()
        .is_empty());
    drop(sandbox);
}

#[test]
fn rule_crud_overlap_warnings_reorder_and_restore_defaults() {
    let (_sandbox, state, _downloads) = setup_sandbox_state().unwrap();

    let initial = services::rules::list_rules(&state).unwrap();
    assert_eq!(initial.rules.len(), 8);
    assert!(initial.overlaps.is_empty());

    // Create custom rule overlapping `.ts` (claimed by Code default rule) and compound `.tar.gz`.
    let custom = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "  Transport Streams  ".to_owned(),
            destination: r"Media\TS".to_owned(),
            extensions: vec![".TS".to_owned(), " .tar.gz ".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap();
    assert_eq!(custom.name, "Transport Streams");
    assert_eq!(custom.destination, "Media/TS/");
    assert_eq!(custom.extensions, vec!["tar.gz", "ts"]);
    assert_eq!(custom.priority, 9);

    // Duplicate normalized extension within one rule is rejected.
    let dup_ext = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Invalid".to_owned(),
            destination: "Out/".to_owned(),
            extensions: vec!["png".to_owned(), ".PNG".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap_err();
    assert_eq!(dup_ext.code(), "INVALID_INPUT");

    // Reorder custom rule to priority 1 and verify overlap winner flips to custom rule.
    let mut all_ids: Vec<i64> = initial.rules.iter().map(|r| r.id).collect();
    all_ids.insert(0, custom.id);
    let reordered = services::rules::reorder_rules(
        &state,
        ReorderRulesRequest {
            ordered_rule_ids: all_ids,
        },
    )
    .unwrap();
    assert_eq!(reordered.rules[0].id, custom.id);
    assert_eq!(reordered.rules[0].priority, 1);

    let ts_overlap = reordered
        .overlaps
        .iter()
        .find(|w| w.extension == "ts")
        .expect("expected ts overlap warning");
    assert_eq!(ts_overlap.winner_rule_id, custom.id);
    assert_eq!(ts_overlap.winner_rule_name, "Transport Streams");

    // Reject incomplete reorder request.
    let bad_reorder = services::rules::reorder_rules(
        &state,
        ReorderRulesRequest {
            ordered_rule_ids: vec![custom.id],
        },
    )
    .unwrap_err();
    assert_eq!(bad_reorder.code(), "INVALID_INPUT");

    // Edit a default rule and delete another, then restore defaults while keeping custom rule.
    let images_rule = reordered
        .rules
        .iter()
        .find(|r| r.default_key.as_deref() == Some("default.images"))
        .unwrap();
    services::rules::update_rule(
        &state,
        UpdateRuleRequest {
            id: images_rule.id,
            name: "Renamed Images".to_owned(),
            destination: "Pictures/".to_owned(),
            extensions: vec!["png".to_owned()],
            enabled: false,
            name_contains: Some("Screenshot".to_owned()),
            min_size_bytes: Some(100),
            max_size_bytes: None,
            min_age_days: Some(7),
        },
    )
    .unwrap();
    let audio_rule = reordered
        .rules
        .iter()
        .find(|r| r.default_key.as_deref() == Some("default.audio"))
        .unwrap();
    services::rules::delete_rule(&state, audio_rule.id).unwrap();

    let restored = services::rules::restore_default_rules(&state).unwrap();
    assert_eq!(restored.rules.len(), 9);
    assert!(restored.rules.iter().any(|r| r.id == custom.id));
    let restored_images = restored
        .rules
        .iter()
        .find(|r| r.default_key.as_deref() == Some("default.images"))
        .unwrap();
    assert_eq!(restored_images.name, "Images");
    assert_eq!(restored_images.destination, "Images/");
    assert!(restored_images.enabled);
    assert!(restored
        .rules
        .iter()
        .any(|r| r.default_key.as_deref() == Some("default.audio")));
}

#[test]
fn settings_crud_validates_bounds_and_bumps_config_revision() {
    let (_sandbox, state, _downloads) = setup_sandbox_state().unwrap();

    let before_status = services::get_database_status(&state).unwrap();
    let defaults = services::settings::get_settings(&state).unwrap();
    assert_eq!(defaults, AppSettings::default());

    let updated = services::settings::update_settings(
        &state,
        AppSettings {
            default_scan_depth: 5,
            retention_days: None,
            notifications: false,
            ..defaults
        },
    )
    .unwrap();
    assert_eq!(updated.default_scan_depth, 5);
    assert_eq!(updated.retention_days, None);
    assert!(!updated.notifications);

    let after_status = services::get_database_status(&state).unwrap();
    assert!(after_status.config_revision > before_status.config_revision);

    let bad_depth = services::settings::update_settings(
        &state,
        AppSettings {
            default_scan_depth: 99,
            ..updated.clone()
        },
    )
    .unwrap_err();
    assert_eq!(bad_depth.code(), "INVALID_INPUT");

    let bad_retention = services::settings::update_settings(
        &state,
        AppSettings {
            retention_days: Some(0),
            ..updated
        },
    )
    .unwrap_err();
    assert_eq!(bad_retention.code(), "INVALID_INPUT");

    assert!(after_status.db_size_bytes > 0);
    assert!(after_status.db_path.ends_with("sortdeck.db"));

    let diag = services::export_redacted_diagnostics(&state).unwrap();
    let diag_json = serde_json::to_string(&diag).unwrap();
    assert!(!diag_json.contains("Downloads"));
    assert!(!diag_json.contains("sortdeck-data"));
    assert_eq!(diag.schema_version, after_status.schema_version);
    assert_eq!(diag.total_rule_count, 8);

    // Smart filter bounds validation and search_move_logs LIKE escaping
    let bad_bounds = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Bad size".to_owned(),
            destination: "Big/".to_owned(),
            extensions: vec!["bin".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: Some(500),
            max_size_bytes: Some(100),
            min_age_days: None,
        },
    )
    .unwrap_err();
    assert_eq!(bad_bounds.code(), "INVALID_INPUT");

    let empty_search = services::history::search_move_logs(&state, "   ", 20).unwrap();
    assert!(empty_search.is_empty());

    // Seed move_logs and test partial filename + LIKE wildcard search
    state
        .db
        .with_writer(|conn| {
            sortdeck_lib::database::transactions::create_batch(
                conn,
                "b-search-1",
                sortdeck_lib::models::BatchKind::Manual,
                r"D:\Downloads",
                None,
                None,
                2,
            )?;
            conn.execute(
                "INSERT INTO move_logs (batch_id, original_path, new_path, status, cross_volume, file_size, moved_at, undone)
                 VALUES ('b-search-1', 'D:\\Downloads\\Client_50%_Report.pdf', 'D:\\Downloads\\Documents\\Client_50%_Report.pdf', 'completed', 0, 1234, 1700000000000, 0)",
                [],
            )
            .map_err(|e| AppError::Database { context: "seed move log".into(), message: e.to_string() })?;
            conn.execute(
                "INSERT INTO move_logs (batch_id, original_path, new_path, status, cross_volume, file_size, moved_at, undone)
                 VALUES ('b-search-1', 'D:\\Downloads\\unrelated.png', 'D:\\Downloads\\Images\\unrelated.png', 'completed', 0, 100, 1700000000000, 0)",
                [],
            )
            .map_err(|e| AppError::Database { context: "seed move log 2".into(), message: e.to_string() })?;
            Ok(())
        })
        .unwrap();

    let hits = services::history::search_move_logs(&state, "50%_Report", 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].new_path.contains("Client_50%_Report.pdf"));
}
