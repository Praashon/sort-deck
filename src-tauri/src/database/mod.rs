#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, ErrorCode};

use crate::errors::AppError;
use crate::fsops::app_data;

pub mod migrations;
pub mod queries;
pub mod transactions;

const PRAGMAS_SQL: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA busy_timeout = 5000;
PRAGMA trusted_schema = OFF;
"#;

/// Thread-safe SQLite database wrapper with a single serialized writer connection.
///
/// Callers MUST NOT hold the writer lock across any filesystem operation (`AGENTS.md` §5.1).
pub struct Database {
    writer: Mutex<Connection>,
    db_path: PathBuf,
    backup_dir: PathBuf,
    schema_version: u32,
    recovered_from_corruption: bool,
    corrupt_backup_path: Option<PathBuf>,
}

impl Database {
    /// Opens or initializes `sortdeck.db` inside `data_dir` with required pragmas,
    /// startup `quick_check`, corruption preservation, downgrade refusal,
    /// pre-migration backups, and default rule seeding.
    pub fn open_in_dir(data_dir: &Path) -> Result<Self, AppError> {
        let raw_dir = data_dir.to_string_lossy();
        if raw_dir.starts_with(r"\\") || raw_dir.starts_with("//") {
            return Err(AppError::PathValidation {
                path: raw_dir.into_owned(),
                reason: "SQLite WAL database cannot reside on a network/UNC share".to_owned(),
            });
        }

        app_data::ensure_dir(data_dir)?;
        let backup_dir = data_dir.join("backups");
        app_data::ensure_dir(&backup_dir)?;

        let db_path = data_dir.join("sortdeck.db");
        let now_ms = queries::now_epoch_ms();

        let mut recovered_from_corruption = false;
        let mut corrupt_backup_path = None;

        let mut conn = match open_and_check(&db_path) {
            Ok(conn) => conn,
            Err(OpenOrCheckError::Corrupt) => {
                let preserved = app_data::preserve_corrupt_database(&db_path, now_ms)?;
                recovered_from_corruption = true;
                corrupt_backup_path = Some(preserved);
                open_and_check(&db_path).map_err(OpenOrCheckError::into_app_error)?
            }
            Err(err @ OpenOrCheckError::Other(_)) => return Err(err.into_app_error()),
        };

        let schema_version = migrations::run_migrations(&mut conn, &backup_dir, now_ms)?;
        migrations::seed_defaults_if_needed(&mut conn, now_ms)?;

        Ok(Self {
            writer: Mutex::new(conn),
            db_path,
            backup_dir,
            schema_version,
            recovered_from_corruption,
            corrupt_backup_path,
        })
    }

    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    #[must_use]
    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn recovered_from_corruption(&self) -> bool {
        self.recovered_from_corruption
    }

    #[must_use]
    pub fn corrupt_backup_path(&self) -> Option<&Path> {
        self.corrupt_backup_path.as_deref()
    }

    /// Executes a database closure against the serialized writer connection.
    pub fn with_writer<F, T>(&self, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Connection) -> Result<T, AppError>,
    {
        let mut guard = self.writer.lock().map_err(|_| AppError::Internal {
            message: "database writer mutex poisoned".to_owned(),
        })?;
        f(&mut guard)
    }
}

enum OpenOrCheckError {
    Corrupt,
    Other(AppError),
}

impl OpenOrCheckError {
    fn into_app_error(self) -> AppError {
        match self {
            Self::Corrupt => AppError::Database {
                context: "startup quick_check".to_owned(),
                message: "database corruption persisted after recovery".to_owned(),
            },
            Self::Other(err) => err,
        }
    }
}

fn is_corruption_sqlite_error(err: &rusqlite::Error) -> bool {
    matches!(
        err.sqlite_error_code(),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

fn open_and_check(db_path: &Path) -> Result<Connection, OpenOrCheckError> {
    let conn = Connection::open(db_path).map_err(|err| {
        if is_corruption_sqlite_error(&err) {
            OpenOrCheckError::Corrupt
        } else {
            OpenOrCheckError::Other(AppError::Database {
                context: "open SQLite database".to_owned(),
                message: err.to_string(),
            })
        }
    })?;

    if let Err(err) = conn.execute_batch(PRAGMAS_SQL) {
        if is_corruption_sqlite_error(&err) {
            return Err(OpenOrCheckError::Corrupt);
        }
        return Err(OpenOrCheckError::Other(AppError::Database {
            context: "apply SQLite pragmas".to_owned(),
            message: err.to_string(),
        }));
    }

    let check_result: Result<String, rusqlite::Error> =
        conn.query_row("PRAGMA quick_check", [], |row| row.get(0));

    match check_result {
        Ok(status) if status.eq_ignore_ascii_case("ok") => Ok(conn),
        Ok(_) => Err(OpenOrCheckError::Corrupt),
        Err(err) if is_corruption_sqlite_error(&err) => Err(OpenOrCheckError::Corrupt),
        Err(err) => Err(OpenOrCheckError::Other(AppError::Database {
            context: "run PRAGMA quick_check".to_owned(),
            message: err.to_string(),
        })),
    }
}
