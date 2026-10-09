#![forbid(unsafe_code)]

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::engine::paths::{
    self, contains_excluded_dir_component, is_project_root, is_protected_location,
    normalize_absolute_path, NormalizedPath, ProtectedRoots, EXCLUDED_DIR_NAMES,
};
use crate::errors::AppError;
use crate::fsops::{CancelToken, FileAttributes, FileFingerprint, FsBackend};
use crate::models::{Rule, SkipReason, WatchedFolder};

/// Temporary and active download extensions that SortDeck never moves.
pub const TEMP_DOWNLOAD_EXTENSIONS: [&str; 15] = [
    "crdownload",
    "part",
    "partial",
    "tmp",
    "temp",
    "opdownload",
    "download",
    "downloading",
    "!ut",
    "!qb",
    "aria2",
    "ytdl",
    "filepart",
    "swp",
    "lock",
];

/// Special OS/SortDeck marker filenames always skipped.
pub const SPECIAL_FILENAMES: [&str; 4] = ["desktop.ini", "thumbs.db", ".ds_store", ".sortignore"];

/// Default maximum entries per scan.
pub const DEFAULT_SCAN_LIMIT: usize = 1_000_000;

/// Check cancellation token at least every N entries.
pub const CANCEL_CHECK_INTERVAL: usize = 256;

/// Configurable bounds for read-only directory scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanConfig {
    pub max_entries: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_SCAN_LIMIT,
        }
    }
}

/// A single discovered file alongside its read-only metadata and optional pre-rule skip reason.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub path_display: String,
    pub parent_key: String,
    pub file_name: String,
    pub size: u64,
    pub attributes: FileAttributes,
    pub fingerprint: FileFingerprint,
    pub pre_skip_reason: Option<SkipReason>,
}

/// Output of a read-only directory scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub folder_norm: NormalizedPath,
    pub files: Vec<ScannedFile>,
    pub warnings: Vec<String>,
    pub sortignore_detected: bool,
    pub total_discovered: u64,
}

