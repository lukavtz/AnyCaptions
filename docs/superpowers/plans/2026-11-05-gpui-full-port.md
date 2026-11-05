# AnyCaptions Full GPUI Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the complete Tauri/React Windows desktop application with a behavior-compatible GPUI 0.2.2 + Rust application, including native integration, accessibility, packaging, and updater migration.

**Architecture:** Pure Rust presenters live in `anycaptions-core`; GPUI rendering and widgets live in `anycaptions-ui`; native side effects live in `anycaptions-platform-win32`; `anycaptions` composes them. Existing Tauri/React code remains until the final cutover.

**Tech Stack:** Rust 1.90+, GPUI 0.2.2, raw-window-handle 0.6, windows 0.62.2, Win32/UIA, NSIS, PowerShell, GitHub Actions.

## Global Constraints

- Pin `gpui = "=0.2.2"` and `raw-window-handle = "0.6"`; use `windows = "0.62.2"` for native APIs.
- Do not add runtime frameworks for widgets, tray, hotkeys, accessibility, or packaging.
- Preserve 141 core tests and the behavioral substance of all 76 frontend tests.
- Preserve six locales, 37 translation targets, 83 ASR languages, nine font IDs, colored flags, UIA, and keyboard/IME behavior.
- Overlay uses `WindowKind::Normal` and stays in the taskbar; dictation uses `WindowKind::PopUp` and stays click-through.
- Keep `%APPDATA%\com.anycaptions.app`, `anycaptions.exe`, model layout, flat update zip, release filenames, and the existing minisign public key.
- Agents edit only their assigned files. The coordinator owns manifests, crate roots, integration, and final checks.
- Every behavior task starts with a failing test and ends with targeted tests plus an independent review.
- PRs target `lukavtz/AnyCaptions` only. No merge, release, or force-push without explicit approval.

---

### Task 1: Workspace contracts and native geometry

**Files:**
- Create: `crates/anycaptions-ui/Cargo.toml`, `crates/anycaptions-ui/src/lib.rs`
- Create: `crates/anycaptions-platform-win32/Cargo.toml`, `crates/anycaptions-platform-win32/src/lib.rs`
- Create: `crates/anycaptions-core/src/window_geometry.rs`
- Modify: `crates/anycaptions-core/src/lib.rs`

**Interfaces:**
- Produces: `Rect { x, y, width, height }`, `WorkArea { name, area, scale }`, `initial_geometry`, `restored_geometry`, `capture_geometry`, `ensure_visible`.
- Produces: crate dependency direction `anycaptions-ui -> anycaptions-core` and `anycaptions-platform-win32 -> anycaptions-core`; Task 11 adds the binary host.

- [ ] **Step 1: Write failing geometry tests**

Port the six `backend/src/window.rs` geometry tests and add an off-screen restore case:

```rust
let restored = ensure_visible(
    Rect::new(-1900, -1300, 800, 160),
    &[WorkArea::new("left", Rect::new(-1920, -1080, 1920, 1080), 1.0)],
);
assert_eq!(restored, initial_geometry(areas[0].area));
assert!(restored.overlaps_at_least(32, 32, Rect::new(-1920, -1080, 1920, 1080)));
```

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-core window_geometry`
Expected: FAIL because `window_geometry` does not exist.

- [ ] **Step 3: Implement geometry and crate contracts**

Port the calculations from `backend/src/window.rs:27-65,164-221` without Tauri types. Register the UI and platform crates with exact GPUI/raw-handle versions. The binary crate is introduced with real host behavior in Task 11.

- [ ] **Step 4: Run the green check**

Run: `cargo test -p anycaptions-core window_geometry && cargo check -p anycaptions-ui -p anycaptions-platform-win32`
Expected: all geometry tests and both crate checks pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/anycaptions-core/src/lib.rs crates/anycaptions-core/src/window_geometry.rs crates/anycaptions-ui crates/anycaptions-platform-win32
git commit -m "feat: establish GPUI workspace and window geometry"
```

