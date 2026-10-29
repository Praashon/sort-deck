use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use proptest::prelude::*;
use sortdeck_lib::database::Database;
use sortdeck_lib::engine::naming::{collision_candidate, split_stem_and_extension};
use sortdeck_lib::engine::paths::ProtectedRoots;
use sortdeck_lib::engine::planner::{
    build_dry_run_plan, page_plan, revalidate_plan, DEFAULT_PLAN_TTL_MS, FAT32_MAX_FILE_SIZE,
};
use sortdeck_lib::engine::rules::{extract_candidate_extensions, match_rule, RuleMatchOutcome};
use sortdeck_lib::engine::scanner::{ScanConfig, TEMP_DOWNLOAD_EXTENSIONS};
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard};
use sortdeck_lib::fsops::{
    CancelToken, FileAttributes, FileFingerprint, FsBackend, FsEntry, SameVolumeMoveOutcome,
    VolumeInfo,
};
use sortdeck_lib::models::{
    AddWatchedFolderRequest, CreateRuleRequest, OperationStatus, PlanPageRequest, Rule, SkipReason,
    UpdateRuleRequest, ALL_SKIP_REASONS,
};
use sortdeck_lib::services;
use sortdeck_lib::services::state::AppState;

fn write_test_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::from_io("create parent dir", e))?;
    }
    let mut file =
        std::fs::File::create(path).map_err(|e| AppError::from_io("create test file", e))?;
    file.write_all(bytes)
        .map_err(|e| AppError::from_io("write test file", e))
}

fn setup_sandbox_env() -> Result<(tempfile::TempDir, AppState, PathBuf), AppError> {
    let dir = tempfile::tempdir().map_err(|e| AppError::from_io("create sandbox dir", e))?;
    SandboxGuard::init_sandbox_dir(dir.path())?;
    let data_dir = dir.path().join("sortdeck-data");
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir_all(&downloads)
        .map_err(|e| AppError::from_io("create Downloads dir", e))?;

    let db = Database::open_in_dir(&data_dir)?;
    let protected =
        ProtectedRoots::from_explicit(vec![data_dir.clone(), data_dir.join("logs")], None);
    let fs = Arc::new(SandboxFsBackend::new(LocalFsBackend));
    let state = AppState::new(db, protected, fs);
    Ok((dir, state, downloads))
}

type TreeSnapshot = BTreeMap<PathBuf, (bool, u64, Vec<u8>)>;

fn snapshot_tree(root: &Path) -> Result<TreeSnapshot, AppError> {
    let mut map = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| AppError::from_io("read snapshot dir", e))?;
        for entry_res in entries {
            let entry = entry_res.map_err(|e| AppError::from_io("iterate snapshot", e))?;
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path)
                .map_err(|e| AppError::from_io("snapshot metadata", e))?;
            if meta.is_dir() {
                map.insert(path.clone(), (true, 0, Vec::new()));
                stack.push(path);
            } else {
                let bytes =
                    std::fs::read(&path).map_err(|e| AppError::from_io("read snapshot file", e))?;
                map.insert(path, (false, meta.len(), bytes));
            }
        }
    }
    Ok(map)
}

#[derive(Default)]
struct CustomMockFs {
    entries_by_dir: HashMap<String, Vec<FsEntry>>,
    attrs_by_path: HashMap<String, FileAttributes>,
    fps_by_path: HashMap<String, FileFingerprint>,
    vol_by_prefix: HashMap<String, VolumeInfo>,
}

impl CustomMockFs {
    fn key(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/").to_lowercase()
    }

    fn set_dir(&mut self, dir: &Path, entries: Vec<FsEntry>) {
        self.attrs_by_path.insert(
            Self::key(dir),
            FileAttributes {
                is_dir: true,
                ..FileAttributes::default()
            },
        );
        self.entries_by_dir.insert(Self::key(dir), entries);
    }

