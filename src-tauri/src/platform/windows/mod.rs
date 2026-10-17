//! Windows-specific Win32 FFI boundary (AGENTS.md §2.5).
//!
//! All `unsafe` Win32 calls and RAII handle wrappers live exclusively in this
//! module and its submodules. Every `unsafe` block MUST carry a `// SAFETY:`
//! comment documenting the invariants relied upon.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BAD_NETPATH,
    ERROR_BAD_NET_NAME, ERROR_CANT_ACCESS_FILE, ERROR_DISK_FULL, ERROR_FILENAME_EXCED_RANGE,
    ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_HANDLE_DISK_FULL, ERROR_INVALID_NAME,
    ERROR_LOCK_VIOLATION, ERROR_NETWORK_UNREACHABLE, ERROR_NOT_SAME_DEVICE, ERROR_PATH_NOT_FOUND,
    ERROR_REQUEST_ABORTED, ERROR_SHARING_VIOLATION, ERROR_UNEXP_NET_ERR, HANDLE,
    INVALID_HANDLE_VALUE, RPC_E_CHANGED_MODE, S_FALSE, S_OK,
};
use windows_sys::Win32::Storage::FileSystem::{
    CopyFileExW, CreateFileW, DeleteFileW, FlushFileBuffers, GetDiskFreeSpaceExW,
    GetFileInformationByHandle, GetVolumeInformationW, GetVolumePathNameW, MoveFileExW,
    RemoveDirectoryW, BY_HANDLE_FILE_INFORMATION, COPYPROGRESSROUTINE_PROGRESS,
    COPY_FILE_FAIL_IF_EXISTS, DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA,
    LPPROGRESS_ROUTINE_CALLBACK_REASON, OPEN_EXISTING, PROGRESS_CANCEL, PROGRESS_CONTINUE,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::errors::AppError;
use crate::fsops::{CancelToken, FileFingerprint, SameVolumeMoveOutcome, VolumeInfo};

const FILE_NAMED_STREAMS_FLAG: u32 = 0x0004_0000;
const FILETIME_UNIX_EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;

/// RAII wrapper around a Win32 `HANDLE` that closes the handle on drop.
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn from_raw(handle: HANDLE, context_path: &Path) -> Result<Self, AppError> {
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            // SAFETY: GetLastError reads the calling thread's last-error slot immediately after CreateFileW failed.
            let code = unsafe { GetLastError() };
            return Err(map_win32_error(code, context_path, "CreateFileW"));
        }
        Ok(Self(handle))
    }

    const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            // SAFETY: `self.0` is an open Win32 handle owned exclusively by this `OwnedHandle`.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// Converts an absolute Windows path into a null-terminated UTF-16 `\\?\`
/// extended-length path buffer so paths >260 chars work reliably (§1.3, §4.6.1).
#[must_use]
pub fn to_extended_wide(path: &Path) -> Vec<u16> {
    // Win32 verbatim `\\?\` paths disable path normalization and reject `/`,
    // so normalize forward slashes before applying the verbatim prefix.
    let raw_str = path.to_string_lossy().replace('/', "\\");
    let extended = if raw_str.starts_with(r"\\?\") {
        raw_str
    } else if let Some(unc_tail) = raw_str.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc_tail}")
    } else {
        format!(r"\\?\{raw_str}")
    };

    std::ffi::OsStr::new(&extended)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Maps a Win32 error code into SortDeck's typed [`AppError`] preserving the
