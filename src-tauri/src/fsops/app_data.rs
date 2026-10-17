#![forbid(unsafe_code)]

//! Confined filesystem helpers for SortDeck's own application data directory
//! (`sortdeck.db`, backups, and logs) per AGENTS.md §2.6 and §5.1.

use std::path::{Path, PathBuf};

use crate::errors::AppError;

/// Ensures an internal SortDeck application directory exists.
pub fn ensure_dir(path: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(path).map_err(|err| AppError::from_io("create app data directory", err))
}

/// Preserves a corrupt database file without deleting it (`sortdeck.corrupt-<ts>.db`)
/// and removes stale WAL/SHM sidecar files before creating a fresh database.
pub fn preserve_corrupt_database(db_path: &Path, timestamp_ms: i64) -> Result<PathBuf, AppError> {
    let Some(parent) = db_path.parent() else {
        return Err(AppError::Internal {
            message: "database path has no parent directory".to_owned(),
        });
    };

    let backup_name = format!("sortdeck.corrupt-{timestamp_ms}.db");
    let backup_path = parent.join(backup_name);

    // Confined to fsops/app_data.rs for SortDeck's own DB file per AGENTS.md §2.6 & §5.1.
    #[allow(clippy::disallowed_methods)]
    if db_path.exists() {
        std::fs::rename(db_path, &backup_path)
            .map_err(|err| AppError::from_io("preserve corrupt database", err))?;
    }

    for suffix in ["-wal", "-shm"] {
        let mut sidecar = db_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar_path = PathBuf::from(sidecar);
        if sidecar_path.exists() {
            #[allow(clippy::disallowed_methods)]
            let _ = std::fs::remove_file(&sidecar_path);
        }
    }

    Ok(backup_path)
}

/// Retains only the newest `keep` pre-migration database backups in `backup_dir`.
pub fn prune_old_backups(backup_dir: &Path, keep: usize) -> Result<(), AppError> {
    if !backup_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(backup_dir)
        .map_err(|err| AppError::from_io("read backup directory", err))?;

    let mut backups: Vec<(std::time::SystemTime, String, PathBuf)> = Vec::new();
    for entry_res in entries {
        let entry = entry_res.map_err(|err| AppError::from_io("iterate backup entries", err))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("sortdeck-") || !name.ends_with(".db") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        backups.push((modified, name, path));
    }

    // Sort newest first; break ties by descending filename (which embeds timestamp).
    backups.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    for (_, _, stale_path) in backups.into_iter().skip(keep) {
        // Confined to fsops/app_data.rs for pre-migration backup retention (§5.1).
        #[allow(clippy::disallowed_methods)]
        std::fs::remove_file(&stale_path)
            .map_err(|err| AppError::from_io("prune stale pre-migration backup", err))?;
    }

    Ok(())
}
