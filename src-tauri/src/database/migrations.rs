#![forbid(unsafe_code)]

use std::path::Path;
use std::time::Duration;

use rusqlite::{backup::Backup, params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::config::AppSettings;
use crate::errors::AppError;
use crate::fsops::app_data;

pub const LATEST_SCHEMA_VERSION: u32 = 2;
pub const DEFAULTS_VERSION: u32 = 2;

pub struct DefaultRuleSpec {
    pub default_key: &'static str,
    pub name: &'static str,
    pub destination: &'static str,
    pub extensions: &'static [&'static str],
}

/// Factory default sorting rules from `AGENTS.md` §6.1 plus v2 Adobe Creative support.
pub const DEFAULT_RULES_V1: [DefaultRuleSpec; 7] = [
    DefaultRuleSpec {
        default_key: "default.images",
        name: "Images",
        destination: "Images/",
        extensions: &[
            "png", "jpg", "jpeg", "webp", "gif", "svg", "ico", "bmp", "heic", "avif",
        ],
    },
    DefaultRuleSpec {
        default_key: "default.documents",
        name: "Documents",
        destination: "Documents/",
        extensions: &[
            "pdf", "doc", "docx", "txt", "md", "rtf", "odt", "xls", "xlsx", "csv", "ppt", "pptx",
        ],
    },
    DefaultRuleSpec {
        default_key: "default.installers",
        name: "Installers",
        destination: "Installers/",
        extensions: &["exe", "msi", "msix", "bat", "cmd", "ps1"],
    },
    DefaultRuleSpec {
        default_key: "default.archives",
        name: "Archives",
        destination: "Archives/",
        extensions: &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
    },
    DefaultRuleSpec {
        default_key: "default.videos",
        name: "Videos",
        destination: "Videos/",
        extensions: &["mp4", "mkv", "mov", "avi", "webm", "wmv", "flv"],
    },
    DefaultRuleSpec {
        default_key: "default.audio",
        name: "Audio",
        destination: "Audio/",
        extensions: &["mp3", "wav", "flac", "aac", "ogg", "m4a"],
    },
    DefaultRuleSpec {
        default_key: "default.code",
        name: "Code",
        destination: "Code/",
        extensions: &[
            "json", "xml", "yaml", "yml", "sql", "py", "js", "ts", "html", "css",
        ],
    },
];

pub const DEFAULT_RULES_V2_ADDITIONS: [DefaultRuleSpec; 1] = [DefaultRuleSpec {
    default_key: "default.adobe",
    name: "Adobe Creative",
    destination: "Adobe/",
    extensions: &[
        "psd",
        "psb",
        "ai",
        "eps",
        "indd",
        "indt",
        "idml",
        "prproj",
        "aep",
        "aet",
        "mogrt",
        "xd",
        "lrcat",
        "lrtemplate",
        "xmp",
        "dng",
        "sesx",
        "fla",
        "abr",
        "ase",
    ],
}];

pub const ALL_DEFAULT_RULES: [DefaultRuleSpec; 8] = [
    DefaultRuleSpec {
        default_key: "default.images",
        name: "Images",
        destination: "Images/",
        extensions: &[
            "png", "jpg", "jpeg", "webp", "gif", "svg", "ico", "bmp", "heic", "avif",
        ],
    },
    DefaultRuleSpec {
        default_key: "default.documents",
        name: "Documents",
        destination: "Documents/",
        extensions: &[
            "pdf", "doc", "docx", "txt", "md", "rtf", "odt", "xls", "xlsx", "csv", "ppt", "pptx",
        ],
    },
    DefaultRuleSpec {
        default_key: "default.installers",
        name: "Installers",
        destination: "Installers/",
        extensions: &["exe", "msi", "msix", "bat", "cmd", "ps1"],
    },
    DefaultRuleSpec {
        default_key: "default.archives",
        name: "Archives",
        destination: "Archives/",
        extensions: &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
    },
    DefaultRuleSpec {
        default_key: "default.videos",
        name: "Videos",
        destination: "Videos/",
        extensions: &["mp4", "mkv", "mov", "avi", "webm", "wmv", "flv"],
    },
    DefaultRuleSpec {
        default_key: "default.audio",
        name: "Audio",
        destination: "Audio/",
        extensions: &["mp3", "wav", "flac", "aac", "ogg", "m4a"],
    },
    DefaultRuleSpec {
        default_key: "default.code",
        name: "Code",
        destination: "Code/",
        extensions: &[
            "json", "xml", "yaml", "yml", "sql", "py", "js", "ts", "html", "css",
        ],
    },
    DefaultRuleSpec {
        default_key: "default.adobe",
        name: "Adobe Creative",
        destination: "Adobe/",
        extensions: &[
            "psd",
            "psb",
            "ai",
            "eps",
            "indd",
            "indt",
            "idml",
            "prproj",
            "aep",
            "aet",
            "mogrt",
            "xd",
            "lrcat",
            "lrtemplate",
            "xmp",
            "dng",
            "sesx",
            "fla",
            "abr",
            "ase",
        ],
    },
];