---

### Task 2: Caption presenter and GPUI text measurement

**Files:**
- Create: `crates/anycaptions-core/src/caption/mod.rs`
- Create: `crates/anycaptions-core/src/caption/presenter.rs`
- Create: `crates/anycaptions-core/src/caption/tests.rs`
- Modify: `crates/anycaptions-core/src/lib.rs`
- Create: `crates/anycaptions-ui/src/caption.rs`
- Create: `crates/anycaptions-ui/src/text_measurement.rs`

**Interfaces:**
- Consumes: core `CaptionUpdate`, `CaptionMode`, `Appearance`, and GPUI `LineWrapper`/`TextRun`.
- Produces: `CaptionFits`, `CaptionFrame`, `CaptionPart`, `CaptionPresenter::{accept,set_context,set_speaker_labels,refresh_layout,clear}`, and `GpuiCaptionFits`.

- [ ] **Step 1: Port the 18 presenter tests into Rust**

Include the exact acceptance cases from `captions.test.ts` and `captions-speakers.test.ts`. Example:

```rust
presenter.accept(update(1, 1, 2, "hello world", false));
let words = presenter.frame().unwrap().committed_text.clone();
presenter.accept(update(1, 1, 1, "hello world again", false));
assert_eq!(presenter.frame().unwrap().committed_text, words);
```

Use a `WordCountFits` test adapter with a configurable words-per-line limit; do not use GPUI in pure presenter tests.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-core caption::tests`
Expected: FAIL because `CaptionPresenter` and `CaptionFits` do not exist.

- [ ] **Step 3: Implement the presenter and measurement adapter**

Port `frontend/src/captions.ts` helpers and state transitions. Define:

```rust
pub trait CaptionFits {
    fn fits(&mut self, text: &str, max_lines: usize) -> bool;
}
```

Implement `GpuiCaptionFits` with GPUI text runs and `wrap_line(...).count() + 1 <= max_lines`. Keep binary-search pagination in this task. Use `TextRun` for speaker labels so their width is included.

- [ ] **Step 4: Run presenter and GPUI measurement tests**

Run: `cargo test -p anycaptions-core caption::tests && cargo test -p anycaptions-ui text_measurement`
Expected: 18 presenter tests and real GPUI wrapping tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-core/src/caption crates/anycaptions-core/src/lib.rs crates/anycaptions-ui/src/caption.rs crates/anycaptions-ui/src/text_measurement.rs
git commit -m "feat: port caption presenter to Rust and GPUI"
```

---

### Task 3: Localization, appearance, languages, and errors

**Files:**
- Create: `crates/anycaptions-core/src/presentation/i18n.rs`
- Create: `crates/anycaptions-core/src/presentation/appearance.rs`
- Create: `crates/anycaptions-core/src/presentation/languages.rs`
- Create: `crates/anycaptions-core/src/presentation/errors.rs`
- Create: `crates/anycaptions-core/src/presentation/tests.rs`
- Modify: `crates/anycaptions-core/src/presentation/mod.rs`, `crates/anycaptions-core/src/lib.rs`

**Interfaces:**
- Produces: `Locale`, `MessageKey`, `translate`, `normalize_caption_color`, `same_appearance`, `CaptionFontCatalog`, `LanguageCatalog`, `error_message_key`.

- [ ] **Step 1: Write failing parity tests**

