use std::path::{Path, PathBuf};

use proptest::prelude::*;
use sortdeck_lib::engine::paths::{
    contains_excluded_dir_component, is_protected_location, normalize_absolute_path,
    validate_rule_destination, validate_watched_folder_path, ProtectedRoots,
};
use sortdeck_lib::engine::rules::normalize_extension;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::local::LocalFsBackend;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard, StubFsBackend};
use sortdeck_lib::fsops::{
    CancelToken, FileAttributes, FileFingerprint, FsBackend, FsEntry, SameVolumeMoveOutcome,
    VolumeInfo,
};

struct ReparseStubFs {
    reparse: bool,
    is_dir: bool,
}

impl FsBackend for ReparseStubFs {
    fn exists(&self, path: &Path) -> Result<bool, AppError> {
        let is_vcs_marker = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| matches!(n, ".git" | ".hg" | ".svn"));
        Ok(!is_vcs_marker)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, AppError> {
        Ok(path.to_path_buf())
    }

    fn attributes(&self, _path: &Path) -> Result<FileAttributes, AppError> {
        Ok(FileAttributes {
            is_dir: self.is_dir,
            is_reparse_point: self.reparse,
            ..FileAttributes::default()
        })
    }

    fn fingerprint(&self, _path: &Path) -> Result<FileFingerprint, AppError> {
        unreachable!()
    }

    fn read_dir(&self, _dir: &Path) -> Result<Vec<FsEntry>, AppError> {
        Ok(Vec::new())
    }

    fn move_no_overwrite(
        &self,
        _src: &Path,
        _dst: &Path,
    ) -> Result<SameVolumeMoveOutcome, AppError> {
        unreachable!()
    }

    fn copy_to_temp(&self, _src: &Path, _dst: &Path, _c: &CancelToken) -> Result<(), AppError> {
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

    fn volume_info(&self, _path: &Path) -> Result<VolumeInfo, AppError> {
        unreachable!()
    }

    fn check_access_for_move(&self, _path: &Path) -> Result<(), AppError> {
        unreachable!()
    }
}

fn sample_protected_roots() -> ProtectedRoots {
    ProtectedRoots::from_explicit(
        vec![
            PathBuf::from(r"C:\Windows"),
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"C:\Program Files (x86)"),
            PathBuf::from(r"C:\ProgramData"),
            PathBuf::from(r"C:\Users\Alice\AppData"),
            PathBuf::from(r"D:\CustomAppData\Local"),
            PathBuf::from(r"D:\SortDeckData"),
            PathBuf::from(r"D:\SortDeckData\logs"),
        ],
        Some(PathBuf::from(r"C:\Users\Alice")),
    )
}

#[test]
fn inv_06_protected_locations_are_rejected() {
    let roots = sample_protected_roots();
    let fs = StubFsBackend;

    for protected in [
        r"C:\Windows",
        r"C:\WINDOWS\System32",
        r"C:\Program Files\Vendor",
        r"C:\Program Files (x86)\App",
        r"C:\ProgramData\SortDeck",
        r"C:\Users\Alice\AppData\Local\Temp",
        r"D:\CustomAppData\Local\Cache",
        r"D:\SortDeckData",
        r"D:\SortDeckData\logs",
        r"D:\$RECYCLE.BIN\S-1-5-21",
        r"E:\System Volume Information",
    ] {
        let norm = normalize_absolute_path(protected).unwrap();
        assert!(
            is_protected_location(&norm, &roots),
            "expected {protected} to be protected"
        );

        let err = validate_watched_folder_path(protected, false, &roots, &fs).unwrap_err();
        assert_eq!(err.code(), "PATH_VALIDATION");
    }
}

#[test]
fn inv_06_similar_non_protected_user_directories_are_allowed() {
    let roots = sample_protected_roots();

    for allowed in [
        r"D:\Projects\Windows",
        r"D:\Projects\Windows\Docs",
        r"C:\Users\Alice\Projects\ProgramFilesDocs",
        r"C:\WindowsBackup",
    ] {
        let norm = normalize_absolute_path(allowed).unwrap();
        assert!(
            !is_protected_location(&norm, &roots),
            "did not expect {allowed} to be marked protected"
        );
    }
}

#[test]
fn inv_06_excluded_dir_components_and_project_roots_are_rejected() {
    let roots = sample_protected_roots();

    for excluded in [
        r"D:\Workspace\.git\objects",
        r"D:\Workspace\.HG",
        r"D:\Workspace\.svn",
        r"D:\Workspace\app\NODE_MODULES\pkg",
        r"D:\Workspace\app\.venv",
        r"D:\Workspace\app\venv\Lib",
        r"D:\Workspace\app\__pycache__",
    ] {
        let norm = normalize_absolute_path(excluded).unwrap();
        assert!(
            contains_excluded_dir_component(&norm),
            "expected {excluded} to match excluded dir component"
        );
    }

    // Substring match must NOT trigger excluded dir rejection.
    let not_excluded = normalize_absolute_path(r"D:\Workspace\my_node_modules_notes").unwrap();
    assert!(!contains_excluded_dir_component(&not_excluded));

    // Real sandbox project root containing `.git` must be refused.
    let dir = tempfile::tempdir().unwrap();
    SandboxGuard::init_sandbox_dir(dir.path()).unwrap();
    let project_dir = dir.path().join("MyRepo");
    std::fs::create_dir_all(project_dir.join(".git")).unwrap();

    let sandbox_fs = SandboxFsBackend::new(LocalFsBackend);
    let err =
        validate_watched_folder_path(project_dir.to_str().unwrap(), false, &roots, &sandbox_fs)
            .unwrap_err();
    assert_eq!(err.code(), "PATH_VALIDATION");
}