pub const MIGRATION_001_SQL: &str = r#"
CREATE TABLE schema_migrations (
  version     INTEGER PRIMARY KEY,
  name        TEXT NOT NULL,
  checksum    TEXT NOT NULL,
  applied_at  INTEGER NOT NULL
);

CREATE TABLE settings (
  key         TEXT PRIMARY KEY,
  value       TEXT NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE TABLE watched_folders (
  id                      INTEGER PRIMARY KEY AUTOINCREMENT,
  path                    TEXT NOT NULL,
  path_key                TEXT NOT NULL UNIQUE,
  enabled                 INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  auto_sort               INTEGER NOT NULL DEFAULT 0 CHECK (auto_sort IN (0,1)),
  recursive               INTEGER NOT NULL DEFAULT 0 CHECK (recursive IN (0,1)),
  max_depth               INTEGER NOT NULL DEFAULT 1 CHECK (max_depth BETWEEN 1 AND 32),
  min_file_age_secs       INTEGER NOT NULL DEFAULT 5 CHECK (min_file_age_secs >= 0),
  status                  TEXT NOT NULL DEFAULT 'ok'
                          CHECK (status IN ('ok','unavailable','ignored','invalid')),
  auto_sort_enabled_at    INTEGER,
  last_reconciled_at      INTEGER,
  created_at              INTEGER NOT NULL,
  updated_at              INTEGER NOT NULL
);

CREATE TABLE rules (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  name          TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  destination   TEXT NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  priority      INTEGER NOT NULL,
  is_default    INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0,1)),
  default_key   TEXT UNIQUE,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE rule_extensions (
  rule_id     INTEGER NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
  extension   TEXT NOT NULL CHECK (extension = lower(extension)
                                   AND length(extension) BETWEEN 1 AND 32
                                   AND substr(extension,1,1) <> '.'),
  PRIMARY KEY (rule_id, extension)
);

CREATE TABLE batches (
  id                 TEXT PRIMARY KEY,
  kind               TEXT NOT NULL CHECK (kind IN ('manual','auto','undo','recovery')),
  source_folder      TEXT NOT NULL,
  source_folder_id   INTEGER REFERENCES watched_folders(id) ON DELETE SET NULL,
  parent_batch_id    TEXT REFERENCES batches(id),
  status             TEXT NOT NULL
                     CHECK (status IN ('running','completed','partial','failed',
                                       'cancelled','recovery_required')),
  total_files        INTEGER NOT NULL DEFAULT 0,
  successful_moves   INTEGER NOT NULL DEFAULT 0,
  failed_moves       INTEGER NOT NULL DEFAULT 0,
  skipped_files      INTEGER NOT NULL DEFAULT 0,
  total_bytes        INTEGER NOT NULL DEFAULT 0,
  created_at         INTEGER NOT NULL,
  completed_at       INTEGER
);

CREATE TABLE move_logs (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id         TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  original_path    TEXT NOT NULL,
  new_path         TEXT NOT NULL,
  status           TEXT NOT NULL CHECK (status IN ('completed','undone','undo_failed')),
  cross_volume     INTEGER NOT NULL DEFAULT 0 CHECK (cross_volume IN (0,1)),
  sha256           TEXT,
  file_size        INTEGER NOT NULL,
  volume_serial    INTEGER,
  file_id          TEXT,
  modified_at      INTEGER,
  rule_id          INTEGER REFERENCES rules(id) ON DELETE SET NULL,
  moved_at         INTEGER NOT NULL,
  undone           INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0,1)),
  undone_at        INTEGER,
  undo_batch_id    TEXT REFERENCES batches(id),
  undo_note        TEXT
);

CREATE TABLE operation_journal (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id          TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  source_path       TEXT NOT NULL,
  destination_path  TEXT NOT NULL,
  temporary_path    TEXT,
  operation_type    TEXT NOT NULL
                    CHECK (operation_type IN ('move_same_volume','move_cross_volume',
                                              'undo_move','create_dir')),
  state             TEXT NOT NULL
                    CHECK (state IN ('planned','copying','verified','committed','completed',
                                     'undo_pending','undone','failed','recovery_required')),
  fingerprint_json  TEXT,
  error_code        INTEGER,
  error_message     TEXT,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
);

