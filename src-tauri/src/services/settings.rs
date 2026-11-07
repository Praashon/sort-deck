#![forbid(unsafe_code)]

use crate::config::AppSettings;
use crate::database::queries;
use crate::errors::AppError;
use crate::services::state::AppState;

const MAX_SCAN_DEPTH: u32 = 32;
const MAX_RETENTION_DAYS: u32 = 36_500;

/// Reads persistent application settings.
pub fn get_settings(state: &AppState) -> Result<AppSettings, AppError> {
    state.db.with_writer(|conn| queries::get_settings(conn))
}

/// Validates and saves persistent application settings.
pub fn update_settings(state: &AppState, settings: AppSettings) -> Result<AppSettings, AppError> {
    if !(1..=MAX_SCAN_DEPTH).contains(&settings.default_scan_depth) {
        return Err(AppError::InvalidInput {
            message: format!("defaultScanDepth must be between 1 and {MAX_SCAN_DEPTH}"),
        });
    }

    if let Some(days) = settings.retention_days {
        if !(1..=MAX_RETENTION_DAYS).contains(&days) {
            return Err(AppError::InvalidInput {
                message: format!("retentionDays must be between 1 and {MAX_RETENTION_DAYS}"),
            });
        }
    }

    state
        .db
        .with_writer(|conn| queries::save_settings(conn, &settings))
}
