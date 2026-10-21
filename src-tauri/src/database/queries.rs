#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::config::AppSettings;
use crate::database::migrations::ALL_DEFAULT_RULES;
use crate::errors::AppError;
use crate::models::{FolderStatus, Rule, UpdateWatchedFolderRequest, WatchedFolder};

/// Returns the current UTC Unix timestamp in milliseconds.
#[must_use]
pub fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|dur| i64::try_from(dur.as_millis()).ok())
        .unwrap_or(0)
}

/// Reads the current monotonic configuration revision counter.
pub fn get_config_revision(conn: &Connection) -> Result<u64, AppError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'config_revision'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| AppError::Database {
            context: "read config_revision".to_owned(),
            message: err.to_string(),
        })?;

    Ok(raw
        .as_deref()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(1))
}

fn bump_config_revision(tx: &rusqlite::Transaction<'_>, now_ms: i64) -> Result<u64, AppError> {
    let current = get_config_revision(tx)?;
    let next = current.saturating_add(1);
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('config_revision', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![next.to_string(), now_ms],
    )
    .map_err(|err| AppError::Database {
        context: "increment config_revision".to_owned(),
        message: err.to_string(),
    })?;
    Ok(next)
}

/// Reads persisted application settings, falling back to v1 defaults if missing.
pub fn get_settings(conn: &Connection) -> Result<AppSettings, AppError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app_settings'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| AppError::Database {
            context: "read app_settings".to_owned(),
            message: err.to_string(),
        })?;

    match raw {
        Some(json) => serde_json::from_str(&json).map_err(|err| AppError::Database {
            context: "deserialize app_settings".to_owned(),
            message: err.to_string(),
        }),
        None => Ok(AppSettings::default()),
    }
}

/// Persists validated application settings and bumps `config_revision`.
pub fn save_settings(
    conn: &mut Connection,
    settings: &AppSettings,
) -> Result<AppSettings, AppError> {
    let now_ms = now_epoch_ms();
    let json = serde_json::to_string(settings).map_err(|err| AppError::Internal {
        message: format!("serialize settings: {err}"),
    })?;

    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin save_settings transaction".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('app_settings', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![json, now_ms],
    )
    .map_err(|err| AppError::Database {
        context: "update app_settings".to_owned(),
        message: err.to_string(),
    })?;

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit save_settings".to_owned(),
        message: err.to_string(),
    })?;

    Ok(settings.clone())
}