/// Executes a strictly read-only scan of `folder`, enforcing all hardcoded
/// safety exclusions.
pub fn scan_watched_folder(
    folder: &WatchedFolder,
    rules: &[Rule],
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
    cancel: &CancelToken,
    config: ScanConfig,
) -> Result<ScanResult, AppError> {
    let folder_norm = normalize_absolute_path(&folder.path)?;

    if !fs.exists(&folder_norm.path)? {
        return Err(AppError::NotFound {
            resource: folder_norm.display,
            os_error_code: None,
        });
    }

    let root_attrs = fs.attributes(&folder_norm.path)?;
    if root_attrs.is_reparse_point {
        return Err(AppError::PathValidation {
            path: folder_norm.display,
            reason: "watched folder is a reparse point".to_owned(),
        });
    }
    if !root_attrs.is_dir {
        return Err(AppError::PathValidation {
            path: folder_norm.display,
            reason: "watched folder is not a directory".to_owned(),
        });
    }
    if is_protected_location(&folder_norm, roots) {
        return Err(AppError::PathValidation {
            path: folder_norm.display,
            reason: "watched folder is a protected location".to_owned(),
        });
    }
    if contains_excluded_dir_component(&folder_norm) || is_project_root(&folder_norm.path, fs)? {
        return Err(AppError::PathValidation {
            path: folder_norm.display,
            reason: "watched folder is a project root or excluded directory".to_owned(),
        });
    }

    let mut warnings = Vec::new();
    let mut sortignore_detected = false;

    if has_sortignore_in_self_or_ancestors(&folder_norm.path, fs)? {
        sortignore_detected = true;
        warnings.push(format!(
            "Directory '{}' or an ancestor contains.sortignore; no files will be sorted.",
            folder_norm.display
        ));
        return Ok(ScanResult {
            folder_norm,
            files: Vec::new(),
            warnings,
            sortignore_detected,
            total_discovered: 0,
        });
    }

    let managed_dest_keys = collect_managed_destination_keys(&folder_norm, rules);
    let effective_max_depth = if folder.recursive {
        folder.max_depth.clamp(1, 32)
    } else {
        1
    };

    let mut files = Vec::new();
    let mut queue: VecDeque<(NormalizedPath, u32)> = VecDeque::new();
    queue.push_back((folder_norm.clone(), 1));

    let mut inspected_entries: usize = 0;
    let mut hit_scan_limit = false;

    while let Some((dir_norm, depth)) = queue.pop_front() {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }

        let entries = match fs.read_dir(&dir_norm.path) {
            Ok(entries) => entries,
            Err(err) => {
                warnings.push(format!(
                    "Unable to read directory '{}': {err}",
                    dir_norm.display
                ));
                continue;
            }
        };

        // Pre-collect Office lock owner files (`~$<name>`) in this directory (Â§4.3.5).
        let office_locks = collect_office_lock_targets(&entries);

        for entry in entries {
            inspected_entries += 1;
            if inspected_entries.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }

            if inspected_entries > config.max_entries {
                hit_scan_limit = true;
                warnings.push(format!(
                    "Scan limit of {} entries reached; remaining entries were skipped.",
                    config.max_entries
                ));
                let entry_norm_str = entry.path.to_string_lossy().into_owned();
                files.push(ScannedFile {
                    path: entry.path,
                    path_display: entry_norm_str,
                    parent_key: dir_norm.key.clone(),
                    file_name: entry.file_name,
                    size: entry.size,
                    attributes: entry.attributes,
                    fingerprint: FileFingerprint {
                        volume_serial: None,
                        file_id: None,
                        size: entry.size,
                        modified_epoch_ms: entry.modified_epoch_ms,
                    },
                    pre_skip_reason: Some(SkipReason::ScanLimit),
                });
                break;
            }

            if entry.is_non_unicode {
                files.push(ScannedFile {
                    path: entry.path.clone(),
                    path_display: entry.path.to_string_lossy().into_owned(),
                    parent_key: dir_norm.key.clone(),
                    file_name: entry.file_name,
                    size: entry.size,
                    attributes: entry.attributes,
                    fingerprint: FileFingerprint {
                        volume_serial: None,
                        file_id: None,
                        size: entry.size,
                        modified_epoch_ms: entry.modified_epoch_ms,
                    },
                    pre_skip_reason: Some(SkipReason::NonUnicodePath),
                });
                continue;
            }

            let entry_raw = match entry.path.to_str() {
                Some(s) => s,
                None => {
                    files.push(ScannedFile {
                        path: entry.path.clone(),
                        path_display: entry.path.to_string_lossy().into_owned(),
                        parent_key: dir_norm.key.clone(),
                        file_name: entry.file_name,
                        size: entry.size,
                        attributes: entry.attributes,
                        fingerprint: FileFingerprint {
                            volume_serial: None,
                            file_id: None,
                            size: entry.size,
                            modified_epoch_ms: entry.modified_epoch_ms,
                        },
                        pre_skip_reason: Some(SkipReason::NonUnicodePath),
                    });
                    continue;
                }
            };

            let entry_norm = match normalize_absolute_path(entry_raw) {
                Ok(n) => n,
                Err(_) => {
                    files.push(ScannedFile {
                        path: entry.path.clone(),
                        path_display: entry_raw.to_owned(),
                        parent_key: dir_norm.key.clone(),
                        file_name: entry.file_name,
                        size: entry.size,
                        attributes: entry.attributes,
                        fingerprint: FileFingerprint {
                            volume_serial: None,
                            file_id: None,
                            size: entry.size,
                            modified_epoch_ms: entry.modified_epoch_ms,
                        },
                        pre_skip_reason: Some(SkipReason::InvalidName),
                    });
                    continue;
                }
            };

            if entry.attributes.is_dir {
                handle_subdirectory(
                    &entry_norm,
                    &entry.file_name,
                    entry.attributes,
                    depth,
                    effective_max_depth,
                    folder.recursive,
                    &managed_dest_keys,
                    roots,
                    fs,
                    &mut queue,
                    &mut files,
                    &mut warnings,
                    &mut sortignore_detected,
                );
                continue;
            }

            let fingerprint = match fs.fingerprint(&entry_norm.path) {
                Ok(fp) => fp,
                Err(_) => FileFingerprint {
                    volume_serial: None,
                    file_id: None,
                    size: entry.size,
                    modified_epoch_ms: entry.modified_epoch_ms,
                },
            };

            let pre_skip_reason = classify_file_safety(
                &entry_norm,
                &entry.file_name,
                entry.attributes,
                &office_locks,
                roots,
            );

            files.push(ScannedFile {
                path: entry_norm.path,
                path_display: entry_norm.display,
                parent_key: dir_norm.key.clone(),
                file_name: entry.file_name,
                size: fingerprint.size,
                attributes: entry.attributes,
                fingerprint,
                pre_skip_reason,
            });
        }

        if hit_scan_limit {
            break;
        }
    }

    let total_discovered = u64::try_from(files.len()).unwrap_or(u64::MAX);
    Ok(ScanResult {
        folder_norm,
        files,
        warnings,
        sortignore_detected,
        total_discovered,
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_subdirectory(
    dir_norm: &NormalizedPath,
    dir_name: &str,
    attrs: FileAttributes,
    current_depth: u32,
    effective_max_depth: u32,
    is_recursive: bool,
    managed_dest_keys: &HashSet<String>,
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
    queue: &mut VecDeque<(NormalizedPath, u32)>,
    files: &mut Vec<ScannedFile>,
    warnings: &mut Vec<String>,
    sortignore_detected: &mut bool,
) {
    // Managed destination directories are never re-scanned (Â§4.4).
    if managed_dest_keys.contains(&dir_norm.key) {
        return;
    }

    if !is_recursive {
        return;
    }

    if attrs.is_reparse_point {
        warnings.push(format!(
            "Skipped reparse-point directory '{}'.",
            dir_norm.display
        ));
        return;
    }

    if is_protected_location(dir_norm, roots) {
        warnings.push(format!(
            "Skipped protected directory '{}'.",
            dir_norm.display
        ));
        return;
    }

    if EXCLUDED_DIR_NAMES.contains(&dir_name.to_lowercase().as_str()) {
        return;
    }

    if is_project_root(&dir_norm.path, fs).unwrap_or(false) {
        warnings.push(format!(
            "Skipped version-controlled project root '{}'.",
            dir_norm.display
        ));
        return;
    }

    if fs
        .exists(&dir_norm.path.join(".sortignore"))
        .unwrap_or(false)
    {
        *sortignore_detected = true;
        warnings.push(format!(
            "Skipped directory '{}' due to.sortignore.",
            dir_norm.display
        ));
        return;
    }

    if current_depth >= effective_max_depth {
        files.push(ScannedFile {
            path: dir_norm.path.clone(),
            path_display: dir_norm.display.clone(),
            parent_key: dir_norm.key.clone(),
            file_name: dir_name.to_owned(),
            size: 0,
            attributes: attrs,
            fingerprint: FileFingerprint {
                volume_serial: None,
                file_id: None,
                size: 0,
                modified_epoch_ms: None,
            },
            pre_skip_reason: Some(SkipReason::DepthLimit),
        });
        return;
    }

    queue.push_back((dir_norm.clone(), current_depth + 1));
}

fn has_sortignore_in_self_or_ancestors(
    watched_root: &Path,
    fs: &dyn FsBackend,
) -> Result<bool, AppError> {
    for (idx, ancestor) in watched_root.ancestors().enumerate() {
        let marker = ancestor.join(".sortignore");
        match fs.exists(&marker) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(AppError::SandboxViolation { .. }) if idx > 0 => {
                // Stop ancestor probing once we cross above a test sandbox root (Â§13.1).
                break;
            }
            Err(err) => return Err(err),
        }
    }
    Ok(false)
}

fn collect_managed_destination_keys(
    folder_norm: &NormalizedPath,
    rules: &[Rule],
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for rule in rules.iter().filter(|r| r.enabled) {
        let base_template = strip_template_suffix(&rule.destination);
        if !base_template.is_empty() {
            if let Some(dest_norm) = resolve_destination_dir(folder_norm, &base_template) {
                keys.insert(dest_norm.key);
            }
        }
    }
    keys
}

fn strip_template_suffix(destination: &str) -> String {
    if !destination.contains('{') {
        return destination.to_owned();
    }
    let normalized = destination.replace('\\', "/");
    let mut static_parts = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment.contains('{') || segment.contains('}') {
            break;
        }
        static_parts.push(segment);
    }
    if static_parts.is_empty() {
        String::new()
    } else {
        format!("{}/", static_parts.join("/"))
    }
}