```rust
for locale in Locale::ALL {
    for key in MessageKey::ALL {
        assert!(translate(locale, key).is_some(), "missing {locale:?}/{key:?}");
    }
}
assert_eq!(normalize_caption_color(" #a1b2c3 ").unwrap(), "#A1B2C3");
assert!(normalize_caption_color("red").is_err());
assert_eq!(error_message_key(&CommandError::EventUnavailable), MessageKey::ErrGeneric);
```

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-core presentation::tests`
Expected: FAIL because the presentation catalog does not exist.

- [ ] **Step 3: Port every referenced catalog entry**

Port every referenced i18n key and all six locale tables from `frontend/src/i18n`. Remove only `translationMissing`, `errResize`, `errSettingsFit`, `errOutputsList`, `errTrayClose`, and `errCaptionFit` after confirming no Rust caller. Port appearance, fonts, 37 targets, 83 ASR languages, engine filtering, and complete error precedence.

- [ ] **Step 4: Run parity and behavior tests**

Run: `cargo test -p anycaptions-core presentation::tests`
Expected: locale key/placeholder, appearance, language, font, and error tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-core/src/presentation crates/anycaptions-core/src/lib.rs
git commit -m "feat: port localization appearance and language catalogs"
```

---

### Task 4: Application, download, update, dictation, and indicator models

**Files:**
- Create: `crates/anycaptions-core/src/presentation/application.rs`
- Create: `crates/anycaptions-core/src/presentation/downloads.rs`
- Create: `crates/anycaptions-core/src/presentation/updates.rs`
- Create: `crates/anycaptions-core/src/presentation/dictation.rs`
- Create: `crates/anycaptions-core/src/dictation/indicator.rs`
- Create: `crates/anycaptions-core/src/presentation/model_tests.rs`

**Interfaces:**
- Consumes: core snapshots, download/update events, `DictationEventPayload`, `Controller`, and `WindowGeometry`.
- Produces: `ApplicationModel::{accept,try_begin,complete}`, `DownloadsModel`, `UpdatesModel`, `DictationModel`, `IndicatorSnapshot`, `place_indicator`.

- [ ] **Step 1: Write failing state-model tests**

Port `useApplication`, download, update-action, dictation, and indicator placement cases. Include:

```rust
let mut app = ApplicationModel::new(snapshot(7));
app.accept(snapshot(6));
assert_eq!(app.snapshot().revision, 7);
assert!(app.try_begin(Action::StartCapture));
assert!(!app.try_begin(Action::StopCapture));
```

Also test save order `Appearance → Shortcut → Dictation`, rejected-save draft retention, percent clamping, one-shot update notice, elapsed `m:ss`, and above/below indicator placement.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-core presentation::model_tests`
Expected: FAIL because the models do not exist.

- [ ] **Step 3: Implement deterministic models**

Keep I/O behind traits returning explicit results. No model reads clocks directly; elapsed time is an input. No model spawns threads. Implement the exact React state transitions and three `dictation_window.rs` placement cases.

- [ ] **Step 4: Run model tests**

Run: `cargo test -p anycaptions-core presentation::model_tests`
Expected: all application, download, update, dictation, and placement tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-core/src/presentation crates/anycaptions-core/src/dictation/indicator.rs
git commit -m "feat: port application and indicator state models"
```

---

### Task 5: GPUI widgets, focus, keyboard, and IME

**Files:**
- Create: `crates/anycaptions-ui/src/widgets/mod.rs`
- Create: `crates/anycaptions-ui/src/widgets/button.rs`
- Create: `crates/anycaptions-ui/src/widgets/picker.rs`
- Create: `crates/anycaptions-ui/src/widgets/text_input.rs`
- Create: `crates/anycaptions-ui/src/widgets/value_controls.rs`
- Create: `crates/anycaptions-ui/src/widgets/tooltip.rs`
- Create: `crates/anycaptions-ui/src/widgets/tests.rs`

**Interfaces:**
- Consumes: core i18n/language catalogs and GPUI `EntityInputHandler`, `FocusHandle`, `ScrollHandle`.
- Produces: `Button`, `Toggle`, `Slider`, `NumberInput`, `TextInput`, `Picker`, `Progress`, `Tooltip`, `FocusScope`.

- [ ] **Step 1: Write failing GPUI interaction tests**

