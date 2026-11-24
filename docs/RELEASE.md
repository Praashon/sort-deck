# SortDeck Release Engineering

> **Status:** Baseline configuration in Phase 0; full packaging, signing, and VM QA occur in Phase 6 (`AGENTS.md` §16).

## Build Configuration

- **Version sync:** `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` share version `0.1.0`.
- **Rust release profile:** `lto = "thin"`, `codegen-units = 1`, `strip = true`, `opt-level = 3`, `panic = "unwind"`.
- **Installer target:** NSIS (`currentUser` install mode) with `downloadBootstrapper` WebView2 mode.
- **Updates:** No automatic update checks or outbound network calls in v1.

## Pre-Release Checklist (Phase 6)

- Run all verification gates in `AGENTS.md` §14.
- Sign executable and NSIS installer (method TBD in Phase 6).
- Generate SHA-256 checksums and SBOMs (`cargo-cyclonedx` and npm SBOM).
- Verify install, first run, sort, undo, autostart, tray, upgrade, and uninstall on a clean Windows 11 VM.
