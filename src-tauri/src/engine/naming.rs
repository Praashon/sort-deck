#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

use crate::engine::paths::NormalizedPath;
use crate::fsops::FsBackend;
use crate::models::SkipReason;

/// Compound archive extensions preserved intact during collision suffixing (`AGENTS.md` §4.1.2).
pub const COMPOUND_EXTENSIONS: [&str; 4] = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"];

/// Maximum collision probe attempts per file before failing cleanly (`AGENTS.md` §4.1.1).
pub const MAX_COLLISION_ATTEMPTS: u32 = 10_000;

const MAX_COMPONENT_UTF16_UNITS: usize = 255;
const MAX_EXTENDED_PATH_UTF16_UNITS: usize = 32_767;

/// Result of reserving a collision-free destination filename inside a dry-run plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedName {
    pub filename: String,
    pub was_renamed: bool,
}

/// Splits a filename into `(stem, extension_with_dot)` while preserving compound
/// archive suffixes and treating single-dot dotfiles (`.gitignore`) as extensionless.
#[must_use]
pub fn split_stem_and_extension(filename: &str) -> (&str, &str) {
    let lower = filename.to_ascii_lowercase();
    for compound in COMPOUND_EXTENSIONS {
        if lower.ends_with(compound) && filename.len() > compound.len() {
            let split_idx = filename.len() - compound.len();
            return filename.split_at(split_idx);
        }
    }

    if let Some(last_dot) = filename.rfind('.') {
        if last_dot > 0 && last_dot < filename.len() - 1 {
            return filename.split_at(last_dot);
        }
    }

    (filename, "")
}

/// Formats collision candidate `attempt` (`0` = original name, `n >= 1` = `<stem> (<n>)<ext>`).
#[must_use]
pub fn collision_candidate(filename: &str, attempt: u32) -> String {
    if attempt == 0 {
        return filename.to_owned();
    }
    let (stem, ext) = split_stem_and_extension(filename);
    format!("{stem} ({attempt}){ext}")
}

/// Tracks case-insensitive reserved destination names across both existing files
/// on disk and earlier operations in the same plan (`AGENTS.md` §4.1.2, §7.3).
#[derive(Debug, Default)]
pub struct NameReservationTable {
    reserved_by_dir: HashMap<String, HashSet<String>>,
    disk_entries_by_dir: HashMap<String, HashSet<String>>,
}

impl NameReservationTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves a case-insensitively unique filename in `dest_dir` without creating
    /// or modifying anything on disk.
    pub fn reserve_name(
        &mut self,
        dest_dir: &NormalizedPath,
        original_filename: &str,
        fs: &dyn FsBackend,
    ) -> Result<ReservedName, SkipReason> {
        let existing_disk = self.load_disk_names(dest_dir, fs);
        let reserved = self
            .reserved_by_dir
            .entry(dest_dir.key.clone())
            .or_default();

        for attempt in 0..=MAX_COLLISION_ATTEMPTS {
            let candidate = collision_candidate(original_filename, attempt);
            if candidate.encode_utf16().count() > MAX_COMPONENT_UTF16_UNITS {
                return Err(SkipReason::PathTooLong);
            }

            let full_candidate_path = dest_dir.path.join(&candidate);
            if full_candidate_path.to_string_lossy().encode_utf16().count()
                > MAX_EXTENDED_PATH_UTF16_UNITS
            {
                return Err(SkipReason::PathTooLong);
            }

            let candidate_lower = candidate.to_lowercase();
            if reserved.contains(&candidate_lower) || existing_disk.contains(&candidate_lower) {
                continue;
            }

            if fs.exists(&full_candidate_path).unwrap_or(false) {
                continue;
            }

            reserved.insert(candidate_lower);
            return Ok(ReservedName {
                filename: candidate,
                was_renamed: attempt > 0,
            });
        }

        Err(SkipReason::PathTooLong)
    }

    fn load_disk_names(
        &mut self,
        dest_dir: &NormalizedPath,
        fs: &dyn FsBackend,
    ) -> HashSet<String> {
        if let Some(cached) = self.disk_entries_by_dir.get(&dest_dir.key) {
            return cached.clone();
        }

        let mut names = HashSet::new();
        if fs.exists(&dest_dir.path).unwrap_or(false) {
            if let Ok(entries) = fs.read_dir(&dest_dir.path) {
                for entry in entries {
                    names.insert(entry.file_name.to_lowercase());
                }
            }
        }

        self.disk_entries_by_dir
            .insert(dest_dir.key.clone(), names.clone());
        names
    }
}
