use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;
use sortdeck_lib::database::Database;
use sortdeck_lib::errors::AppError;
use sortdeck_lib::fsops::sandbox::SandboxGuard;

fn create_sandbox_data_dir() -> Result<(tempfile::TempDir, std::path::PathBuf), AppError> {
    let dir = tempfile::tempdir().map_err(|e| AppError::from_io("create temp dir", e))?;
    SandboxGuard::init_sandbox_dir(dir.path())?;
    let data_dir = dir.path().join("data");
    Ok((dir, data_dir))
}

#[test]
fn migration_initializes_empty_database_with_pragmas_schema_and_defaults() {
    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    let db = Database::open_in_dir(&data_dir).unwrap();

    assert_eq!(db.schema_version(), 2);
    assert!(!db.recovered_from_corruption());
    assert!(db.corrupt_backup_path().is_none());

    db.with_writer(|conn| {
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        let trusted: i64 = conn
            .query_row("PRAGMA trusted_schema", [], |r| r.get(0))
            .unwrap();
        let user_ver: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();

        assert_eq!(fk, 1);
        assert_eq!(journal.to_lowercase(), "wal");
        assert_eq!(sync, 2);
        assert_eq!(trusted, 0);
        assert_eq!(user_ver, 2);

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let tables: HashSet<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for expected in [
            "schema_migrations",
            "settings",
            "watched_folders",
            "rules",
            "rule_extensions",
            "batches",
            "move_logs",
            "operation_journal",
            "created_directories",
            "restore_suppressions",
        ] {
            assert!(tables.contains(expected), "missing table {expected}");
        }

        let mut idx_stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_%'")
            .unwrap();
        let indexes: HashSet<String> = idx_stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for expected_idx in [
            "idx_move_logs_batch",
            "idx_move_logs_new_path",
            "idx_journal_batch",
            "idx_journal_open_state",
            "idx_batches_created",
            "idx_suppress_path",
        ] {
            assert!(
                indexes.contains(expected_idx),
                "missing index {expected_idx}"
            );
        }

        let rule_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rules WHERE is_default = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rule_count, 8);

        let checksum: String = conn
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(checksum.len(), 64);

        Ok(())
    })
    .unwrap();
}

#[test]
fn migration_from_existing_fixture_creates_pre_migration_backup_and_prunes_to_three() {
    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    std::fs::create_dir_all(data_dir.join("backups")).unwrap();

    // Seed 3 pre-existing backups so adding the new pre-migration backup triggers pruning to 3.
    for idx in 1..=3 {
        let old_path = data_dir
            .join("backups")
            .join(format!("sortdeck-v0-100000{idx}.db"));
        let conn = Connection::open(&old_path).unwrap();
        conn.execute("CREATE TABLE old_backup (id INTEGER PRIMARY KEY)", [])
            .unwrap();
    }

    // Create a pre-migration v0 fixture database containing an existing table.
    let db_path = data_dir.join("sortdeck.db");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE legacy_marker (id INTEGER PRIMARY KEY, note TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO legacy_marker (note) VALUES ('v0-fixture')", [])
            .unwrap();
    }

    let db = Database::open_in_dir(&data_dir).unwrap();
    assert_eq!(db.schema_version(), 2);

    let backups: Vec<String> = std::fs::read_dir(db.backup_dir())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("sortdeck-") && name.ends_with(".db") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(backups.len(), 3);
    assert!(backups
        .iter()
        .any(|name| name.starts_with("sortdeck-v0-") && !name.contains("1000001")));
}

#[test]
fn downgrade_protection_refuses_newer_schema_version() {
    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("sortdeck.db");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 99_u32).unwrap();
    }

    let err = match Database::open_in_dir(&data_dir) {
        Ok(_) => panic!("expected newer schema version to be refused"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "DATABASE_DOWNGRADE");
}

#[test]
fn corruption_preserves_corrupt_file_and_starts_fresh_database() {
    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("sortdeck.db");
    let corrupt_payload = b"this is definitely not a valid sqlite3 database header";
    std::io::Write::write_all(
        &mut std::fs::File::create(&db_path).unwrap(),
        corrupt_payload,
    )
    .unwrap();

    let db = Database::open_in_dir(&data_dir).unwrap();
    assert!(db.recovered_from_corruption());
    let preserved = db
        .corrupt_backup_path()
        .expect("corrupt backup path recorded");
    assert!(preserved.exists());
    assert!(preserved
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("sortdeck.corrupt-"));
    assert_eq!(std::fs::read(preserved).unwrap(), corrupt_payload);

    db.with_writer(|conn| {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rules", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 8);
        Ok(())
    })
    .unwrap();
}

#[test]
fn upgrade_from_defaults_v1_appends_adobe_without_overwriting_user_edits() {
    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    let db_path = data_dir.join("sortdeck.db");
    sortdeck_lib::fsops::app_data::ensure_dir(&data_dir).unwrap();

    // Create a v1-seeded DB with a user-renamed Images rule and defaults_version = 1.
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(sortdeck_lib::database::migrations::MIGRATION_001_SQL)
        .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (1, 'initial', 'abc', 1000)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1_u32).unwrap();
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES ('defaults_version', '1', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO rules (name, destination, enabled, priority, is_default, default_key, created_at, updated_at)
         VALUES ('My Photos', 'CustomPhotos/', 1, 1, 1, 'default.images', 1000, 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let db = Database::open_in_dir(&data_dir).unwrap();
    db.with_writer(|c| {
        let images_name: String = c
            .query_row(
                "SELECT name FROM rules WHERE default_key = 'default.images'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(images_name, "My Photos");

        let adobe_dest: String = c
            .query_row(
                "SELECT destination FROM rules WHERE default_key = 'default.adobe'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(adobe_dest, "Adobe/");

        let version: String = c
            .query_row(
                "SELECT value FROM settings WHERE key = 'defaults_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");

        // Ensure v2 schema migration added smart filter columns cleanly on v1 DB.
        c.execute(
            "UPDATE rules SET name_contains = 'Screenshot', min_size_bytes = 1024 WHERE default_key = 'default.images'",
            [],
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
}

#[test]
fn database_constraints_enforce_foreign_keys_and_reject_unc_data_dir() {
    let unc_err = match Database::open_in_dir(Path::new(r"\\fileserver\share\SortDeck")) {
        Ok(_) => panic!("expected UNC data_dir to be rejected"),
        Err(err) => err,
    };
    assert_eq!(unc_err.code(), "PATH_VALIDATION");

    let (_guard, data_dir) = create_sandbox_data_dir().unwrap();
    let db = Database::open_in_dir(&data_dir).unwrap();
    db.with_writer(|conn| {
        let res = conn.execute(
            "INSERT INTO rule_extensions (rule_id, extension) VALUES (999999, 'png')",
            [],
        );
        assert!(res.is_err(), "foreign key must reject missing rule_id");

        let dot_res = conn.execute(
            "INSERT INTO rule_extensions (rule_id, extension) VALUES (1, '.png')",
            [],
        );
        assert!(dot_res.is_err(), "CHECK must reject leading dot");
        Ok(())
    })
    .unwrap();
}
