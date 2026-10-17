#![forbid(unsafe_code)]

use serde::{Serialize, Serializer};
use thiserror::Error;
use ts_rs::TS;

/// Serialized error payload sent across IPC to the frontend.
///
/// The frontend maps `code` and `message_key` through its i18n catalog rather
/// than parsing free-form error text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppErrorPayload {
    pub code: String,
    pub message_key: String,
    pub details: Option<String>,
}

/// Typed application error preserving stable error codes, i18n keys, and raw
/// OS error codes for diagnostics.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("path validation failed for {path}: {reason}")]
    PathValidation { path: String, reason: String },

    #[error("sandbox violation for {path}: {reason}")]
    SandboxViolation { path: String, reason: String },

    #[error("resource not found: {resource}")]
    NotFound {
        resource: String,
        os_error_code: Option<i32>,
    },

    #[error("permission denied for {resource}")]
    PermissionDenied {
        resource: String,
        os_error_code: Option<i32>,
    },

    #[error("target already exists: {path}")]
    AlreadyExists {
        path: String,
        os_error_code: Option<i32>,
    },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error(
        "database schema version {found_version} is newer than supported version {supported_version}"
    )]
    DatabaseDowngrade {
        found_version: u32,
        supported_version: u32,
    },

    #[error("database error during {context}: {message}")]
    Database { context: String, message: String },

    #[error("dry-run plan '{plan_id}' has expired")]
    PlanExpired { plan_id: String },

    #[error("dry-run plan '{plan_id}' is no longer valid: {reason}")]
    PlanInvalidated { plan_id: String, reason: String },

    #[error("operation cancelled")]
    Cancelled,

    #[error("file in use: {path}")]
    FileInUse {
        path: String,
        os_error_code: Option<i32>,
    },

    #[error("destination volume is full while writing {path}")]
    DiskFull {
        path: String,
        os_error_code: Option<i32>,
    },

    #[error("cloud provider file error on {path}")]
    CloudFileError {
        path: String,
        os_error_code: Option<i32>,
    },

    #[error("network path unavailable: {path}")]
    NetworkUnavailable {
        path: String,
        os_error_code: Option<i32>,
    },

    #[error("SHA-256 verification failed for {path}")]
    HashMismatch { path: String },

    #[error("source file changed during cross-volume copy: {path}")]
    SourceChangedDuringCopy { path: String },

    #[error("recovery required: {detail}")]
    RecoveryRequired { detail: String },

    #[error("I/O error during {context}: {source}")]
    Io {
        context: String,
        os_error_code: Option<i32>,
        #[source]
        source: std::io::Error,
    },

    #[error("internal error: {message}")]
    Internal { message: String },
}