    fn add_file(
        &mut self,
        path: &Path,
        size: u64,
        attrs: FileAttributes,
        is_non_unicode: bool,
    ) -> FsEntry {
        let key = Self::key(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.attrs_by_path.insert(key.clone(), attrs);
        self.fps_by_path.insert(
            key,
            FileFingerprint {
                volume_serial: None,
                file_id: None,
                size,
                modified_epoch_ms: Some(1_700_000_000_000),
            },
        );
        FsEntry {
            path: path.to_path_buf(),
            file_name: name,
            attributes: attrs,
            size,
            modified_epoch_ms: Some(1_700_000_000_000),
            is_non_unicode,
        }
    }
}

impl FsBackend for CustomMockFs {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        let key = Self::key(path);
        Ok(self.attrs_by_path.contains_key(&key) || self.entries_by_dir.contains_key(&key))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        Ok(path.to_path_buf())
    }

    fn attributes(&self, path: &Path) -> Result<FileAttributes, AppError> {
        self.attrs_by_path
            .get(&Self::key(path))
            .copied()
            .ok_or_else(|| AppError::NotFound {
                resource: path.display().to_string(),
                os_error_code: None,
            })
    }

    fn fingerprint(&self, path: &Path) -> Result<FileFingerprint, AppError> {
        self.fps_by_path
            .get(&Self::key(path))
            .cloned()
            .ok_or_else(|| AppError::NotFound {
                resource: path.display().to_string(),
                os_error_code: None,
            })
    }

    fn read_dir(&self, dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        Ok(self
            .entries_by_dir
            .get(&Self::key(dir))
            .cloned()
            .unwrap_or_default())
    }

    fn move_no_overwrite(
        &self,
        _src: &Path,
        _dst: &Path,
    ) -> Result<SameVolumeMoveOutcome, AppError> {
        unreachable!()
    }

    fn copy_to_temp(&self, _s: &Path, _d: &Path, _c: &CancelToken) -> Result<(), AppError> {
        unreachable!()
    }

    fn flush_file(&self, _path: &Path) -> Result<(), AppError> {
        unreachable!()
    }

    fn hash_sha256(&self, _path: &Path, _c: &CancelToken) -> Result<String, AppError> {
        unreachable!()
    }

    fn delete_file(&self, _path: &Path) -> Result<(), AppError> {
        unreachable!()
    }

    fn create_dir_all(&self, _path: &Path) -> Result<Vec<PathBuf>, AppError> {
        unreachable!()
    }

    fn remove_empty_dir(&self, _path: &Path) -> Result<(), AppError> {
        unreachable!()
    }

    fn volume_info(&self, path: &Path) -> Result<VolumeInfo, AppError> {
        let key = Self::key(path);
        for (prefix, info) in &self.vol_by_prefix {
            if key.starts_with(prefix) {
                return Ok(info.clone());
            }
        }
        Ok(VolumeInfo {
            volume_serial: Some(1),
            filesystem_name: "NTFS".to_owned(),
            supports_file_ids: true,
            supports_ads: true,
            max_component_len: 255,
            available_bytes: Some(10 * 1024 * 1024 * 1024),
        })
    }

    fn check_access_for_move(&self, _path: &Path) -> Result<(), AppError> {
        unreachable!()
    }
}

#[test]
fn inv_05_dry_run_changes_nothing_on_disk() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();
    write_test_file(&downloads.join("photo.png"), b"png-data").unwrap();
    write_test_file(&downloads.join("report.pdf"), b"pdf-data").unwrap();
    write_test_file(&downloads.join("archive.tar.gz"), b"tgz-data").unwrap();
    write_test_file(&downloads.join("unknown.xyz"), b"unknown").unwrap();

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

    let before = snapshot_tree(&downloads).unwrap();
    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    let after = snapshot_tree(&downloads).unwrap();

    assert_eq!(before, after, "dry-run must not mutate anything on disk");
    assert!(!downloads.join("Images").exists());
    assert!(!downloads.join("Documents").exists());
    assert_eq!(summary.total_discovered, 4);
    assert_eq!(summary.eligible_count, 3);
    assert_eq!(summary.skipped_count, 1);
}

