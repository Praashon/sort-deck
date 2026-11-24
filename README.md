<div align="center">

<img src="./docs/assets/logo.svg" alt="SortDeck logo" width="84" height="84" />

# SortDeck

**Safe, local-first file organizer for Windows 11.**

Preview every move before touching disk, sort with smart rules and date folders, find moved files in seconds, inspect folder space and exact duplicates, and undo any sort cleanly.

[![Platform](https://img.shields.io/badge/Windows%2011-x64-141417?style=flat-square&labelColor=1d1d22&color=2cb5a0)](https://github.com/Praashon/sort-deck/releases/latest)
[![Stack](https://img.shields.io/badge/Tauri%20v2-Rust%20%2B%20React%2019-141417?style=flat-square&labelColor=1d1d22&color=3f3f46)](./docs/ARCHITECTURE.md)
[![Release](https://img.shields.io/badge/Release-v0.1.0-141417?style=flat-square&labelColor=1d1d22&color=2cb5a0)](https://github.com/Praashon/sort-deck/releases/latest)
[![License](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-141417?style=flat-square&labelColor=1d1d22&color=3f3f46)](./LICENSE)

[Download Latest Release](https://github.com/Praashon/sort-deck/releases/latest) | [Safety Contract](./docs/SAFETY.md) | [Architecture](./docs/ARCHITECTURE.md) | [Changelog](./CHANGELOG.md)

</div>

---

## Download

Pre-built Windows 11 x64 executables are published on the [GitHub Releases](https://github.com/Praashon/sort-deck/releases/latest) page:

| Artifact                       | Best For           | Description                                                                |
| ------------------------------ | ------------------ | -------------------------------------------------------------------------- |
| `SortDeck_0.1.0_x64-setup.exe` | Most Windows users | Standard per-user NSIS installer with Start Menu entry and uninstaller.    |
| `sortdeck.exe`                 | Portable use       | Standalone Windows executable that runs directly without installation.     |
| `SHA256SUMS.txt`               | Verification       | SHA-256 checksums for verifying downloaded `.exe` binaries before running. |

Verify your download in PowerShell before running:

```powershell
Get-FileHash .\SortDeck_0.1.0_x64-setup.exe -Algorithm SHA256
Get-FileHash .\sortdeck.exe -Algorithm SHA256
```

> **Note on Windows SmartScreen:** Early `v0.1.0` builds are unsigned while code signing is added in a future release. If Windows Defender SmartScreen prompts on first launch, compare the SHA-256 hash above with `SHA256SUMS.txt` from the release page.

---

## Key Features

| Feature                               | What It Does                                                                                                                                                                                                                                  |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Dry-Run Move Preview**              | Scans a folder read-only and shows every proposed move, automatic collision rename (`photo (1).png`), cross-drive indicator, and skip reason. Uncheck any individual file before confirming.                                                  |
| **Smart Sorting Rules**               | Match by file endings (multi-part endings such as `.tar.gz` match before `.gz`) plus optional filters: filename text match, minimum/maximum file size, and untouched age in days.                                                             |
| **Date Subfolder Templates**          | Use `{year}`, `{month}`, and `{day}` inside any destination path (for example `Photos/RAW/{year}/{month}/`) based on each file's modified date.                                                                                               |
| **One-Click Starter Rule Packs**      | Install curated rule kits cleanly without moving existing files: **Adobe App Folders**, **Screenshots & Receipts**, **Camera RAW Photos by Date**, and **3D, CAD & Music Projects**, alongside 8 built-in rules including **Adobe Creative**. |
| **Find a Moved File**                 | Search your local move history by any part of an old or new filename, open the file directly in Windows File Explorer, or restore it with **Put Back**.                                                                                       |
| **Folder Space & Duplicate Check**    | Run a read-only audit on any watched folder to view space used by category, files untouched for 90+ days, the top 10 largest files, and byte-for-byte SHA-256 duplicate groups. SortDeck never deletes duplicates for you.                    |
| **Write-Ahead Undo & Crash Recovery** | Every move is recorded in a local SQLite WAL journal before disk changes begin. Undo an entire sort run or individual items cleanly, with automatic startup crash recovery in **Fix Issues**.                                                 |

---

## Safety Guarantees

SortDeck prioritizes data safety over speed. Core safety rules are enforced inside the Rust engine regardless of UI state or user rules ([`docs/SAFETY.md`](./docs/SAFETY.md)):

| Guarantee                              | How SortDeck Enforces It                                                                                                                                                                                                                                         |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Never overwrites destination files** | Same-drive moves call Win32 `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` or `MOVEFILE_COPY_ALLOWED`. Name collisions automatically get ` (1)`, ` (2)` suffixes case-insensitively.                                                                          |
| **Verified cross-drive moves**         | Cross-volume moves copy via `CopyFileExW` with `COPY_FILE_FAIL_IF_EXISTS` to `.sortdeck-*.sdpart`, flush buffers, verify streaming SHA-256 of both files, re-check source identity, commit, and remove source only after destination verification passes.        |
| **Protected locations & projects**     | Refuses system roots (`Windows`, `Program Files`, `ProgramData`, `AppData`, `$RECYCLE.BIN`, `System Volume Information`), drive roots, `%USERPROFILE%`, reparse points, `.git`/`.hg`/`.svn` repositories, `node_modules`, `.venv`, and `.sortignore` exclusions. |
| **Special file protection**            | Skips hidden, system, OneDrive/cloud placeholder files (`OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS`), Office `~$` locks, active partial downloads (`.crdownload`, `.part`, `.tmp`), and read-only cross-volume sources.                                 |
| **Backend revalidation & strict IPC**  | The frontend has zero direct filesystem access (`tauri-plugin-fs` and `tauri-plugin-shell` disabled). Every confirmed dry-run plan is revalidated against current config revision and file fingerprints before execution.                                        |
| **Local-first privacy**                | Zero telemetry, zero analytics, no accounts, and zero default outbound network requests. All settings, rules, journals, and move logs remain in `%LOCALAPPDATA%\SortDeck\`.                                                                                      |

---

## Architecture

SortDeck keeps strict boundaries between presentation, application services, filesystem policy, and Win32 calls:

```text
React 19 + TypeScript (Strict UI)
        │  typed IPC wrappers (ts-rs bindings)
        ▼
Tauri v2 Commands (thin validation layer)
        │
        ▼
Services (Folders, Rules, Planner, Execution, History, Settings)
        │
        ├──▶ Engine (Scanner, Rules, Paths, Naming, Planner, safe_move, Undo)
        │           │  abstracted via FsBackend trait
        │           ▼
        │    Win32 Platform Layer (MoveFileExW, CopyFileExW, FlushFileBuffers, SHA-256)
        │
        └──▶ SQLite (WAL + Synchronous FULL, Migrations, Backups, Journal, Move Logs)
```

- **Backend (`src-tauri/src/`):** Rust with `rusqlite` (bundled), `sha2`, `walkdir`, `thiserror`, `tracing`, `windows-sys`, `ts-rs`.
- **Frontend (`src/`):** React 19, TypeScript strict mode, Tailwind CSS v4 (Architectural Carbon dark default and Warm Alabaster light theme), `@tanstack/react-virtual`, Lucide React, Vite.

---

## Build and Verify From Source

### Prerequisites (Windows 11 x64)

- Node.js `24.20.0` (see [`.nvmrc`](./.nvmrc)) and `npm`
- Rust `1.99.1` (see [`rust-toolchain.toml`](./rust-toolchain.toml)) with `rustfmt` and `clippy`
- Visual Studio Build Tools 2022 (MSVC C++ x64) and WebView2 Runtime

### Verification Commands

```powershell
# Frontend formatting, lint, typecheck, unit & axe accessibility tests, production bundle
npm ci
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
npm run check:banned-apis

# Backend formatting, strict clippy, and unit/integration/proptest suites
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

### Build Windows Installers Locally

```powershell
npm run tauri build
```

Generated installers and executables are placed under `src-tauri/target/release/` (`bundle/nsis/SortDeck_0.1.0_x64-setup.exe` and `sortdeck.exe`).

---

## Current Status & Upcoming Work

| Area                         | Scope                                                                                   | Status      |
| ---------------------------- | --------------------------------------------------------------------------------------- | ----------- |
| **Core & Persistence**       | Tauri v2 shell, SQLite WAL migrations, online backups, path policy                      | Ready       |
| **Sorting & Preview Engine** | Read-only scanner, multi-dot & smart rules, collision naming, dry-run planner           | Ready       |
| **Safe Move & Recovery**     | Win32 `safe_move()` state machine, SHA-256 cross-volume copy, undo, crash recovery      | Ready       |
| **Desktop App & Utilities**  | React 19 UI, dry-run preview, starter packs, moved-file search, space & duplicate check | Ready       |
| **Live Background Watcher**  | Real-time folder watcher (`notify-debouncer-full`), job queue, system tray & autostart  | In Progress |
| **Packaging & Signing**      | Authenticode signing, SBOMs, clean Windows 11 VM verification                           | Planned     |

---

## Documentation

- [Architecture Overview](./docs/ARCHITECTURE.md)
- [Safety Invariants and Threat Model](./docs/SAFETY.md)
- [Database Schema and Journal State Machine](./docs/DATABASE.md)
- [IPC Commands and Error Codes](./docs/IPC.md)
- [Testing Guide](./docs/TESTING.md)
- [Release Engineering](./docs/RELEASE.md)
- [Changelog](./CHANGELOG.md)

## License

Dual-licensed under [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE), at your option.
