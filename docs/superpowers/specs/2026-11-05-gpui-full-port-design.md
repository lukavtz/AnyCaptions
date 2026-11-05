# AnyCaptions Full GPUI Port Design

**Date:** 2026-11-05
**Status:** Approved architecture and implementation plan
**Baseline:** P0 spike `f248a85`; P1 core extraction `20fd0b0`; frontend 0.2.0

## 1. Goal and non-goals

Replace the AnyCaptions Windows desktop UI and host from Tauri 2 + React/TypeScript to GPUI 0.2.2 + Rust while preserving observable behavior, localized content, accessibility, platform integration, and update compatibility. Remove Tauri, React, WebView2, and the overlay spike after the GPUI cutover.

In scope:

- Caption overlay, streaming presenter, text measurement, speaker turns, fades, and appearance.
- Settings, shared widgets, six UI locales, language metadata, colored flags, and bundled fonts.
- Tray, global shortcuts, close-to-tray, recovery, dictation indicator, and requested single-instance behavior.
- Windows UI Automation accessibility, keyboard navigation, IME, clipboard, tooltips, and reduced motion.
- Windows installer, resource embedding, updater compatibility, CI, documentation, and migration verification.

Out of scope:

- Site frontend/backend changes except release compatibility.
- Authenticode signing, MSIX/Store packaging, auto-update without a user action, and non-Windows targets.
- Feature redesign or visual redesign. Existing behavior and visual language are the reference.

## 2. Fixed decisions

1. Use `gpui = "=0.2.2"` from crates.io and `raw-window-handle = "0.6"`; never copy APIs from Zed main.
2. Use the existing `windows = "0.62.2"` crate for native functionality. Do not add tray, hotkey, widget, accessibility, or bundler frameworks.
3. Build the overlay as `WindowKind::Normal`, frameless and transparent, to preserve taskbar presence. Use `WindowKind::PopUp` only for the dictation indicator.
4. Obtain `HWND` through `HasWindowHandle`; do not find production windows by title.
5. Implement drag through `WindowControlArea::Drag`. GPUI 0.2.2 `start_window_move` and `start_window_resize` are no-ops on Windows; implement resize and runtime positioning through Win32.
6. Implement Windows UI Automation instead of dropping the existing accessibility contract.
7. Embed colored PNG flags and TTF/OTF fonts. GPUI SVG rendering is a tinted alpha mask and DirectWrite does not accept the installed WOFF2 assets.
8. Keep `%APPDATA%\com.anycaptions.app`, executable name `anycaptions.exe`, model layout, release schema, minisign key, and flat update archive contract unchanged.
9. Use the existing upstream v0.2.0 release as the pinned source for locally absent DLL/model verification inputs. Keep large inputs gitignored and hash-locked.
10. Deliver focused, reviewable commits and PRs to `lukavtz/AnyCaptions`; never target upstream with a PR.

## 3. Architecture

```text
crates/
├── anycaptions-core/             # GUI-independent domain and presenters
│   ├── caption/                  # admission, pagination, speaker turns, frames
│   ├── presentation/             # app/settings/download/update/indicator models
│   ├── window_geometry.rs        # monitor/work-area and restore calculations
│   └── existing P1 modules
├── anycaptions-ui/               # GPUI 0.2.2 only
│   ├── overlay/                  # toolbar, captions, lock, resize grips, notices
│   ├── settings/                 # panel sections and draft/save lifecycle
│   ├── widgets/                  # button, picker, input, slider, toggle, tooltip
│   ├── dictation/                # indicator window and native sound hook
│   └── assets.rs                 # embedded fonts/flags/icons
├── anycaptions-platform-win32/   # Windows-only native adapters
│   ├── window.rs                 # topmost, styles, work area, move/resize, hide/show
│   ├── tray.rs                   # notification icon and menu
│   ├── hotkeys.rs                # message window and RegisterHotKey
│   ├── single_instance.rs        # named mutex and activation forwarding
│   ├── accessibility.rs          # Windows UI Automation providers
│   └── system.rs                 # color dialog, sound, browser, animation setting
└── anycaptions/                  # final anycaptions.exe host and startup wiring
```

`anycaptions-core` has no GPUI or Windows UI dependency. `anycaptions-ui` depends on core and GPUI, not on native global services. `anycaptions-platform-win32` owns OS side effects behind small adapter traits. The binary composes all modules on the GPUI thread and moves core callbacks to that thread through GPUI async channels.