#[test]
fn inv_06_scanner_excludes_sortignore_project_roots_excluded_dirs_and_reparse_points() {
    let (sandbox, state, downloads) = setup_sandbox_env().unwrap();

    // Case 1: recursive scan skips `.git` project root, `node_modules`, and `.sortignore` subdir.
    write_test_file(&downloads.join("top.png"), b"ok").unwrap();
    write_test_file(&downloads.join("MyProject/.git/config"), b"git").unwrap();
    write_test_file(&downloads.join("MyProject/README.md"), b"repo").unwrap();
    write_test_file(&downloads.join("WebApp/node_modules/pkg/index.js"), b"js").unwrap();
    write_test_file(&downloads.join("Ignored/.sortignore"), b"").unwrap();
    write_test_file(&downloads.join("Ignored/secret.png"), b"ignored").unwrap();

    let folder = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: Some(true),
            max_depth: Some(5),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )
    .unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert!(summary.sortignore_detected);
    assert_eq!(summary.eligible_count, 1);
    let page = services::planner::get_plan_page(
        &state,
        PlanPageRequest {
            plan_id: summary.plan_id,
            offset: 0,
            limit: 50,
            status_filter: Some(OperationStatus::Eligible),
            skip_reason_filter: None,
            search: None,
        },
    )
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].original_filename, "top.png");

    // Case 2: `.sortignore` on an ancestor disables the entire watched folder.
    let nested_root = sandbox.path().join("ParentIgnored").join("SubWatch");
    write_test_file(&sandbox.path().join("ParentIgnored/.sortignore"), b"").unwrap();
    write_test_file(&nested_root.join("pic.png"), b"1").unwrap();
    let nested_folder = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: nested_root.to_string_lossy().into_owned(),
            recursive: Some(false),
            max_depth: Some(1),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )
    .unwrap();
    let ignored_summary = services::planner::create_dry_run_plan(&state, nested_folder.id).unwrap();
    assert!(ignored_summary.sortignore_detected);
    assert_eq!(ignored_summary.total_discovered, 0);
    assert_eq!(ignored_summary.eligible_count, 0);

    // Case 3: Reparse-point directory & file skipped via CustomMockFs.
    let mut mock = CustomMockFs::default();
    let root = PathBuf::from(r"D:\MockWatch");
    let link_dir = root.join("LinkDir");
    let link_file = root.join("link.png");
    let entries = vec![
        mock.add_file(
            &link_dir,
            0,
            FileAttributes {
                is_dir: true,
                is_reparse_point: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &link_file,
            10,
            FileAttributes {
                is_reparse_point: true,
                ..FileAttributes::default()
            },
            false,
        ),
    ];
    mock.set_dir(&root, entries);
    let mock_folder = sortdeck_lib::models::WatchedFolder {
        id: 1,
        path: r"D:\MockWatch".to_owned(),
        path_key: "d:/mockwatch".to_owned(),
        enabled: true,
        auto_sort: false,
        recursive: true,
        max_depth: 5,
        min_file_age_secs: 5,
        status: sortdeck_lib::models::FolderStatus::Ok,
        auto_sort_enabled_at: None,
        last_reconciled_at: None,
        created_at: 0,
        updated_at: 0,
    };
    let rules = services::rules::list_rules(&state).unwrap().rules;
    let mock_plan = build_dry_run_plan(
        &mock_folder,
        &rules,
        1,
        &ProtectedRoots::from_explicit(vec![], None),
        &mock,
        &CancelToken::new(),
        ScanConfig::default(),
        1000,
        DEFAULT_PLAN_TTL_MS,
    )
    .unwrap();
    assert_eq!(mock_plan.summary.eligible_count, 0);
    assert_eq!(mock_plan.operations.len(), 1);
    assert_eq!(
        mock_plan.operations[0].skip_reason,
        Some(SkipReason::ReparsePoint)
    );
}

