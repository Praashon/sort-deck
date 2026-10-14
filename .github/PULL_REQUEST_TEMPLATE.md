## Summary

Describe the changes and reference the relevant phase from `AGENTS.md`.

## Safety Invariants (`INV-xx`)

- [ ] List invariants touched and the tests verifying them (or `N/A` for docs-only changes).

## Verification Checklist

- [ ] `npm run lint && npm run typecheck && npm run format:check && npm test && npm run build`
- [ ] `npm run gen:types && git diff --exit-code src/types/generated`
- [ ] `npm run check:banned-apis`
- [ ] `cargo fmt --all -- --check` (from `src-tauri/`)
- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings` (from `src-tauri/`)
- [ ] `cargo test --locked` (from `src-tauri/`)
- [ ] Updated `docs/` and `CHANGELOG.md`