The existing `backend/` remains buildable until the final cutover commit. That commit removes the Tauri backend, frontend, spike, generated ACL/CSP configuration, duplicate lockfile, and obsolete scripts. The removed webview IPC/CSP attack surface is documented as removed, not weakened.

## 4. Presentation behavior

### 4.1 Captions

Port `frontend/src/captions.ts` into a pure Rust `CaptionPresenter` with an injected text-fit interface. Preserve:

- Session/mode/segment/revision admission and stale-event rejection.
- The translated-final exception that replaces a provisional update even with a lower revision.
- Regressive-partial rejection, segment rollover, consumed-word bookkeeping, and bounded history.
- Single-line auto captions, three-line translation/speaker layout, and tail pagination.
- Exact speaker-span trust rule, unknown speaker 0 behavior, eight speaker colors, and label width in measurement.
- Two trailing partial words, committed/final/provisional styling, 120 ms fade, and cancellation by new input.
- Literal text handling, reduced-motion behavior, resize reflow, and no self-clearing while a session remains active.

Replace only DOM measurement. Use GPUI `LineWrapper`/`shape_text` with the actual font runs and available width. Keep the tested binary-search pagination initially; a one-pass wrapper optimization is allowed only after all presenter tests pass unchanged.

### 4.2 Application and settings

Port the `useApplication` contract: monotonic snapshot revisions, authoritative core snapshots, single-flight snapshot-producing actions, error recovery, and draft preservation after rejected saves.

Settings retain two mutation styles:

- Draft + Save: appearance, overlay shortcut, dictation settings.
- Immediate: UI language, source language, translation engine, speaker labels, startup update check, audio output, downloads, and update actions.

Save order is appearance → overlay shortcut → dictation. Close only after all succeed. Preserve field enablement, model visibility, download cancellation/retry, update progress, one-shot update notice, audio stale-ID handling, and localized sanitized errors.

### 4.3 Localization and assets

Port every referenced key from the 124-key catalog across `pt-br`, `en`, `es`, `fr`, `ru`, and `zh-cn`, with placeholder parity for `{size}`, `{percent}`, `{version}`, and `{n}`. Remove the six verified dead keys only after every remaining caller is mapped. Add compile-time or generated key/placeholder parity checks.

Port 37 translation targets, 83 ASR languages, six UI languages, engine filtering, nine caption font IDs, and flag fallback behavior. Embed PNG flag assets with the flag-icons MIT notice. Vendor Inter and Fira Code TTF/OTF files with OFL notices; use Windows families for the remaining catalog entries.

### 4.4 Shared widgets and accessibility

Build only the required GPUI widgets:

- Button/icon button, toggle, slider, number input, text input, picker/listbox, progress, tooltip, scroll container, and color field/dialog.
- Text input supports selection, clipboard, IME composition, max length, and invalid state.
- Picker supports ArrowUp/ArrowDown/Home/End, Enter/Space, Escape, outside-close, disabled/hidden options, and footer actions.
- Tab/Shift+Tab order, focus-visible styling, settings Escape/focus return, and picker Escape precedence match the React implementation.

Implement Windows UIA roles, names, values, descriptions, selection, expansion, invalid state, progress, alert/status notification, and polite caption live-region announcements. Add automated UIA queries for the overlay and settings plus keyboard-only acceptance scenarios.

## 5. Native Windows behavior

Move pure geometry and indicator placement calculations into core and port their upstream tests. Preserve logical 800×160 initial size, 320×160 minimum, 32 px bottom margin, multi-monitor negative coordinates, high DPI, work-area clamping, relative geometry restore, and 32×32 visibility overlap.

The Win32 adapter implements:

- `WS_EX_TOPMOST`, `WS_EX_NOACTIVATE`, `WS_EX_LAYERED`, and locked-mode `WS_EX_TRANSPARENT`.
- Overlay `WS_EX_APPWINDOW` without `WS_EX_TOOLWINDOW`; dictation indicator `WS_EX_TOOLWINDOW` and permanent click-through.
- Work-area enumeration, left/right/bottom resize, runtime move, hide, show-without-activation, and close-to-tray.
- Message-only hotkey window for overlay and dictation chords, including recording-only Escape and registration rollback.
- Tray menu with the existing Portuguese labels, capture enablement from resources, lock/recover, and ordered quit.
- Named single-instance mutex and activation forwarding to the existing window. This is the requested addition beyond upstream behavior.
- `ChooseColorW`, generated dictation cues, constant-only browser opening, and Windows reduced-motion queries.