fn row_to_watched_folder(row: &rusqlite::Row<'_>) -> Result<WatchedFolder, rusqlite::Error> {
    let status_raw: String = row.get(8)?;
    let status = FolderStatus::from_db(&status_raw).unwrap_or(FolderStatus::Invalid);
    Ok(WatchedFolder {
        id: row.get(0)?,
        path: row.get(1)?,
        path_key: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        auto_sort: row.get::<_, i64>(4)? != 0,
        recursive: row.get::<_, i64>(5)? != 0,
        max_depth: row.get(6)?,
        min_file_age_secs: row.get(7)?,
        status,
        auto_sort_enabled_at: row.get(9)?,
        last_reconciled_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

const SELECT_WATCHED_FOLDERS: &str = "
SELECT id, path, path_key, enabled, auto_sort, recursive, max_depth, min_file_age_secs,
       status, auto_sort_enabled_at, last_reconciled_at, created_at, updated_at
FROM watched_folders
ORDER BY id ASC
";

/// Lists all registered watched folders.
pub fn list_watched_folders(conn: &Connection) -> Result<Vec<WatchedFolder>, AppError> {
    let mut stmt = conn
        .prepare(SELECT_WATCHED_FOLDERS)
        .map_err(|err| AppError::Database {
            context: "prepare list_watched_folders".to_owned(),
            message: err.to_string(),
        })?;

    let rows = stmt
        .query_map([], row_to_watched_folder)
        .map_err(|err| AppError::Database {
            context: "query list_watched_folders".to_owned(),
            message: err.to_string(),
        })?;

    let mut folders = Vec::new();
    for row_res in rows {
        folders.push(row_res.map_err(|err| AppError::Database {
            context: "map watched_folder row".to_owned(),
            message: err.to_string(),
        })?);
    }
    Ok(folders)
}

/// Fetches a single watched folder by ID.
pub fn get_watched_folder(conn: &Connection, id: i64) -> Result<WatchedFolder, AppError> {
    conn.query_row(
        "SELECT id, path, path_key, enabled, auto_sort, recursive, max_depth, min_file_age_secs,
                status, auto_sort_enabled_at, last_reconciled_at, created_at, updated_at
         FROM watched_folders WHERE id = ?1",
        params![id],
        row_to_watched_folder,
    )
    .optional()
    .map_err(|err| AppError::Database {
        context: "get watched_folder".to_owned(),
        message: err.to_string(),
    })?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("watched folder {id}"),
        os_error_code: None,
    })
}

/// Inserts a new watched folder with `auto_sort` disabled by default (`AGENTS.md` §1.2 #8).
pub fn insert_watched_folder(
    conn: &mut Connection,
    path: &str,
    path_key: &str,
    recursive: bool,
    max_depth: u32,
    min_file_age_secs: u32,
) -> Result<WatchedFolder, AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin insert_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "INSERT INTO watched_folders
         (path, path_key, enabled, auto_sort, recursive, max_depth, min_file_age_secs, status, created_at, updated_at)
         VALUES (?1, ?2, 1, 0, ?3, ?4, ?5, 'ok', ?6, ?6)",
        params![
            path,
            path_key,
            i64::from(recursive),
            max_depth,
            min_file_age_secs,
            now_ms
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    let id = tx.last_insert_rowid();
    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit insert_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    get_watched_folder(conn, id)
}

/// Updates an existing watched folder and sets `auto_sort_enabled_at` when auto-sort is turned on.
pub fn update_watched_folder(
    conn: &mut Connection,
    req: &UpdateWatchedFolderRequest,
) -> Result<WatchedFolder, AppError> {
    let now_ms = now_epoch_ms();
    let existing = get_watched_folder(conn, req.id)?;
    let auto_sort_enabled_at = match (existing.auto_sort, req.auto_sort) {
        (false, true) => Some(now_ms),
        (true, true) => existing.auto_sort_enabled_at.or(Some(now_ms)),
        (_, false) => None,
    };

    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin update_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    tx.execute(
        "UPDATE watched_folders
         SET enabled = ?1, auto_sort = ?2, recursive = ?3, max_depth = ?4,
             min_file_age_secs = ?5, auto_sort_enabled_at = ?6, updated_at = ?7
         WHERE id = ?8",
        params![
            i64::from(req.enabled),
            i64::from(req.auto_sort),
            i64::from(req.recursive),
            req.max_depth,
            req.min_file_age_secs,
            auto_sort_enabled_at,
            now_ms,
            req.id
        ],
    )
    .map_err(|err| AppError::Database {
        context: "update watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit update_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    get_watched_folder(conn, req.id)
}

/// Removes a watched folder configuration without touching any files on disk.
pub fn delete_watched_folder(conn: &mut Connection, id: i64) -> Result<(), AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin delete_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    let affected = tx
        .execute("DELETE FROM watched_folders WHERE id = ?1", params![id])
        .map_err(|err| AppError::Database {
            context: "delete watched_folder".to_owned(),
            message: err.to_string(),
        })?;

    if affected == 0 {
        return Err(AppError::NotFound {
            resource: format!("watched folder {id}"),
            os_error_code: None,
        });
    }

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit delete_watched_folder".to_owned(),
        message: err.to_string(),
    })?;

    Ok(())
}

/// Lists all sorting rules in deterministic `(priority ASC, id ASC)` order.
pub fn list_rules(conn: &Connection) -> Result<Vec<Rule>, AppError> {
    let mut ext_stmt = conn
        .prepare(
            "SELECT rule_id, extension FROM rule_extensions ORDER BY rule_id ASC, extension ASC",
        )
        .map_err(|err| AppError::Database {
            context: "prepare rule_extensions query".to_owned(),
            message: err.to_string(),
        })?;

    let mut extensions_by_rule: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let ext_rows = ext_stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| AppError::Database {
            context: "query rule_extensions".to_owned(),
            message: err.to_string(),
        })?;

    for row_res in ext_rows {
        let (rule_id, ext) = row_res.map_err(|err| AppError::Database {
            context: "map rule_extension row".to_owned(),
            message: err.to_string(),
        })?;
        extensions_by_rule.entry(rule_id).or_default().push(ext);
    }

    let mut rule_stmt = conn
        .prepare(
            "SELECT id, name, destination, enabled, priority, is_default, default_key,
                    name_contains, min_size_bytes, max_size_bytes, min_age_days,
                    created_at, updated_at
             FROM rules
             ORDER BY priority ASC, id ASC",
        )
        .map_err(|err| AppError::Database {
            context: "prepare list_rules query".to_owned(),
            message: err.to_string(),
        })?;

    let rule_rows = rule_stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let min_size = row
                .get::<_, Option<i64>>(8)?
                .and_then(|v| u64::try_from(v).ok());
            let max_size = row
                .get::<_, Option<i64>>(9)?
                .and_then(|v| u64::try_from(v).ok());
            let min_age = row
                .get::<_, Option<i64>>(10)?
                .and_then(|v| u32::try_from(v).ok());
            Ok(Rule {
                id,
                name: row.get(1)?,
                destination: row.get(2)?,
                extensions: Vec::new(),
                enabled: row.get::<_, i64>(3)? != 0,
                priority: row.get(4)?,
                is_default: row.get::<_, i64>(5)? != 0,
                default_key: row.get(6)?,
                name_contains: row.get(7)?,
                min_size_bytes: min_size,
                max_size_bytes: max_size,
                min_age_days: min_age,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|err| AppError::Database {
            context: "query rules".to_owned(),
            message: err.to_string(),
        })?;

    let mut rules = Vec::new();
    for row_res in rule_rows {
        let mut rule = row_res.map_err(|err| AppError::Database {
            context: "map rule row".to_owned(),
            message: err.to_string(),
        })?;
        if let Some(exts) = extensions_by_rule.remove(&rule.id) {
            rule.extensions = exts;
        }
        rules.push(rule);
    }

    Ok(rules)
}