impl AppError {
    /// Returns the stable machine-readable error code for IPC and logs.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "INVALID_INPUT",
            Self::PathValidation { .. } => "PATH_VALIDATION",
            Self::SandboxViolation { .. } => "SANDBOX_VIOLATION",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::PermissionDenied { .. } => "PERMISSION_DENIED",
            Self::AlreadyExists { .. } => "ALREADY_EXISTS",
            Self::Conflict { .. } => "CONFLICT",
            Self::DatabaseDowngrade { .. } => "DATABASE_DOWNGRADE",
            Self::Database { .. } => "DATABASE_ERROR",
            Self::PlanExpired { .. } => "PLAN_EXPIRED",
            Self::PlanInvalidated { .. } => "PLAN_INVALIDATED",
            Self::Cancelled => "CANCELLED",
            Self::FileInUse { .. } => "FILE_IN_USE",
            Self::DiskFull { .. } => "DISK_FULL",
            Self::CloudFileError { .. } => "CLOUD_FILE_ERROR",
            Self::NetworkUnavailable { .. } => "NETWORK_UNAVAILABLE",
            Self::HashMismatch { .. } => "HASH_MISMATCH",
            Self::SourceChangedDuringCopy { .. } => "SOURCE_CHANGED_DURING_COPY",
            Self::RecoveryRequired { .. } => "RECOVERY_REQUIRED",
            Self::Io { .. } => "IO_ERROR",
            Self::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    /// Returns the frontend i18n message key corresponding to this error.
    #[must_use]
    pub const fn message_key(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "errors.INVALID_INPUT",
            Self::PathValidation { .. } => "errors.PATH_VALIDATION",
            Self::SandboxViolation { .. } => "errors.SANDBOX_VIOLATION",
            Self::NotFound { .. } => "errors.NOT_FOUND",
            Self::PermissionDenied { .. } => "errors.PERMISSION_DENIED",
            Self::AlreadyExists { .. } => "errors.ALREADY_EXISTS",
            Self::Conflict { .. } => "errors.CONFLICT",
            Self::DatabaseDowngrade { .. } => "errors.DATABASE_DOWNGRADE",
            Self::Database { .. } => "errors.DATABASE_ERROR",
            Self::PlanExpired { .. } => "errors.PLAN_EXPIRED",
            Self::PlanInvalidated { .. } => "errors.PLAN_INVALIDATED",
            Self::Cancelled => "errors.CANCELLED",
            Self::FileInUse { .. } => "errors.FILE_IN_USE",
            Self::DiskFull { .. } => "errors.DISK_FULL",
            Self::CloudFileError { .. } => "errors.CLOUD_FILE_ERROR",
            Self::NetworkUnavailable { .. } => "errors.NETWORK_UNAVAILABLE",
            Self::HashMismatch { .. } => "errors.HASH_MISMATCH",
            Self::SourceChangedDuringCopy { .. } => "errors.SOURCE_CHANGED_DURING_COPY",
            Self::RecoveryRequired { .. } => "errors.RECOVERY_REQUIRED",
            Self::Io { .. } => "errors.IO_ERROR",
            Self::Internal { .. } => "errors.INTERNAL_ERROR",
        }
    }

    /// Returns the preserved raw operating-system error code, if any.
    #[must_use]
    pub const fn os_error_code(&self) -> Option<i32> {
        match self {
            Self::NotFound { os_error_code, .. }
            | Self::PermissionDenied { os_error_code, .. }
            | Self::AlreadyExists { os_error_code, .. }
            | Self::FileInUse { os_error_code, .. }
            | Self::DiskFull { os_error_code, .. }
            | Self::CloudFileError { os_error_code, .. }
            | Self::NetworkUnavailable { os_error_code, .. }
            | Self::Io { os_error_code, .. } => *os_error_code,
            Self::InvalidInput { .. }
            | Self::PathValidation { .. }
            | Self::SandboxViolation { .. }
            | Self::Conflict { .. }
            | Self::DatabaseDowngrade { .. }
            | Self::Database { .. }
            | Self::PlanExpired { .. }
            | Self::PlanInvalidated { .. }
            | Self::Cancelled
            | Self::HashMismatch { .. }
            | Self::SourceChangedDuringCopy { .. }
            | Self::RecoveryRequired { .. }
            | Self::Internal { .. } => None,
        }
    }

    /// Wraps a `std::io::Error` while preserving its raw OS error code.
    #[must_use]
    pub fn from_io(context: impl Into<String>, source: std::io::Error) -> Self {
        let os_error_code = source.raw_os_error();
        Self::Io {
            context: context.into(),
            os_error_code,
            source,
        }
    }
}

impl From<&AppError> for AppErrorPayload {
    fn from(err: &AppError) -> Self {
        let details = match err {
            AppError::InvalidInput { message } | AppError::Conflict { message } => {
                Some(message.clone())
            }
            AppError::PathValidation { reason, .. } | AppError::SandboxViolation { reason, .. } => {
                Some(reason.clone())
            }
            AppError::DatabaseDowngrade {
                found_version,
                supported_version,
            } => Some(format!(
                "found schema version {found_version}, supported up to {supported_version}"
            )),
            AppError::Database { context, .. } => Some(context.clone()),
            AppError::PlanInvalidated { reason, .. } => Some(reason.clone()),
            AppError::RecoveryRequired { detail } => Some(detail.clone()),
            AppError::HashMismatch { path } | AppError::SourceChangedDuringCopy { path } => {
                Some(path.clone())
            }
            AppError::PlanExpired { .. }
            | AppError::Cancelled
            | AppError::FileInUse { .. }
            | AppError::DiskFull { .. }
            | AppError::CloudFileError { .. }
            | AppError::NetworkUnavailable { .. }
            | AppError::NotFound { .. }
            | AppError::PermissionDenied { .. }
            | AppError::AlreadyExists { .. }
            | AppError::Io { .. }
            | AppError::Internal { .. } => None,
        };

        Self {
            code: err.code().to_owned(),
            message_key: err.message_key().to_owned(),
            details,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AppErrorPayload::from(self).serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_error_with_stable_code_and_message_key() {
        let err = AppError::SandboxViolation {
            path: "C:\\temp\\outside".to_owned(),
            reason: "missing sandbox sentinel".to_owned(),
        };

        assert_eq!(err.code(), "SANDBOX_VIOLATION");
        assert_eq!(err.message_key(), "errors.SANDBOX_VIOLATION");
        assert_eq!(err.os_error_code(), None);

        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["code"], "SANDBOX_VIOLATION");
        assert_eq!(value["messageKey"], "errors.SANDBOX_VIOLATION");
        assert_eq!(value["details"], "missing sandbox sentinel");
    }

    #[test]
    fn preserves_raw_os_error_code_from_io() {
        let io_err = std::io::Error::from_raw_os_error(32);
        let err = AppError::from_io("open for delete check", io_err);

        assert_eq!(err.code(), "IO_ERROR");
        assert_eq!(err.os_error_code(), Some(32));

        let payload = AppErrorPayload::from(&err);
        assert_eq!(payload.details, None);
    }
}
