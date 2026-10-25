#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::fsops::FsBackend;
use crate::models::{Rule, WatchedFolder};

/// Excluded directory component names matched exact-component, case-insensitively (AGENTS.md §4.3.2).
pub const EXCLUDED_DIR_NAMES: [&str; 7] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
];

/// VCS marker directories identifying a project root (AGENTS.md §4.3.3).
pub const PROJECT_ROOT_MARKERS: [&str; 3] = [".git", ".hg", ".svn"];

const RESERVED_DOS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

const INVALID_SEGMENT_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Canonicalized, validated path carrying both display string and case-folded
/// comparison key (`/`-separated lowercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedPath {
    pub display: String,
    pub key: String,
    pub path: PathBuf,
    pub is_unc: bool,
}

/// Set of protected system and application roots resolved at runtime (AGENTS.md §4.3.1).
#[derive(Debug, Clone)]
pub struct ProtectedRoots {
    root_keys: Vec<String>,
    user_profile_key: Option<String>,
}

impl ProtectedRoots {
    /// Resolves protected OS roots from Windows environment variables and SortDeck's
    /// own data/log directories (never hardcoding drive letters).
    #[must_use]
    pub fn from_system_env(data_dir: &Path) -> Self {
        let env_vars = [
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
            "LOCALAPPDATA",
            "APPDATA",
        ];

        let mut raw_roots: Vec<PathBuf> = Vec::new();
        for key in env_vars {
            if let Some(val) = std::env::var_os(key) {
                let path = PathBuf::from(val);
                if !path.as_os_str().is_empty() {
                    raw_roots.push(path);
                }
            }
        }

        let user_profile = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty());
        if let Some(ref profile) = user_profile {
            raw_roots.push(profile.join("AppData"));
        }

        raw_roots.push(data_dir.to_path_buf());
        raw_roots.push(data_dir.join("logs"));

        Self::from_explicit(raw_roots, user_profile)
    }

    /// Builds a [`ProtectedRoots`] policy from explicit paths (primarily for tests).
    #[must_use]
    pub fn from_explicit(roots: Vec<PathBuf>, user_profile: Option<PathBuf>) -> Self {
        let mut root_keys = Vec::new();
        for root in roots {
            if let Some(raw) = root.to_str() {
                if let Ok(norm) = normalize_absolute_path(raw) {
                    if !root_keys.contains(&norm.key) {
                        root_keys.push(norm.key);
                    }
                }
            }
        }

        let user_profile_key = user_profile
            .as_deref()
            .and_then(Path::to_str)
            .and_then(|raw| normalize_absolute_path(raw).ok())
            .map(|norm| norm.key);

        Self {
            root_keys,
            user_profile_key,
        }
    }

    #[must_use]
    pub fn root_keys(&self) -> &[String] {
        &self.root_keys
    }

    #[must_use]
    pub fn user_profile_key(&self) -> Option<&str> {
        self.user_profile_key.as_deref()
    }
}

/// Validates a single Windows path segment (file or directory name).
pub fn validate_path_segment(segment: &str, context_path: &str) -> Result<(), AppError> {
    if segment.is_empty() {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "empty path component is not allowed".to_owned(),
        });
    }

    if segment == "." || segment == ".." {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "'.' and '..' path traversal components are not allowed".to_owned(),
        });
    }

    if segment.chars().any(char::is_control) {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "control characters are not allowed in path names".to_owned(),
        });
    }

    if segment.encode_utf16().count() > 255 {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "path component exceeds 255 UTF-16 code units".to_owned(),
        });
    }

    if segment.ends_with('.') || segment.ends_with(' ') {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "path component must not end with a dot or space".to_owned(),
        });
    }

    if segment
        .chars()
        .any(|ch| INVALID_SEGMENT_CHARS.contains(&ch))
    {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: "path component contains invalid characters or ADS ':' syntax".to_owned(),
        });
    }

    let stem = segment.split('.').next().unwrap_or(segment);
    let upper_stem = stem.to_ascii_uppercase();
    if RESERVED_DOS_NAMES.contains(&upper_stem.as_str()) {
        return Err(AppError::PathValidation {
            path: context_path.to_owned(),
            reason: format!("reserved Windows device name '{stem}' is not allowed"),
        });
    }

    Ok(())
}