Startup retains the upstream order: update leftovers → preferences/engine fallback → services → resources → tray/shortcut recovery → dictation → windows. Shutdown is dictation join → session join → geometry save → process quit.

## 6. Parallel workstreams and integration

The coordinator owns manifests, crate roots, shared traits, application composition, integration, and final verification. Independent agents own separate files and tests:

1. Caption presenter and measurement.
2. Localization, appearance, language metadata, and error mapping.
3. Download/update/dictation presenters and geometry.
4. GPUI shared widgets and focus model.
5. Win32 window primitives, tray, hotkeys, and single-instance.
6. Assets, theme constants, native color/sound/reduced-motion, and UIA.
7. Packaging, installer, release verification, and CI scaffolding.

After interface freeze, overlay, settings sections, dictation window, and host adapters run as separate integration workstreams. Agents never edit another owner's files. Every slice receives an independent review; the coordinator integrates and runs the complete checks.

## 7. Packaging and CI

Use `cargo build --release --locked` and a std-only `build.rs` invoking the pinned Windows SDK `rc.exe`; embed a PerMonitorV2 manifest, common-controls v6, version information, and icon resource ID 1. GPUI release builds use the pinned `fxc.exe` through `GPUI_FXC_PATH`.

Add `packaging/installer.nsi` with the current per-user NSIS registry/install/uninstall contract. Package:

- `anycaptions.exe` and the 17 pinned inference DLLs flat at install root.
- ASR and both OPUS model directories with every companion file.
- No Riva or diarization models; they remain in-app downloads.
- No WebView2 bootstrapper and no runtime UI asset files.

The update zip contains only `anycaptions.exe` and the locked DLL set, flat, without models. Preserve `AnyCaptions_<version>_x64-setup.exe`, `anycaptions-update-<version>.zip`, `update.json`, `update.json.minisig`, the public minisign key, and `%APPDATA%` settings. Hash-lock every external DLL/model input.

CI uses a pinned Windows runner/toolchain and runs formatting, Clippy, all tests, debug build, dependency checks, and artifact-lock validation. Release CI additionally builds and verifies NSIS/update assets only when the pinned large inputs are present; missing inputs fail loudly.

## 8. Verification and acceptance

### Automated

- Preserve all 141 core tests and port the behavioral substance of all 76 frontend tests to Rust/GPUI owners.
- Preserve the 153-test legacy backend suite and classify all 11 real-model tests; execute every test whose pinned model/audio inputs are available.
- Add GPUI tests for widgets, focus, picker keyboard behavior, captions, settings, and window events.
- Add pure tests for geometry, indicator placement, single-instance decisions, hotkey rollback, and update gates.
- Add UIA and native integration scripts for taskbar/style/accessibility contracts.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`.
- Run the scoped Assay gate for each changed slice; inherited full-crate findings are not silently used to hide new findings.
- Run `cargo audit`; assess reachability rather than suppressing advisories.
- Assert no `tauri`, `wry`, or WebView2 dependency remains after cutover.

### Interactive

- Capture overlay/settings/dictation screenshots and compare layout, fonts, flags, colors, and states.
- Verify actual click-through into another application, interactive drag, three resize grips, lock recovery, and close-to-tray.
- Verify six locales, long translated text, IME, keyboard-only operation, UIA narration contracts, and reduced motion.
- Verify model download/cancel/retry, dictation start/stop/Escape, audio selection, and update failure handling.
- Verify silent install/uninstall, taskbar/icon/work area, settings preservation, and Tauri 0.2.0 → GPUI update migration on a Windows VM.

No phase is complete by compilation alone. Final completion requires the full acceptance matrix or an explicitly named external prerequisite with all reachable work finished.

## 9. Delivery

1. P2a: pure presenters, geometry, widgets, assets, native primitives, and tests.
2. P2b: overlay/settings/dictation GPUI integration and interactive verification.
3. P3: tray/hotkeys/single-instance/recovery, UIA completion, and platform tests.
4. P4: packaging, CI, update migration, final cutover, documentation, and final review.

Each stage is committed and pushed on a focused branch. PRs include exact before/after evidence, residual limitations, and verification commands. No merge, release, or force-push occurs without explicit user approval.
