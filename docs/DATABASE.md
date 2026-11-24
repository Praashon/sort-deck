# SortDeck Database Specification

## Storage & Connection Pragmas

- Database path: `%LOCALAPPDATA%\SortDeck\sortdeck.db` (overridden by `SORTDECK_DATA_DIR` in dev/tests; UNC/network shares are rejected because SQLite WAL is unsafe on network filesystems).
- Applied on every connection open:
  - `PRAGMA foreign_keys = ON;`
  - `PRAGMA journal_mode = WAL;`
  - `PRAGMA synchronous = FULL;`
  - `PRAGMA busy_timeout = 5000;`
  - `PRAGMA trusted_schema = OFF;`
- Startup integrity:
  - `PRAGMA quick_check`: on corruption, preserves the original file as `sortdeck.corrupt-<timestamp>.db` (never deleting user history silently) and starts a fresh database.
  - Downgrade protection: if `PRAGMA user_version` exceeds the app's supported version (`2`), opening fails with `DATABASE_DOWNGRADE`.
  - Pre-migration backup: before applying pending migrations to an existing database, takes an online backup via SQLite's `Backup` API into `backups/` and retains the last 3 backups.

## Schema v1 & v2 (`MIGRATION_001_SQL`, `MIGRATION_002_SQL`)

Tables defined in [migrations.rs](../src-tauri/src/database/migrations.rs):

- `schema_migrations(version, name, checksum, applied_at)`
- `settings(key, value, updated_at)`: stores `app_settings`, `defaults_version` (`2`), and monotonic `config_revision`
- `watched_folders(id, path, path_key, enabled, auto_sort, recursive, max_depth, min_file_age_secs, status, auto_sort_enabled_at, last_reconciled_at, created_at, updated_at)`
- `rules(id, name, destination, enabled, priority, is_default, default_key, created_at, updated_at, name_contains, min_size_bytes, max_size_bytes, min_age_days)`
- `rule_extensions(rule_id, extension)`: normalized dotless lowercase extensions with `ON DELETE CASCADE`
- `batches`, `move_logs`, `operation_journal`, `created_directories`, `restore_suppressions`: written by execution (`transactions.rs`), undo (`engine/undo.rs`), and startup recovery (`recovery/mod.rs`).
- Indexes: `idx_move_logs_batch`, `idx_move_logs_new_path`, `idx_journal_batch`, `idx_journal_open_state`, `idx_batches_created`, `idx_suppress_path`.

## Journal State Machine & Recovery Matrix

`JournalState::can_transition` enforces valid state transitions:

- **Same-volume:** `planned -> completed | failed | recovery_required`
- **Cross-volume:** `planned -> copying -> verified -> committed -> completed` (or `-> failed | recovery_required`)
- **Undo:** `undo_pending -> undone | failed | recovery_required`
- **Atomic completion:** `complete_move_atomically` writes `move_logs` and marks `operation_journal.state = 'completed'` inside one SQLite transaction. `complete_undo_atomically` updates `move_logs`, `operation_journal`, and inserts a 24h `restore_suppressions` row in one transaction.
