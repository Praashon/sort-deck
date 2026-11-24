# SortDeck Safety Model

## Core Safety Invariants

Every invariant below is part of SortDeck's safety contract (`AGENTS.md` §4.9) and will be verified by dedicated named tests as each phase lands:

| ID       | Invariant                                                                                                                         | Target Phase                                                   |
| -------- | --------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `INV-01` | An existing file is never overwritten, including under concurrent creation of the destination name.                               | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-02` | A source file is never deleted unless a verified copy exists at its final destination (cross-volume), or it was moved atomically. | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-03` | A destination file is never deleted by SortDeck (app-owned temp files excepted).                                                  | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-04` | Any failure leaves the source file intact.                                                                                        | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-05` | A dry-run changes nothing on disk.                                                                                                | Phase 2 (verified: `scanner_planner`)                          |
| `INV-06` | No file inside a protected location, project root, `.sortignore` subtree, or reparse-point directory is ever moved.               | Phase 1–2 (verified: `path_policy`, `scanner_planner`)         |
| `INV-07` | Hidden, system, cloud-placeholder, temp-extension, and in-use files are never moved.                                              | Phase 2 (verified: `scanner_planner`)                          |
| `INV-08` | Every completed move has a persisted `move_logs` row sufficient for undo.                                                         | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-09` | After a crash at any injected point, recovery reaches a consistent state with no data loss.                                       | Phase 3 (verified: `crash_recovery`)                           |
| `INV-10` | Undo never overwrites an existing file and never deletes anything except empty directories SortDeck created.                      | Phase 3 (verified: `safe_mover_and_undo`)                      |
| `INV-11` | A backend plan revalidation rejects any operation whose source, destination, rule, or fingerprint changed.                        | Phase 2–3 (verified: `scanner_planner`, `safe_mover_and_undo`) |
| `INV-12` | Executing a sweep twice is idempotent.                                                                                            | Phase 2–3 (verified: `scanner_planner`, `safe_mover_and_undo`) |
| `INV-13` | Undo/restore of a file does not cause the watcher to re-sort it.                                                                  | Phase 3 (`restore_suppressions`); Phase 5 (watcher)            |
| `INV-14` | The UI never reports full success for a partial or failed batch.                                                                  | Phase 3 (`BatchStatus`); Phase 4 (UI)                          |
| `INV-15` | No code path moves, copies, or deletes files except through `FsBackend`.                                                          | Phase 0+ (verified: `clippy.toml`, `sandbox_guard`)            |

## Phase 0 Enforcement Mechanisms

- **Banned APIs (`clippy.toml` + `scripts/check-banned-apis.mjs`):** `std::fs::rename`, `std::fs::copy`, `std::fs::remove_file`, `std::fs::remove_dir`, `std::fs::remove_dir_all`, `std::fs::write`, and `std::fs::canonicalize` are denied outside `src-tauri/src/fsops/` and `src-tauri/src/platform/`.
- **Test Sandbox Sentinel (`fsops::sandbox::SandboxGuard`):** Any test `FsBackend` operation requires an absolute path without `..` traversal whose ancestor directory contains `.sortdeck-test-sandbox` and whose canonical ancestor does not escape the sandbox root.
- **No `unwrap()`/`expect()`/`panic!()`:** Denied by Clippy in all non-test Rust code.

## Threat Model Summary (`AGENTS.md` §9.2)

- **Untrusted frontend:** Minimal Tauri capabilities (`core:default`, `dialog:allow-open` only; no `fs` or `shell` plugins), strict CSP (`default-src 'self'`), `withGlobalTauri: false`, and backend-validated IPC.
- **Standard-user execution:** Windows manifest enforces `asInvoker` (no elevation), `longPathAware`, and `PerMonitorV2` DPI awareness.
- **Residual risks / known limitations:** TOCTOU reparse-point swaps between validation and OS call are mitigated via immediate pre-operation revalidation (Phase 2/3) but cannot be completely eliminated on standard Win32 paths without kernel-level transactions; cross-volume moves inherit destination ACLs; FAT32/exFAT lack NTFS file IDs and ADS (`Zone.Identifier`); non-Unicode paths are skipped.