/// raw OS error code (`AGENTS.md` §4.7).
#[must_use]
pub fn map_win32_error(code: u32, path: &Path, context: &str) -> AppError {
    let os_code = i32::try_from(code).ok();
    let path_str = path.display().to_string();

    match code {
        ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS => AppError::AlreadyExists {
            path: path_str,
            os_error_code: os_code,
        },
        ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION => AppError::FileInUse {
            path: path_str,
            os_error_code: os_code,
        },
        ERROR_ACCESS_DENIED => AppError::PermissionDenied {
            resource: path_str,
            os_error_code: os_code,
        },
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => AppError::NotFound {
            resource: path_str,
            os_error_code: os_code,
        },
        ERROR_DISK_FULL | ERROR_HANDLE_DISK_FULL => AppError::DiskFull {
            path: path_str,
            os_error_code: os_code,
        },
        ERROR_FILENAME_EXCED_RANGE | ERROR_INVALID_NAME => AppError::PathValidation {
            path: path_str,
            reason: format!("Windows rejected path/filename (Win32 error {code})"),
        },
        ERROR_REQUEST_ABORTED => AppError::Cancelled,
        ERROR_CANT_ACCESS_FILE => AppError::CloudFileError {
            path: path_str,
            os_error_code: os_code,
        },
        ERROR_NETWORK_UNREACHABLE
        | ERROR_BAD_NETPATH
        | ERROR_UNEXP_NET_ERR
        | ERROR_BAD_NET_NAME => AppError::NetworkUnavailable {
            path: path_str,
            os_error_code: os_code,
        },
        _ => AppError::Io {
            context: format!("{context} ({path_str})"),
            os_error_code: os_code,
            source: std::io::Error::from_raw_os_error(os_code.unwrap_or(-1)),
        },
    }
}

/// Performs a race-safe same-volume move without overwriting any existing file.
///
/// Calls `MoveFileExW(src, dst, 0)` (no `MOVEFILE_REPLACE_EXISTING`, no
/// `MOVEFILE_COPY_ALLOWED`).
pub fn move_file_no_overwrite(src: &Path, dst: &Path) -> Result<SameVolumeMoveOutcome, AppError> {
    let src_w = to_extended_wide(src);
    let dst_w = to_extended_wide(dst);

    // SAFETY: `src_w` and `dst_w` are valid null-terminated UTF-16 path strings.
    let ok = unsafe { MoveFileExW(src_w.as_ptr(), dst_w.as_ptr(), 0) };
    if ok != 0 {
        return Ok(SameVolumeMoveOutcome::Moved);
    }

    // SAFETY: GetLastError reads the calling thread's error code immediately after MoveFileExW failure.
    let code = unsafe { GetLastError() };
    match code {
        ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS => Ok(SameVolumeMoveOutcome::AlreadyExists),
        ERROR_NOT_SAME_DEVICE => Ok(SameVolumeMoveOutcome::CrossVolumeRequired),
        _ => Err(map_win32_error(code, dst, "MoveFileExW")),
    }
}

unsafe extern "system" fn copy_progress_callback(
    _total_file_size: i64,
    _total_bytes_transferred: i64,
    _stream_size: i64,
    _stream_bytes_transferred: i64,
    _dw_stream_number: u32,
    _dw_callback_reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _h_source_file: HANDLE,
    _h_destination_file: HANDLE,
    lp_data: *const c_void,
) -> COPYPROGRESSROUTINE_PROGRESS {
    if lp_data.is_null() {
        return PROGRESS_CONTINUE;
    }
    // SAFETY: `lp_data` points to a live `&CancelToken` for the synchronous duration of `CopyFileExW`.
    let cancel = unsafe { &*lp_data.cast::<CancelToken>() };
    if cancel.is_cancelled() {
        PROGRESS_CANCEL
    } else {
        PROGRESS_CONTINUE
    }
}

/// Copies `src` to `temp_dst` using `CopyFileExW` with `COPY_FILE_FAIL_IF_EXISTS`,
/// preserving Alternate Data Streams (`Zone.Identifier`), attributes, and timestamps,
/// and honoring cancellation via the progress callback.
pub fn copy_file_no_overwrite(
    src: &Path,
    temp_dst: &Path,
    cancel: &CancelToken,
) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    let src_w = to_extended_wide(src);
    let dst_w = to_extended_wide(temp_dst);
    let data_ptr = (&raw const *cancel).cast::<c_void>();

    // SAFETY: `src_w` and `dst_w` are valid null-terminated UTF-16 strings; `data_ptr`
    // points to `cancel` which outlives the synchronous `CopyFileExW` call; `pbcancel` is null.
    let ok = unsafe {
        CopyFileExW(
            src_w.as_ptr(),
            dst_w.as_ptr(),
            Some(copy_progress_callback),
            data_ptr,
            ptr::null_mut(),
            COPY_FILE_FAIL_IF_EXISTS,
        )
    };

    if ok != 0 {
        return Ok(());
    }

    // SAFETY: GetLastError reads the calling thread's last error after CopyFileExW failure.
    let code = unsafe { GetLastError() };
    if code == ERROR_REQUEST_ABORTED || cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    Err(map_win32_error(code, temp_dst, "CopyFileExW"))
}