/// Fetches a single rule by ID.
pub fn get_rule(conn: &Connection, id: i64) -> Result<Rule, AppError> {
    let rules = list_rules(conn)?;
    rules
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::NotFound {
            resource: format!("rule {id}"),
            os_error_code: None,
        })
}

/// Inserts a new custom rule at the lowest priority (`max + 1`).
#[allow(clippy::too_many_arguments)]
pub fn insert_rule(
    conn: &mut Connection,
    name: &str,
    destination: &str,
    extensions: &[String],
    enabled: bool,
    name_contains: Option<&str>,
    min_size_bytes: Option<u64>,
    max_size_bytes: Option<u64>,
    min_age_days: Option<u32>,
) -> Result<Rule, AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin insert_rule".to_owned(),
        message: err.to_string(),
    })?;

    let next_priority: u32 = tx
        .query_row(
            "SELECT COALESCE(MAX(priority), 0) + 1 FROM rules",
            [],
            |row| row.get(0),
        )
        .map_err(|err| AppError::Database {
            context: "compute next rule priority".to_owned(),
            message: err.to_string(),
        })?;

    let min_size_i64 = min_size_bytes.map(|v| i64::try_from(v).unwrap_or(i64::MAX));
    let max_size_i64 = max_size_bytes.map(|v| i64::try_from(v).unwrap_or(i64::MAX));
    let min_age_i64 = min_age_days.map(i64::from);

    tx.execute(
        "INSERT INTO rules (name, destination, enabled, priority, is_default, default_key,
                            name_contains, min_size_bytes, max_size_bytes, min_age_days,
                            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            name,
            destination,
            i64::from(enabled),
            next_priority,
            name_contains,
            min_size_i64,
            max_size_i64,
            min_age_i64,
            now_ms
        ],
    )
    .map_err(|err| AppError::Database {
        context: "insert rule".to_owned(),
        message: err.to_string(),
    })?;

    let rule_id = tx.last_insert_rowid();
    for ext in extensions {
        tx.execute(
            "INSERT INTO rule_extensions (rule_id, extension) VALUES (?1, ?2)",
            params![rule_id, ext],
        )
        .map_err(|err| AppError::Database {
            context: "insert rule_extension".to_owned(),
            message: err.to_string(),
        })?;
    }

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit insert_rule".to_owned(),
        message: err.to_string(),
    })?;

    get_rule(conn, rule_id)
}

