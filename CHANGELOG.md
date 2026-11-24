# Changelog

All notable changes to SortDeck are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-24

### Added

- **Core Sorting Engine & Safety Rules**
  - Read-only folder scanner (`engine/scanner.rs`) enforcing `.sortignore` (self and ancestors), VCS project roots (`.git`, `.hg`, `.svn`), excluded folders (`node_modules`, `.venv`, `__pycache__`), reparse-point skipping, Office `~$` lock detection, temp download extensions, depth limits, and scan caps.
  - Longest-first multi-dot extension matching (`engine/rules.rs`), smart rule filters (`nameContains`, `minSizeBytes`, `maxSizeBytes`, `minAgeDays`), and `{year}`/`{month}`/`{day}` destination date subfolder templates.
  - Case-insensitive collision name reservation (`engine/naming.rs`) preserving compound archive extensions (`.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`).
  - Backend-owned dry-run planner (`engine/planner.rs`) with `SkipReason` reporting, paginated `get_plan_page` queries, TTL expiry, `config_revision` invalidation, and execution-time revalidation.

- **Win32 Safe Move Engine, SQLite Journal & Recovery**
  - Dedicated `FsBackend` implementation in `platform/windows/` (`MoveFileExW` without replace/copy, `CopyFileExW` with `COPY_FILE_FAIL_IF_EXISTS` and progress cancellation, `FlushFileBuffers`, `GetFileInformationByHandle`, `GetVolumeInformationW`, `GetDiskFreeSpaceExW`, `DeleteFileW`, `RemoveDirectoryW`, RAII handles, extended `\\?\` wide paths, and typed OS error codes).
  - Write-ahead `safe_move()` state machine (`engine/mover.rs`), reverse-order `execute_undo()` (`engine/undo.rs`) with 24h `restore_suppressions`, startup recovery matrix (`recovery/mod.rs`), and bounded `ActiveJobs` registry (`jobs/mod.rs`).
  - Embedded SQLite WAL persistence (`database/`) with `synchronous = FULL`, online pre-migration backups (retaining the last 3), corruption preservation (`sortdeck.corrupt-*.db`), downgrade protection, v2 defaults (`Adobe Creative`), and schema migrations.

- **Desktop App & Smart Organizing Tools**
  - Tauri v2 + React 19 + TypeScript desktop UI with Architectural Carbon (dark default) and Warm Alabaster (light/system) themes (`src/styles/index.css`), custom SortDeck emblem icons, and bidirectional/control character filename sanitization (`src/utils/paths.ts`).
  - Persistent sidebar navigation (`Home`, `Folders`, `Rules`, `History`, `Fix Issues`, `Settings`), first-run onboarding card with Downloads suggestion (`auto_sort = false`), virtualized Dry-Run Preview with per-file checkboxes, priority-reorderable Rules editor with overlap warnings, paginated History viewer with Explorer reveal (`SHOpenFolderAndSelectItems`) and undo, and redacted diagnostics export.
  - One-click Starter Rule Packs modal (`Adobe App Folders`, `Screenshots & Receipts`, `Camera RAW Photos by Date`, `3D, CAD & Music Projects`), **Find a Moved File** search (`search_move_logs` + Put Back), and read-only **Folder Space & Duplicate Check** (`analyze_folder_storage` with SHA-256 duplicate grouping).
