# SortDeck Release Engineering

## Build Configuration

- **Version sync:** `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` share version `0.1.0`.
- **Rust release profile:** `lto = "thin"`, `codegen-units = 1`, `strip = true`, `opt-level = 3`, `panic = "unwind"`.
- **Installer target:** NSIS (`currentUser` install mode) with `downloadBootstrapper` WebView2 mode.
- **Updates:** No automatic update checks or outbound network calls in v1.

## Pre-Release Checklist

- Run all frontend and Rust verification checks (`npm run lint`, `npm run typecheck`, `npm test`, `npm run check:banned-apis`, `cargo fmt --check`, `cargo clippy`, `cargo test --locked`).
- Generate SHA-256 checksums (`SHA256SUMS.txt`) for `SortDeck_0.1.0_x64-setup.exe` and `sortdeck.exe`.
- Sign executable and NSIS installer (planned before general distribution).
- Verify install, first run, sort, undo, autostart, tray, upgrade, and uninstall on a clean Windows 11 VM.
