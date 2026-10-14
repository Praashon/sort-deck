# Contributing to SortDeck

Thank you for your interest in contributing to SortDeck. Please read [AGENTS.md](./AGENTS.md) and [docs/SAFETY.md](./docs/SAFETY.md) carefully before writing code — data safety is SortDeck's highest priority.

## Development Setup

1. Install Node.js `24.20.0` (matching `.nvmrc`) and Rust `1.99.1` (matching `rust-toolchain.toml`).
2. Install Visual Studio 2022 Build Tools (Desktop development with C++) and WebView2 Runtime on Windows 11.
3. Run `npm ci` at the repository root.

## Critical Safety Rules for Contributors

- **Never** test against real user folders (`Downloads`, `Desktop`, `Documents`) or real drives. Use `TestSandbox` and `SORTDECK_DATA_DIR` inside temporary directories guarded by `.sortdeck-test-sandbox`.
- **Never** call `std::fs::rename`, `std::fs::copy`, `std::fs::remove_*`, `std::fs::write`, or `std::fs::canonicalize` outside `src-tauri/src/fsops/` and `src-tauri/src/platform/`.
- **Never** use `unwrap()`, `expect()`, or `panic!()` in non-test Rust code paths.
- **Confine** all `unsafe` code and Win32 FFI to `src-tauri/src/platform/windows/` with `// SAFETY:` comments.
- **Never** add telemetry, analytics, or default network access.

## Pull Request Checklist

- All verification commands in `AGENTS.md` §14 pass locally.
- Relevant `INV-xx` safety invariants are referenced and tested, including failure paths.
- Generated IPC types (`src/types/generated/`) are up to date (`npm run gen:types`).
- Documentation (`docs/`) and `CHANGELOG.md` are updated alongside code changes.