/// Resolves a rule destination (relative to `folder_norm` or absolute) into a [`NormalizedPath`].
#[must_use]
pub fn resolve_destination_dir(
    folder_norm: &NormalizedPath,
    rule_destination: &str,
) -> Option<NormalizedPath> {
    let is_abs = rule_destination.starts_with(r"\\")
        || rule_destination.starts_with("//")
        || rule_destination.as_bytes().get(1) == Some(&b':');

    if is_abs {
        normalize_absolute_path(rule_destination).ok()
    } else {
        let joined = folder_norm.path.join(rule_destination.replace('/', "\\"));
        let joined_str = joined.to_str()?;
        normalize_absolute_path(joined_str).ok()
    }
}

fn collect_office_lock_targets(entries: &[crate::fsops::FsEntry]) -> HashSet<String> {
    let mut targets = HashSet::new();
    for entry in entries {
        if !entry.attributes.is_dir {
            if let Some(target) = entry.file_name.strip_prefix("~$") {
                if !target.is_empty() {
                    targets.insert(target.to_lowercase());
                }
            }
        }
    }
    targets
}

/// Evaluates file-level safety exclusionsÂ§4.3.5 and Â§4.3.6.
#[must_use]
pub fn classify_file_safety(
    file_norm: &NormalizedPath,
    file_name: &str,
    attrs: FileAttributes,
    office_lock_targets: &HashSet<String>,
    roots: &ProtectedRoots,
) -> Option<SkipReason> {
    if is_protected_location(file_norm, roots) {
        return Some(SkipReason::ProtectedLocation);
    }

    if contains_excluded_dir_component(file_norm) {
        return Some(SkipReason::ExcludedDirectory);
    }

    if attrs.is_reparse_point {
        return Some(SkipReason::ReparsePoint);
    }
    if attrs.is_hidden {
        return Some(SkipReason::HiddenAttribute);
    }
    if attrs.is_system {
        return Some(SkipReason::SystemAttribute);
    }
    if attrs.is_offline_placeholder {
        return Some(SkipReason::CloudPlaceholder);
    }

    let lower_name = file_name.to_lowercase();
    if SPECIAL_FILENAMES.contains(&lower_name.as_str()) || is_sortdeck_temp_file(&lower_name) {
        return Some(SkipReason::SpecialFile);
    }

    if lower_name.starts_with("~$") {
        return Some(SkipReason::OfficeLockFile);
    }

    if is_office_locked(&lower_name, office_lock_targets) {
        return Some(SkipReason::FileInUse);
    }

    if has_temp_download_extension(&lower_name) {
        return Some(SkipReason::TemporaryExtension);
    }

    if paths::validate_path_segment(file_name, &file_norm.display).is_err() {
        return Some(SkipReason::InvalidName);
    }

    None
}

fn is_sortdeck_temp_file(lower_name: &str) -> bool {
    lower_name.starts_with(".sortdeck-") && lower_name.ends_with(".sdpart")
}

fn is_office_locked(lower_name: &str, lock_targets: &HashSet<String>) -> bool {
    if lock_targets.contains(lower_name) {
        return true;
    }
    // Word/PowerPoint/Excel sometimes omit the first 2 characters in `~$` lock names
    // for long filenames (e.g., `~$port.docx` for `report.docx`).
    if lower_name.len() > 2 {
        for target in lock_targets {
            if target.len() > 2 && lower_name.ends_with(target) {
                return true;
            }
        }
    }
    false
}

fn has_temp_download_extension(lower_name: &str) -> bool {
    if let Some(last_dot) = lower_name.rfind('.') {
        if last_dot > 0 && last_dot < lower_name.len() - 1 {
            let ext = &lower_name[last_dot + 1..];
            return TEMP_DOWNLOAD_EXTENSIONS.contains(&ext);
        }
    }
    false
}