#[test]
fn inv_07_scanner_skips_hidden_system_cloud_temp_special_office_and_limits() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();
    let rules = services::rules::list_rules(&state).unwrap().rules;

    // Seed real sandbox temp extensions, special files, and Office lock pair.
    for ext in TEMP_DOWNLOAD_EXTENSIONS {
        write_test_file(&downloads.join(format!("dl.{ext}")), b"tmp").unwrap();
    }
    for special in [
        "desktop.ini",
        "Thumbs.db",
        ".DS_Store",
        ".sortdeck-abc-1.sdpart",
    ] {
        write_test_file(&downloads.join(special), b"sp").unwrap();
    }
    write_test_file(&downloads.join("~$report.docx"), b"lock").unwrap();
    write_test_file(&downloads.join("report.docx"), b"doc").unwrap();
    write_test_file(&downloads.join("safe.png"), b"img").unwrap();

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

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(summary.eligible_count, 1);

    let counts: HashMap<SkipReason, u64> = summary
        .skip_reason_counts
        .iter()
        .map(|c| (c.reason, c.count))
        .collect();
    assert_eq!(counts.get(&SkipReason::TemporaryExtension), Some(&15));
    assert_eq!(counts.get(&SkipReason::SpecialFile), Some(&4));
    assert_eq!(counts.get(&SkipReason::OfficeLockFile), Some(&1));
    assert_eq!(counts.get(&SkipReason::FileInUse), Some(&1));

    // Mock hidden, system, cloud placeholder, non-unicode, and cross-volume limits.
    let mut mock = CustomMockFs::default();
    let src_dir = PathBuf::from(r"C:\MockDl");
    let e_dest = PathBuf::from(r"E:\Out\Images");
    mock.attrs_by_path.insert(
        CustomMockFs::key(&e_dest),
        FileAttributes {
            is_dir: true,
            ..FileAttributes::default()
        },
    );
    mock.vol_by_prefix.insert(
        "c:/".to_owned(),
        VolumeInfo {
            volume_serial: Some(10),
            filesystem_name: "NTFS".to_owned(),
            supports_file_ids: true,
            supports_ads: true,
            max_component_len: 255,
            available_bytes: Some(100 * 1024 * 1024 * 1024),
        },
    );
    mock.vol_by_prefix.insert(
        "e:/".to_owned(),
        VolumeInfo {
            volume_serial: Some(20),
            filesystem_name: "FAT32".to_owned(),
            supports_file_ids: false,
            supports_ads: false,
            max_component_len: 255,
            available_bytes: Some(200 * 1024 * 1024),
        },
    );

    let entries = vec![
        mock.add_file(
            &src_dir.join("h.png"),
            10,
            FileAttributes {
                is_hidden: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &src_dir.join("s.png"),
            10,
            FileAttributes {
                is_system: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &src_dir.join("cloud_dehydrated.png"),
            10,
            FileAttributes {
                is_offline_placeholder: true,
                is_cloud_managed: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &src_dir.join("cloud_hydrated.png"),
            10,
            FileAttributes {
                is_cloud_managed: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &src_dir.join("ro.png"),
            10,
            FileAttributes {
                is_readonly: true,
                ..FileAttributes::default()
            },
            false,
        ),
        mock.add_file(
            &src_dir.join("bad_utf16.png"),
            10,
            FileAttributes::default(),
            true,
        ),
        mock.add_file(
            &src_dir.join("huge.png"),
            FAT32_MAX_FILE_SIZE + 1,
            FileAttributes::default(),
            false,
        ),
        mock.add_file(
            &src_dir.join("too_big_for_free.png"),
            180 * 1024 * 1024,
            FileAttributes::default(),
            false,
        ),
    ];
    mock.set_dir(&src_dir, entries);

    let mut custom_rules = rules;
    custom_rules[0].destination = r"E:\Out\Images".to_owned();
    let mock_folder = sortdeck_lib::models::WatchedFolder {
        id: 2,
        path: r"C:\MockDl".to_owned(),
        path_key: "c:/mockdl".to_owned(),
        enabled: true,
        auto_sort: false,
        recursive: false,
        max_depth: 1,
        min_file_age_secs: 5,
        status: sortdeck_lib::models::FolderStatus::Ok,
        auto_sort_enabled_at: None,
        last_reconciled_at: None,
        created_at: 0,
        updated_at: 0,
    };

    let mock_plan = build_dry_run_plan(
        &mock_folder,
        &custom_rules,
        1,
        &ProtectedRoots::from_explicit(vec![], None),
        &mock,
        &CancelToken::new(),
        ScanConfig::default(),
        1000,
        DEFAULT_PLAN_TTL_MS,
    )
    .unwrap();

    let reasons: Vec<Option<SkipReason>> = mock_plan
        .operations
        .iter()
        .map(|op| op.skip_reason)
        .collect();
    assert!(reasons.contains(&Some(SkipReason::HiddenAttribute)));
    assert!(reasons.contains(&Some(SkipReason::SystemAttribute)));
    assert!(reasons.contains(&Some(SkipReason::CloudPlaceholder)));
    assert!(reasons.contains(&Some(SkipReason::CloudManagedCrossVolume)));
    assert!(reasons.contains(&Some(SkipReason::ReadOnlyCrossVolume)));
    assert!(reasons.contains(&Some(SkipReason::NonUnicodePath)));
    assert!(reasons.contains(&Some(SkipReason::FilesystemLimit)));
    assert!(reasons.contains(&Some(SkipReason::InsufficientSpace)));
}

#[test]
fn inv_11_plan_revalidation_rejects_expired_stale_config_and_changed_sources() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();
    let a_path = downloads.join("a.png");
    let b_path = downloads.join("b.png");
    let c_path = downloads.join("c.pdf");
    write_test_file(&a_path, b"aaa").unwrap();
    write_test_file(&b_path, b"bbb").unwrap();
    write_test_file(&c_path, b"ccc").unwrap();

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

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    let page = services::planner::get_plan_page(
        &state,
        PlanPageRequest {
            plan_id: summary.plan_id.clone(),
            offset: 0,
            limit: 50,
            status_filter: None,
            skip_reason_filter: None,
            search: None,
        },
    )
    .unwrap();

    let op_a = page
        .items
        .iter()
        .find(|i| i.original_filename == "a.png")
        .unwrap()
        .operation_id
        .clone();
    let op_b = page
        .items
        .iter()
        .find(|i| i.original_filename == "b.png")
        .unwrap()
        .operation_id
        .clone();
    let op_c = page
        .items
        .iter()
        .find(|i| i.original_filename == "c.pdf")
        .unwrap()
        .operation_id
        .clone();

    // Modify `a.png` (changes size) and deselect `b.png`.
    write_test_file(&a_path, b"aaa-modified-size").unwrap();
    let report = services::planner::revalidate_plan(&state, &summary.plan_id, &[op_b]).unwrap();
    assert_eq!(report.valid_operation_ids, vec![op_c]);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].operation_id, op_a);
    assert_eq!(report.rejected[0].reason, SkipReason::SourceChanged);

    // Editing a rule bumps config_revision -> existing plan is invalidated.
    let rules = services::rules::list_rules(&state).unwrap().rules;
    let img = rules
        .iter()
        .find(|r| r.default_key.as_deref() == Some("default.images"))
        .unwrap();
    services::rules::update_rule(
        &state,
        UpdateRuleRequest {
            id: img.id,
            name: img.name.clone(),
            destination: img.destination.clone(),
            extensions: img.extensions.clone(),
            enabled: false,
            name_contains: img.name_contains.clone(),
            min_size_bytes: img.min_size_bytes,
            max_size_bytes: img.max_size_bytes,
            min_age_days: img.min_age_days,
        },
    )
    .unwrap();
    let inv_err = services::planner::revalidate_plan(&state, &summary.plan_id, &[]).unwrap_err();
    assert_eq!(inv_err.code(), "PLAN_INVALIDATED");

    // Expired TTL check via engine::planner::revalidate_plan directly.
    let rules_after = services::rules::list_rules(&state).unwrap().rules;
    let fresh_plan = build_dry_run_plan(
        &folder,
        &rules_after,
        99,
        &state.protected_roots,
        state.fs.as_ref(),
        &CancelToken::new(),
        ScanConfig::default(),
        1_000,
        5_000,
    )
    .unwrap();
    let exp_err = revalidate_plan(
        &fresh_plan,
        &[],
        99,
        &folder,
        &rules_after,
        &state.protected_roots,
        state.fs.as_ref(),
        7_000,
    )
    .unwrap_err();
    assert_eq!(exp_err.code(), "PLAN_EXPIRED");
}

