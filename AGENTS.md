# AGENTS.md — SortDeck

> Authoritative instructions for AI coding agents and human contributors working on SortDeck.
> This file defines the architecture, engineering standards, safety invariants, implementation phases, and verification gates. It is a contract, not a suggestion.

**Document version:** 2.0  
**Last reviewed:** 2026-11-24  
**Primary target:** Windows 11 x64

---

## Table of Contents

0. [How to Read This File](#0-how-to-read-this-file)
1. [Project Overview](#1-project-overview)
2. [Technology Stack](#2-technology-stack)
3. [Repository Architecture](#3-repository-architecture)
4. [Strict Safety Requirements](#4-strict-safety-requirements)
5. [Transaction Logging, Journal, Undo, Recovery](#5-transaction-logging-journal-undo-and-recovery)
6. [Sorting Rules Engine](#6-sorting-rules-engine)
7. [Scanner and Dry-Run Planner](#7-scanner-and-dry-run-planner)
8. [Background Watcher and Job Queue](#8-background-watcher-and-job-queue)
9. [Tauri Security and Threat Model](#9-tauri-security-and-threat-model)
10. [Frontend and UI/UX Requirements](#10-frontend-and-uiux-requirements)
11. [Performance and Resource Budgets](#11-performance-and-resource-budgets)
12. [Errors, Logging, Configuration, and Data Location](#12-errors-logging-configuration-and-data-location)
13. [Testing Requirements](#13-testing-requirements)
14. [Verification Commands and CI Gates](#14-verification-commands-and-ci-gates)
15. [Implementation Roadmap](#15-implementation-roadmap)
16. [Release Engineering and Distribution](#16-release-engineering-and-distribution)
17. [Documentation and Open-Source Governance](#17-documentation-and-open-source-governance)
18. [AI Coding Agent Instructions](#18-ai-coding-agent-instructions)
19. [Open Decisions and Defaults](#19-open-decisions-and-defaults)
20. [Final Project Definition](#20-final-project-definition)

---

# 0. How to Read This File

## 0.1 Precedence

When instructions conflict, apply this order:

1. Safety Invariants (§4.9) and the Non-Negotiable Principles (§1.2).
2. The rest of this file.
3. Documents under `docs/`.
4. Code comments and existing code.
5. An agent's own judgement.

If two rules in this file appear to conflict, **stop and ask the maintainer**. Do not pick the more convenient one.

## 0.2 Normative Language

- **MUST / MUST NOT**: absolute requirement. A violation is a defect.
- **SHOULD / SHOULD NOT**: required unless a documented, reviewed reason exists.
- **MAY**: optional.

## 0.3 Glossary

| Term | Meaning |
|---|---|
| **Watched folder** | A user-selected root directory SortDeck is allowed to scan. |
| **Sweep** | A user-initiated, one-time sorting run over a watched folder. |
| **Plan** | A backend-owned, read-only list of proposed operations produced by the planner. Never executed without revalidation. |
| **Batch** | A group of operations sharing one UUID: one manual sweep, one undo, one recovery run, or one auto-sort window. |
| **Operation** | One file's move (or one file's restoration) inside a batch. |
| **Journal** | The persisted write-ahead record (`operation_journal`) used for crash recovery. |
| **Move log** | The persisted, user-facing record (`move_logs`) used for history and undo. |
| **Protected location** | A path SortDeck must never move files from or into (see §4.3). |
| **Managed destination** | A folder that is the destination of any enabled rule. Files already inside are never re-sorted. |
| **Project root** | A directory containing a VCS marker (`.git`, `.hg`, `.svn`). Its entire subtree is excluded. |
| **Fingerprint** | Source identity snapshot: volume serial, file ID, size, modification time. |
| **Volume** | A Windows storage volume identified by volume GUID/serial, not by drive letter. |

## 0.4 Non-Goals (Do Not Build Unless Explicitly Instructed)

- Cloud sync, accounts, telemetry, analytics, or any outbound network call by default.
- Content-based classification, OCR, or AI/ML file classification.
- Deduplication or deletion of user files.
- Renaming files other than collision suffixes.
- macOS and Linux builds (code SHOULD stay portable where free, but nothing is promised).
- Redo of an undone batch.
- Shell integration (context menus), scripting APIs, or plugins.

---

# 1. Project Overview

**SortDeck** is an open-source, local-first Windows 11 desktop application that organizes files in user-selected folders and drives using file extensions, custom rules, and user preferences.

It MUST be safe, reliable, lightweight, and visually polished.

## 1.1 Core Objectives

- Automatically organize files into meaningful categories.
- Support manual sorting and continuous background monitoring.
- Allow custom sorting rules.
- Preview every manual sorting operation before execution.
- Maintain a complete local transaction history.
- Support reliable undo and crash recovery.
- Never overwrite existing files or silently discard user data.
- Work entirely locally; no cloud services required.
- Deliver a polished, accessible Windows 11 desktop experience.

## 1.2 Non-Negotiable Principles

1. Data safety takes priority over sorting speed.
2. Every file operation is traceable.
3. Every successful move has a recoverable undo record.
4. Uncertain files are skipped, never moved.
5. The backend independently validates every requested operation. The frontend is untrusted.
6. No implementation is complete without tests that exercise its failure paths.
7. SortDeck never deletes a user file except the source of a fully verified cross-volume copy, and never deletes a destination file.
8. SortDeck never surprises the user: enabling a folder, editing a rule, restoring defaults, or launching the app MUST NOT cause unexpected file movement.
9. No telemetry, no analytics, no network access in the default configuration.

## 1.3 Supported Platforms

| Item | Requirement |
|---|---|
| OS | Windows 11 x64 (officially supported). Windows 10 22H2 x64 is best-effort, not a release gate. |
| Filesystems | NTFS (full support). ReFS, exFAT, FAT32: supported with documented degradations (§4.6.5). Network/UNC paths: best-effort with polling fallback (§8). |
| Runtime | WebView2 (bundled/bootstrapped by installer, §16). |
| Paths | Drive letters, UNC, long paths (>260 chars), and Unicode filenames MUST work. |
| ARM64 | Post-v1. Do not add ARM-specific code in v1. |

---

# 2. Technology Stack

Use the following unless a technical limitation requires a documented alternative. Every deviation or addition MUST be recorded in `docs/ARCHITECTURE.md` under "Dependency Decisions" with its purpose and alternatives considered.

## 2.1 Frontend

- Tauri v2 (`@tauri-apps/api`, `@tauri-apps/plugin-dialog`)
- React 19, TypeScript with `"strict": true` (also `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`)
- Vite
- Tailwind CSS v4 (CSS-first configuration via `@theme`)
- Lucide React
- `react-router` (hash or memory router; no reliance on browser history/URLs)
- `@tanstack/react-virtual` for large lists (previews and history)
- Quality tooling: ESLint (typescript-eslint, react-hooks, jsx-a11y), Prettier, Vitest, React Testing Library, `vitest-axe` (or `jest-axe`)
- E2E: WebdriverIO with `tauri-driver` on Windows CI (smoke level)

State management SHOULD be React state/context plus hooks. Adding a state library (e.g., Zustand, TanStack Query) requires a documented justification.

## 2.2 Backend

- Rust (stable, pinned in `rust-toolchain.toml`; MSRV documented), edition 2021 or newer
- Tauri v2 command system
- `rusqlite` with `bundled` SQLite
- `serde`, `serde_json`
- `walkdir` for controlled traversal (with explicit reparse-point handling, §4.4)
- `notify` + **`notify-debouncer-full`** (replaces `notify-debouncer-mini`: the full debouncer tracks file-system IDs and stitches rename events together on Windows, which reduces false create/delete pairs when files are renamed by browsers and other apps)
- `uuid` (v4 or v7)
- `sha2` (SHA-256), streaming
- `thiserror` (typed errors), `anyhow` only at top-level binaries/tests
- `tracing`, `tracing-subscriber`, `tracing-appender` (rotating local logs)
- `windows-sys` (preferred) or `windows` crate for Win32 calls (§2.5)
- `tokio` (bounded runtime, only if required beyond Tauri's runtime) or `std::thread` + channels; no unbounded spawning
- `tauri-plugin-autostart`
- `tauri-plugin-single-instance` (**required**: two instances would mean two watchers and a contended database)
- `tauri-plugin-notification` (native notifications while the window is hidden)
- `tauri-plugin-window-state` (optional, remembers window geometry)
- Tauri system tray
- Type generation from Rust to TypeScript for IPC types (`ts-rs` or `specta`); CI MUST fail if generated types are stale.

Dev/test only: `tempfile`, `proptest`, `assert_fs` or equivalent, `serial_test`, `cargo-llvm-cov`, `cargo-mutants` (nightly/pre-release, safety modules only).

## 2.3 Platform

- Primary target: Windows 11 x64.
- Handle cross-volume operations explicitly.
- Do not assume every filesystem supports identical permissions, metadata, file IDs, or alternate data streams.
- The app manifest MUST declare long-path awareness and `PerMonitorV2` DPI awareness (Tauri defaults SHOULD be verified, not assumed).

## 2.4 Engineering Constraints

- Never perform blocking filesystem operations on the UI/IPC thread.
- Never place file-moving logic in React components or in Tauri command bodies.
- Rust commands are thin: validate → call a service → map the result.
- All IPC request/response types are explicit, versioned where needed, and generated for TypeScript.
- Do not expose unrestricted filesystem access to the frontend. Do not enable `tauri-plugin-fs` or `tauri-plugin-shell`.
- Do not add dependencies without a stated purpose, license check (`cargo deny`), and maintenance check (recent releases, advisory status).

## 2.5 Unsafe Code and Win32 FFI Policy

- All Win32 FFI lives in `src-tauri/src/platform/windows/`. Everywhere else: `#![forbid(unsafe_code)]`.
- Inside `platform/`: `#![deny(unsafe_op_in_unsafe_fn)]`; every `unsafe` block carries a `// SAFETY:` comment stating the invariants relied upon.
- Wide-string handling, handle lifetime (RAII wrappers that close handles on drop), and buffer sizing MUST be covered by tests.
- Every Win32 failure is converted to a typed error preserving the raw error code (§4.7).

## 2.6 Banned APIs in the Move Path

`std::fs::rename` **replaces an existing destination file on Windows**. It MUST NOT be used for any SortDeck-initiated move. Likewise forbidden outside the `fsops`/`platform` layers:

- `std::fs::rename`, `std::fs::copy`, `std::fs::remove_file`, `std::fs::remove_dir`, `std::fs::remove_dir_all`, `std::fs::write` on user paths
- `std::fs::canonicalize` for display paths (it yields `\\?\` verbatim paths; use `dunce` or an equivalent normalizer for UI strings, and keep verbatim paths internal)
- Any shell-out (`cmd`, `powershell`, `robocopy`, `explorer`) to move, copy, or delete files

Enforce with `clippy.toml` `disallowed-methods` plus a CI grep step. Exceptions require a code comment and maintainer approval.

---

# 3. Repository Architecture

Maintain strict separation between presentation, IPC, services, engine, persistence, and platform operations.

```text
SortDeck/
├── .github/
│   ├── workflows/            # ci.yml, release.yml, codeql.yml, audit.yml
│   ├── ISSUE_TEMPLATE/
│   ├── dependabot.yml
│   └── PULL_REQUEST_TEMPLATE.md
├── src/
│   ├── components/{layout,folders,rules,history,dialogs,common}/
│   ├── pages/{Dashboard,Folders,Rules,History,Settings}.tsx
│   ├── hooks/
│   ├── services/             # typed IPC wrappers only
│   ├── types/                # includes generated bindings (do not hand-edit)
│   ├── i18n/                 # string catalog (en) — no hard-coded UI strings
│   ├── utils/
│   ├── App.tsx
│   └── main.tsx
├── src-tauri/
│   ├── capabilities/         # minimal, per-window
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/         # thin IPC handlers
│   │   ├── services/         # orchestration (sweeps, rules, folders, history, settings)
│   │   ├── engine/
│   │   │   ├── scanner.rs
│   │   │   ├── rules.rs
│   │   │   ├── planner.rs
│   │   │   ├── mover.rs      # safe_move state machine (pure orchestration over FsBackend)
│   │   │   ├── naming.rs     # collision naming
│   │   │   └── paths.rs      # path validation / protected-location policy
│   │   ├── fsops/            # FsBackend trait + real impl + fault-injecting test impl
│   │   ├── platform/
│   │   │   └── windows/      # ALL unsafe/Win32 code
│   │   ├── database/{mod,migrations,queries}.rs
│   │   ├── watcher/mod.rs
│   │   ├── jobs/             # bounded queue, in-flight registry, cancellation
│   │   ├── recovery/mod.rs
│   │   ├── tray/
│   │   ├── models/mod.rs
│   │   ├── errors.rs
│   │   ├── config.rs
│   │   └── logging.rs
│   ├── tests/                # integration tests + fixtures
│   ├── clippy.toml
│   ├── deny.toml
│   └── Cargo.toml
├── docs/
│   ├── ARCHITECTURE.md
│   ├── SAFETY.md             # invariants, threat model, known residual risks
│   ├── DATABASE.md           # schema, migrations, state machine
│   ├── IPC.md                # commands, events, error codes
│   ├── TESTING.md
│   └── RELEASE.md
├── scripts/                  # dev helpers (sandbox creation, VHD creation for cross-volume tests)
├── public/
├── package.json
├── rust-toolchain.toml
├── .nvmrc
├── README.md
├── CHANGELOG.md
├── CONTRIBUTING.md
├── SECURITY.md
├── CODE_OF_CONDUCT.md
├── LICENSE
└── AGENTS.md
```

The structure may evolve. Do not create needless abstractions; do not merge unrelated responsibilities into oversized files.

## 3.1 Dependency Direction (Enforced)

```text
commands → services → { engine, watcher, recovery, jobs } → fsops (FsBackend) → platform
                  └→ database (via repository functions only)
```

- `engine/` MUST NOT import Tauri types. It MUST be unit-testable without launching Tauri.
- `engine/` MUST perform all filesystem access through the `FsBackend` trait so that crashes, hash mismatches, sharing violations, disk-full, and cross-volume errors can be injected in tests.
- `commands/` MUST NOT touch the filesystem or database directly.
- The frontend MUST NOT know how files are moved; it only requests plans, confirms plan IDs, and displays results.

---

# 4. Strict Safety Requirements

These requirements override performance optimizations and convenience features.

## 4.1 Never Overwrite Existing Files

- Never replace an existing destination file.
- Never silently merge files with identical names.
- Always resolve destination collisions before moving, and never assume a destination remains available after checking it.

Automatic collision naming:

```text
photo.png
photo (1).png
photo (2).png
```

### 4.1.1 Race-Safe No-Overwrite Primitives (Windows)

Collision safety MUST come from the operating system, not from a prior "does it exist?" check. A pre-check is only a hint for choosing candidate names.

| Need | Required primitive |
|---|---|
| Same-volume move without overwrite | `MoveFileExW(src, dst, 0)` — **without** `MOVEFILE_REPLACE_EXISTING` and **without** `MOVEFILE_COPY_ALLOWED`. Fails with `ERROR_ALREADY_EXISTS`/`ERROR_FILE_EXISTS` if `dst` exists, and with `ERROR_NOT_SAME_DEVICE` across volumes. |
| Create a new destination/temp file | `CreateFileW` with `CREATE_NEW` (or `OpenOptions::create_new(true)`). |
| Copy to a temp destination | `CopyFileExW`/`CopyFile2` with `COPY_FILE_FAIL_IF_EXISTS`, a progress callback for cancellation, and streaming behavior for large files. |
| Commit temp → final name | Same-volume `MoveFileExW` without replace (temp lives in the destination directory so this is always same-volume). |

Algorithm: pick candidate name → attempt the no-overwrite primitive → on "already exists", advance to the next candidate and retry. The retry loop MUST be bounded (default 10,000 attempts) and fail the operation, not loop forever.

### 4.1.2 Collision Naming Rules

- Suffix format is `<stem> (<n>)<extension>`, `n` starting at 1.
- For compound extensions (`.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) the compound suffix is preserved: `archive (1).tar.gz`.
- Dotfiles (`.gitignore`) and extensionless names: suffix appended to the full name.
- Collision detection MUST be case-insensitive (NTFS default). Never depend on case to avoid a collision.
- The resulting file name component MUST NOT exceed 255 UTF-16 code units, and the full path MUST remain within supported limits (§4.6). If it would exceed, the operation fails with `PathTooLong` — never truncate silently.
- Within one plan, names MUST be reserved so two sources targeting the same destination name get distinct names (§7.3).

## 4.2 Safe Moves (Same-Volume and Cross-Volume)

Implement a dedicated `safe_move()` in `engine/mover.rs` as an explicit state machine over `FsBackend`.

### 4.2.1 Classification: Attempt First, Then Classify by Error

Do not guess "same volume" from drive letters (they can be mapped, substituted, or mounted as folders). Volume comparison (by volume GUID path/serial) is used only for **preview estimates**. At execution:

1. Attempt the no-overwrite same-volume move.
2. If it fails with `ERROR_NOT_SAME_DEVICE`, take the cross-volume path.
3. Any other error is handled per §4.7 and MUST NOT be reinterpreted as "cross-volume".

### 4.2.2 Same-Volume Operation

1. Validate source and destination (§4.3, §4.6).
2. Re-check source fingerprint against the plan (§7.4).
3. Resolve a collision-safe destination name.
4. Journal state `planned` (committed to SQLite with durable sync **before** the filesystem call, §5.2).
5. Perform the no-overwrite move.
6. Verify the destination exists, size matches, and (NTFS) file ID matches the source fingerprint.
7. Write the `move_logs` row and journal state `completed` in one SQLite transaction.

### 4.2.3 Cross-Volume Operation

1. Validate source and destination; check destination free space ≥ file size + safety margin (default: max(64 MiB, 1%)), and filesystem limits (e.g., FAT32 4 GiB).
2. Generate a unique temporary name **in the destination directory**: `.sortdeck-<batch-uuid>-<n>.sdpart`. (Do not use `.tmp`/`.part`; those extensions are on the exclusion list and the watcher also ignores this pattern.)
3. Journal `planned` → `copying` (with `temporary_path`), durably.
4. Copy with `CopyFileExW`/`CopyFile2` + `COPY_FILE_FAIL_IF_EXISTS` (preserves alternate data streams, attributes, and timestamps; see §4.6.4). Stream; never read whole files into memory.
5. Flush and close the destination handle (`FlushFileBuffers`).
6. Compute SHA-256 of **both** source and temp copy by streaming, compare. Also compare sizes. Size alone is never sufficient.
7. Re-check that the source fingerprint is unchanged since step 1 (it may have been modified during the copy). If changed: abort, remove the app-owned temp, preserve the source, report `SourceChangedDuringCopy`.
8. Journal `verified`.
9. Commit temp → final collision-safe name via no-overwrite same-volume move. Journal `committed`.
10. Verify the committed destination (size + hash recorded).
11. Delete the original source (see §4.2.5). Only now.
12. Write `move_logs` + journal `completed` in one transaction.

### 4.2.4 Failure Handling

If any step fails:

- Preserve the original source.
- Never report success.
- Remove only application-owned incomplete temp files, and only when ownership is established by a matching journal record (path + `.sortdeck-` naming + recorded file ID).
- Record the failure with the raw OS error code.
- If cleanup itself fails, set `recovery_required`; never leave an unexplained orphan.
- Disk-full (`ERROR_DISK_FULL`/`ERROR_HANDLE_DISK_FULL`) → remove temp, fail the operation, continue or stop the batch per policy (default: stop batch to avoid cascading failures).

### 4.2.5 Source Deletion After Cross-Volume Copy

- Delete only after steps 6–10 succeed.
- If the source cannot be deleted (sharing violation, read-only attribute, access denied): do **not** delete the destination, do **not** retry destructively. Mark the operation `recovery_required` with detail `source_remains_after_verified_copy`, warn the user (both copies exist), and let recovery or the user resolve it.
- Read-only source files on cross-volume moves are skipped in the plan (`ReadOnlyCrossVolume`); SortDeck does not clear attributes on user files.

### 4.2.6 Cancellation and Shutdown

- Cancellation uses a token checked between operations and inside copy progress callbacks.
- Cancelling mid-copy aborts the copy, removes the app-owned temp, preserves the source, and marks the operation `failed`/`cancelled`.
- A cancelled batch keeps completed moves undoable; its status is `cancelled` (never `completed`).
- On app exit, session end (`WM_QUERYENDSESSION`), or sleep, in-flight operations are either completed within a short grace period or aborted safely; the journal MUST always allow recovery to determine the correct next action.

## 4.3 Hardcoded Safety Exclusions

Never automatically move files from, or into, protected system or application locations. These rules are compiled in and cannot be disabled by settings or rules.

### 4.3.1 Protected Locations (Exact-Path Roots)

Resolved at runtime from known-folder APIs and environment (never hardcoded drive letters), canonicalized, compared case-insensitively, **by path component prefix** (never substring):

```text
%SystemRoot%               (e.g., C:\Windows)
%ProgramFiles%
%ProgramFiles(x86)%
%ProgramData%
%USERPROFILE%\AppData      (and %LOCALAPPDATA%, %APPDATA% wherever relocated)
<any volume>\$RECYCLE.BIN
<any volume>\System Volume Information
SortDeck's own data directory and log directory
```

A user directory that merely *contains* a name like `Windows` (e.g., `D:\Projects\Windows`) is **not** protected by this list. The list protects actual system locations.

### 4.3.2 Excluded Directory Names (Matched As Any Path Component)

```text
.git   .hg   .svn   node_modules   .venv   venv   __pycache__
```

Matching is by exact component name (case-insensitive), never substring. A directory excluded by name is not entered, and nothing inside it is moved.

### 4.3.3 Project Roots

A directory that **contains** `.git`, `.hg`, or `.svn` is a project root. Its entire subtree is excluded in recursive scans (moving a repository's `README.md` or source files would corrupt a project). In depth-1 scans, subdirectories are never entered, so this applies when the watched folder itself is a project root: SortDeck refuses to scan it and reports `ProjectRoot`.

### 4.3.4 Disallowed Watched Folders

SortDeck MUST refuse to add (or scan) as a watched folder: a drive root (`C:\`), the user profile root (`%USERPROFILE%`), any protected location, any directory inside a protected location, a reparse-point directory, a network root without explicit "advanced" confirmation, or a folder that overlaps another watched folder or a managed destination in a loop-causing way (§4.4.3). Typical user folders (Downloads, Desktop, Documents, custom folders) are allowed.

### 4.3.5 Protected Attributes and Special Files

Skip files with Windows attributes:

- `HIDDEN`, `SYSTEM`
- `OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS` (cloud placeholders that would trigger a download if touched)
- `ENCRYPTED` (EFS) for cross-volume moves
- Any reparse point that is a symbolic link or junction

Cloud-provider files (OneDrive, etc. — cloud reparse tags): **dehydrated** files are skipped (`CloudPlaceholder`); **fully hydrated** files are allowed for same-volume moves only; cross-volume moves of cloud-managed files are skipped (`CloudManagedCrossVolume`) in v1.

Also always skip: `desktop.ini`, `Thumbs.db`, `.DS_Store`, the `.sortignore` marker itself, SortDeck temp files (`.sortdeck-*.sdpart`), and Office owner/lock files (`~$*`).

**Office lock detection:** if `~$<name>` exists beside `<name>` (Word/Excel/PowerPoint), the file is open — skip it (`FileInUse`).

### 4.3.6 Temporary and Active Download Files

Never move files with these extensions:

```text
.crdownload .part .partial .tmp .temp .opdownload .download .downloading
.!ut .!qB .aria2 .ytdl .filepart .swp .lock
```

Also skip files that are still being modified or are open by another process (§4.5).

### 4.3.7 `.sortignore`

If a directory contains a `.sortignore` file:

- Exclude that directory and all descendants.
- Never move files from it, never remove or modify the marker.
- Check ignored **ancestors** up to the watched-folder root (and the watched folder itself).
- A `.sortignore` in the watched folder root disables the whole folder, with a visible status in the UI.
- The marker is presence-only in v1 (contents ignored).

### 4.3.8 Destination-Side Protection

Protected locations, project roots, `.sortignore`d directories, reparse points, and non-writable locations MUST NOT be accepted as rule destinations. A destination inside the source subtree of a recursive watched folder MUST be a registered managed destination (so it is excluded from scanning, §4.4.3).

## 4.4 Directory Scanning

Defaults:

```text
max_depth = 1        # immediate files only
recursive = false
```

- Recursive scanning is explicit, per folder, with a configurable `max_depth` (hard cap, default 5).
- Never follow symlinks/junctions/reparse-point directories (`walkdir` `follow_links(false)` plus explicit attribute checks: `follow_links(false)` alone is not sufficient for junctions on every path — check `FILE_ATTRIBUTE_REPARSE_POINT` and the tag).
- Never treat a directory as a file; never move directory trees.
- Never traverse protected locations, project roots, or `.sortignore`d subtrees.
- Never scan managed destination directories.
- Enforce cancellation points at least every N entries (default 256) and a maximum entries-per-scan guard (default 1,000,000) with a clear warning.
- Directory enumeration errors (access denied, vanished directory) are recorded as warnings and the scan continues; they never abort the whole scan silently.

### 4.4.1 Behavior in Recursive Mode

Files from nested folders are flattened into the rule's destination by default (collision naming applies). A per-folder `preserve_structure` option (default **false**) MAY be added later; until then flattening is the documented behavior and the history MUST record the original path of every file.

### 4.4.2 Files Already in Place

A file already located inside its matched rule's destination directory is `AlreadyInDestination` and is skipped. SortDeck MUST be idempotent: running a sweep twice produces zero operations the second time.

### 4.4.3 Overlap and Loop Prevention

Reject configurations where:

- One watched folder is an ancestor or descendant of another (unless the descendant is explicitly excluded from the ancestor).
- A rule destination lies inside a watched folder in a way that would cause SortDeck to re-process its own output.
- A destination equals its own source directory with a rule that matches (no-op loops).

Validation runs at configuration time **and** again at plan time and execution time.

## 4.5 File Stability and Lock Protection

A quiet filesystem event does not mean a file is finished. Before **automatically** moving a file:

1. Debounce events (default ≈ 2000 ms).
2. Require a minimum file age since last modification (default 5 s, configurable per folder).
3. Observe size **and** modification time unchanged over at least 2 consecutive checks separated by an interval (default 1 s each).
4. Perform a Windows access check that mirrors the real operation: open with `DELETE` access (required to move) and read access using `FILE_SHARE_READ` only, so the open fails if a writer is active or if another handle denies delete-sharing. Treat `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION` as "in use".
5. Confirm the file is not an excluded temporary download and has no Office lock sibling.
6. Revalidate the source fingerprint immediately before the move.

The lock check is **advisory** (check-then-use race). The real guard is that the OS move itself fails if the file is in use; that failure MUST be handled as defer/skip, never retried with force.

A failed lock check or an in-use failure results in a **deferred** operation with bounded exponential backoff (default: 5 attempts over ~10 minutes), then `skipped(FileInUse)`. Never force-move.

## 4.6 Path Handling and Windows Filesystem Semantics

### 4.6.1 Normalization and Validation

- Normalize every path once at the boundary: absolute, canonical, case-preserving, with a stable comparison key (case-insensitive, Unicode-aware).
- Reject: relative paths from IPC, paths containing `..` after normalization escaping their root, device paths (`\\.\`, `\\?\GLOBALROOT`), reserved DOS device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1–9`, `LPT1–9`, with or without extensions), names with trailing dots/spaces, invalid characters (`<>:"/\|?*`, control characters), and alternate-data-stream syntax (`name:stream`) in user-provided names and destinations.
- Reject path components > 255 UTF-16 code units. Support total paths > 260 via `\\?\` internally (long-path aware).
- Destinations from rules are **relative to the watched folder root** by default; an absolute destination is allowed only after full validation (§4.3.8). A relative destination that resolves outside its root is rejected (traversal).
- Resolve and re-check 8.3 short names and mapped/subst drive letters through canonicalization before protected-location comparison.

### 4.6.2 Non-Unicode Paths

Windows paths are potentially ill-formed UTF-16. A file whose path does not round-trip through UTF-8 is skipped (`NonUnicodePath`); it is never moved using a lossy name.

### 4.6.3 TOCTOU on Reparse Points

Between planning and execution a directory may be replaced by a junction. Immediately before each operation, SortDeck MUST verify that no component of the source parent chain or destination parent chain (below the watched/destination root) is a reparse point, and that the canonical parent still lies inside the expected root. Residual risk is documented in `docs/SAFETY.md`.

### 4.6.4 Metadata Preservation

- Same-volume move: ACLs, timestamps, attributes, and alternate data streams are preserved by the OS.
- Cross-volume copy MUST preserve the **Zone.Identifier** alternate data stream (Mark-of-the-Web) so downloaded files do not silently lose their security zone, plus timestamps and attributes. This is why `CopyFileExW`/`CopyFile2` is required rather than a manual read/write loop.
- Cross-volume: the destination directory's inherited ACL applies; this is documented behavior, not a bug.

### 4.6.5 Filesystem Capability Detection

Query volume capabilities (`GetVolumeInformationW`): file IDs, ADS support, max component length, case sensitivity. On FAT/exFAT (no stable file IDs, no ADS) fall back to size + mtime + hash identity and warn that Mark-of-the-Web cannot be preserved. Refuse operations the destination cannot represent (e.g., > 4 GiB to FAT32) at plan time.

### 4.6.6 Removable and Disconnected Volumes

A watched folder or destination on a disconnected drive is `unavailable`: never auto-removed, never treated as empty, retried on reconnect. Operations never fall back to a different path.

## 4.7 Windows Error Handling Contract

Errors MUST be mapped explicitly (preserving the raw code in logs). Minimum mapping:

| Win32 error | Meaning | Outcome |
|---|---|---|
| `ERROR_NOT_SAME_DEVICE` (17) | Different volume | Switch to cross-volume path (§4.2.3) |
| `ERROR_ALREADY_EXISTS` (183), `ERROR_FILE_EXISTS` (80) | Destination exists | Next collision candidate (bounded) |
| `ERROR_SHARING_VIOLATION` (32), `ERROR_LOCK_VIOLATION` (33) | File in use | Defer with backoff, then skip |
| `ERROR_ACCESS_DENIED` (5) | Permission/read-only/AV | Fail operation, preserve source, report |
| `ERROR_FILE_NOT_FOUND` (2), `ERROR_PATH_NOT_FOUND` (3) | Source/destination vanished | Skip (`SourceVanished`) or create parent (destination only, when safe) |
| `ERROR_DISK_FULL` (112), `ERROR_HANDLE_DISK_FULL` (39) | No space | Clean temp, fail, stop batch |
| `ERROR_FILENAME_EXCED_RANGE` (206), `ERROR_INVALID_NAME` (123) | Name/path invalid | Skip (`PathTooLong`/`InvalidName`) |
| `ERROR_CANT_ACCESS_FILE` (1920), cloud-file errors | Cloud/virtual file issue | Skip, no retry storm |
| `ERROR_NETWORK_UNREACHABLE`, `ERROR_BAD_NETPATH`, `ERROR_UNEXP_NET_ERR` | Network failure | Mark folder `unavailable`, retry later |
| Anything else | Unknown | Fail operation safely, log code, never force |

No `unwrap()`/`expect()` on filesystem or database results in non-test code. `clippy::unwrap_used` and `clippy::expect_used` are denied outside tests.

## 4.8 Parent Directory Creation

- Create missing destination parent directories only after all validation passes, and only inside the validated destination root.
- Created directories are recorded in the journal (so undo can remove directories SortDeck created **only if they are empty**; it never deletes non-empty directories and never deletes directories it did not create).
- A dry-run creates no directories.

## 4.9 Safety Invariants (Every One Needs a Test)

Tests MUST assert these directly and reference the invariant ID in the test name or docstring (e.g., `inv_03_...`).

| ID | Invariant |
|---|---|
| INV-01 | An existing file is never overwritten, including under concurrent creation of the destination name. |
| INV-02 | A source file is never deleted unless a verified copy exists at its final destination (cross-volume), or it was moved atomically. |
| INV-03 | A destination file is never deleted by SortDeck (temp files with proven ownership excepted). |
| INV-04 | Any failure leaves the source file intact. |
| INV-05 | A dry-run changes nothing on disk (verified by before/after tree snapshot including timestamps and no new directories). |
| INV-06 | No file inside a protected location, project root, `.sortignore` subtree, or reparse-point directory is ever moved. |
| INV-07 | Hidden, system, cloud-placeholder, temp-extension, and in-use files are never moved. |
| INV-08 | Every completed move has a persisted `move_logs` row sufficient for undo. |
| INV-09 | After a crash at any injected point, recovery reaches a consistent state with no data loss and no duplicate-without-warning. |
| INV-10 | Undo never overwrites an existing file and never deletes anything except empty directories SortDeck created. |
| INV-11 | A backend plan revalidation rejects any operation whose source, destination, rule, or fingerprint changed. |
| INV-12 | Executing a sweep twice is idempotent (second run plans zero operations). |
| INV-13 | Undo/restore of a file does not cause the watcher to re-sort it (§5.6). |
| INV-14 | The UI never reports full success for a partial or failed batch. |
| INV-15 | No code path moves, copies, or deletes files except through `FsBackend`. |

---
# 5. Transaction Logging, Journal, Undo, and Recovery

This is one of SortDeck's most important systems. Every manual sweep, automatic sorting window, undo, and recovery run receives a unique batch UUID.

## 5.1 Database Foundations

- Location: the per-user local app data directory (`%LOCALAPPDATA%\SortDeck\`), resolved via Tauri/known-folder APIs. Never a network share (WAL is unsafe there). Override with `SORTDECK_DATA_DIR` for development and tests only.
- Pragmas at every connection open: `foreign_keys=ON`, `journal_mode=WAL`, `synchronous=FULL`, `busy_timeout=5000`, `trusted_schema=OFF`.
- Versioned, forward-only migrations in `database/migrations.rs`, tracked in `schema_migrations` and `PRAGMA user_version`. Each migration runs in a transaction.
- Before applying any migration: take a consistent backup using SQLite's backup API into `backups/` (keep the last 3). If the database `user_version` is **newer** than the app supports, refuse to open it (downgrade protection) and show a clear message; never "best-effort" read a newer schema.
- At startup run `PRAGMA quick_check`. On corruption: do not delete the database; preserve it as `sortdeck.corrupt-<timestamp>.db`, start with a fresh database, and tell the user plainly that history/undo from the corrupt file is unavailable but their files are untouched.
- Connection model: one serialized writer (single connection behind a mutex or a dedicated DB thread) plus optional read-only connections. **Never hold a database lock across a filesystem operation.**
- Use prepared statements and parameter binding only; never build SQL by string concatenation.
- Timestamps: Unix epoch milliseconds, UTC, `INTEGER`. Display in local time in the UI.
- Paths are stored as UTF-8 `TEXT` (display) plus a normalized comparison key where uniqueness or lookup matters (`*_key`). Paths that do not round-trip are never stored (§4.6.2).

## 5.2 Durability Rule (Write-Ahead Intent)

For every operation: the journal row (`planned`, with source, destination, fingerprint) MUST be committed to SQLite **before** the corresponding filesystem call. State transitions that follow a completed filesystem step (`verified`, `committed`) MUST be committed before the next irreversible step. The final `move_logs` insert and the journal `completed` update happen in a single SQLite transaction.

SQLite transactions do not make filesystem operations atomic. The recovery matrix (§5.4) exists precisely because the two can diverge after a crash.

## 5.3 Schema (Minimum Required Definition)

Additional columns, indexes, and constraints are allowed; removing or weakening these is not. `docs/DATABASE.md` MUST mirror the final schema.

```sql
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
  priority      INTEGER NOT NULL,             -- 1 = highest; ties broken by lowest id
  is_default    INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0,1)),
  default_key   TEXT UNIQUE,                  -- stable key for seeded defaults
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

-- Normalized extensions (replaces a single `extensions` text column) so uniqueness
-- and normalization are enforced by the database. The API still exposes an array.
CREATE TABLE rule_extensions (
  rule_id     INTEGER NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
  extension   TEXT NOT NULL CHECK (extension = lower(extension)
                                   AND length(extension) BETWEEN 1 AND 32
                                   AND substr(extension,1,1) <> '.'),
  PRIMARY KEY (rule_id, extension)
);

CREATE TABLE batches (
  id                 TEXT PRIMARY KEY,         -- UUID
  kind               TEXT NOT NULL CHECK (kind IN ('manual','auto','undo','recovery')),
  source_folder      TEXT NOT NULL,
  source_folder_id   INTEGER REFERENCES watched_folders(id) ON DELETE SET NULL,
  parent_batch_id    TEXT REFERENCES batches(id),   -- for undo/recovery batches
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

-- id is AUTOINCREMENT: ascending id == execution order; undo iterates DESC.
CREATE TABLE move_logs (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id         TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  original_path    TEXT NOT NULL,
  new_path         TEXT NOT NULL,
  status           TEXT NOT NULL CHECK (status IN ('completed','undone','undo_failed')),
  cross_volume     INTEGER NOT NULL DEFAULT 0 CHECK (cross_volume IN (0,1)),
  sha256           TEXT,                       -- required when cross_volume = 1
  file_size        INTEGER NOT NULL,
  volume_serial    INTEGER,                    -- identity of the file at new_path
  file_id          TEXT,                       -- NTFS/ReFS file ID at new_path (NULL on FAT/exFAT)
  modified_at      INTEGER,                    -- source mtime at move time
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
  fingerprint_json  TEXT,                      -- source fingerprint snapshot at planning time
  error_code        INTEGER,                   -- raw OS error when relevant
  error_message     TEXT,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
);

CREATE TABLE created_directories (             -- directories SortDeck created (for safe undo cleanup)
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id    TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  path        TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE restore_suppressions (            -- prevents undo -> auto-sort loops (§5.6)
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  path_key     TEXT NOT NULL,
  volume_serial INTEGER,
  file_id      TEXT,
  file_size    INTEGER,
  modified_at  INTEGER,
  expires_at   INTEGER NOT NULL
);

CREATE INDEX idx_move_logs_batch      ON move_logs(batch_id, id);
CREATE INDEX idx_move_logs_new_path   ON move_logs(new_path);
CREATE INDEX idx_journal_batch        ON operation_journal(batch_id);
CREATE INDEX idx_journal_open_state   ON operation_journal(state)
  WHERE state NOT IN ('completed','undone','failed');
CREATE INDEX idx_batches_created      ON batches(created_at DESC);
CREATE INDEX idx_suppress_path        ON restore_suppressions(path_key, expires_at);
```

## 5.4 Operation State Machine and Recovery Matrix

States: `planned`, `copying`, `verified`, `committed`, `completed`, `undo_pending`, `undone`, `failed`, `recovery_required`.

Allowed transitions (anything else is a bug and MUST be rejected by the state-machine code):

```text
same-volume move : planned → completed | failed
cross-volume move: planned → copying → verified → committed → completed
                   any non-terminal → failed | recovery_required
undo             : completed(move_log) → undo_pending → undone | failed | recovery_required
terminal states  : completed, undone, failed   (recovery_required requires explicit resolution)
```

### Recovery Matrix (Run At Startup, Before Any Watcher or Job Starts)

Recovery inspects every journal row in a non-terminal state and acts **only** on facts it can verify on disk (file existence, file ID, size, SHA-256). It never guesses and never takes a destructive action on unverified data.

| Journal state | Disk evidence | Recovery action |
|---|---|---|
| `planned` (same-volume) | Source present; destination absent | Move never happened → `failed(interrupted_before_move)`. No retry without a new sweep. |
| `planned` (same-volume) | Source absent; destination present with matching file ID | Move happened → write `move_logs`, mark `completed`. |
| `planned` (same-volume) | Source absent and destination absent/unmatched | `recovery_required` (surface to user; no action). |
| `copying` | Source present; temp present and ownership proven | Delete the temp (app-owned only), mark `failed(interrupted_copy)`. |
| `copying` | Source present; temp absent | Mark `failed(interrupted_copy)`. |
| `copying` | Source absent | `recovery_required` (should be impossible; escalate). |
| `verified` | Source unchanged; temp present and re-hash matches | MAY resume: commit temp → final name, continue the state machine. Otherwise discard temp, mark `failed`. |
| `committed` | Destination verified (hash) == source; source present | Delete source, mark `completed`. |
| `committed` | Destination present; source absent | Verify destination hash vs journal; mark `completed` if consistent, else `recovery_required`. |
| `committed` | Destination present; source present but differs | `recovery_required`. **Do nothing destructive.** |
| `undo_pending` | Per §5.5 verification | Resume the undo for that item, or mark `failed(conflict)`. |
| `recovery_required` | — | Shown in the Recovery Center; user may "Acknowledge", "Reveal files", or "Retry verification". |

Every recovery action is itself logged (a `recovery` batch) and idempotent: running recovery twice yields the same result. Recovery must be safe if interrupted mid-recovery.

## 5.5 Undo Requirements

Undo operates on a whole batch **or** on individual items within a batch. It MUST:

1. Identify the selected batch/items and verify eligibility (batch not `running`; item `status = 'completed'` and not undone).
2. Create an `undo` batch (`parent_batch_id` = original) and journal each item (`undo_pending`).
3. Process items in **reverse execution order** (`move_logs.id DESC`).
4. Confirm the file at `new_path` is **the same file**:
   - NTFS/ReFS: same volume serial + file ID (an edited file is still the same file; a different file with the same name is not).
   - Volumes without stable IDs, or cross-volume moves: SHA-256 match against the recorded hash.
   - On mismatch: skip with `DestinationChanged`/`ModifiedSinceMove`; never restore.
5. Check the original path is free. If occupied: skip that item, report `OriginalPathOccupied`, **never overwrite**.
6. Re-validate the original path with the same rules as forward operations (protected locations, reparse points, traversal). **Database content is untrusted**: a tampered row must not allow moving files into protected locations.
7. Recreate missing original parent directories only when safe (inside the original watched root, not protected, not a reparse point). If the watched root itself no longer exists or is unavailable: skip with `OriginalFolderMissing`.
8. Restore using the same `safe_move()` engine (same-volume or cross-volume), so all collision, verification, and journaling guarantees apply.
9. After each successful item: update `move_logs` (`status='undone'`, `undone=1`, `undone_at`, `undo_batch_id`) and the journal in a single transaction.
10. After the batch: remove directories listed in `created_directories` **only if empty**; never remove non-empty directories or directories SortDeck did not create.
11. Report a precise outcome: restored count, skipped count with per-item reasons. A batch with any skipped item shows as "partially undone", never "undone".

If undo is interrupted, recovery resumes it safely (§5.4). Undo never deletes any file.

## 5.6 Preventing Undo → Auto-Sort Loops

Restoring a file into a watched folder would otherwise trigger the watcher and re-sort it, forever. Therefore:

- Every file restored by undo into an auto-sorted watched folder gets a `restore_suppressions` row (path key, file identity, size, mtime) with a default TTL of 24 hours.
- Auto-sort ignores a file while a matching suppression exists **and** the file is unchanged. A change to size/mtime, or expiry, lifts the suppression.
- Suppression affects **automatic** sorting only. A manual sweep still lists the file, with a "restored by undo" badge.
- INV-13 is verified by a test that undoes a batch in a watched folder and asserts zero new operations.

## 5.7 Retention and Maintenance

- Default retention: 365 days of history (configurable, including "keep forever").
- Pruning removes only fully terminal batches older than retention; never batches with a non-terminal or `recovery_required` journal row.
- Periodic `PRAGMA wal_checkpoint(TRUNCATE)` at idle/shutdown; `incremental_vacuum` after large prunes.
- History export (CSV/JSON) of batches and move logs SHOULD be provided in Settings.

## 5.8 Batch Status Semantics

| Status | Meaning |
|---|---|
| `running` | Execution in progress. |
| `completed` | Every planned operation either succeeded or was a pre-declared skip. |
| `partial` | At least one success and at least one failure. |
| `failed` | No operation succeeded and at least one failed. |
| `cancelled` | User/system cancelled; completed moves remain undoable. |
| `recovery_required` | At least one operation needs human review. |

The UI MUST map statuses faithfully (INV-14): never green/"success" for `partial`, `failed`, `cancelled`, or `recovery_required`.

---

# 6. Sorting Rules Engine

SortDeck supports default rules and user-created rules.

## 6.1 Default Rules (Seed v1)

| Category | Extensions | Destination |
|---|---|---|
| Images | png, jpg, jpeg, webp, gif, svg, ico, bmp, heic, avif | Images/ |
| Documents | pdf, doc, docx, txt, md, rtf, odt, xls, xlsx, csv, ppt, pptx | Documents/ |
| Installers | exe, msi, msix, bat, cmd, ps1 | Installers/ |
| Archives | zip, rar, 7z, tar, gz, bz2, xz, iso | Archives/ |
| Videos | mp4, mkv, mov, avi, webm, wmv, flv | Videos/ |
| Audio | mp3, wav, flac, aac, ogg, m4a | Audio/ |
| Code | json, xml, yaml, yml, sql, py, js, ts, html, css | Code/ |

Notes (documented, not silently changed):

- `ts` is also the MPEG transport-stream video extension. It is seeded under Code; users can re-map it. The UI shows overlap warnings.
- `bat`, `cmd`, `ps1` are scripts seeded under Installers per the original product definition; they are never executed by SortDeck. Maintainer may revisit (§19).
- Seeding happens once, in a single transaction, on first initialization and records a `defaults_version` in `settings` so later app versions can **add** new default categories without overwriting user edits.
- Destinations are **relative to the watched folder root** (`<watched>/Images/`).

## 6.2 User Capabilities

Users can enable/disable rules, rename rules, add/remove extensions, change destinations, reorder priority, create custom rules, and restore defaults.

**Restore Defaults** re-creates any missing default rules and resets edited default rules to factory values **after explicit confirmation**. It never deletes custom rules and never triggers file movement.

## 6.3 Matching Semantics

- Normalize extensions on input and on comparison: trim, lowercase, strip one leading dot, reject empty values and the characters `/ \ : * ? " < > |` and whitespace.
- Case-insensitive matching (Unicode-aware lowercase).
- Multi-dot filenames: build candidate suffixes longest-first. `archive.tar.gz` → candidates `tar.gz`, then `gz`. `report.final.v2.pdf` → `final.v2.pdf`, `v2.pdf`, `pdf`; the first candidate that any enabled rule contains wins (most specific suffix first).
- Dotfiles: `.gitignore` has **no extension** (the leading dot is part of the name). `.env.local` → extension `local`.
- Trailing dot or no extension → unmatched.
- Priority: ascending `priority` (1 = highest). Ties broken by lowest rule `id`. Deterministic across runs.
- Overlapping extensions across enabled rules are allowed; the highest-priority enabled rule wins. The UI MUST show a non-blocking warning naming the winner.
- Disabled rules never match.
- Unmatched files are skipped with `NoMatchingRule`. SortDeck never invents a destination.
- Rule changes **never** trigger movement by themselves; they affect only the next explicit sweep or newly observed files (§8.2).

## 6.4 Rule Validation

On create/edit and again at plan time:

- Name: 1–80 chars, trimmed, no control characters.
- Extensions: at least one; each ≤ 32 chars; normalized; no duplicates within a rule.
- Destination: non-empty; valid Windows path segments (no reserved names, trailing dots/spaces, invalid characters); relative destinations MUST NOT escape the watched root (`..`, rooted components); absolute destinations pass full §4.3.8 validation.
- Priority: dense, deterministic ordering maintained by the backend (the frontend sends an ordered list of rule IDs, not raw numbers).

Extensibility: the schema and matcher MUST allow adding match types (name pattern, size, age) later without a breaking migration, but v1 implements extension matching only.

---

# 7. Scanner and Dry-Run Planner

## 7.1 Scanner

The scanner is **read-only**. It identifies candidate files, applies all exclusions (§4.3–4.4), matches rules, and emits a proposed plan. It MUST NOT move, rename, copy, delete, create, or touch files or directories (no directory creation, no attribute changes, no probe files).

## 7.2 Planned Operation Fields

```text
operation_id          (stable within the plan)
source_path
destination_path      (proposed; final name chosen at execution, §4.1.1)
original_filename
proposed_filename
file_size
source_fingerprint    (volume serial, file ID, size, mtime)
matched_rule_id / matched_rule_name
operation_status      (eligible | skipped | conflict_renamed | warning)
skip_reason           (SkipReason enum, below)
cross_volume_estimate (bool)
warnings[]
```

### SkipReason (Typed Enum, Stable Codes)

```text
NoMatchingRule, RuleDisabled, ProtectedLocation, ExcludedDirectory, ProjectRoot,
SortIgnore, HiddenAttribute, SystemAttribute, ReparsePoint, CloudPlaceholder,
CloudManagedCrossVolume, TemporaryExtension, SpecialFile, OfficeLockFile,
FileInUse, UnstableFile, TooRecent, AlreadyInDestination, DestinationInvalid,
DestinationUnavailable, PathTooLong, InvalidName, NonUnicodePath, ReadOnlyCrossVolume,
InsufficientSpace, FilesystemLimit, SourceVanished, SourceChanged, PermissionDenied,
RestoredByUndo, DepthLimit, ScanLimit
```

Each code has a stable identifier, an English message in the i18n catalog, and a test.

## 7.3 Plan Semantics

- **Name reservation:** the planner reserves destination names within the plan (case-insensitive) so two sources never target the same final name; reserved names are advisory previews.
- **Advisory collisions:** previews show proposed `photo (1).png` names; final names are selected race-safely at execution.
- **No side effects:** destination directories are not created during planning; writability is assessed by inspecting the parent and ACL/attributes without creating files.
- **Plan summary** (always shown): total discovered, eligible, skipped (grouped by reason), proposed destinations, conflict renames, total bytes to move, files likely requiring cross-volume copy, warnings.
- **Large plans:** the backend stores the plan and exposes the summary plus paged items (`get_plan_page(plan_id, offset, limit)`); the frontend virtualizes the list. Never send 100k rows over IPC in one message.
- **Selective execution:** the user may deselect individual operations before confirming; deselected IDs are sent with the confirmation.

## 7.4 Plan Lifecycle and Revalidation

- The plan is **owned by the backend** and addressed by `plan_id`. The frontend sends `plan_id` (+ deselected operation IDs) to execute — never a list of paths. A frontend-supplied plan is never authoritative.
- A plan expires (default 10 minutes) and is invalidated if rules, folder settings, or global safety settings change (config revision counter).
- At execution the backend revalidates **every** operation: source exists; fingerprint unchanged; still not excluded; rule still matches and is enabled; destination still valid; not overlapping; stability checks (for auto mode); free space.
- Invalid operations are rejected or recomputed individually and reported (`SourceChanged`, etc.); the rest of the plan proceeds.
- Manual sweeps require explicit user confirmation each time. There is no "always allow".

---

# 8. Background Watcher and Job Queue

Implement monitoring with `notify` + `notify-debouncer-full`.

## 8.1 Requirements

- Multiple watched directories; per-folder `enabled`, `auto_sort`, recursion, depth, and minimum file age.
- Respect all safety exclusions, disabled rules, `.sortignore`, and the global pause/resume.
- Avoid processing the same file multiple times (in-flight registry keyed by normalized path **and** file identity).
- **Ignore SortDeck's own events:** paths written by SortDeck (temp files `.sortdeck-*.sdpart`, destinations, restored files) are suppressed by an expected-event table with short TTL, keyed by path + file identity; anything else is evaluated normally.
- Handle watcher errors and dropped events. `ReadDirectoryChangesW` buffers can overflow: treat overflow/error as "events lost" and trigger a **reconciliation scan** of that folder.
- Periodic low-priority reconciliation (default every 10 minutes) for robustness.
- Never run long file operations inside event callbacks. Callbacks only enqueue lightweight candidates.
- Network/UNC and some virtual filesystems do not deliver reliable events: use a polling watcher fallback with a longer interval and document it.
- A watched folder that disappears or becomes unreachable is marked `unavailable`, retried with backoff, and never auto-removed.

## 8.2 Auto-Sort Scope (No Surprise Moves)

- Enabling `auto_sort` on a folder sets `auto_sort_enabled_at`. Auto-sort only acts on files for which a create/rename-in/modify event is observed afterward, plus files created/modified after `last_reconciled_at` found by reconciliation (e.g., downloaded while the app was closed).
- **Pre-existing files are untouched** until the user runs a manual sweep (with preview). A one-time "also sort existing files" offer MAY exist, but only through the normal preview + confirmation flow.
- Launching the app, editing rules, or restoring defaults never moves pre-existing files.
- Auto-sort uses the same validation, stability checks, safety systems, and transaction logging as manual sorting. Auto batches group operations by time window (default 30 s) so history is not flooded with single-file batches; per-item undo remains available.

## 8.3 Job Queue and Concurrency

- Bounded queue with backpressure; duplicates coalesced; overflow degrades to reconciliation, never to dropped safety checks.
- Default: **1** mover worker per destination volume, **2** globally. Hashing and copying run in low-priority/background I/O mode so SortDeck does not degrade the user's foreground work.
- A single in-flight registry guarantees one operation per source path at a time across manual sweeps, auto-sort, undo, and recovery.
- Worker panics are caught per job (`catch_unwind`), logged, and mark the job failed; a panic MUST NOT kill the watcher or leave an operation without a journal outcome.
- Recovery (§5.4) completes **before** the watcher and queue start.

## 8.4 Pause, Resume, Quit

- **Pause:** no new automatic operations start; in-flight operations finish (or are cancelled safely if the user chooses). Pause state persists across restarts and is reflected in the tray and dashboard.
- **Resume:** triggers reconciliation for the pause window using `last_reconciled_at` rules (no mass-sorting of old files).
- **Quit (real):** stop accepting jobs → drain or cancel in-flight work within a grace period (default 10 s) → ensure journal consistency → stop watchers → checkpoint WAL → exit. A forced kill is recoverable by design.

## 8.5 Health States

Expose per-folder and global states to the UI: `watching`, `paused`, `unavailable`, `ignored (.sortignore)`, `error`, `reconciling`. Surface watcher errors as notifications with actionable text.

---

# 9. Tauri Security and Threat Model

Treat the frontend as an **untrusted client** of the Rust backend.

## 9.1 Requirements

- Use narrowly scoped capabilities per window; expose only the plugins and permissions required (dialog, autostart, notification, single-instance, window-state). Do not enable `fs` or `shell` plugins.
- Strict Content Security Policy: no `unsafe-eval`, no remote scripts/styles/fonts/images, `default-src 'self'`; no `withGlobalTauri`.
- Disable devtools in release builds. Block external navigation and new windows; any "open link" action goes through a validated backend command.
- Validate every IPC argument in Rust (types, ranges, lengths, enum values). Validate and normalize every path in Rust (§4.6). Reject unknown/extra fields where feasible.
- IPC commands accept **IDs** (`plan_id`, `folder_id`, `batch_id`, `rule_id`) rather than arbitrary paths wherever possible. The only user-provided paths are from the native folder picker (re-validated by the backend) and rule destinations (validated by §6.4).
- "Reveal in Explorer" is a backend command taking a `move_log` ID (never a raw path from the UI) and revealing a validated path without executing anything.
- Errors returned to the frontend are typed codes + safe messages; they never include stack traces or raw internal paths beyond what the user already owns and sees.
- No unrestricted shell execution; never execute user-provided file contents; never open files on the user's behalf except via the OS reveal action above.
- Do not expose database internals. Do not store unnecessary personal information.
- SortDeck runs as a standard user: `asInvoker` manifest, never requests elevation, autostart is per-user (HKCU).

## 9.2 Threat Model Summary (Maintain in `docs/SAFETY.md`)

| Threat | Mitigation |
|---|---|
| Compromised/buggy WebView sends arbitrary paths | Backend-owned plans, ID-based IPC, backend path validation |
| Path traversal in rule destinations | §6.4, §4.6.1 |
| Junction/symlink swap between plan and execution (TOCTOU) | §4.6.3 revalidation; residual risk documented |
| Tampered/corrupt database used to drive undo | Revalidate every undo path; verify file identity; quick_check at startup |
| Malicious filenames (RTL override characters, control characters, very long names) | Render as text only; escape/visualize bidi and control characters in the UI; no `dangerouslySetInnerHTML` |
| Supply-chain compromise | Lockfiles committed, `npm ci`, `cargo --locked`, `cargo deny`/`cargo audit`/`npm audit` in CI, pinned actions, signed releases |
| Data loss from crash/power loss | Write-ahead journal, recovery matrix, verified copies |
| Local privilege abuse | No elevation, per-user data directory, no auto-run of user content |

Out of scope: a fully compromised user account or administrator-level attacker.

---
# 10. Frontend and UI/UX Requirements

## 10.1 Visual Direction

SortDeck feels like a polished Windows 11 desktop utility.

- Fluent-inspired design: clean, calm, dense-but-readable.
- Dark mode by default; light mode and "Follow system" available. Theme switching is instant and persisted.
- Neutral slate/zinc palette, subtle borders and elevation, one accent color (respect the Windows accent color when available, with a contrast-safe fallback).
- Optional Mica/Acrylic window backdrop where supported, with a solid-color fallback; never required for legibility.
- Consistent spacing scale, a single type scale, minimal animation, clear hierarchy.
- No unnecessary gradients, no emojis in interface labels, no oversized typography, no card-on-card clutter.
- Lucide React icons only.
- Responsive within desktop window constraints (minimum window size defined and enforced, e.g., 960×600), correct under 100–250% DPI scaling.

## 10.2 Accessibility (Release Gate)

Target **WCAG 2.2 AA** for the in-app UI.

- Full keyboard operability; logical tab order; visible focus indicators; no keyboard traps; dialogs trap focus correctly and restore it on close.
- Correct semantics/roles/labels for every control (screen readers: Narrator, NVDA). Icon-only buttons have accessible names. Live regions announce toast notifications and sweep progress politely.
- Color contrast ≥ 4.5:1 for text, ≥ 3:1 for UI components in both themes; status is never conveyed by color alone (icon + text).
- Honor `prefers-reduced-motion` and Windows High Contrast (`forced-colors`).
- Automated checks (`vitest-axe`) on every page and dialog; manual screen-reader pass before release.

## 10.3 Internationalization Readiness

- All user-visible strings come from `src/i18n/` catalogs (English only in v1). No hard-coded UI strings in components.
- Dates, numbers, and byte sizes use `Intl` formatters. Layout tolerates ~40% longer strings.

## 10.4 Filename and Path Display

- Filenames and paths render as plain text only; never inject HTML.
- Bidirectional-override and control characters are visualized/escaped (a file named `invoice‮fdp.exe` must not be able to impersonate a PDF).
- Long paths truncate in the middle with the full path available in a tooltip and a "Copy path" action. Paths never overflow the layout.

## 10.5 Main Navigation and Pages

### Dashboard

- Total watched folders; total files organized; recent activity; storage organization statistics.
- Global watcher status with pause/resume; quick access to manual sorting.
- A visible banner when any operation is `recovery_required` or a folder is `unavailable`.

### Folders

- Add watched folders via the native folder picker (backend re-validates, §4.3.4).
- Remove, enable/disable; configure auto-sort, recursion, max depth, min file age; ignore behavior status (`.sortignore` detected).
- Trigger manual sweeps; preview sorting operations (virtualized, grouped by destination and by skip reason, filterable and searchable, with selective deselect).
- Removing a watched folder never moves or deletes files; it keeps history.

### Rules

- Display default and custom rules; rule editor with inline validation, overlap warnings, drag-and-keyboard priority reordering, and a clear "Restore defaults" confirmation that states it will not move files.

### History

- Paginated batch list; per-batch details (original path, destination, timestamps, status, reasons).
- Batch-level **and** item-level undo; undo availability; conflict reporting with per-item reasons; "Reveal in Explorer" (§9.1).
- Filters by status/date/folder; export.

### Settings

- Theme (Dark/Light/System), launch on Windows startup, start minimized, minimize to tray, global auto-watch toggle, default scan depth, retention period, background operation preferences, notification preferences.
- Application version; database location/size/schema version; recovery information; "Export diagnostics (redacted)"; "Open logs folder".

## 10.6 First-Run Experience

- A short, skippable welcome explaining: SortDeck never overwrites files, always previews manual sweeps, and keeps undo history.
- No folder is auto-enabled. The Downloads folder (from the Windows known-folder API, which can be relocated) MAY be offered as a one-click suggestion, added with auto-sort **off** and a preview shown first.
- Empty states always explain the next action.

## 10.7 System Tray

- Menu: Open SortDeck, Pause/Resume Auto-Watch, Quit SortDeck. The tray icon/tooltip reflects watching, paused, and attention-needed states.
- Closing the window minimizes to tray when that preference is enabled; otherwise it quits cleanly.
- Real **Quit** performs the clean shutdown in §8.4. Left-click opens; the app is single-instance (a second launch focuses the first window).

## 10.8 Feedback and Notifications

- Toasts in-app; native notifications when the window is hidden, rate-limited and coalesced (one summary per auto batch, not one per file).
- Notify for: sweep results, skipped files, failures, undo completion, recovery-required operations, watcher errors, unavailable folders.
- **Never display success when the backend completed only part of an operation.** Use distinct visuals and wording for success / partial / failed / cancelled (INV-14).
- Destructive or irreversible-feeling actions (Restore defaults, Remove folder, Execute sweep, Undo batch) require a confirmation dialog that states exactly what will and will not happen.

## 10.9 Loading, Empty, and Error States

Every list/page has designed loading (skeleton or spinner), empty, and error states. Long operations show progress (counts + bytes), are cancellable, and survive navigation between pages (job state lives in the backend, not in a component).

## 10.10 Frontend Engineering Rules

- IPC only through `src/services/` typed wrappers using generated types.
- No business logic that affects safety in the frontend.
- No `dangerouslySetInnerHTML`, no `eval`, no inline event-handler strings.
- Subscribe to backend events (`sweep:progress`, `watcher:status`, `recovery:required`, etc.) with throttling; always unsubscribe on unmount.
- Avoid unnecessary rerenders (memoization where measured to matter); virtualize large lists.

---

# 11. Performance and Resource Budgets

Prioritize predictable resource usage over aggressive optimization. The following are **engineering targets to validate by measurement** on release builds; they are not marketing claims (do not publish installer sizes or RAM numbers without measured evidence).

| Area | Target |
|---|---|
| Idle CPU (5 watched folders, no activity) | ≈ 0%; < 1% average over a 10-minute sample |
| Cold start to interactive window | < 2 s on reference hardware (document the reference machine) |
| Plan generation | Streams results; UI stays responsive for 100k+ entries; no UI thread blocking |
| History | Paginated (default 50/page); queries use indexes; no full-table loads |
| Memory | Bounded: no whole-file reads, no unbounded queues, no unbounded in-memory plans (spill/paginate) |
| Copy/hash | Streaming with ≥ 1 MiB buffers; low-priority I/O; cancellable |

Rules:

- Avoid unnecessary rescans and avoid scanning on every event; coalesce.
- Bounded concurrency (§8.3). Release handles promptly (RAII).
- Stop/pause watchers cleanly on shutdown.
- Provide cancellation for scans, plans, sweeps, hashing, and copies.
- Track benchmark baselines (`criterion` for scanner/rule matching/path validation); a regression > 20% in CI-tracked benchmarks requires explanation.
- Record measured numbers in `docs/ARCHITECTURE.md` after each release build.

---

# 12. Errors, Logging, Configuration, and Data Location

## 12.1 Error Model

- A single typed `AppError` (via `thiserror`) with a **stable string code**, a user-safe message key (i18n), and optional structured details. The raw OS error code is preserved for logs.
- Errors crossing IPC serialize to `{ code, messageKey, details? }`. The frontend maps codes to localized messages; it never parses error text.
- Skip reasons (§7.2) and error codes are documented in `docs/IPC.md` and covered by tests.
- No `unwrap()`/`expect()`/`panic!` in non-test code paths that can be reached by user data. Install a panic hook that logs the panic with context; panics inside workers are contained (§8.3).
- Never silently ignore an error: handle it, log it, or propagate it.

## 12.2 Logging

- `tracing` with structured fields; rotating local files in `%LOCALAPPDATA%\SortDeck\logs\` (default: 10 files × 5 MB). Levels: `error`, `warn`, `info` (operations/batches), `debug`/`trace` (developer only, off in release by default).
- Every operation log line carries `batch_id`, `operation_id`, and the outcome. This is what makes operations traceable.
- Logs stay local. "Export diagnostics" produces a zip with logs, schema version, app version, and OS info, **redacting** user names and file names by default (opt-in to include paths).
- Never log file contents. Never log secrets.

## 12.3 Configuration and Data Layout

```text
%LOCALAPPDATA%\SortDeck\
├── sortdeck.db            (+ -wal, -shm)
├── backups/               (pre-migration backups, last 3)
├── logs/
└── (no other writable state)
```

- Settings live in the `settings` table (typed accessors in Rust; defaults centralized in `config.rs`).
- Default settings (v1): theme=dark, launch_on_startup=false, start_minimized=false, minimize_to_tray=true, global_auto_watch=true (affects only folders whose `auto_sort=1`), default_scan_depth=1, retention_days=365, notifications=true.
- No registry writes other than the autostart entry managed by the plugin.
- **Uninstall:** removes the app; asks whether to delete user data (history, settings). Default: keep. Uninstall never touches the user's sorted files.
- Single instance enforced at startup; the second instance activates the first.

---

# 13. Testing Requirements

Never test destructive filesystem operations against personal folders, real drives, or production data.

## 13.1 Test Harness Safety (Mandatory)

- Every filesystem test runs inside a fresh temp sandbox containing a sentinel file `.sortdeck-test-sandbox`.
- `FsBackend` used by tests (and a debug assertion in the real backend when `cfg(test)`/`SORTDECK_TEST=1`) MUST **refuse any operation whose path is outside a sandbox containing the sentinel**.
- Tests never read the real `%USERPROFILE%`, Downloads, Desktop, or the real SortDeck data directory (`SORTDECK_DATA_DIR` always points into the sandbox).
- Tests make no network calls and do not touch the real registry/autostart (mock the plugin boundary).

## 13.2 Fault Injection and Crash Testing

- All engine logic runs against the `FsBackend` trait. A `FaultyFsBackend` can inject, at named points: `ERROR_NOT_SAME_DEVICE`, `ERROR_ALREADY_EXISTS` races, sharing violations, access denied, disk full, copy truncation, hash mismatch, source modified during copy, delete-source failure, cancellation mid-copy.
- **Crash simulation:** named failpoints between every journal transition and every filesystem step; crash tests run the operation in a child process that is killed at the failpoint, then run recovery in a fresh process and assert the recovery-matrix outcome (INV-09). Every row of §5.4 has a crash test.
- Recovery MUST be tested for idempotency (run twice) and for interruption during recovery.

## 13.3 Real Cross-Volume Tests

- Fault injection covers the logic; real behavior is also tested on actual separate volumes: CI creates and mounts a small **VHDX** (`diskpart`) on the Windows runner; local developers use `scripts/` to do the same. These tests are `#[ignore]`d by default and run in CI and before release.
- Also test: NTFS file ID behavior, ADS preservation (`Zone.Identifier` survives a cross-volume move), long paths, Unicode names, FAT32/exFAT volume (4 GiB limit, no file IDs), read-only files, and cross-volume undo.

## 13.4 Test Environment Template

```text
SortDeck-Test/            (sandbox root, contains .sortdeck-test-sandbox)
├── Downloads/
│   ├── image.png
│   ├── document.pdf
│   ├── archive.zip
│   ├── archive.tar.gz
│   ├── video.mp4
│   ├── script.py
│   ├── .gitignore
│   ├── unknown.xyz
│   └── noextension
├── Protected/
│   ├── .sortignore
│   └── protected.png
├── Project/
│   ├── .git/
│   └── README.md
├── Destination/
└── SecondVolume/         (VHDX mount for cross-volume tests)
```

## 13.5 Required Scenarios

Each scenario has a predictable, asserted outcome; each safety invariant (§4.9) maps to at least one named test.

1. Normal sorting; idempotent second run (INV-12).
2. Duplicate filenames in one plan and across runs (case-insensitive).
3. Existing destination conflicts, including a competing process creating the destination name mid-operation (INV-01).
4. Cross-drive moves (real VHDX) and simulated cross-volume.
5. Interrupted copying at every step; hash verification failure; source modified during copy.
6. Disk full during copy; destination removed mid-copy.
7. Undo after success; undo when original is occupied; undo when destination file replaced; undo of cross-volume moves; partial undo; undo interruption; per-item undo.
8. Application crash during every operation state; restart with pending transactions (§5.4 matrix).
9. Temporary download files; Office `~$` lock files; locked/in-use files; read-only files.
10. Hidden/system files; cloud placeholder attributes; reparse points (symlink, junction); `.sortignore` in root/ancestor/descendant.
11. Recursive scanning with depth limits; project roots (`.git`); `node_modules`; protected locations by exact path **and** negative tests that `D:\Projects\Windows` is *not* wrongly protected.
12. Path edge cases: long paths (>260), reserved device names, trailing dots/spaces, invalid characters, ADS syntax, `..` traversal in rule destinations, non-Unicode paths, 8.3 short names, mapped/subst drives, UNC.
13. Watcher: generated events from SortDeck's own operations; duplicate notifications; event overflow → reconciliation; folder removed/reconnected; rapid bursts (10k files); rename storms; browser-style `.crdownload` → final rename.
14. Undo → watcher loop (INV-13).
15. Rule engine: priorities, overlaps, disabled rules, multi-dot names, dotfiles, compound extensions, case variants, Unicode lowercase.
16. Large files (≥ 4 GiB on NTFS, streaming memory bound), many files (100k), huge directories.
17. Unmatched extensions skipped with `NoMatchingRule`; unmatched never gets a destination.
18. Dry-run invariance (INV-05) via full before/after snapshot including mtimes and directory set.
19. Plan lifecycle: expired plan, config-revision change, source changed after preview, selective deselect.
20. Database: migrations from every released schema fixture; downgrade refusal; corruption handling; crash during migration; foreign keys enforced; WAL checkpoint; concurrent readers.
21. Frontend: component tests, accessibility (axe) on every page/dialog, status mapping (INV-14), filename sanitization (bidi/control chars), IPC error mapping, loading/empty/error states.
22. E2E smoke (WebdriverIO + `tauri-driver`): launch, add folder in sandbox, preview, execute, undo, pause/resume, tray quit.
23. Single-instance behavior; graceful shutdown mid-operation; shutdown/sleep during copy.

## 13.6 Quality Gates

- Property-based tests (`proptest`) for collision naming, rule matching, path normalization/validation, and state-machine transitions.
- Fuzzing (`cargo-fuzz` or `proptest` at high case counts) for the path validator and extension parser.
- Coverage (`cargo-llvm-cov`): `engine/`, `fsops/`, `recovery/`, `database/` target ≥ 90% line coverage with branch review. Coverage is a tool, not the goal: uncovered safety branches need an explanation.
- Mutation testing (`cargo-mutants`) on `engine/paths.rs`, `engine/rules.rs`, `engine/naming.rs`, `engine/mover.rs`, `recovery/` before each release; surviving mutants in safety logic are triaged.
- Tests must be deterministic. No sleeps for synchronization where a signal exists; no order dependence; no flakes tolerated (quarantine requires an issue and owner).
- Manual pre-release QA on a clean Windows 11 VM: install, first run, autostart, tray, upgrade-in-place, uninstall (keep/delete data), 100–250% DPI, dark/light/high-contrast, OneDrive-backed Downloads, USB drive removal mid-copy, SMB share, long paths, Narrator pass.

---

# 14. Verification Commands and CI Gates

Before declaring any task complete, run the applicable commands and report real results. If a command cannot be executed, state explicitly that it was **not verified**.

From the repository root:

```bash
npm ci
npm run lint
npm run typecheck
npm run format:check
npm test
npm run build
npm run gen:types && git diff --exit-code src/types/generated   # generated IPC types must be current
```

Rust (from `src-tauri/`):

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo test --locked -- --ignored        # real cross-volume/VHDX tests, Windows CI only
cargo deny check                        # licenses, advisories, bans, sources
cargo audit
```

Supply-chain and policy checks:

```bash
npm audit --omit=dev
# Banned-API guard: fails if std::fs::rename/copy/remove_* appear outside fsops/ and platform/
```

Packaging (Windows only, from the repository root):

```bash
npm run tauri build
```

Do not claim an MSI/EXE installer works merely because the frontend and backend compile. Installer claims require running the installer on a clean Windows environment (§16, §13.6).

## 14.1 CI Pipeline (Required)

`.github/workflows/ci.yml` on `windows-latest` (and Linux for fast pure-Rust unit tests where applicable): lint, typecheck, unit/integration tests, VHDX cross-volume tests, `cargo deny`, `cargo audit`, `npm audit`, generated-type freshness, build. `codeql.yml` for JS/TS and Rust where supported. Actions pinned by commit SHA. Dependabot enabled for npm, cargo, and GitHub Actions. PRs cannot merge on red CI.

## 14.2 Completion Criteria

A phase is complete only when:

- The implementation matches this specification.
- Relevant Rust tests pass, including failure-path and crash tests for that phase.
- TypeScript builds with strict mode; lint and format checks pass.
- No unexplained compiler/clippy warnings or errors remain.
- Existing functionality is not broken; previous phases' tests still pass.
- Safety requirements and invariants are preserved (the relevant `INV-xx` tests exist and pass).
- Documentation is updated; limitations are documented.

---

# 15. Implementation Roadmap

Implement **one phase at a time**. Do not proceed until the current phase meets its exit gate **and the maintainer explicitly instructs you to continue**.

## Phase 0 — Repository Bootstrap and Engineering Baseline

Implement:

- Tauri v2 + React 19 + TypeScript (strict) scaffold; Tailwind v4; Vite; pinned `rust-toolchain.toml`, `.nvmrc`, lockfiles.
- Lint/format/type tooling (ESLint, Prettier, clippy config incl. `disallowed-methods`, rustfmt), `deny.toml`, CI skeleton, Dependabot, issue/PR templates.
- `errors.rs`, `logging.rs`, `config.rs` skeletons; `FsBackend` trait + test sandbox harness (sentinel guard).
- Docs skeleton (`ARCHITECTURE.md`, `SAFETY.md`, `DATABASE.md`, `IPC.md`, `TESTING.md`, `RELEASE.md`), `README`, `CONTRIBUTING`, `SECURITY`, `CHANGELOG`, license.
- Capabilities/CSP baseline (§9.1), single-instance plugin, app manifest (`asInvoker`, long-path aware).

**Exit gate:** CI green on an empty app; banned-API guard proven by a failing example; sandbox guard proven by a refusing test.

## Phase 1 — Foundation and Database

Implement:

- Shared models; generated TypeScript types.
- SQLite initialization, pragmas, versioned migrations with pre-migration backup, downgrade protection, quick-check handling (§5.1).
- Full schema (§5.3); default rule seeding with `defaults_version`.
- `engine/paths.rs`: path normalization, protected-location policy, disallowed-watched-folder rules (§4.3, §4.6) — required here because watched-folder CRUD depends on it.
- Watched-folder CRUD with overlap/loop validation; rule CRUD with §6.4 validation and backend-owned priority ordering; settings accessors.
- Typed IPC commands (thin) with validation.

**Deliverable:** A working application foundation with persistent, validated local settings.  
**Exit gate:** migration tests from empty and from fixtures; CRUD validation tests; path-policy unit and property tests (including negative cases); INV-06 tests for the path policy.

## Phase 2 — Scanner and Rule Engine

Implement:

- Read-only scanner with depth limits, cancellation, reparse-point handling, attribute checks (hidden/system/cloud/offline), Office lock detection.
- Extension matching with compound suffixes, priorities, overlap handling (§6.3).
- All exclusions: protected locations, excluded names, project roots, `.sortignore` (ancestors), temp extensions.
- Destination validation; collision **name reservation**; file stability metadata and fingerprints.
- Dry-run planner with `SkipReason` enum, backend-owned plans, paging API, expiry/revision handling (§7).

Write unit and property tests for every safety filter and rule-matching behavior.

**Deliverable:** A scanner that generates reliable previews without modifying files.  
**Exit gate:** INV-05, INV-06, INV-07, INV-11 (plan-level), INV-12 (plan idempotency) tests pass; before/after snapshot test proves zero side effects.

## Phase 3 — Safe File Operations and Recovery

Implement:

- `FsBackend` real implementation on Win32 primitives (§4.1.1); `platform/windows` wrappers with RAII handles.
- Collision-safe naming; same-volume no-overwrite moves; cross-volume copy → verify (SHA-256) → commit → delete-source state machine (§4.2).
- Temp-file ownership and cleanup; ADS/timestamp preservation; free-space and filesystem-limit checks.
- Operation journal with write-ahead durability; batch transaction logging; created-directory tracking.
- Undo (batch and item) with identity verification and suppression table (§5.5, §5.6).
- Startup recovery implementing the full matrix (§5.4).
- Cancellation, in-flight registry, and safe shutdown hooks.

Write comprehensive filesystem integration, fault-injection, and crash tests.

**Deliverable:** A tested file-moving engine with recoverable transactions.  
**Exit gate:** INV-01 through INV-15 each have passing tests; every recovery-matrix row has a crash test; real VHDX cross-volume tests pass on Windows CI.

## Phase 4 — Frontend Application

Implement:

- Sidebar/app layout, Dashboard, Folders, Rules editor, dry-run preview (virtualized, selective deselect), History (paginated, undo UI), Settings, first-run experience, Recovery Center, toasts/notifications, loading/empty/error states, i18n catalog, theme system.
- Accessibility pass (§10.2) and filename sanitization (§10.4).

**Deliverable:** A fully usable desktop interface connected to the Rust backend.  
**Exit gate:** component and axe tests pass; INV-14 UI tests pass; E2E smoke passes against the real backend in a sandbox.

## Phase 5 — Background Watcher and System Tray

Implement:

- `notify` + `notify-debouncer-full` watchers, overflow handling, reconciliation, polling fallback.
- Stability checks, lock detection, minimum age, deferral/backoff.
- Bounded job queue, in-flight registry, self-event suppression, panic containment.
- Auto-sort scoping (§8.2), auto-batch windows, global/per-folder pause/resume, unavailable-folder handling.
- System tray, minimize-to-tray, autostart integration, single-instance activation, native notifications, graceful shutdown (§8.4).

**Deliverable:** Reliable automatic file organization running in the background.  
**Exit gate:** INV-13 and watcher scenario tests pass; soak test (10k-file burst, 1h idle) shows bounded memory and ≈0 idle CPU; shutdown-mid-operation test passes.

## Phase 6 — Polish, Testing, and Packaging

Implement:

- Dark/light/system theme polish, high-contrast support, final accessibility audit.
- Error-handling review, performance optimization against the §11 targets (measured).
- Database migration regression suite; mutation testing and coverage review of safety modules.
- Windows packaging (NSIS default; MSI optional), WebView2 install mode, installer verification on a clean VM, code signing (§16).
- Documentation, `CHANGELOG`, release notes, SBOM, checksums, GitHub release preparation.

**Deliverable:** A tested Windows 11 application ready for public release.  
**Exit gate:** full CI green; manual QA checklist (§13.6) completed and recorded; signed installer verified on a clean Windows 11 VM; known limitations documented.

---
# 16. Release Engineering and Distribution

## 16.1 Versioning and Changelog

- Semantic Versioning. `CHANGELOG.md` follows Keep a Changelog. The app version, `Cargo.toml`, `package.json`, and `tauri.conf.json` versions are kept in sync by a single release script/check.
- Release tags are signed. Every release has human-written release notes including **known limitations** and any database migration.

## 16.2 Build Profile and Reproducibility

- Release profile: `lto = "thin"` or `"fat"`, `codegen-units = 1`, `strip = true`, `opt-level = 3` (or `"s"` if measured better). Keep `panic = "unwind"` (worker panic containment in §8.3 depends on it).
- Builds use `npm ci` and `cargo build --locked`; lockfiles are committed. Toolchain versions are pinned.
- Every release publishes: installer(s), SHA-256 checksums, and an SBOM (`cargo-cyclonedx` and an npm SBOM).

## 16.3 Installer

- Default installer: **NSIS** (`-setup.exe`). **MSI** (WiX) is optional for enterprise/GPO deployment. Tauri's Windows bundler builds MSI on Windows only.
- WebView2: configure `webviewInstallMode` deliberately (bootstrapper by default; offline installer variant optional) and test on a machine without WebView2.
- Per-user install by default (no elevation). App identifier, publisher name, icons, and version metadata are set; start-menu shortcut created; uninstall entry works.
- Uninstall asks whether to remove user data; it never touches sorted files (§12.3).
- In-place upgrade MUST preserve data and run migrations safely (with the pre-migration backup, §5.1). Test upgrades from every previously released version.

## 16.4 Code Signing

- Sign the installer **and** the application executable, with a trusted timestamp, so Windows SmartScreen and enterprise policies treat the release as coming from a verified publisher. SmartScreen reputation may still take time to build.
- Tauri supports OV/EV certificates (PFX or certificate thumbprint) and cloud-based signing through a custom `signCommand` (for example Azure Key Vault with `relic`, or Azure's managed signing service). Choose the method in Phase 6 (§19); verify the current provider requirements at that time.
- Signing keys and certificates are **never** committed; CI uses protected secrets, and only the release workflow on tagged commits can access them.

## 16.5 Updates

- v1 default: **no automatic updates and no update checks** (SortDeck makes no network calls). Users download releases from GitHub Releases and verify checksums/signatures.
- If an updater is added later, it MUST be opt-in, off by default, use `tauri-plugin-updater` with signature verification, and be documented as the only network access in the app.

## 16.6 Release Verification Checklist

- Clean Windows 11 VM: install, first run, add folder in a test directory, preview, sort, undo, autostart, tray, quit, upgrade, uninstall (keep and delete data).
- Verify signatures (`signtool verify`), checksums, WebView2 behavior, high-DPI, long paths, SmartScreen behavior documented.
- Run the full verification commands (§14) and the manual QA list (§13.6); record results in the release PR.

---

# 17. Documentation and Open-Source Governance

## 17.1 Required Documents

| File | Contents |
|---|---|
| `README.md` | What/why, screenshots (once real), install, safety guarantees in plain language, build from source, limitations. No unmeasured performance claims. |
| `docs/ARCHITECTURE.md` | Layering, module responsibilities, dependency decisions, measured performance numbers. |
| `docs/SAFETY.md` | Safety invariants, threat model, TOCTOU residual risk, filesystem caveats, recovery behavior. |
| `docs/DATABASE.md` | Final schema, state machine, migration policy. |
| `docs/IPC.md` | Commands, events, error codes, skip reasons. |
| `docs/TESTING.md` | How to run each test tier, invariant → test traceability matrix, VHDX setup. |
| `docs/RELEASE.md` | Release procedure, signing, verification checklist. |
| `CONTRIBUTING.md` | Dev setup, coding standards, PR checklist, how to run tests safely. |
| `SECURITY.md` | Private vulnerability reporting process, supported versions. |
| `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `LICENSE`, third-party license notices | Standard. |

Architectural decisions that change this file's contract are recorded as short ADRs under `docs/adr/`.

## 17.2 Documented Limitations (Keep Current)

Maintain an honest list, including at minimum: cross-volume moves apply the destination's inherited ACLs; FAT/exFAT lack file IDs and alternate data streams; network shares have unreliable change notifications; cloud placeholders are skipped; TOCTOU risk on reparse points is mitigated but not eliminated; antivirus/indexers can delay or lock files; paths that are not valid Unicode are skipped; BitLocker-locked or removed volumes are treated as unavailable.

---

# 18. AI Coding Agent Instructions

These instructions are mandatory for every AI coding agent working on SortDeck.

## 18.1 Before Writing Code

1. Read this entire AGENTS.md before modifying the project.
2. Inspect the existing repository (`git status`, existing modules, tests, docs) before creating or replacing files.
3. Never assume a file, dependency, or module already exists.
4. Never overwrite working project configuration without a reason.
5. Implement **only** the requested phase. Never proceed to another phase without explicit instruction.
6. If a requirement is ambiguous, conflicting, or an open decision (§19) blocks you: use the documented default, record the assumption, and ask the maintainer when the choice affects safety, schema, or public IPC.
7. When uncertain about a Windows API's behavior, consult official documentation and write a probe **test inside the sandbox**; never guess. Record findings in `docs/SAFETY.md`.

## 18.2 Implementation Rules

8. Do not skip implementation steps to save tokens.
9. Do not generate placeholder implementations for critical file operations. No `todo!()`, `unimplemented!()`, or stubbed safety checks at phase completion.
10. Do not silently ignore errors. Do not use `unwrap()`/`expect()` on fallible runtime results outside tests.
11. Do not use unsafe filesystem shortcuts. Use only `FsBackend` for file operations; never `std::fs::rename` (§2.6).
12. Never weaken a safety requirement, invariant, lint, or test to make something pass. Never mark a failing test `#[ignore]`, delete it, or update snapshots blindly.
13. Never introduce unrequested cloud functionality, telemetry, or network access.
14. Keep TypeScript strict mode enabled. Keep Rust modular and maintainable; respect the dependency direction (§3.1).
15. Confine `unsafe` and Win32 FFI to `platform/windows/` with `// SAFETY:` comments (§2.5).
16. Write tests alongside critical backend functionality, including failure paths. Verify real behavior, not just compilation.
17. Add dependencies only with a stated purpose, license, and maintenance check; record them in `docs/ARCHITECTURE.md`.
18. Document architectural changes and update docs in the same change.

## 18.3 Forbidden Actions

- Running destructive commands (`rm -rf`, `Remove-Item -Recurse`, `del /s`, formatting, partitioning tools) on anything except a test sandbox or the provided VHDX scripts.
- Running SortDeck or its tests against real user folders (Downloads, Desktop, Documents), real drives, or the real data directory. Development uses `SORTDECK_DATA_DIR` inside a sandbox.
- Editing the registry, installing/uninstalling software, or changing system settings, other than inside the documented installer/autostart code paths (and those are mocked in tests).
- Committing secrets, certificates, signing keys, `.env` files, or real personal paths/file names in fixtures or logs.
- Force-pushing, rewriting shared history, or deleting branches without instruction.
- Disabling or relaxing CI gates, `clippy` lints, `deny.toml`, or capabilities/CSP.

## 18.4 Ask the Maintainer Before

- Changing a safety invariant, the schema after a release, or a public IPC contract.
- Adding a runtime dependency, a Tauri plugin, or a capability/permission.
- Choosing beyond the defaults in §19.
- Anything that would make SortDeck delete, overwrite, or move data it currently would not.

## 18.5 Commit and PR Hygiene

- One phase per PR; small, reviewable commits with clear messages (Conventional Commits recommended).
- Every PR lists which invariants (`INV-xx`) it touches and the tests proving them.

## 18.6 Required Final Report

After completing a phase, report:

- What was implemented (mapped to the phase's bullets).
- Files created or modified.
- Dependencies added or changed (with purpose).
- Tests written (grouped by invariant/scenario).
- Commands executed and **actual results** (pass/fail counts, warnings).
- Safety invariants verified, and which were not.
- Deviations from this specification and why.
- Known limitations.
- Unverified items (anything not run, and why).
- Remaining work and recommended next step.

If something fails, diagnose and fix it before declaring completion. Report incomplete work honestly.

---

# 19. Open Decisions and Defaults

Agents use the **default** until the maintainer decides otherwise, and record the assumption. Items marked ⚑ should be confirmed before the phase noted.

| # | Decision | Default assumed | Confirm by |
|---|---|---|---|
| 1 | ⚑ Software license | `MIT OR Apache-2.0` (dual) | Phase 0 |
| 2 | TS type generation | `ts-rs` or `specta`; pick one, document it | Phase 0 |
| 3 | App identifier / publisher name | Placeholder until maintainer supplies | Phase 0 |
| 4 | Minimum window size | 960×600 | Phase 4 |
| 5 | Same-volume hashing | Off (file-ID identity); optional setting | Phase 3 |
| 6 | Recursive destination layout | Flatten (no structure preservation) | Phase 2 |
| 7 | `bat`/`cmd`/`ps1` category | Installers, as originally specified | Phase 2 |
| 8 | Cloud-managed files, cross-volume | Skipped in v1 | Phase 3 |
| 9 | History retention | 365 days | Phase 3 |
| 10 | Installer format | NSIS default, MSI optional | Phase 6 |
| 11 | ⚑ Code-signing method | OV/EV certificate or managed cloud signing | Phase 6 |
| 12 | Auto-update | None in v1 | Phase 6 |
| 13 | Windows 10 support | Best-effort, not a release gate | Phase 6 |

---

# 20. Final Project Definition

SortDeck is not a file-extension sorting script with a graphical interface.

It is a local-first Windows desktop application with a reliable filesystem transaction engine, configurable automation, persistent operation history, verified recovery mechanisms, and an accessible, polished interface.

The primary engineering objective is to make file organization **predictable, transparent, reversible, and safe**.

**Safety first. Reliability second. Performance third. Visual polish fourth.**

Never sacrifice the first two objectives for the remaining ones.

---

## Appendix A — Document History

| Version | Date | Summary |
|---|---|---|
| 1.0 | — | Initial specification. |
| 2.0 | 2026-11-24 | Hardened for production: Windows no-overwrite primitives and banned APIs, journal recovery matrix, undo/watcher loop prevention, full schema, plan lifecycle, threat model, fault-injection and crash testing, accessibility, release engineering, agent guardrails, phase exit gates, and an open-decisions register. |