```rust
#[gpui::test]
async fn picker_escape_closes_without_closing_settings(cx: &mut TestAppContext) {
    let (picker, cx) = cx.add_window_view(|window, cx| Picker::new(test_options(), window, cx));
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("escape");
    assert!(!picker.read(cx).open);
    assert!(picker.read(cx).trigger_focus.is_focused(cx));
}
```

Add IME replacement, selection, clipboard, max length, Home/End, disabled option, slider bounds, and Tab/Shift+Tab cases.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-ui widgets`
Expected: FAIL because widget entities do not exist.

- [ ] **Step 3: Implement widgets using GPUI 0.2.2 examples**

Use `examples/input.rs` for `EntityInputHandler`; stateful `div` for buttons/toggles; `anchored`/`deferred` for pickers; drag events for sliders; `FocusHandle` and explicit Tab bindings for traversal. Preserve disabled behavior and literal text. No alternate widget dependency.

- [ ] **Step 4: Run widget tests**

Run: `cargo test -p anycaptions-ui widgets`
Expected: all widget interaction tests pass with `test-support`.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-ui/src/widgets
git commit -m "feat: add accessible GPUI settings widgets"
```

---

### Task 6: Win32 window, tray, hotkey, and single-instance adapters

**Files:**
- Create: `crates/anycaptions-platform-win32/src/window.rs`
- Create: `crates/anycaptions-platform-win32/src/tray.rs`
- Create: `crates/anycaptions-platform-win32/src/hotkeys.rs`
- Create: `crates/anycaptions-platform-win32/src/single_instance.rs`
- Create: `crates/anycaptions-platform-win32/src/tests.rs`
- Modify: `crates/anycaptions-platform-win32/src/lib.rs`

**Interfaces:**
- Produces: `WindowEffects`, `TrayService`, `HotkeyService`, `SingleInstance`, and raw-HWND adapters used by the host.
- Consumes: core `ShortcutRegistry`, dictation `HotkeyRegistry`, and geometry types.

- [ ] **Step 1: Write failing pure and native tests**

```rust
let interactive = base_ex_style();
let locked = set_click_through(interactive, true);
assert!(is_click_through(locked));
assert_eq!(set_click_through(locked, false), interactive);
assert_eq!(resize_rect(original, ResizeEdge::West, -40, 0).width, original.width + 40);
```

Add unique-name mutex acquisition/conflict and hotkey registration rollback tests without colliding with real application identifiers.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-platform-win32`
Expected: FAIL because adapters do not exist.

- [ ] **Step 3: Implement Win32 services**

Use raw `HWND` and `windows 0.62.2`. Implement Normal overlay taskbar style, topmost/no-activate/layered/click-through, work-area geometry, Win32 move/resize/hide/show, message-only hotkey window, tray commands, named mutex, and activation forwarding. Keep callback invocation outside registry locks.

- [ ] **Step 4: Run native tests and P0 regression checks**

Run: `cargo test -p anycaptions-platform-win32 && powershell -NoProfile -File scripts/verify-overlay.ps1`
Expected: native tests and existing P0 style probe pass; any probe limitation is recorded for Task 9.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-platform-win32
git commit -m "feat: add Win32 overlay and recovery services"
```

---

### Task 7: Embedded assets, native system actions, and UIA

**Files:**
- Create: `crates/anycaptions-ui/assets/fonts/` and OFL notices
- Create: `crates/anycaptions-ui/assets/flags/` and flag-icons MIT notice
- Create: `crates/anycaptions-ui/src/assets.rs`
- Create: `crates/anycaptions-ui/src/theme.rs`
- Create: `crates/anycaptions-platform-win32/src/system.rs`
- Create: `crates/anycaptions-platform-win32/src/accessibility.rs`
- Create: `crates/anycaptions-platform-win32/src/accessibility_tests.rs`
- Create: `scripts/verify-accessibility.ps1`