#[test]
fn inv_12_plan_idempotency_skips_files_already_in_destination() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();
    write_test_file(&downloads.join("Images/already.png"), b"img").unwrap();
    write_test_file(&downloads.join("Documents/readme.pdf"), b"doc").unwrap();
    write_test_file(&downloads.join("Images/Nested/deep.png"), b"img2").unwrap();

    // Both non-recursive and recursive sweeps skip managed destinations, yielding 0 moves.
    let folder = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: Some(true),
            max_depth: Some(5),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )
    .unwrap();

    let first_summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(first_summary.eligible_count, 0);
    assert_eq!(first_summary.conflict_renamed_count, 0);
    assert_eq!(first_summary.total_bytes_to_move, 0);

    let second_summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(second_summary.eligible_count, 0);
    assert_eq!(second_summary.conflict_renamed_count, 0);
    assert_eq!(second_summary.total_bytes_to_move, 0);
}

#[test]
fn rule_matching_compound_multi_dot_dotfiles_priorities_and_collision_naming() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();

    // Custom rule claiming `tar.gz` at lower priority than Archives (`gz`),
    // longest suffix `tar.gz` still wins over `gz` (§6.3)!
    let tar_gz_rule = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Tarballs".to_owned(),
            destination: "Tarballs/".to_owned(),
            extensions: vec!["tar.gz".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap();

    let disabled_rule = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Disabled XYZ".to_owned(),
            destination: "XYZ/".to_owned(),
            extensions: vec!["xyz".to_owned()],
            enabled: false,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap();

    let rules = services::rules::list_rules(&state).unwrap().rules;
    assert_eq!(
        match_rule("backup.TAR.GZ", &rules),
        RuleMatchOutcome::Matched(&tar_gz_rule)
    );
    assert_eq!(
        match_rule("item.xyz", &rules),
        RuleMatchOutcome::MatchedDisabledOnly(&disabled_rule)
    );
    assert_eq!(
        match_rule(".gitignore", &rules),
        RuleMatchOutcome::Unmatched
    );
    assert_eq!(match_rule("file.", &rules), RuleMatchOutcome::Unmatched);
    assert_eq!(match_rule("README", &rules), RuleMatchOutcome::Unmatched);
    assert_eq!(
        extract_candidate_extensions("report.final.v2.pdf"),
        vec!["final.v2.pdf", "v2.pdf", "pdf"]
    );
    assert_eq!(extract_candidate_extensions(".env.local"), vec!["local"]);

    // Collision reservation across existing destination file + flattened nested files (case-insensitive).
    write_test_file(&downloads.join("Tarballs/bundle.tar.gz"), b"existing").unwrap();
    write_test_file(&downloads.join("bundle.tar.gz"), b"src1").unwrap();
    write_test_file(&downloads.join("Sub/BUNDLE.tar.gz"), b"src2").unwrap();
    write_test_file(&downloads.join(".gitignore"), b"git").unwrap();
    write_test_file(&downloads.join("item.xyz"), b"xyz").unwrap();

    let folder = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: Some(true),
            max_depth: Some(3),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )
    .unwrap();

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(summary.conflict_renamed_count, 2);

    let renamed_page = services::planner::get_plan_page(
        &state,
        PlanPageRequest {
            plan_id: summary.plan_id.clone(),
            offset: 0,
            limit: 10,
            status_filter: Some(OperationStatus::ConflictRenamed),
            skip_reason_filter: None,
            search: Some("bundle".to_owned()),
        },
    )
    .unwrap();
    let proposed: Vec<&str> = renamed_page
        .items
        .iter()
        .filter_map(|op| op.proposed_filename.as_deref())
        .collect();
    assert!(proposed.contains(&"bundle (1).tar.gz"));
    assert!(proposed.contains(&"BUNDLE (2).tar.gz"));
}

