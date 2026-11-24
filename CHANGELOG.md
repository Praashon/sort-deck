# Changelog

All notable changes to SortDeck will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Phase 4 frontend application & smart organizing features:
  - Architectural Carbon (dark default) & Warm Alabaster (light/system) CSS-first theme system (`src/styles/index.css`), bespoke multi-layer SortDeck brand icon (`BrandLogo.tsx` and generated Windows icons), plain-English localization catalog (`src/i18n/en.ts`), and bidirectional/control character filename sanitization (`src/utils/paths.ts`).
  - Schema v2 (`MIGRATION_002_SQL`, `defaults_version = 2`) adding the built-in Adobe Creative rule (`Adobe/`), smart rule filters (`nameContains`, `minSizeBytes`, `maxSizeBytes`, `minAgeDays`), and `{year}`/`{month}`/`{day}` destination date subfolder templates.
  - One-click Starter Rule Packs modal (`StarterPacksModal.tsx`) for Adobe App Folders, Screenshots & Receipts, Camera RAW Photos by Date, and 3D/CAD & Music Projects.
  - "Find a Moved File" search (`search_move_logs` + `MovedFileSearch.tsx`) with direct File Explorer selection and one-click Put Back undo, plus read-only Folder Space & Duplicate Check (`analyze_folder_storage` + `FolderInsightsModal.tsx`) with byte-for-byte SHA-256 duplicate grouping.
  - Persistent sidebar shell (`Dashboard`, `Folders`, `Rules`, `History`, `Recovery`, `Settings`), first-run onboarding card with Downloads suggestion (`auto_sort = false`), virtualized Dry-Run Plan Preview (`@tanstack/react-virtual` + `get_plan_page` + selective deselect), priority-reorderable Rules editor with overlap warnings, paginated History viewer with batch/per-item undo + Explorer reveal (`SHOpenFolderAndSelectItems`), Recovery Center, and redacted diagnostics export.
  - Phase 4 frontend test suites (`src/App.test.tsx`, `src/utils/paths.test.ts`, `src/components/common/StatusBadge.test.tsx`) verifying `INV-14` status fidelity, bidi escaping, smart rules/starter packs, moved-file search, folder insights, and WCAG 2.2 AA accessibility via `vitest-axe` across every view and modal.

- Phase 3 safe file operations and recovery:
  - Win32 `FsBackend` implementation in `platform/windows/` (`MoveFileExW` without replace/copy, `CopyFileExW` with `COPY_FILE_FAIL_IF_EXISTS` and progress cancellation, `FlushFileBuffers`, `GetFileInformationByHandle`, `GetVolumeInformationW`, `GetDiskFreeSpaceExW`, `DeleteFileW`, `RemoveDirectoryW`, RAII `OwnedHandle`, extended `\\?\` wide paths, and explicit §4.7 error mapping).
  - `safe_move()` state machine (`engine/mover.rs`), `execute_undo()` (`engine/undo.rs`) with 24h `restore_suppressions`, startup recovery matrix (`recovery/mod.rs`), `InFlightRegistry` and `ActiveJobs` (`jobs/mod.rs`), and `FaultyFsBackend` (`fsops/faulty.rs`).
  - Execution, undo, batch history, and recovery IPC commands (`execute_plan`, `cancel_batch`, `undo_batch`, `list_batches`, `get_batch_detail`, `list_recovery_items`, `acknowledge_recovery_item`, `retry_recovery`).
  - Phase 3 exit-gate suites (`safe_mover_and_undo.rs`, child-process `crash_recovery.rs`, and `#[ignore]` `vhdx_cross_volume.rs` + `scripts/create-test-vhdx.ps1`) verifying `INV-01`–`INV-15`.

- Phase 2 scanner and rule engine:
  - Read-only directory scanner (`engine/scanner.rs`) enforcing `.sortignore` (self + ancestors), project roots, excluded directory names, reparse-point skipping, Office `~$` lock detection, temp download extensions, attribute exclusions, depth caps, cancellation, and scan limits.
  - Longest-first multi-dot extension matcher (`engine/rules.rs`) and case-insensitive collision name reservation (`engine/naming.rs`) preserving compound archive extensions (`.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`).
  - Backend-owned dry-run planner (`engine/planner.rs`) and bounded `PlanStore` service (`services/planner.rs`) with `SkipReason` taxonomy, paginated `get_plan_page` queries, TTL expiry, `config_revision` invalidation, and `revalidate_plan` (`INV-11`).
  - Phase 2 exit-gate tests (`scanner_planner.rs`, `src/i18n/en.test.ts`) verifying `INV-05`, `INV-06`, `INV-07`, `INV-11`, and `INV-12`.

- Phase 1 foundation and database:
  - Embedded SQLite (`rusqlite` bundled + backup) with required pragmas, pre-migration online backups (retaining last 3), downgrade protection, startup `PRAGMA quick_check`, and non-destructive corrupt DB preservation (`sortdeck.corrupt-*.db`).
  - Full v1 schema (`schema_migrations`, `settings`, `watched_folders`, `rules`, `rule_extensions`, `batches`, `move_logs`, `operation_journal`, `created_directories`, `restore_suppressions`) and 7 factory default rules with `defaults_version` and monotonic `config_revision`.
  - `engine/paths.rs` and `engine/rules.rs` for Windows path normalization, protected-location policy, project-root/reparse-point/overlap rejection, extension normalization, and overlap warnings.
  - Watched-folder, rule (with dense priority reordering and restore-defaults), and settings services, thin IPC commands, generated TypeScript bindings, and frontend IPC wrappers.
  - Phase 1 exit-gate integration and `proptest` suites (`database_migrations`, `path_policy`, `crud_services`).

- Phase 0 repository bootstrap and engineering baseline:
  - Tauri v2 + React 19 + TypeScript (strict) + Tailwind CSS v4 + Vite scaffold.
  - Toolchain pinning (`.nvmrc`, `rust-toolchain.toml`) and formatting/linting baselines (ESLint, Prettier, rustfmt, clippy with `disallowed-methods`, `cargo-deny` policy).
  - Core backend skeletons (`errors.rs`, `config.rs`, `logging.rs`, `fsops::FsBackend` trait, `fsops::sandbox::SandboxGuard` and `SandboxFsBackend`, `platform/windows` FFI boundary).
  - Rust-to-TypeScript IPC binding generation via `ts-rs` (`scripts/gen-types.mjs`) and banned-API source guard (`scripts/check-banned-apis.mjs`).
  - Minimal CSP, capability baseline, `tauri-plugin-single-instance`, and Windows `asInvoker` + long-path + `PerMonitorV2` DPI manifest.
  - Documentation skeletons (`ARCHITECTURE.md`, `SAFETY.md`, `DATABASE.md`, `IPC.md`, `TESTING.md`, `RELEASE.md`) and GitHub CI/CodeQL/Dependabot templates.