**Interfaces:**
- Produces: embedded `AssetSource`, font registration, theme constants, `SystemActions`, `ReducedMotion`, and UIA provider metadata/factories.

- [ ] **Step 1: Write failing asset and accessibility tests**

```rust
for locale in Locale::ALL { assert!(assets::ui_language_name(locale).is_some()); }
for flag in LanguageCatalog::required_flags() { assert!(assets::has_flag(flag)); }
assert_eq!(uia_role(WidgetKind::Caption), UiaRole::Text);
assert_eq!(uia_live_setting(WidgetKind::Caption), UiaLiveSetting::Polite);
```

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-ui assets && cargo test -p anycaptions-platform-win32 accessibility`
Expected: FAIL because embedded assets/providers do not exist.

- [ ] **Step 3: Vendor assets and implement native providers**

Embed PNG flags with MIT notice and TTF/OTF Inter/Fira Code with OFL notices. Register fonts before opening windows. Implement `ChooseColorW`, constant-only browser opening, dictation cues, reduced-motion query, and UIA role/name/value/selection/live-region mapping. No webview or remote UI asset is introduced.

- [ ] **Step 4: Run asset and UIA tests**

Run: `cargo test -p anycaptions-ui assets && cargo test -p anycaptions-platform-win32 accessibility`
Expected: all assets resolve and native UIA mappings pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-ui/assets crates/anycaptions-ui/src/assets.rs crates/anycaptions-ui/src/theme.rs crates/anycaptions-platform-win32/src/system.rs crates/anycaptions-platform-win32/src/accessibility.rs crates/anycaptions-platform-win32/src/accessibility_tests.rs scripts/verify-accessibility.ps1
git commit -m "feat: embed UI assets and Windows accessibility"
```

---

### Task 8: Packaging and CI scaffolding

**Files:**
- Create: `packaging/installer.nsi`, `packaging/artifacts.lock.json`
- Create: `scripts/package-release.ps1`, `scripts/verify-release.ps1`, `scripts/test-packaging.ps1`
- Create: `crates/anycaptions/build.rs`, `crates/anycaptions/app.manifest`
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Create: `rust-toolchain.toml`

**Interfaces:**
- Consumes: `anycaptions.exe`, 17 pinned DLLs, ASR/OPUS model directories, `fxc.exe`, `rc.exe`, NSIS, and minisign.
- Produces: installer/update assets with the existing filenames and a deterministic CI contract.

- [ ] **Step 1: Write failing packaging validation tests**

Use a temporary fixture root with one missing DLL and one hash mismatch:

```powershell
& scripts/test-packaging.ps1
# Assert missing DLL => exit 2; mismatched model companion => exit 3;
# complete fixture => manifest size/hash and flat zip entries validate.
```

- [ ] **Step 2: Run the red check**

Run: `powershell -NoProfile -File scripts/test-packaging.ps1`
Expected: FAIL because package/verify scripts do not exist.

- [ ] **Step 3: Implement resource and release tooling**

Use std-only `build.rs` + `rc.exe`; embed icon ID 1 and PerMonitorV2/common-controls manifest. Implement pinned input verification, NSIS installer, flat update zip, manifest size/SHA-256, minisign verification, and pinned Windows CI. No Authenticode claim and no model files in the update zip.

- [ ] **Step 4: Run packaging tests**

Run: `powershell -NoProfile -File scripts/test-packaging.ps1`
Expected: missing/mismatched inputs fail with the specified codes and complete fixtures validate.

- [ ] **Step 5: Commit**

```bash
git add packaging scripts/package-release.ps1 scripts/verify-release.ps1 scripts/test-packaging.ps1 crates/anycaptions/build.rs crates/anycaptions/app.manifest .github rust-toolchain.toml
git commit -m "build: add deterministic GPUI packaging and CI"
```

---

### Task 9: Overlay, captions, and dictation window integration