/// Flushes buffered writes on `path` to disk using `FlushFileBuffers`.
pub fn flush_file_buffers(path: &Path) -> Result<(), AppError> {
    let path_w = to_extended_wide(path);
    // SAFETY: `path_w` is a valid null-terminated UTF-16 string.
    let handle = unsafe {
        CreateFileW(
            path_w.as_ptr(),
            FILE_WRITE_DATA,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    let owned = OwnedHandle::from_raw(handle, path)?;

    // SAFETY: `owned.raw()` is a valid open handle with `FILE_WRITE_DATA` access.
    let ok = unsafe { FlushFileBuffers(owned.raw()) };
    if ok == 0 {
        // SAFETY: GetLastError reads the calling thread's last error after FlushFileBuffers failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "FlushFileBuffers"));
    }
    Ok(())
}

/// Queries volume serial, 64-bit NTFS/ReFS file ID, exact file size, and mtime
/// via `GetFileInformationByHandle`.
pub fn get_file_identity(path: &Path) -> Result<FileFingerprint, AppError> {
    let path_w = to_extended_wide(path);
    // SAFETY: `path_w` is a valid null-terminated UTF-16 string; desired access is 0 (metadata-only).
    let handle = unsafe {
        CreateFileW(
            path_w.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    let owned = OwnedHandle::from_raw(handle, path)?;

    let mut info: BY_HANDLE_FILE_INFORMATION =
        // SAFETY: `BY_HANDLE_FILE_INFORMATION` is a plain Win32 C struct of integers/FILETIMEs.
        unsafe { std::mem::zeroed() };
    // SAFETY: `owned.raw()` is a valid handle and `info` points to properly aligned writable memory.
    let ok = unsafe { GetFileInformationByHandle(owned.raw(), &raw mut info) };
    if ok == 0 {
        // SAFETY: GetLastError reads the calling thread's last error after GetFileInformationByHandle failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "GetFileInformationByHandle"));
    }

    let size = (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow);
    let file_index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    let file_id = if file_index == 0 {
        None
    } else {
        Some(format!("{file_index:016x}"))
    };

    let mtime_100ns = (u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
        | u64::from(info.ftLastWriteTime.dwLowDateTime);
    let modified_epoch_ms = mtime_100ns
        .checked_sub(FILETIME_UNIX_EPOCH_DIFF_100NS)
        .and_then(|ticks| i64::try_from(ticks / 10_000).ok());

    Ok(FileFingerprint {
        volume_serial: Some(u64::from(info.dwVolumeSerialNumber)),
        file_id,
        size,
        modified_epoch_ms,
    })
}

/// Checks advisory move/delete readiness by opening `path` with `DELETE | FILE_GENERIC_READ`
/// and `FILE_SHARE_READ` only (`AGENTS.md` §4.5 #4).
pub fn check_access_for_move(path: &Path) -> Result<(), AppError> {
    let path_w = to_extended_wide(path);
    // SAFETY: `path_w` is a valid null-terminated UTF-16 string.
    let handle = unsafe {
        CreateFileW(
            path_w.as_ptr(),
            DELETE | FILE_GENERIC_READ,
            FILE_SHARE_READ,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    let _owned = OwnedHandle::from_raw(handle, path)?;
    Ok(())
}

/// Deletes a file using `DeleteFileW` without altering attributes.
pub fn delete_file_strict(path: &Path) -> Result<(), AppError> {
    let path_w = to_extended_wide(path);
    // SAFETY: `path_w` is a valid null-terminated UTF-16 string.
    let ok = unsafe { DeleteFileW(path_w.as_ptr()) };
    if ok == 0 {
        // SAFETY: GetLastError reads the calling thread's last error after DeleteFileW failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "DeleteFileW"));
    }
    Ok(())
}

/// Removes an empty directory using `RemoveDirectoryW`.
pub fn remove_empty_dir_strict(path: &Path) -> Result<(), AppError> {
    let path_w = to_extended_wide(path);
    // SAFETY: `path_w` is a valid null-terminated UTF-16 string.
    let ok = unsafe { RemoveDirectoryW(path_w.as_ptr()) };
    if ok == 0 {
        // SAFETY: GetLastError reads the calling thread's last error after RemoveDirectoryW failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "RemoveDirectoryW"));
    }
    Ok(())
}

/// Queries volume capabilities and free space via `GetVolumePathNameW`,
/// `GetVolumeInformationW`, and `GetDiskFreeSpaceExW` (`AGENTS.md` §4.6.5).
pub fn get_volume_info(path: &Path) -> Result<VolumeInfo, AppError> {
    let path_w = to_extended_wide(path);
    let mut vol_root_buf = vec![0u16; 1024];

    // SAFETY: `path_w` is valid null-terminated UTF-16 and `vol_root_buf` holds `vol_root_buf.len()` UTF-16 elements.
    let got_root = unsafe {
        GetVolumePathNameW(
            path_w.as_ptr(),
            vol_root_buf.as_mut_ptr(),
            u32::try_from(vol_root_buf.len()).unwrap_or(1024),
        )
    };
    if got_root == 0 {
        // SAFETY: GetLastError reads the calling thread's error after GetVolumePathNameW failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "GetVolumePathNameW"));
    }

    let mut serial: u32 = 0;
    let mut max_comp: u32 = 255;
    let mut flags: u32 = 0;
    let mut fs_name_buf = [0u16; 64];

    // SAFETY: `vol_root_buf` is null-terminated by `GetVolumePathNameW`; output pointers point to live locals.
    let got_info = unsafe {
        GetVolumeInformationW(
            vol_root_buf.as_ptr(),
            ptr::null_mut(),
            0,
            &raw mut serial,
            &raw mut max_comp,
            &raw mut flags,
            fs_name_buf.as_mut_ptr(),
            u32::try_from(fs_name_buf.len()).unwrap_or(64),
        )
    };
    if got_info == 0 {
        // SAFETY: GetLastError reads the calling thread's error after GetVolumeInformationW failure.
        let code = unsafe { GetLastError() };
        return Err(map_win32_error(code, path, "GetVolumeInformationW"));
    }

    let fs_len = fs_name_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(fs_name_buf.len());
    let filesystem_name = String::from_utf16_lossy(&fs_name_buf[..fs_len]);
    let upper_fs = filesystem_name.to_ascii_uppercase();
    let supports_file_ids = matches!(upper_fs.as_str(), "NTFS" | "REFS");
    let supports_ads = (flags & FILE_NAMED_STREAMS_FLAG) != 0;

    let mut free_available: u64 = 0;
    // SAFETY: `vol_root_buf` is null-terminated; `free_available` is writable u64.
    let got_space = unsafe {
        GetDiskFreeSpaceExW(
            vol_root_buf.as_ptr(),
            &raw mut free_available,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    let available_bytes = if got_space != 0 {
        Some(free_available)
    } else {
        None
    };

    Ok(VolumeInfo {
        volume_serial: Some(u64::from(serial)),
        filesystem_name,
        supports_file_ids,
        supports_ads,
        max_component_len: max_comp,
        available_bytes,
    })
}

fn to_shell_wide(path: &Path) -> Vec<u16> {
    let simplified = dunce::simplified(path).to_string_lossy().replace('/', "\\");
    std::ffi::OsStr::new(&simplified)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

struct ComApartmentGuard(bool);

impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: COM was initialized by `CoInitializeEx` on this thread in this scope.
            unsafe {
                CoUninitialize();
            }
        }
    }
}

struct PidlGuard(*mut ITEMIDLIST);

impl Drop for PidlGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is an allocated PIDL returned by `ILCreateFromPathW`.
            unsafe {
                ILFree(self.0);
            }
        }
    }
}

/// Reveals a validated file or folder in Windows Explorer without executing it (`AGENTS.md` §9.1).
pub fn reveal_in_explorer(path: &Path) -> Result<(), AppError> {
    let path_wide = to_shell_wide(path);

    // SAFETY: `CoInitializeEx` initializes STA COM for Shell API calls on the current thread.
    let hr_com = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let should_uninit = hr_com == S_OK || hr_com == S_FALSE;
    if !should_uninit && hr_com != RPC_E_CHANGED_MODE {
        return Err(AppError::Internal {
            message: format!("COM initialization failed with HRESULT 0x{hr_com:08X}"),
        });
    }
    let _com_guard = ComApartmentGuard(should_uninit);

    // SAFETY: `path_wide` is a null-terminated UTF-16 string; returned PIDL is freed by `PidlGuard`.
    let pidl = unsafe { ILCreateFromPathW(path_wide.as_ptr()) };
    if pidl.is_null() {
        return Err(AppError::NotFound {
            resource: dunce::simplified(path).display().to_string(),
            os_error_code: None,
        });
    }
    let _pidl_guard = PidlGuard(pidl);

    // SAFETY: `pidl` is a valid non-null absolute PIDL owned until `_pidl_guard` drops.
    let hr_open = unsafe { SHOpenFolderAndSelectItems(pidl, 0, ptr::null(), 0) };
    if hr_open < 0 {
        return Err(AppError::Internal {
            message: format!("SHOpenFolderAndSelectItems failed with HRESULT 0x{hr_open:08X}"),
        });
    }

    Ok(())
}

/// Opens a validated directory (such as SortDeck's logs folder) in Windows Explorer.
pub fn open_directory_in_explorer(dir: &Path) -> Result<(), AppError> {
    let verb_wide: Vec<u16> = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dir_wide = to_shell_wide(dir);

    // SAFETY: Both `verb_wide` and `dir_wide` are null-terminated UTF-16 strings referencing a validated directory.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb_wide.as_ptr(),
            dir_wide.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) <= 32 {
        return Err(AppError::from_io(
            format!("open folder {}", dunce::simplified(dir).display()),
            std::io::Error::other(format!("ShellExecuteW returned code {}", result as isize)),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_string_adds_extended_prefix_for_drive_and_unc_paths() {
        let drive = to_extended_wide(Path::new(r"C:\Users\Alice\file.txt"));
        assert_eq!(drive.last(), Some(&0));
        let drive_str = String::from_utf16_lossy(&drive[..drive.len() - 1]);
        assert_eq!(drive_str, r"\\?\C:\Users\Alice\file.txt");

        let unc = to_extended_wide(Path::new(r"\\nas\share\docs\a.pdf"));
        let unc_str = String::from_utf16_lossy(&unc[..unc.len() - 1]);
        assert_eq!(unc_str, r"\\?\UNC\nas\share\docs\a.pdf");

        let already = to_extended_wide(Path::new(r"\\?\D:\Long\path"));
        let already_str = String::from_utf16_lossy(&already[..already.len() - 1]);
        assert_eq!(already_str, r"\\?\D:\Long\path");

        let mixed = to_extended_wide(Path::new(r"C:\Temp\Downloads/Images/photo.png"));
        let mixed_str = String::from_utf16_lossy(&mixed[..mixed.len() - 1]);
        assert_eq!(mixed_str, r"\\?\C:\Temp\Downloads\Images\photo.png");
    }

    #[test]
    fn win32_error_mapping_preserves_codes_and_outcomes() {
        let path = Path::new(r"D:\Sandbox\file.txt");

        let sharing = map_win32_error(ERROR_SHARING_VIOLATION, path, "test");
        assert_eq!(sharing.code(), "FILE_IN_USE");
        assert_eq!(sharing.os_error_code(), Some(32));

        let full = map_win32_error(ERROR_DISK_FULL, path, "test");
        assert_eq!(full.code(), "DISK_FULL");
        assert_eq!(full.os_error_code(), Some(112));

        let cloud = map_win32_error(ERROR_CANT_ACCESS_FILE, path, "test");
        assert_eq!(cloud.code(), "CLOUD_FILE_ERROR");
        assert_eq!(cloud.os_error_code(), Some(1920));

        let net = map_win32_error(ERROR_BAD_NETPATH, path, "test");
        assert_eq!(net.code(), "NETWORK_UNAVAILABLE");
        assert_eq!(net.os_error_code(), Some(53));

        let denied = map_win32_error(ERROR_ACCESS_DENIED, path, "test");
        assert_eq!(denied.code(), "PERMISSION_DENIED");
        assert_eq!(denied.os_error_code(), Some(5));
    }
}
