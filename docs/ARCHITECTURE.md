# SortDeck Architecture

## Layering and Dependency Direction

```text
React UI (src/)
   ↓ typed IPC via src/services/ipc.ts + src/types/generated/
Tauri Commands (src-tauri/src/commands/)
   ↓ validate → call service → map AppError
Services (src-tauri/src/services/)
   ↓ orchestrate
Engine / Recovery / Watcher / Jobs (planned in Phases 1–5)
   ↓ FsBackend trait (INV-15)
FsBackend & SandboxGuard (src-tauri/src/fsops/)
   ↓ confined unsafe FFI
Windows Platform Layer (src-tauri/src/platform/windows/)
```

- `commands/` must stay thin and never touch the filesystem or database directly.
- `engine/` (Phases 1+) must never import Tauri types and must perform all filesystem access through `FsBackend`.
- All `unsafe` Win32 FFI is confined to `src-tauri/src/platform/windows/`. Every other module enforces `#![forbid(unsafe_code)]`.
- **Scanner traversal decision:** Instead of invoking `walkdir` directly on `std::fs`, `engine/scanner.rs` implements iterative BFS traversal over `FsBackend::read_dir` so every scan obeys `SandboxGuard`, supports `FsBackend` fault injection, and checks `FILE_ATTRIBUTE_REPARSE_POINT` before descending.

## Dependency Decisions (Phases 0–2)

| Dependency                                                                                      | Layer              | Purpose                                                                                                                      | Alternatives Considered                                              |
| ----------------------------------------------------------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `tauri` `2.12.7` + `tauri-build` `2.7.6`                                                        | Backend            | Desktop application shell, IPC, capabilities, Windows bundling                                                               | Electron (heavier memory footprint)                                  |
| `tauri-plugin-single-instance` `2.5.5`                                                          | Backend            | Prevent concurrent watchers/DB writers by enforcing single instance                                                          | Custom named mutex                                                   |
| `tauri-plugin-dialog` `2.8.4`                                                                   | Backend/Frontend   | Native OS folder picker for watched folders                                                                                  | Raw Win32 `IFileOpenDialog`                                          |
| `dunce` `1.0.5`                                                                                 | Backend            | Strip `\\?\` verbatim prefixes safely for Windows display paths                                                              | Manual string slicing                                                |
| `rusqlite` `0.43.0` (`bundled`, `backup`)                                                       | Backend            | Embedded SQLite with WAL, online backups, and strict pragmas                                                                 | `sqlx` (heavier async pool for single-writer local DB)               |
| `sha2` `0.11.1`                                                                                 | Backend            | Streaming SHA-256 checksums for migration verification and Phase 3 copies                                                    | `ring`                                                               |
| `uuid` `1.29.3` (`v4`)                                                                          | Backend            | Unique `plan_id` and batch UUID generation                                                                                   | Custom RNG formatting                                                |
| `windows-sys` `0.61.2` (`Win32_Storage_FileSystem`, `Win32_UI_Shell`, `Win32_System_Com`, etc.) | Backend (Windows)  | Win32 no-overwrite file primitives (`MoveFileExW`, `CopyFileExW`), file IDs, and Shell reveal (`SHOpenFolderAndSelectItems`) | `windows` crate (heavier codegen)                                    |
| `serde` `1.0.229`, `serde_json` `1.0.154`                                                       | Backend            | Typed IPC serialization and config/journal payloads                                                                          | Manual JSON building                                                 |
| `thiserror` `2.0.21`                                                                            | Backend            | Structured `AppError` enum preserving OS error codes                                                                         | `anyhow` in library (rejected for typed IPC)                         |
| `tracing`, `tracing-subscriber`, `tracing-appender`                                             | Backend            | Structured local rotating file logs                                                                                          | `log` + `env_logger`                                                 |
| `ts-rs` `12.0.1`                                                                                | Backend → Frontend | Deterministic Rust-to-TypeScript IPC type generation in `src/types/generated/`                                               | `specta` / `tauri-specta` (chosen `ts-rs` for minimal macro surface) |
| `proptest` `1.12.0`                                                                             | Dev/Test           | Property-based testing for path normalization, reserved names, and extensions                                                | Manual fuzz loops                                                    |
| `tempfile` `3.27.0`                                                                             | Dev/Test           | Ephemeral isolated test sandbox creation (`TestSandbox`)                                                                     | Fixed test directories                                               |
| `react` `19.3.1`, `react-dom` `19.3.1`, `react-router` `8.5.2`                                  | Frontend           | Component UI and in-memory routing (`MemoryRouter`)                                                                          | Vanilla TS                                                           |
| `tailwindcss` `4.3.5`, `@tailwindcss/vite` `4.3.5`                                              | Frontend           | CSS-first `@theme` design tokens                                                                                             | CSS modules                                                          |
| `lucide-react` `1.79.0`                                                                         | Frontend           | Consistent iconography                                                                                                       | Custom SVG set                                                       |
| `@tanstack/react-virtual` `3.14.28`                                                             | Frontend           | Virtualized plan preview and history lists (Phase 4)                                                                         | Unvirtualized DOM lists                                              |
| `vitest`, `@testing-library/react`, `vitest-axe`                                                | Frontend Test      | Component tests and automated WCAG accessibility assertions                                                                  | Jest                                                                 |

## Measured Performance Baselines

_To be recorded on release builds in Phase 6 against `AGENTS.md` §11 budgets._