/// Normalizes and validates an absolute Windows path (drive-letter or UNC) once
/// at the backend boundary (`AGENTS.md` §4.6.1).
pub fn normalize_absolute_path(raw: &str) -> Result<NormalizedPath, AppError> {
    if raw.trim().is_empty() {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "path must not be empty".to_owned(),
        });
    }

    if raw.chars().any(char::is_control) {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "path must not contain control characters".to_owned(),
        });
    }

    let upper_prefix = raw.get(..14).unwrap_or(raw).to_ascii_uppercase();
    if raw.starts_with(r"\\.\")
        || raw.starts_with("//./")
        || upper_prefix.starts_with(r"\\?\GLOBALROOT")
        || upper_prefix.starts_with("//?/GLOBALROOT")
    {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "device and GLOBALROOT paths are forbidden".to_owned(),
        });
    }

    let normalized_slashes = raw.replace('/', "\\");
    let stripped = if let Some(rest) = normalized_slashes.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = normalized_slashes.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        normalized_slashes
    };

    if let Some(unc_rest) = stripped.strip_prefix(r"\\") {
        normalize_unc_path(raw, unc_rest)
    } else {
        normalize_drive_path(raw, &stripped)
    }
}

fn normalize_unc_path(raw: &str, unc_rest: &str) -> Result<NormalizedPath, AppError> {
    let parts: Vec<&str> = unc_rest.split('\\').collect();
    // Require at least server and share; trim trailing empty segment only.
    let mut trimmed = parts;
    while trimmed.len() > 2 && trimmed.last() == Some(&"") {
        trimmed.pop();
    }

    if trimmed.len() < 2 {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "UNC path must include both server and share".to_owned(),
        });
    }

    let server = trimmed[0];
    let share = trimmed[1];
    validate_path_segment(server, raw)?;
    validate_path_segment(share, raw)?;

    for seg in &trimmed[2..] {
        validate_path_segment(seg, raw)?;
    }

    let display_tail = trimmed.join("\\");
    let display = format!(r"\\{display_tail}");
    let key_segments: Vec<String> = trimmed.iter().map(|s| s.to_lowercase()).collect();
    let key = format!("//{}", key_segments.join("/"));
    let path = PathBuf::from(&display);

    Ok(NormalizedPath {
        display,
        key,
        path,
        is_unc: true,
    })
}

fn normalize_drive_path(raw: &str, stripped: &str) -> Result<NormalizedPath, AppError> {
    let mut chars = stripped.chars();
    let Some(drive) = chars.next() else {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "path must be absolute".to_owned(),
        });
    };

    if !drive.is_ascii_alphabetic() || chars.next() != Some(':') || chars.next() != Some('\\') {
        return Err(AppError::PathValidation {
            path: raw.to_owned(),
            reason: "path must be an absolute Windows drive or UNC path".to_owned(),
        });
    }

    let drive_upper = drive.to_ascii_uppercase();
    let drive_lower = drive.to_ascii_lowercase();
    let remainder = &stripped[3..];

    let mut segments: Vec<&str> = if remainder.is_empty() {
        Vec::new()
    } else {
        remainder.split('\\').collect()
    };

    while segments.last() == Some(&"") {
        segments.pop();
    }

    for seg in &segments {
        validate_path_segment(seg, raw)?;
    }

    let display = if segments.is_empty() {
        format!("{drive_upper}:\\")
    } else {
        format!("{drive_upper}:\\{}", segments.join("\\"))
    };

    let key = if segments.is_empty() {
        format!("{drive_lower}:/")
    } else {
        let lower_segs: Vec<String> = segments.iter().map(|s| s.to_lowercase()).collect();
        format!("{drive_lower}:/{}", lower_segs.join("/"))
    };

    let path = PathBuf::from(&display);

    Ok(NormalizedPath {
        display,
        key,
        path,
        is_unc: false,
    })
}

/// Returns true if `normalized` points directly to a drive root (`C:\`) or UNC share root (`\\server\share`).
#[must_use]
pub fn is_volume_or_share_root(normalized: &NormalizedPath) -> bool {
    if normalized.is_unc {
        // `//server/share` has 2 non-empty segments after `//`.
        normalized.key.trim_start_matches('/').split('/').count() <= 2
    } else {
        // `c:/` ends with `:/`.
        normalized.key.ends_with(":/")
    }
}

