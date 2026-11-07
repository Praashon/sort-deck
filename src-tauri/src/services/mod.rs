#![forbid(unsafe_code)]

pub mod execution;
pub mod folders;
pub mod history;
pub mod planner;
pub mod rules;
pub mod settings;
pub mod state;

use crate::config::{is_data_dir_override_active, AppInfo, APP_IDENTIFIER, CURRENT_SCHEMA_VERSION};
use crate::database::{queries, transactions};
use crate::engine::paths;
use crate::errors::AppError;
use crate::fsops::app_data;
use crate::models::{BatchListRequest, DatabaseStatusInfo, RedactedDiagnostics};
use crate::services::state::AppState;

/// Returns static and runtime environment metadata for the SortDeck UI.
#[must_use]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        identifier: APP_IDENTIFIER.to_owned(),
        schema_version: CURRENT_SCHEMA_VERSION,
        data_dir_override_active: is_data_dir_override_active(),
    }
}

/// Returns database health and config revision diagnostics.
pub fn get_database_status(state: &AppState) -> Result<DatabaseStatusInfo, AppError> {
    let config_revision = state
        .db
        .with_writer(|conn| queries::get_config_revision(conn))?;

    let db_path = dunce::simplified(state.db.db_path());
    let db_size_bytes = std::fs::metadata(state.db.db_path())
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(DatabaseStatusInfo {
        schema_version: state.db.schema_version(),
        config_revision,
        db_path: db_path.to_string_lossy().into_owned(),
        db_size_bytes,
        recovered_from_corruption: state.db.recovered_from_corruption(),
        corrupt_backup_path: state
            .db
            .corrupt_backup_path()
            .map(|p| dunce::simplified(p).to_string_lossy().into_owned()),
    })
}

/// Returns the user's Downloads folder if it exists and passes watched-folder safety checks,
/// without enabling or adding it automatically (`AGENTS.md` §10.6).
pub fn get_suggested_downloads_folder(state: &AppState) -> Result<Option<String>, AppError> {
    let Some(profile) = std::env::var_os("USERPROFILE") else {
        return Ok(None);
    };
    let candidate = std::path::PathBuf::from(profile).join("Downloads");
    let candidate_str = candidate.to_string_lossy().into_owned();
    let Ok(normalized) = paths::validate_watched_folder_path(
        &candidate_str,
        false,
        &state.protected_roots,
        state.fs.as_ref(),
    ) else {
        return Ok(None);
    };

    let can_add = state.db.with_writer(|conn| {
        let existing_folders = queries::list_watched_folders(conn)?;
        let existing_rules = queries::list_rules(conn)?;
        Ok(
            paths::validate_folder_no_overlap(&normalized.key, &existing_folders, &existing_rules)
                .is_ok(),
        )
    })?;

    Ok(if can_add {
        Some(normalized.display)
    } else {
        None
    })
}

/// Opens SortDeck's local log directory in Windows Explorer (`AGENTS.md` §10.5).
pub fn open_logs_folder(state: &AppState) -> Result<(), AppError> {
    let Some(data_dir) = state.db.db_path().parent() else {
        return Err(AppError::Internal {
            message: "database path has no parent directory".to_owned(),
        });
    };
    let logs_dir = data_dir.join("logs");
    app_data::ensure_dir(&logs_dir)?;

    #[cfg(windows)]
    {
        crate::platform::windows::open_directory_in_explorer(&logs_dir)
    }
    #[cfg(not(windows))]
    {
        let _ = logs_dir;
        Err(AppError::Internal {
            message: "open_logs_folder is only supported on Windows".to_owned(),
        })
    }
}

/// Exports a redacted diagnostics snapshot containing zero usernames or file paths (`AGENTS.md` §12.2).
pub fn export_redacted_diagnostics(state: &AppState) -> Result<RedactedDiagnostics, AppError> {
    let (
        config_revision,
        settings,
        watched_folder_count,
        total_rule_count,
        enabled_rule_count,
        batch_count,
        recovery_required_count,
    ) = state.db.with_writer(|conn| {
        let rev = queries::get_config_revision(conn)?;
        let settings = queries::get_settings(conn)?;
        let folders = queries::list_watched_folders(conn)?;
        let rules = queries::list_rules(conn)?;
        let batches = transactions::list_batches_paginated(
            conn,
            &BatchListRequest {
                offset: 0,
                limit: 1,
                status_filter: None,
            },
        )?;
        let recovery_items = transactions::list_recovery_required_entries(conn)?;
        let enabled_rules = rules.iter().filter(|r| r.enabled).count();

        Ok((
            rev,
            settings,
            u64::try_from(folders.len()).unwrap_or(u64::MAX),
            u64::try_from(rules.len()).unwrap_or(u64::MAX),
            u64::try_from(enabled_rules).unwrap_or(u64::MAX),
            batches.total,
            u64::try_from(recovery_items.len()).unwrap_or(u64::MAX),
        ))
    })?;

    Ok(RedactedDiagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        identifier: APP_IDENTIFIER.to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        schema_version: state.db.schema_version(),
        config_revision,
        recovered_from_corruption: state.db.recovered_from_corruption(),
        settings,
        watched_folder_count,
        total_rule_count,
        enabled_rule_count,
        batch_count,
        recovery_required_count,
        generated_at: queries::now_epoch_ms(),
    })
}