#[test]
fn inv_06_disallowed_watched_folders_and_reparse_points_are_rejected() {
    let roots = sample_protected_roots();
    let normal_dir_fs = ReparseStubFs {
        reparse: false,
        is_dir: true,
    };

    // Drive root & user profile root
    for disallowed in [r"C:\", r"D:\", r"C:\Users\Alice"] {
        let err =
            validate_watched_folder_path(disallowed, false, &roots, &normal_dir_fs).unwrap_err();
        assert_eq!(err.code(), "PATH_VALIDATION");
    }

    // UNC root disallowed without explicit confirmation, allowed with confirmation
    let unc_err =
        validate_watched_folder_path(r"\\nas\media", false, &roots, &normal_dir_fs).unwrap_err();
    assert_eq!(unc_err.code(), "PATH_VALIDATION");
    assert!(validate_watched_folder_path(r"\\nas\media", true, &roots, &normal_dir_fs).is_ok());

    // Reparse-point directory rejected
    let reparse_fs = ReparseStubFs {
        reparse: true,
        is_dir: true,
    };
    let reparse_err =
        validate_watched_folder_path(r"D:\Downloads\Junction", false, &roots, &reparse_fs)
            .unwrap_err();
    assert_eq!(reparse_err.code(), "PATH_VALIDATION");
}

#[test]
fn path_validation_rejects_windows_edge_cases() {
    let roots = sample_protected_roots();
    let fs = StubFsBackend;
    let long_component = format!(r"D:\{}", "a".repeat(256));

    let bad_paths = [
        "",
        "   ",
        r"relative\folder",
        r"C:drive_relative",
        r"\rooted_without_drive",
        r"\\.\PhysicalDrive0",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1",
        r"D:\Folders\..\Windows",
        r"D:\Folders\CON",
        r"D:\Folders\nul.txt",
        r"D:\Folders\COM1",
        r"D:\Folders\lpt9.pdf",
        r"D:\Folders\trailingdot.",
        r"D:\Folders\trailingspace ",
        r"D:\Folders\bad|name",
        r"D:\Folders\bad?name",
        r"D:\Folders\file.txt:Zone.Identifier",
        "D:\\Folders\\ctrl\x07name",
        long_component.as_str(),
    ];

    for bad in bad_paths {
        let res = normalize_absolute_path(bad);
        assert!(res.is_err(), "expected '{bad}' to be rejected");
    }

    // Relative rule destination traversal & reserved names
    for bad_dest in [
        "",
        r"..\Windows",
        r"Sub\..\Images",
        r"\Rooted",
        "CON",
        "folder.",
        "ads:stream",
        r"C:\Windows\System32",
        r"C:\",
        ".git",
    ] {
        assert!(
            validate_rule_destination(bad_dest, &roots, &[], &fs).is_err(),
            "expected destination '{bad_dest}' to be rejected"
        );
    }
}

proptest! {
    #[test]
    fn prop_valid_drive_paths_normalize_idempotently(
        seg1 in "[A-Za-z0-9_-]{1,16}",
        seg2 in "[A-Za-z0-9_-]{1,16}"
    ) {
        prop_assume!(!matches!(seg1.to_ascii_uppercase().as_str(), "CON" | "PRN" | "AUX" | "NUL"));
        prop_assume!(!matches!(seg2.to_ascii_uppercase().as_str(), "CON" | "PRN" | "AUX" | "NUL"));
        let raw = format!(r"d:\{seg1}\{seg2}\");
        let first = normalize_absolute_path(&raw).unwrap();
        let second = normalize_absolute_path(&first.display).unwrap();
        prop_assert_eq!(first.display, second.display);
        prop_assert_eq!(first.key, second.key);
    }

    #[test]
    fn prop_reserved_dos_names_always_rejected(
        idx in 0usize..22,
        ext in prop::option::of("[a-z]{1,4}")
    ) {
        let names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        let leaf = match ext {
            Some(e) => format!("{}.{e}", names[idx]),
            None => names[idx].to_owned(),
        };
        let path = format!(r"D:\SafeRoot\{leaf}");
        prop_assert!(normalize_absolute_path(&path).is_err());
    }

    #[test]
    fn prop_parent_traversal_always_rejected(seg in "[a-zA-Z0-9_]{1,10}") {
        let path = format!(r"D:\{seg}\..\other");
        prop_assert!(normalize_absolute_path(&path).is_err());
    }

    #[test]
    fn prop_extension_normalization_is_idempotent(
        leading_dot in prop::bool::ANY,
        upper in prop::bool::ANY,
        ext in "[a-z0-9]{1,8}"
    ) {
        let cased = if upper { ext.to_uppercase() } else { ext };
        let raw = if leading_dot { format!(" .{cased} ") } else { format!(" {cased} ") };
        let first = normalize_extension(&raw).unwrap();
        let second = normalize_extension(&first).unwrap();
        prop_assert_eq!(&first, &second);
        prop_assert!(!first.starts_with('.'));
        prop_assert_eq!(&first, &first.to_lowercase());
    }
}