/// Updates an existing rule's name, destination, enabled status, filters, and normalized extensions.
#[allow(clippy::too_many_arguments)]
pub fn update_rule(
    conn: &mut Connection,
    id: i64,
    name: &str,
    destination: &str,
    extensions: &[String],
    enabled: bool,
    name_contains: Option<&str>,
    min_size_bytes: Option<u64>,
    max_size_bytes: Option<u64>,
    min_age_days: Option<u32>,
) -> Result<Rule, AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin update_rule".to_owned(),
        message: err.to_string(),
    })?;

    let min_size_i64 = min_size_bytes.map(|v| i64::try_from(v).unwrap_or(i64::MAX));
    let max_size_i64 = max_size_bytes.map(|v| i64::try_from(v).unwrap_or(i64::MAX));
    let min_age_i64 = min_age_days.map(i64::from);

    let affected = tx
        .execute(
            "UPDATE rules
             SET name = ?1, destination = ?2, enabled = ?3,
                 name_contains = ?4, min_size_bytes = ?5, max_size_bytes = ?6, min_age_days = ?7,
                 updated_at = ?8
             WHERE id = ?9",
            params![
                name,
                destination,
                i64::from(enabled),
                name_contains,
                min_size_i64,
                max_size_i64,
                min_age_i64,
                now_ms,
                id
            ],
        )
        .map_err(|err| AppError::Database {
            context: "update rule".to_owned(),
            message: err.to_string(),
        })?;

    if affected == 0 {
        return Err(AppError::NotFound {
            resource: format!("rule {id}"),
            os_error_code: None,
        });
    }

    tx.execute(
        "DELETE FROM rule_extensions WHERE rule_id = ?1",
        params![id],
    )
    .map_err(|err| AppError::Database {
        context: "clear rule_extensions".to_owned(),
        message: err.to_string(),
    })?;

    for ext in extensions {
        tx.execute(
            "INSERT INTO rule_extensions (rule_id, extension) VALUES (?1, ?2)",
            params![id, ext],
        )
        .map_err(|err| AppError::Database {
            context: "insert updated rule_extension".to_owned(),
            message: err.to_string(),
        })?;
    }

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit update_rule".to_owned(),
        message: err.to_string(),
    })?;

    get_rule(conn, id)
}

/// Deletes a rule and re-densifies priorities `1..=N`.
pub fn delete_rule(conn: &mut Connection, id: i64) -> Result<(), AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin delete_rule".to_owned(),
        message: err.to_string(),
    })?;

    let affected = tx
        .execute("DELETE FROM rules WHERE id = ?1", params![id])
        .map_err(|err| AppError::Database {
            context: "delete rule".to_owned(),
            message: err.to_string(),
        })?;

    if affected == 0 {
        return Err(AppError::NotFound {
            resource: format!("rule {id}"),
            os_error_code: None,
        });
    }

    densify_rule_priorities(&tx, now_ms)?;
    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit delete_rule".to_owned(),
        message: err.to_string(),
    })?;

    Ok(())
}

/// Replaces rule priorities densely (`1..=N`) from an exact permutation of existing rule IDs.
pub fn reorder_rules(conn: &mut Connection, ordered_ids: &[i64]) -> Result<Vec<Rule>, AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin reorder_rules".to_owned(),
        message: err.to_string(),
    })?;

    let existing_ids: HashSet<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM rules")
            .map_err(|err| AppError::Database {
                context: "query rule ids for reorder".to_owned(),
                message: err.to_string(),
            })?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|err| AppError::Database {
                context: "map rule ids for reorder".to_owned(),
                message: err.to_string(),
            })?;
        let mut set = HashSet::new();
        for row_res in rows {
            set.insert(row_res.map_err(|err| AppError::Database {
                context: "collect rule id".to_owned(),
                message: err.to_string(),
            })?);
        }
        set
    };

    if ordered_ids.len() != existing_ids.len() {
        return Err(AppError::InvalidInput {
            message: "orderedRuleIds must include every existing rule exactly once".to_owned(),
        });
    }

    let mut seen = HashSet::with_capacity(ordered_ids.len());
    for id in ordered_ids {
        if !existing_ids.contains(id) || !seen.insert(*id) {
            return Err(AppError::InvalidInput {
                message: format!("invalid or duplicate rule id {id} in reorder request"),
            });
        }
    }

    for (idx, id) in ordered_ids.iter().enumerate() {
        let priority = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        tx.execute(
            "UPDATE rules SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, now_ms, id],
        )
        .map_err(|err| AppError::Database {
            context: "update rule priority".to_owned(),
            message: err.to_string(),
        })?;
    }

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit reorder_rules".to_owned(),
        message: err.to_string(),
    })?;

    list_rules(conn)
}

