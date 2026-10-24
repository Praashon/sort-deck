#![forbid(unsafe_code)]

use crate::database::queries;
use crate::engine::paths;
use crate::errors::AppError;
use crate::models::{AddWatchedFolderRequest, UpdateWatchedFolderRequest, WatchedFolder};
use crate::services::state::AppState;

const DEFAULT_MAX_DEPTH: u32 = 1;
const MAX_ALLOWED_DEPTH: u32 = 32;
const DEFAULT_MIN_FILE_AGE_SECS: u32 = 5;
const MAX_MIN_FILE_AGE_SECS: u32 = 86_400;

/// Lists all watched folders stored in the local database.
pub fn list_watched_folders(state: &AppState) -> Result<Vec<WatchedFolder>, AppError> {
    state
        .db
        .with_writer(|conn| queries::list_watched_folders(conn))
}

/// Validates and registers a new watched folder.
///
/// Filesystem validation runs before acquiring the database lock.
pub fn add_watched_folder(
    state: &AppState,
    req: AddWatchedFolderRequest,
) -> Result<WatchedFolder, AppError> {
    let recursive = req.recursive.unwrap_or(false);
    let max_depth = req.max_depth.unwrap_or(DEFAULT_MAX_DEPTH);
    let min_file_age_secs = req.min_file_age_secs.unwrap_or(DEFAULT_MIN_FILE_AGE_SECS);
    let allow_network_root = req.allow_network_root.unwrap_or(false);

    validate_folder_tuning(max_depth, min_file_age_secs)?;

    let normalized = paths::validate_watched_folder_path(
        &req.path,
        allow_network_root,
        &state.protected_roots,
        state.fs.as_ref(),
    )?;

    state.db.with_writer(|conn| {
        let existing_folders = queries::list_watched_folders(conn)?;
        let existing_rules = queries::list_rules(conn)?;
        paths::validate_folder_no_overlap(&normalized.key, &existing_folders, &existing_rules)?;

        queries::insert_watched_folder(
            conn,
            &normalized.display,
            &normalized.key,
            recursive,
            max_depth,
            min_file_age_secs,
        )
    })
}

/// Updates preferences for an existing watched folder.
pub fn update_watched_folder(
    state: &AppState,
    req: UpdateWatchedFolderRequest,
) -> Result<WatchedFolder, AppError> {
    validate_folder_tuning(req.max_depth, req.min_file_age_secs)?;
    state
        .db
        .with_writer(|conn| queries::update_watched_folder(conn, &req))
}

/// Removes a watched folder configuration without moving or deleting files.
pub fn remove_watched_folder(state: &AppState, id: i64) -> Result<(), AppError> {
    if id <= 0 {
        return Err(AppError::InvalidInput {
            message: "watched folder id must be positive".to_owned(),
        });
    }
    state
        .db
        .with_writer(|conn| queries::delete_watched_folder(conn, id))
}

fn validate_folder_tuning(max_depth: u32, min_file_age_secs: u32) -> Result<(), AppError> {
    if !(1..=MAX_ALLOWED_DEPTH).contains(&max_depth) {
        return Err(AppError::InvalidInput {
            message: format!("maxDepth must be between 1 and {MAX_ALLOWED_DEPTH}"),
        });
    }
    if min_file_age_secs > MAX_MIN_FILE_AGE_SECS {
        return Err(AppError::InvalidInput {
            message: format!("minFileAgeSecs must not exceed {MAX_MIN_FILE_AGE_SECS}"),
        });
    }
    Ok(())
}