**Files:**
- Create: `crates/anycaptions-ui/src/overlay/mod.rs`
- Create: `crates/anycaptions-ui/src/overlay/toolbar.rs`
- Create: `crates/anycaptions-ui/src/overlay/caption_view.rs`
- Create: `crates/anycaptions-ui/src/dictation/mod.rs`
- Create: `crates/anycaptions-ui/src/dictation/window.rs`
- Create: `crates/anycaptions-ui/src/overlay/tests.rs`
- Modify: `scripts/verify-overlay.ps1`

**Interfaces:**
- Consumes: Tasks 1–7 public models/widgets/adapters and core `SessionSink`/`DictationEventSink`.
- Produces: live GPUI overlay and dictation windows with native event wiring.

- [ ] **Step 1: Write failing overlay integration tests**

Port caption visibility, lock, drag-region exclusion, primary-button resize, indicator revision, idle hide, and literal-text cases. Explicitly verify a 60-second session does not self-clear, only the newest translated sentence animates, disposal cancels the fade timer/listeners, and reduced motion selects immediate clearing. Add a native test plan for actual click-through and DPI geometry.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-ui overlay`
Expected: FAIL because the windows are not composed.

- [ ] **Step 3: Implement overlay and dictation composition**

Render caption runs and fades, toolbar, picker, notices, grips, and indicator. Use `WindowControlArea::Drag`, Win32 West/East/South resizing, native lock effects, Normal overlay taskbar style, and indicator placement/follow logic. Core callbacks reach GPUI only through async UI-channel delivery.

- [ ] **Step 4: Run GPUI and interactive verification**

Run: `cargo test -p anycaptions-ui overlay` and the rewritten `scripts/verify-overlay.ps1`.
Expected: GPUI tests pass; actual screenshot, click-through, interactive drag/resize, taskbar, icon, and work-area checks pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-ui/src/overlay crates/anycaptions-ui/src/dictation scripts/verify-overlay.ps1
git commit -m "feat: integrate GPUI captions and dictation windows"
```

---

### Task 10: Complete settings integration

**Files:**
- Create: `crates/anycaptions-ui/src/settings/mod.rs`
- Create: `crates/anycaptions-ui/src/settings/appearance.rs`
- Create: `crates/anycaptions-ui/src/settings/languages.rs`
- Create: `crates/anycaptions-ui/src/settings/models.rs`
- Create: `crates/anycaptions-ui/src/settings/dictation.rs`
- Create: `crates/anycaptions-ui/src/settings/updates.rs`
- Create: `crates/anycaptions-ui/src/settings/tests.rs`

**Interfaces:**
- Consumes: Tasks 2–7 models/widgets/assets and core service traits.
- Produces: complete settings panel and draft/save host actions.

- [ ] **Step 1: Write failing settings acceptance tests**

Port all settings scenarios from `overlay.test.ts`, `fonts-updates.test.tsx`, `update-action.test.tsx`, `speaker-labels.test.tsx`, and dictation model tests. Include color normalization, reset, rejected save/retry, output stale ID, picker Escape, model cancel/retry, update fallback, and draft lifecycle.

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions-ui settings`
Expected: FAIL because settings sections do not exist.

- [ ] **Step 3: Implement every settings section**

Compose appearance, languages/engine, model/speaker downloads, audio/shortcut/dictation, updates, credit, and footer. Preserve immediate versus draft actions, exact save order, output busy states, progress, notices, focus return, and UIA metadata.

- [ ] **Step 4: Run settings and UIA verification**

Run: `cargo test -p anycaptions-ui settings` plus the UIA/keyboard integration script from Task 7.
Expected: all settings behavior and accessibility checks pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions-ui/src/settings
git commit -m "feat: port complete settings surface to GPUI"
```

---

### Task 11: Host services, recovery, and updater wiring