/// Component-boundary prefix comparison on normalized keys (never substring).
#[must_use]
pub fn path_key_is_prefix_or_equal(ancestor_key: &str, candidate_key: &str) -> bool {
    if ancestor_key == candidate_key {
        return true;
    }

    if ancestor_key.ends_with(":/") {
        return candidate_key.starts_with(ancestor_key);
    }

    candidate_key
        .strip_prefix(ancestor_key)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// Returns true if `normalized` is inside any hardcoded or environment-resolved
/// protected location (`AGENTS.md` §4.3.1).
#[must_use]
pub fn is_protected_location(normalized: &NormalizedPath, roots: &ProtectedRoots) -> bool {
    if roots
        .root_keys
        .iter()
        .any(|root_key| path_key_is_prefix_or_equal(root_key, &normalized.key))
    {
        return true;
    }

    is_volume_special_protected_dir(&normalized.key)
}

fn is_volume_special_protected_dir(key: &str) -> bool {
    let first_dir = if let Some(rest) = key.strip_prefix("//") {
        // UNC: //server/share/<first_dir>/...
        rest.split('/').nth(2)
    } else {
        // Drive: c:/<first_dir>/...
        key.split('/').nth(1)
    };

    matches!(
        first_dir,
        Some("$recycle.bin" | "system volume information")
    )
}

/// Returns true if any component of `normalized` matches an excluded directory
/// name (`AGENTS.md` §4.3.2) case-insensitively and by exact component match.
#[must_use]
pub fn contains_excluded_dir_component(normalized: &NormalizedPath) -> bool {
    let tail = if let Some(rest) = normalized.key.strip_prefix("//") {
        rest.split('/').skip(2).collect::<Vec<_>>()
    } else {
        normalized.key.split('/').skip(1).collect::<Vec<_>>()
    };

    tail.into_iter()
        .any(|component| EXCLUDED_DIR_NAMES.contains(&component))
}

/// Returns true if `dir` directly contains `.git`, `.hg`, or `.svn` (`AGENTS.md` §4.3.3).
pub fn is_project_root(dir: &Path, fs: &dyn FsBackend) -> Result<bool, AppError> {
    for marker in PROJECT_ROOT_MARKERS {
        if fs.exists(&dir.join(marker))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Full backend validation for adding a watched folder (`AGENTS.md` §4.3.4, §4.6).
pub fn validate_watched_folder_path(
    raw: &str,
    allow_network_root: bool,
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
) -> Result<NormalizedPath, AppError> {
    let initial = normalize_absolute_path(raw)?;
    validate_folder_policy(&initial, allow_network_root, roots)?;

    if !fs.exists(&initial.path)? {
        return Err(AppError::NotFound {
            resource: initial.display,
            os_error_code: None,
        });
    }

    let attrs = fs.attributes(&initial.path)?;
    if attrs.is_reparse_point {
        return Err(AppError::PathValidation {
            path: initial.display,
            reason: "reparse-point directories (symlinks/junctions) cannot be watched".to_owned(),
        });
    }
    if !attrs.is_dir {
        return Err(AppError::PathValidation {
            path: initial.display,
            reason: "watched folder path must be a directory".to_owned(),
        });
    }

    // Resolve 8.3 short names, mapped/subst drives, and parent reparse points via canonicalization.
    let canonical_path = fs.canonicalize(&initial.path)?;
    let canonical_str = canonical_path
        .to_str()
        .ok_or_else(|| AppError::PathValidation {
            path: initial.display.clone(),
            reason: "path is not valid Unicode".to_owned(),
        })?;
    let canonical = normalize_absolute_path(canonical_str)?;
    validate_folder_policy(&canonical, allow_network_root, roots)?;

    let canonical_attrs = fs.attributes(&canonical.path)?;
    if canonical_attrs.is_reparse_point {
        return Err(AppError::PathValidation {
            path: canonical.display,
            reason: "canonical path is a reparse point".to_owned(),
        });
    }

    if is_project_root(&canonical.path, fs)? {
        return Err(AppError::PathValidation {
            path: canonical.display,
            reason: "project roots containing .git/.hg/.svn cannot be watched".to_owned(),
        });
    }

    Ok(canonical)
}

fn validate_folder_policy(
    norm: &NormalizedPath,
    allow_network_root: bool,
    roots: &ProtectedRoots,
) -> Result<(), AppError> {
    if !norm.is_unc && is_volume_or_share_root(norm) {
        return Err(AppError::PathValidation {
            path: norm.display.clone(),
            reason: "drive root directories cannot be watched".to_owned(),
        });
    }

    if norm.is_unc && is_volume_or_share_root(norm) && !allow_network_root {
        return Err(AppError::PathValidation {
            path: norm.display.clone(),
            reason: "UNC share roots require explicit advanced confirmation".to_owned(),
        });
    }

    if let Some(profile_key) = roots.user_profile_key() {
        if norm.key == profile_key {
            return Err(AppError::PathValidation {
                path: norm.display.clone(),
                reason: "user profile root cannot be watched".to_owned(),
            });
        }
    }

    if is_protected_location(norm, roots) {
        return Err(AppError::PathValidation {
            path: norm.display.clone(),
            reason: "protected system or application locations cannot be watched".to_owned(),
        });
    }

    if contains_excluded_dir_component(norm) {
        return Err(AppError::PathValidation {
            path: norm.display.clone(),
            reason: "path lies inside an excluded directory".to_owned(),
        });
    }

    Ok(())
}

/// Ensures a candidate watched folder does not overlap existing watched folders
/// or rule destinations in a loop-causing manner (`AGENTS.md` §4.4.3).
pub fn validate_folder_no_overlap(
    candidate_key: &str,
    existing_folders: &[WatchedFolder],
    rules: &[Rule],
) -> Result<(), AppError> {
    for folder in existing_folders {
        if candidate_key == folder.path_key {
            return Err(AppError::Conflict {
                message: format!("folder '{}' is already watched", folder.path),
            });
        }
        if path_key_is_prefix_or_equal(&folder.path_key, candidate_key)
            || path_key_is_prefix_or_equal(candidate_key, &folder.path_key)
        {
            return Err(AppError::Conflict {
                message: format!(
                    "watched folder overlaps existing watched folder '{}'",
                    folder.path
                ),
            });
        }
    }

    for rule in rules {
        if let Ok(dest_norm) = normalize_absolute_path(&rule.destination) {
            if dest_norm.key == candidate_key
                || path_key_is_prefix_or_equal(&dest_norm.key, candidate_key)
            {
                return Err(AppError::Conflict {
                    message: format!(
                        "watched folder overlaps absolute destination of rule '{}'",
                        rule.name
                    ),
                });
            }
        }
    }

    Ok(())
}

/// Validates and normalizes a rule destination (relative to watched root or validated absolute path)
/// per `AGENTS.md` §4.3.8, §4.4.3, and §6.4.
pub fn validate_rule_destination(
    raw_dest: &str,
    roots: &ProtectedRoots,
    watched_folders: &[WatchedFolder],
    fs: &dyn FsBackend,
) -> Result<String, AppError> {
    let trimmed_input = raw_dest.trim();
    if trimmed_input.is_empty() {
        return Err(AppError::InvalidInput {
            message: "rule destination must not be empty".to_owned(),
        });
    }

    if raw_dest.chars().any(char::is_control) {
        return Err(AppError::PathValidation {
            path: raw_dest.to_owned(),
            reason: "rule destination must not contain control characters".to_owned(),
        });
    }

    // Check if the destination looks like an absolute or rooted/drive-prefixed path.
    let is_unc_like = raw_dest.starts_with(r"\\") || raw_dest.starts_with("//");
    let has_drive_prefix = raw_dest.as_bytes().get(1) == Some(&b':');
    let starts_with_single_sep =
        !is_unc_like && (raw_dest.starts_with('\\') || raw_dest.starts_with('/'));

    if starts_with_single_sep {
        return Err(AppError::PathValidation {
            path: raw_dest.to_owned(),
            reason: "rooted relative destinations without a drive or UNC prefix are not allowed"
                .to_owned(),
        });
    }

    if is_unc_like || has_drive_prefix {
        return validate_absolute_destination(raw_dest, roots, watched_folders, fs);
    }

    validate_relative_destination(raw_dest)
}

fn validate_relative_destination(raw_dest: &str) -> Result<String, AppError> {
    let normalized = raw_dest.replace('\\', "/");
    let mut segments: Vec<&str> = normalized.split('/').collect();
    while segments.last() == Some(&"") {
        segments.pop();
    }

    if segments.is_empty() {
        return Err(AppError::InvalidInput {
            message: "rule destination must contain at least one folder name".to_owned(),
        });
    }

    for seg in &segments {
        validate_relative_template_segment(seg, raw_dest)?;
        if EXCLUDED_DIR_NAMES.contains(&seg.to_lowercase().as_str()) {
            return Err(AppError::PathValidation {
                path: raw_dest.to_owned(),
                reason: format!("excluded directory name '{seg}' cannot be a rule destination"),
            });
        }
    }

    Ok(format!("{}/", segments.join("/")))
}

fn validate_relative_template_segment(segment: &str, context_path: &str) -> Result<(), AppError> {
    if segment.contains('{') || segment.contains('}') {
        let mut cursor = segment;
        while let Some(open) = cursor.find('{') {
            let after_open = &cursor[open + 1..];
            let Some(close) = after_open.find('}') else {
                return Err(AppError::InvalidInput {
                    message: format!("unclosed placeholder in destination '{context_path}'"),
                });
            };
            let token = &after_open[..close];
            if !matches!(token, "year" | "month" | "day") {
                return Err(AppError::InvalidInput {
                    message: format!(
                        "unsupported destination placeholder '{{{token}}}'; supported placeholders are {{year}}, {{month}}, {{day}}"
                    ),
                });
            }
            cursor = &after_open[close + 1..];
        }
        if cursor.contains('}') {
            return Err(AppError::InvalidInput {
                message: format!("unmatched '}}' in destination '{context_path}'"),
            });
        }
        let concrete = segment
            .replace("{year}", "2026")
            .replace("{month}", "11")
            .replace("{day}", "24");
        return validate_path_segment(&concrete, context_path);
    }

    validate_path_segment(segment, context_path)
}

fn validate_absolute_destination(
    raw_dest: &str,
    roots: &ProtectedRoots,
    watched_folders: &[WatchedFolder],
    fs: &dyn FsBackend,
) -> Result<String, AppError> {
    let norm = normalize_absolute_path(raw_dest)?;
    if is_volume_or_share_root(&norm) {
        return Err(AppError::PathValidation {
            path: norm.display,
            reason: "volume or share root cannot be a rule destination".to_owned(),
        });
    }
    if let Some(profile_key) = roots.user_profile_key() {
        if norm.key == profile_key {
            return Err(AppError::PathValidation {
                path: norm.display,
                reason: "user profile root cannot be a rule destination".to_owned(),
            });
        }
    }
    if is_protected_location(&norm, roots) {
        return Err(AppError::PathValidation {
            path: norm.display,
            reason: "protected location cannot be a rule destination".to_owned(),
        });
    }
    if contains_excluded_dir_component(&norm) {
        return Err(AppError::PathValidation {
            path: norm.display,
            reason: "excluded directory cannot be a rule destination".to_owned(),
        });
    }

    for folder in watched_folders {
        if norm.key == folder.path_key || path_key_is_prefix_or_equal(&norm.key, &folder.path_key) {
            return Err(AppError::Conflict {
                message: format!(
                    "rule destination '{}' overlaps watched folder '{}'",
                    norm.display, folder.path
                ),
            });
        }
    }

    if fs.exists(&norm.path)? {
        let attrs = fs.attributes(&norm.path)?;
        if attrs.is_reparse_point || !attrs.is_dir {
            return Err(AppError::PathValidation {
                path: norm.display,
                reason: "existing rule destination must be a non-reparse directory".to_owned(),
            });
        }
        if is_project_root(&norm.path, fs)? {
            return Err(AppError::PathValidation {
                path: norm.display,
                reason: "project root cannot be a rule destination".to_owned(),
            });
        }
    }

    Ok(norm.display)
}
