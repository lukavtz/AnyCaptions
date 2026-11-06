# GPUI cutover evidence

## Replacement coverage

- `cargo test --workspace --locked`: **308 passed**, 11 ignored (12 suites).
- `npm.cmd --prefix frontend test -- --run`: **76 passed** across 9 files.
- `npm.cmd --prefix frontend run typecheck`: passed.
- `cargo test --manifest-path backend/Cargo.toml --all-targets`: **153 passed**, 11 ignored (10 suites).
- Real integration tests migrated to `crates/anycaptions-core/tests/`:
  - `ipc`: 2 passed.
  - `real_opus`: 2 passed.
  - `real_riva`: 3 passed.
  - `real_nemotron`: 3 passed.
  - `real_diarization`: 3 passed.
  - Total: **13 passed**, zero failures.
- ASR exercised Vulkan. NMT used its intended CPU fallback because 274,726,912 bytes of Vulkan device memory could not be allocated; GPU-resident NMT is not claimed.

## Production acceptance

- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo audit`: no vulnerabilities; only allowed maintenance/unsound warnings from transitive dependencies.
- `scripts/verify-overlay.ps1`: passed normal-window styles, taskbar/icon, drag, east/west/south resize, lock, click-through, recovery, no-activate and screenshots.
- `scripts/verify-accessibility.ps1`: caption live region, settings invoke, picker expand/collapse, selected option, invalid color and focus behavior passed. Keyboard scenarios report `SKIP` because the production overlay intentionally uses `WS_EX_NOACTIVATE`.
- `scripts/test-packaging.ps1`: missing DLL exit 2, model mismatch exit 3, deterministic flat update zip and manifest passed.
- `scripts/package-release.ps1 -Version 0.2.0 -SkipInstaller -SkipSignature`: real release update zip and `update.json` verified. NSIS and minisign were unavailable locally, so those two fixture-only checks were skipped explicitly.

## Cutover rules

Legacy source and test files are removed only after the replacement checks above. Runtime assets under `backend/models/`, `backend/local-data/`, `backend/benchmarks/local/` and `backend/icons/` remain because packaging and real-model tests consume them. The Tauri backend, React frontend and overlay spike are removed; the `site/` application is independent and unchanged.
