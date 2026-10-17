#![forbid(unsafe_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::AppError;

pub const APP_IDENTIFIER: &str = "com.sortdeck.app";
pub const DATA_DIR_ENV: &str = "SORTDECK_DATA_DIR";
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// UI color theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

/// Persistent application settings with v1 defaults from AGENTS.md §12.3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub global_auto_watch: bool,
    pub default_scan_depth: u32,
    pub retention_days: Option<u32>,
    pub notifications: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            launch_on_startup: false,
            start_minimized: false,
            minimize_to_tray: true,
            global_auto_watch: true,
            default_scan_depth: 1,
            retention_days: Some(365),
            notifications: true,
        }
    }
}

/// Read-only runtime metadata exposed to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub identifier: String,
    pub schema_version: u32,
    pub data_dir_override_active: bool,
}

/// Resolves SortDeck's local data directory, honoring `SORTDECK_DATA_DIR` for
/// development and sandboxed tests.
pub fn resolve_data_dir() -> Result<PathBuf, AppError> {
    let override_var = std::env::var(DATA_DIR_ENV).ok();
    let local_app_data = std::env::var("LOCALAPPDATA").ok();
    resolve_data_dir_from_inputs(override_var.as_deref(), local_app_data.as_deref())
}

/// Returns true when `SORTDECK_DATA_DIR` is set to a non-empty value.
#[must_use]
pub fn is_data_dir_override_active() -> bool {
    std::env::var(DATA_DIR_ENV)
        .map(|val| !val.trim().is_empty())
        .unwrap_or(false)
}

/// Pure helper resolving the data directory without mutating process state.
pub fn resolve_data_dir_from_inputs(
    override_dir: Option<&str>,
    local_app_data: Option<&str>,
) -> Result<PathBuf, AppError> {
    if let Some(raw_override) = override_dir {
        let trimmed = raw_override.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput {
                message: format!("{DATA_DIR_ENV} must not be empty when set"),
            });
        }
        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(AppError::PathValidation {
                path: trimmed.to_owned(),
                reason: format!("{DATA_DIR_ENV} must be an absolute path"),
            });
        }
        return Ok(path);
    }

    let base = local_app_data
        .map(str::trim)
        .filter(|val| !val.is_empty())
        .ok_or_else(|| AppError::Internal {
            message: "LOCALAPPDATA environment variable is unavailable".to_owned(),
        })?;

    Ok(PathBuf::from(base).join("SortDeck"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_match_specification() {
        let settings = AppSettings::default();
        assert_eq!(settings.theme, ThemeMode::Dark);
        assert!(!settings.launch_on_startup);
        assert!(!settings.start_minimized);
        assert!(settings.minimize_to_tray);
        assert!(settings.global_auto_watch);
        assert_eq!(settings.default_scan_depth, 1);
        assert_eq!(settings.retention_days, Some(365));
        assert!(settings.notifications);
    }

    #[test]
    fn resolve_data_dir_prefers_valid_override() {
        let override_path = if cfg!(windows) {
            "C:\\Sandbox\\SortDeckData"
        } else {
            "/tmp/sortdeck-data"
        };
        let resolved = resolve_data_dir_from_inputs(
            Some(override_path),
            Some("C:\\Users\\Test\\AppData\\Local"),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from(override_path));
    }

    #[test]
    fn resolve_data_dir_rejects_relative_override() {
        let err = resolve_data_dir_from_inputs(Some("relative\\data"), None).unwrap_err();
        assert_eq!(err.code(), "PATH_VALIDATION");
    }

    #[test]
    fn resolve_data_dir_falls_back_to_local_app_data() {
        let base = if cfg!(windows) {
            "C:\\Users\\Test\\AppData\\Local"
        } else {
            "/home/test/.local/share"
        };
        let resolved = resolve_data_dir_from_inputs(None, Some(base)).unwrap();
        assert_eq!(resolved, PathBuf::from(base).join("SortDeck"));
    }
}