CREATE TABLE created_directories (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id    TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  path        TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE restore_suppressions (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  path_key      TEXT NOT NULL,
  volume_serial INTEGER,
  file_id       TEXT,
  file_size     INTEGER,
  modified_at   INTEGER,
  expires_at    INTEGER NOT NULL
);

CREATE INDEX idx_move_logs_batch      ON move_logs(batch_id, id);
CREATE INDEX idx_move_logs_new_path   ON move_logs(new_path);
CREATE INDEX idx_journal_batch        ON operation_journal(batch_id);
CREATE INDEX idx_journal_open_state   ON operation_journal(state)
  WHERE state NOT IN ('completed','undone','failed');
CREATE INDEX idx_batches_created      ON batches(created_at DESC);
CREATE INDEX idx_suppress_path        ON restore_suppressions(path_key, expires_at);
"#;

/// Checks schema version downgrade protection, backs up existing databases before
/// migrating, and executes forward-only migrations inside transactions.
pub fn run_migrations(
    conn: &mut Connection,
    backup_dir: &Path,
    now_ms: i64,
) -> Result<u32, AppError> {
    let current_version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|err| AppError::Database {
            context: "read PRAGMA user_version".to_owned(),
            message: err.to_string(),
        })?;

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(AppError::DatabaseDowngrade {
            found_version: current_version,
            supported_version: LATEST_SCHEMA_VERSION,
        });
    }

    if current_version == LATEST_SCHEMA_VERSION {
        return Ok(current_version);
    }

    let has_existing_tables: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )
        .map_err(|err| AppError::Database {
            context: "inspect existing schema tables".to_owned(),
            message: err.to_string(),
        })?;

    if current_version > 0 || has_existing_tables {
        create_pre_migration_backup(conn, backup_dir, current_version, now_ms)?;
        app_data::prune_old_backups(backup_dir, 3)?;
    }

    if current_version < 1 {
        apply_migration(conn, 1, "initial_schema", MIGRATION_001_SQL, now_ms)?;
    }

    if current_version < 2 {
        apply_migration(conn, 2, "smart_rule_filters", MIGRATION_002_SQL, now_ms)?;
    }

    Ok(LATEST_SCHEMA_VERSION)
}

pub const MIGRATION_002_SQL: &str = r#"
ALTER TABLE rules ADD COLUMN name_contains TEXT;
ALTER TABLE rules ADD COLUMN min_size_bytes INTEGER CHECK (min_size_bytes IS NULL OR min_size_bytes >= 0);
ALTER TABLE rules ADD COLUMN max_size_bytes INTEGER CHECK (max_size_bytes IS NULL OR max_size_bytes >= 0);
ALTER TABLE rules ADD COLUMN min_age_days INTEGER CHECK (min_age_days IS NULL OR min_age_days >= 0);
"#;

fn create_pre_migration_backup(
    conn: &Connection,
    backup_dir: &Path,
    from_version: u32,
    now_ms: i64,
) -> Result<(), AppError> {
    app_data::ensure_dir(backup_dir)?;

    let mut attempt: u32 = 0;
    let backup_path = loop {
        let candidate_name = if attempt == 0 {
            format!("sortdeck-v{from_version}-{now_ms}.db")
        } else {
            format!("sortdeck-v{from_version}-{now_ms}-{attempt}.db")
        };
        let candidate = backup_dir.join(candidate_name);
        if !candidate.exists() {
            break candidate;
        }
        attempt = attempt.saturating_add(1);
    };

    let mut dst_conn = Connection::open(&backup_path).map_err(|err| AppError::Database {
        context: "open pre-migration backup connection".to_owned(),
        message: err.to_string(),
    })?;

    {
        let backup = Backup::new(conn, &mut dst_conn).map_err(|err| AppError::Database {
            context: "initialize SQLite online backup".to_owned(),
            message: err.to_string(),
        })?;
        backup
            .run_to_completion(100, Duration::from_millis(5), None)
            .map_err(|err| AppError::Database {
                context: "execute SQLite online backup".to_owned(),
                message: err.to_string(),
            })?;
    }

    Ok(())
}

fn apply_migration(
    conn: &mut Connection,
    version: u32,
    name: &str,
    sql: &str,
    now_ms: i64,
) -> Result<(), AppError> {
    let checksum = sha256_hex(sql.as_bytes());
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: format!("begin migration v{version}"),
        message: err.to_string(),
    })?;

    tx.execute_batch(sql).map_err(|err| AppError::Database {
        context: format!("apply migration v{version} SQL"),
        message: err.to_string(),
    })?;

    tx.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
        params![version, name, checksum, now_ms],
    )
    .map_err(|err| AppError::Database {
        context: format!("record schema_migrations v{version}"),
        message: err.to_string(),
    })?;

    tx.pragma_update(None, "user_version", version)
        .map_err(|err| AppError::Database {
            context: format!("update PRAGMA user_version to {version}"),
            message: err.to_string(),
        })?;

    tx.commit().map_err(|err| AppError::Database {
        context: format!("commit migration v{version}"),
        message: err.to_string(),
    })?;

    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Seeds factory defaults once inside a single transaction and safely appends