#[test]
fn paging_depth_limit_cancellation_and_scan_limit_behave_correctly() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();
    write_test_file(&downloads.join("a.png"), b"1").unwrap();
    write_test_file(&downloads.join("b.png"), b"2").unwrap();
    write_test_file(&downloads.join("L1/c.png"), b"3").unwrap();
    write_test_file(&downloads.join("L1/L2/deep.png"), b"4").unwrap();

    let folder = services::folders::add_watched_folder(
        &state,
        AddWatchedFolderRequest {
            path: downloads.to_string_lossy().into_owned(),
            recursive: Some(true),
            max_depth: Some(2),
            min_file_age_secs: Some(5),
            allow_network_root: None,
        },
    )
    .unwrap();

    let rules = services::rules::list_rules(&state).unwrap().rules;

    // Depth limit stops descending at depth 2 (`L1/L2` -> DepthLimit).
    let plan = build_dry_run_plan(
        &folder,
        &rules,
        1,
        &state.protected_roots,
        state.fs.as_ref(),
        &CancelToken::new(),
        ScanConfig::default(),
        1_000,
        DEFAULT_PLAN_TTL_MS,
    )
    .unwrap();
    assert_eq!(plan.summary.eligible_count, 3);
    assert!(plan
        .operations
        .iter()
        .any(|op| op.skip_reason == Some(SkipReason::DepthLimit)));

    // Paging slice verification.
    let page1 = page_plan(
        &plan,
        &PlanPageRequest {
            plan_id: plan.summary.plan_id.clone(),
            offset: 0,
            limit: 2,
            status_filter: Some(OperationStatus::Eligible),
            skip_reason_filter: None,
            search: None,
        },
    );
    assert_eq!(page1.total_matching, 3);
    assert_eq!(page1.items.len(), 2);

    // Pre-cancelled token aborts scan cleanly.
    let cancel = CancelToken::new();
    cancel.cancel();
    let cancel_err = build_dry_run_plan(
        &folder,
        &rules,
        1,
        &state.protected_roots,
        state.fs.as_ref(),
        &cancel,
        ScanConfig::default(),
        1_000,
        DEFAULT_PLAN_TTL_MS,
    )
    .unwrap_err();
    assert_eq!(cancel_err.code(), "CANCELLED");

    // Scan limit guard triggers `ScanLimit`.
    let limited = build_dry_run_plan(
        &folder,
        &rules,
        1,
        &state.protected_roots,
        state.fs.as_ref(),
        &CancelToken::new(),
        ScanConfig { max_entries: 2 },
        1_000,
        DEFAULT_PLAN_TTL_MS,
    )
    .unwrap();
    assert!(limited
        .operations
        .iter()
        .any(|op| op.skip_reason == Some(SkipReason::ScanLimit)));

    assert_eq!(ALL_SKIP_REASONS.len(), 32);
    let _ = Rule {
        id: 0,
        name: String::new(),
        destination: String::new(),
        extensions: vec![],
        enabled: false,
        priority: 1,
        is_default: false,
        default_key: None,
        name_contains: None,
        min_size_bytes: None,
        max_size_bytes: None,
        min_age_days: None,
        created_at: 0,
        updated_at: 0,
    };
}

