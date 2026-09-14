use std::path::{Path, PathBuf};

use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::sandbox::{SandboxFsBackend, SandboxGuard, StubFsBackend};
use sortdeck_lib::fsops::FsBackend;

struct TestSandbox {
    dir: tempfile::TempDir,
}

impl TestSandbox {
    fn new() -> Result<Self, AppError> {
        let dir = tempfile::tempdir().map_err(|err| AppError::from_io("create test dir", err))?;
        SandboxGuard::init_sandbox_dir(dir.path())?;
        Ok(Self { dir })
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn child(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }
}

#[test]
fn sandbox_guard_refuses_temp_dir_without_sentinel() {
    let unsandboxed = tempfile::tempdir().unwrap();
    let target = unsandboxed.path().join("file.txt");

    let err = SandboxGuard::validate_path(&target).unwrap_err();
    assert_eq!(err.code(), "SANDBOX_VIOLATION");

    let backend = SandboxFsBackend::new(StubFsBackend);
    let backend_err = backend.exists(&target).unwrap_err();
    assert_eq!(backend_err.code(), "SANDBOX_VIOLATION");
}

#[test]
fn sandbox_guard_refuses_parent_traversal_escape() {
    let sandbox = TestSandbox::new().unwrap();
    let escaped = sandbox
        .root()
        .join("nested")
        .join("..")
        .join("..")
        .join("outside.txt");

    let err = SandboxGuard::validate_path(&escaped).unwrap_err();
    assert_eq!(err.code(), "SANDBOX_VIOLATION");
}

#[test]
fn sandbox_guard_refuses_relative_paths() {
    let relative = std::path::Path::new("Downloads/photo.png");

    let err = SandboxGuard::validate_path(relative).unwrap_err();
    assert_eq!(err.code(), "SANDBOX_VIOLATION");
}

#[test]
fn sandbox_backend_refuses_move_when_either_endpoint_is_outside_sandbox() {
    let sandbox = TestSandbox::new().unwrap();
    let unsandboxed = tempfile::tempdir().unwrap();

    let inside = sandbox.child("inside.txt");
    let outside = unsandboxed.path().join("outside.txt");

    let backend = SandboxFsBackend::new(StubFsBackend);

    let err_dst = backend.move_no_overwrite(&inside, &outside).unwrap_err();
    assert_eq!(err_dst.code(), "SANDBOX_VIOLATION");

    let err_src = backend.move_no_overwrite(&outside, &inside).unwrap_err();
    assert_eq!(err_src.code(), "SANDBOX_VIOLATION");
}

#[test]
fn sandbox_backend_allows_paths_strictly_inside_sandbox() {
    let sandbox = TestSandbox::new().unwrap();
    let src = sandbox.child("Downloads/image.png");
    let dst = sandbox.child("Downloads/Images/image.png");

    let backend = SandboxFsBackend::new(StubFsBackend);

    assert!(!backend.exists(&src).unwrap());
    assert!(backend.move_no_overwrite(&src, &dst).is_ok());
}
