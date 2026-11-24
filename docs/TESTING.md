# SortDeck Testing Guide

## Safety Rules

- Never run tests against real personal folders (`Downloads`, `Desktop`, `Documents`) or real drives.
- All filesystem tests run inside `TestSandbox` (`src-tauri/src/fsops/sandbox.rs`), which creates a temporary directory containing `.sortdeck-test-sandbox`.
- `SandboxFsBackend` rejects relative paths, `..` traversal, canonical escapes, and any path lacking `.sortdeck-test-sandbox` in its ancestor chain.

## Verification Commands

```powershell
# Frontend + Guard Scripts
npm ci
npm run lint
npm run typecheck
npm run format:check
npm test
npm run build
npm run gen:types
npm run check:banned-apis
npm audit --omit=dev

# Rust Backend (from src-tauri/)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

## Invariant Traceability Matrix

| Invariant                                                   | Verification                                                                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `INV-15` (All FS ops via `FsBackend`)                       | `clippy.toml` `disallowed-methods`, `scripts/check-banned-apis.test.mjs`, `src-tauri/tests/sandbox_guard.rs` |
| `INV-05` (Dry-run changes nothing on disk)                  | `src-tauri/tests/scanner_planner.rs` (`inv_05_dry_run_changes_nothing_on_disk`)                              |
| `INV-06` (Protected locations, project roots, reparse pts)  | `src-tauri/tests/path_policy.rs`, `src-tauri/tests/scanner_planner.rs`                                       |
| `INV-07` (Hidden/system/cloud/temp/office/in-use skipped)   | `src-tauri/tests/scanner_planner.rs` (`inv_07_scanner_skips_*`)                                              |
| `INV-11` (Plan revalidation rejects stale/changed items)    | `src-tauri/tests/scanner_planner.rs` (`inv_11_plan_revalidation_*`)                                          |
| `INV-12` (Idempotent sweep planning)                        | `src-tauri/tests/scanner_planner.rs` (`inv_12_plan_idempotency_*`)                                           |
| `INV-01`..`INV-04`, `INV-08`, `INV-10`, `INV-14`            | `src-tauri/tests/safe_mover_and_undo.rs` (fault injection via `FaultyFsBackend`)                             |
| `INV-09` (Crash recovery matrix)                            | `src-tauri/tests/crash_recovery.rs` (real child-process failpoints + direct matrix tests)                    |
| `INV-13` (Undo suppression table)                           | `src-tauri/tests/safe_mover_and_undo.rs` (`restore_suppressions`)                                            |
| `INV-14` (UI status fidelity & accessibility)               | `src/components/common/StatusBadge.test.tsx`, `src/utils/paths.test.ts`, `src/App.test.tsx` (`vitest-axe`)   |
| Real cross-volume (VHDX, `Zone.Identifier` ADS, Unicode)    | `src-tauri/tests/vhdx_cross_volume.rs` (`#[ignore]`, via `scripts/create-test-vhdx.ps1`)                     |
| Database migrations, downgrade refusal, corruption recovery | `src-tauri/tests/database_migrations.rs`                                                                     |

## Real Cross-Volume (VHDX) Tests

Ignored by default because mounting virtual disks requires administrator privileges; use `scripts/create-test-vhdx.ps1` locally to run `src-tauri/tests/vhdx_cross_volume.rs`.