#[test]
fn smart_rules_date_templates_and_storage_analyzer_work_safely() {
    let (_sandbox, state, downloads) = setup_sandbox_env().unwrap();

    // Unsupported template placeholder must be rejected on creation.
    let bad_tpl = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Bad Template".to_owned(),
            destination: "Photos/{bogus}/".to_owned(),
            extensions: vec!["png".to_owned()],
            enabled: true,
            name_contains: None,
            min_size_bytes: None,
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap_err();
    assert_eq!(bad_tpl.code(), "INVALID_INPUT");

    // Create smart rule matching "Screenshot" PNGs into `Screenshots/{year}/{month}/`.
    let screenshot_rule = services::rules::create_rule(
        &state,
        CreateRuleRequest {
            name: "Screenshots".to_owned(),
            destination: "Screenshots/{year}/{month}/".to_owned(),
            extensions: vec!["png".to_owned()],
            enabled: true,
            name_contains: Some("Screenshot".to_owned()),
            min_size_bytes: Some(10),
            max_size_bytes: None,
            min_age_days: None,
        },
    )
    .unwrap();

    // Place Screenshots rule at priority #1 (above Images) -> must NOT emit false overlap warning.
    let rules = services::rules::list_rules(&state).unwrap().rules;
    let mut ids: Vec<i64> = rules
        .iter()
        .filter(|r| r.id != screenshot_rule.id)
        .map(|r| r.id)
        .collect();
    ids.insert(0, screenshot_rule.id);
    let reordered = services::rules::reorder_rules(
        &state,
        sortdeck_lib::models::ReorderRulesRequest {
            ordered_rule_ids: ids,
        },
    )
    .unwrap();
    assert!(
        !reordered.overlaps.iter().any(|w| w.extension == "png"),
        "filtered top rule should not emit false overlap warning"
    );

    write_test_file(
        &downloads.join("Screenshot 2026-11.png"),
        b"screenshot-bytes-over-10",
    )
    .unwrap();
    write_test_file(
        &downloads.join("duplicate-a.pdf"),
        b"exact-same-pdf-payload",
    )
    .unwrap();
    write_test_file(
        &downloads.join("duplicate-b.pdf"),
        b"exact-same-pdf-payload",
    )
    .unwrap();

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

    let summary = services::planner::create_dry_run_plan(&state, folder.id).unwrap();
    assert_eq!(summary.eligible_count, 3);

    let page = services::planner::get_plan_page(
        &state,
        PlanPageRequest {
            plan_id: summary.plan_id.clone(),
            offset: 0,
            limit: 50,
            status_filter: None,
            skip_reason_filter: None,
            search: Some("Screenshot".to_owned()),
        },
    )
    .unwrap();
    assert_eq!(page.items.len(), 1);
    let shot_dest = page.items[0].destination_path.as_deref().unwrap();
    assert!(shot_dest.contains("Screenshots\\20") || shot_dest.contains("Screenshots/20"));
    assert!(!shot_dest.contains("{year}"));

    // Storage analyzer must be read-only and detect exact duplicate group for the two PDFs.
    let snap_before = snapshot_tree(&downloads).unwrap();
    let report = services::planner::analyze_folder_storage(&state, folder.id).unwrap();
    let snap_after = snapshot_tree(&downloads).unwrap();
    assert_eq!(snap_before, snap_after);

    assert_eq!(report.total_files, 3);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.duplicate_groups[0].paths.len(), 2);
    assert_eq!(
        report.duplicate_groups[0].wasted_bytes,
        b"exact-same-pdf-payload".len() as u64
    );
}