/// Re-creates missing default rules and resets edited default rules to factory
/// values without deleting any custom rules (`AGENTS.md` §6.2).
pub fn restore_default_rules(conn: &mut Connection) -> Result<Vec<Rule>, AppError> {
    let now_ms = now_epoch_ms();
    let tx = conn.transaction().map_err(|err| AppError::Database {
        context: "begin restore_default_rules".to_owned(),
        message: err.to_string(),
    })?;

    let mut existing_defaults: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = tx
            .prepare("SELECT default_key, id FROM rules WHERE default_key IS NOT NULL")
            .map_err(|err| AppError::Database {
                context: "query existing default rules".to_owned(),
                message: err.to_string(),
            })?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|err| AppError::Database {
                context: "map existing default rules".to_owned(),
                message: err.to_string(),
            })?;
        for row_res in rows {
            let (key, id) = row_res.map_err(|err| AppError::Database {
                context: "collect existing default rule".to_owned(),
                message: err.to_string(),
            })?;
            existing_defaults.insert(key, id);
        }
    }

    for (idx, spec) in ALL_DEFAULT_RULES.iter().enumerate() {
        let priority = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        let rule_id = if let Some(&existing_id) = existing_defaults.get(spec.default_key) {
            tx.execute(
                "UPDATE rules
                 SET name = ?1, destination = ?2, enabled = 1, priority = ?3, is_default = 1,
                     name_contains = NULL, min_size_bytes = NULL, max_size_bytes = NULL, min_age_days = NULL,
                     updated_at = ?4
                 WHERE id = ?5",
                params![spec.name, spec.destination, priority, now_ms, existing_id],
            )
            .map_err(|err| AppError::Database {
                context: "reset default rule".to_owned(),
                message: err.to_string(),
            })?;
            tx.execute(
                "DELETE FROM rule_extensions WHERE rule_id = ?1",
                params![existing_id],
            )
            .map_err(|err| AppError::Database {
                context: "clear extensions on reset default rule".to_owned(),
                message: err.to_string(),
            })?;
            existing_id
        } else {
            tx.execute(
                "INSERT INTO rules (name, destination, enabled, priority, is_default, default_key, created_at, updated_at)
                 VALUES (?1, ?2, 1, ?3, 1, ?4, ?5, ?5)",
                params![spec.name, spec.destination, priority, spec.default_key, now_ms],
            )
            .map_err(|err| AppError::Database {
                context: "recreate missing default rule".to_owned(),
                message: err.to_string(),
            })?;
            tx.last_insert_rowid()
        };

        for ext in spec.extensions {
            tx.execute(
                "INSERT INTO rule_extensions (rule_id, extension) VALUES (?1, ?2)",
                params![rule_id, ext],
            )
            .map_err(|err| AppError::Database {
                context: "restore default rule extension".to_owned(),
                message: err.to_string(),
            })?;
        }
    }

    // Re-number custom rules cleanly after the restored defaults.
    let custom_ids: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM rules WHERE is_default = 0 ORDER BY priority ASC, id ASC")
            .map_err(|err| AppError::Database {
                context: "query custom rule ids".to_owned(),
                message: err.to_string(),
            })?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|err| AppError::Database {
                context: "map custom rule ids".to_owned(),
                message: err.to_string(),
            })?;
        let mut ids = Vec::new();
        for row_res in rows {
            ids.push(row_res.map_err(|err| AppError::Database {
                context: "collect custom rule id".to_owned(),
                message: err.to_string(),
            })?);
        }
        ids
    };

    let base_priority = ALL_DEFAULT_RULES.len();
    for (idx, custom_id) in custom_ids.iter().enumerate() {
        let priority = u32::try_from(base_priority + idx + 1).unwrap_or(u32::MAX);
        tx.execute(
            "UPDATE rules SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, now_ms, custom_id],
        )
        .map_err(|err| AppError::Database {
            context: "update custom rule priority after restore".to_owned(),
            message: err.to_string(),
        })?;
    }

    bump_config_revision(&tx, now_ms)?;
    tx.commit().map_err(|err| AppError::Database {
        context: "commit restore_default_rules".to_owned(),
        message: err.to_string(),
    })?;

    list_rules(conn)
}

fn densify_rule_priorities(tx: &rusqlite::Transaction<'_>, now_ms: i64) -> Result<(), AppError> {
    let ids: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM rules ORDER BY priority ASC, id ASC")
            .map_err(|err| AppError::Database {
                context: "query rules to densify".to_owned(),
                message: err.to_string(),
            })?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|err| AppError::Database {
                context: "map rules to densify".to_owned(),
                message: err.to_string(),
            })?;
        let mut out = Vec::new();
        for row_res in rows {
            out.push(row_res.map_err(|err| AppError::Database {
                context: "collect rule id to densify".to_owned(),
                message: err.to_string(),
            })?);
        }
        out
    };

    for (idx, id) in ids.iter().enumerate() {
        let priority = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        tx.execute(
            "UPDATE rules SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, now_ms, id],
        )
        .map_err(|err| AppError::Database {
            context: "update densified rule priority".to_owned(),
            message: err.to_string(),
        })?;
    }

    Ok(())
}