/// newly introduced defaults (`DEFAULTS_VERSION` upgrades) without overwriting
/// existing user-edited rules (`AGENTS.md` §6.1).
pub fn seed_defaults_if_needed(conn: &mut Connection, now_ms: i64) -> Result<(), AppError> {
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin default seeding transaction".to_owned(),
        message: err.to_string(),
    })?;

    let existing_version_raw: Option<String> = tx
        .query_row(
            "SELECT value FROM settings WHERE key = 'defaults_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| AppError::Database {
            context: "check defaults_version setting".to_owned(),
            message: err.to_string(),
        })?;

    let current_defaults_version = existing_version_raw
        .as_deref()
        .and_then(|v| v.trim().parse::<u32>().ok());

    if let Some(version) = current_defaults_version {
        if version >= DEFAULTS_VERSION {
            tx.commit().map_err(|err| AppError::Database {
                context: "commit no-op default seed".to_owned(),
                message: err.to_string(),
            })?;
            return Ok(());
        }

        // Append v2 additions without overwriting existing user-edited v1 rules.
        let mut next_priority: u32 = tx
            .query_row("SELECT COALESCE(MAX(priority), 0) FROM rules", [], |row| {
                row.get::<_, u32>(0)
            })
            .map_err(|err| AppError::Database {
                context: "query max rule priority".to_owned(),
                message: err.to_string(),
            })?;

        for spec in &DEFAULT_RULES_V2_ADDITIONS {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM rules WHERE default_key = ?1)",
                    params![spec.default_key],
                    |row| row.get(0),
                )
                .map_err(|err| AppError::Database {
                    context: format!("check default rule {}", spec.default_key),
                    message: err.to_string(),
                })?;

            if !exists {
                next_priority = next_priority.saturating_add(1);
                insert_default_rule(&tx, spec, next_priority, now_ms)?;
            }
        }

        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('defaults_version', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![DEFAULTS_VERSION.to_string(), now_ms],
        )
        .map_err(|err| AppError::Database {
            context: "update defaults_version".to_owned(),
            message: err.to_string(),
        })?;

        tx.commit().map_err(|err| AppError::Database {
            context: "commit default rules upgrade transaction".to_owned(),
            message: err.to_string(),
        })?;
        return Ok(());
    }

    let defaults = AppSettings::default();
    let settings_json = serde_json::to_string(&defaults).map_err(|err| AppError::Internal {
        message: format!("serialize default settings: {err}"),
    })?;

    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES ('app_settings', ?1, ?2)",
        params![settings_json, now_ms],
    )
    .map_err(|err| AppError::Database {
        context: "seed app_settings".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES ('config_revision', '1', ?1)",
        params![now_ms],
    )
    .map_err(|err| AppError::Database {
        context: "seed config_revision".to_owned(),
        message: err.to_string(),
    })?;

    for (idx, spec) in ALL_DEFAULT_RULES.iter().enumerate() {
        let priority = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        insert_default_rule(&tx, spec, priority, now_ms)?;
    }

    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('defaults_version', ?1, ?2)",
        params![DEFAULTS_VERSION.to_string(), now_ms],
    )
    .map_err(|err| AppError::Database {
        context: "record defaults_version".to_owned(),
        message: err.to_string(),
    })?;

    tx.commit().map_err(|err| AppError::Database {
        context: "commit default seeding transaction".to_owned(),
        message: err.to_string(),
    })?;

    Ok(())
}

fn insert_default_rule(
    tx: &rusqlite::Transaction<'_>,
    spec: &DefaultRuleSpec,
    priority: u32,
    now_ms: i64,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO rules (name, destination, enabled, priority, is_default, default_key, created_at, updated_at)
         VALUES (?1, ?2, 1, ?3, 1, ?4, ?5, ?5)",
        params![spec.name, spec.destination, priority, spec.default_key, now_ms],
    )
    .map_err(|err| AppError::Database {
        context: format!("seed default rule {}", spec.name),
        message: err.to_string(),
    })?;

    let rule_id = tx.last_insert_rowid();
    for ext in spec.extensions {
        tx.execute(
            "INSERT INTO rule_extensions (rule_id, extension) VALUES (?1, ?2)",
            params![rule_id, ext],
        )
        .map_err(|err| AppError::Database {
            context: format!("seed default extension {ext}"),
            message: err.to_string(),
        })?;
    }
    Ok(())
}