**Files:**
- Create: `crates/anycaptions/Cargo.toml`, `crates/anycaptions/src/app.rs`
- Create: `crates/anycaptions/src/host.rs`
- Create: `crates/anycaptions/src/services.rs`
- Create: `crates/anycaptions/src/adapters.rs`
- Create: `crates/anycaptions/src/tests.rs`
- Modify: `crates/anycaptions/src/main.rs`
- Modify: `crates/anycaptions-core/src/ipc.rs`

**Interfaces:**
- Consumes: all prior tasks and P1 core services.
- Produces: startup/shutdown, live capture, tray/hotkey recovery, downloads, update install/restart, and single-instance activation.

- [ ] **Step 1: Write failing host orchestration tests**

Include unavailable output, processor start failure, event delivery failure, shortcut conflict, close-to-tray, quit ordering, update busy gates, and second-instance activation. Example:

```rust
let result = host.start_live_capture();
assert_eq!(result, Err(CommandError::Domain(DomainError::CaptureUnavailable)));
assert_eq!(host.snapshot().capture, CaptureState::Error);
```

- [ ] **Step 2: Run the red check**

Run: `cargo test -p anycaptions`
Expected: FAIL because host orchestration does not exist.

- [ ] **Step 3: Implement host composition**

Move `start_live_capture`/`stop_live_capture` into a neutral core helper and reproduce every failure transition. Implement startup order, controller/window effects, GPUI sinks, tray/hotkeys/single-instance, download/update actions, and ordered shutdown. Browser/update URLs remain constants.

- [ ] **Step 4: Run host and platform integration tests**

Run: `cargo test -p anycaptions -p anycaptions-platform-win32`
Expected: all host/recovery/update tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anycaptions crates/anycaptions-core/src/ipc.rs
git commit -m "feat: wire GPUI application services and recovery"
```

---

### Task 12: Cutover, migration verification, documentation, and PRs

**Files:**
- Delete: `backend/`, `frontend/`, `crates/spike-overlay/` after every replacement check passes
- Modify: `Cargo.toml`, `Cargo.lock`, `README.md`, `scripts/verify-overlay.ps1`
- Create: `docs/superpowers/specs/2026-11-05-gpui-cutover-evidence.md`

**Interfaces:**
- Consumes: every previous task's green checks.
- Produces: GPUI-only workspace and verified installer/update migration.

- [ ] **Step 1: Run pre-cutover contract checks**

Run: `cargo test --workspace --locked`, `cargo test --manifest-path backend/Cargo.toml`, and `npm.cmd --prefix frontend test -- --run`.
Expected: current Rust tests, 153 legacy backend tests, and the 76-test frontend baseline pass. Classify all 11 real-model tests and run every test with available pinned inputs. Record equivalent Rust coverage for every legacy test file.

- [ ] **Step 2: Remove the old framework**

Delete Tauri/React/spike files only after their replacements pass. Remove obsolete capabilities, permissions, Node scripts, and duplicate lockfiles. Update the root workspace and README build/release commands.

- [ ] **Step 3: Run final automated verification**

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo tree -p anycaptions --locked | Select-String 'tauri|wry|webview2'
cargo audit
powershell -NoProfile -File scripts/test-packaging.ps1
```

Expected: all Rust checks pass, dependency search has no output, and every audit finding has a documented reachability decision.

- [ ] **Step 4: Run visual, accessibility, installer, and migration verification**

Execute the full interactive matrix from the spec: screenshots, actual click-through, drag/resize, six locales, IME/keyboard/UIA, tray/hotkeys/dictation, silent install/uninstall, and Tauri 0.2.0 → GPUI update on a Windows VM. Record commands, screenshots, outputs, and any unavailable external input explicitly.

- [ ] **Step 5: Review and deliver**

Run the scoped Assay gates and independent whole-branch review. Commit the cutover/evidence/docs, push `feat/full-gpui-port`, and open a final PR to `lukavtz/AnyCaptions:main` with exact results and residual limitations.