proptest! {
    #[test]
    fn prop_collision_candidates_preserve_compound_and_simple_extensions(
        stem in "[A-Za-z0-9_-]{1,12}",
        attempt in 1u32..500u32
    ) {
        let compound = format!("{stem}.tar.gz");
        let cand = collision_candidate(&compound, attempt);
        prop_assert_eq!(cand, format!("{stem} ({attempt}).tar.gz"));

        let simple = format!("{stem}.PNG");
        let (s, e) = split_stem_and_extension(&simple);
        prop_assert_eq!(s, stem.as_str());
        prop_assert_eq!(e, ".PNG");
    }

    #[test]
    fn prop_candidate_extensions_are_lowercase_dotless_and_longest_first(
        stem in "[A-Za-z0-9]{1,8}",
        mid in "[A-Za-z0-9]{1,5}",
        leaf in "[A-Za-z0-9]{1,5}"
    ) {
        let name = format!("{stem}.{mid}.{leaf}");
        let cands = extract_candidate_extensions(&name);
        prop_assert_eq!(cands.len(), 2);
        prop_assert!(cands[0].len() > cands[1].len());
        prop_assert_eq!(&cands[0], &format!("{}.{}", mid.to_lowercase(), leaf.to_lowercase()));
        prop_assert_eq!(&cands[1], &leaf.to_lowercase());
    }
}
