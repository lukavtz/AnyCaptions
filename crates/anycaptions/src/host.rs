use std::{
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anycaptions_core::{
    dictation::DictationSettings,
    domain::{AppSnapshot, Appearance, CaptureState, WindowMode},
    ipc::{CommandError, ShortcutRegistry},
    models::{download, models_root},
    preferences::SavedGeometry,
    presentation::{
        application::{Action, SettingsSave, SnapshotRecovery},
        downloads::DownloadCommand,
    },
    transcription::session::SessionSink,
    update,
};
use anycaptions_platform_win32::{
    TrayCommand, TrayService, WindowEffects,
    system::{BrowserPage, SystemActions},
};
use anycaptions_ui::{
    overlay::{OverlayCommand, OverlayHost, OverlayView},
    settings::{DictationLoad, SettingsHostAction, SettingsView},
};
use async_channel::Sender;
use gpui::{AnyView, App, AppContext as _, Context, Entity, Window, WindowHandle};

use crate::{
    adapters::{HostDownloadSink, HostEvent, HostSessionSink, UpdateDestination},
    services::Services,
};

pub(crate) trait NativeWindowEffects: Send + Sync {
    fn set_click_through(&self, enabled: bool) -> Result<(), CommandError>;
}

pub(crate) struct Win32WindowEffects {
    hwnd: usize,
}

impl Win32WindowEffects {
    pub(crate) fn new(effects: &WindowEffects) -> Self {
        Self {
            hwnd: effects.hwnd().0 as usize,
        }
    }
}

impl NativeWindowEffects for Win32WindowEffects {
    fn set_click_through(&self, enabled: bool) -> Result<(), CommandError> {
        let effects = WindowEffects::new(windows::Win32::Foundation::HWND(
            self.hwnd as *mut core::ffi::c_void,
        ))
        .map_err(|_| CommandError::WindowUnavailable)?;
        effects
            .set_click_through(enabled)
            .map_err(|_| CommandError::WindowUnavailable)
    }
}

#[derive(Clone)]
struct SettingsPanel {
    entity: Entity<SettingsView>,
    generation: u64,
}

#[derive(Clone, Default)]
struct RetainedUpdates {
    checking: bool,
    last_check: Option<
        Result<anycaptions_core::update::UpdateCheck, anycaptions_core::update::UpdateError>,
    >,
    progress: Option<anycaptions_core::updater::UpdateProgress>,
    install_error: Option<CommandError>,
}

#[derive(Clone, Default)]
struct RetainedSettings {
    updates: RetainedUpdates,
    download: Option<anycaptions_core::models::download::DownloadUpdate>,
}

struct HostUi {
    overlay: Option<WindowHandle<OverlayView>>,
    settings: Option<SettingsPanel>,
    next_settings_generation: u64,
}

fn is_settings_transaction_or_service(action: &SettingsHostAction) -> bool {
    matches!(
        action,
        SettingsHostAction::Application(_)
            | SettingsHostAction::SaveSettings
            | SettingsHostAction::LoadDictation
            | SettingsHostAction::RefreshOutputs
            | SettingsHostAction::SelectOutput(_)
            | SettingsHostAction::RefreshModels
            | SettingsHostAction::Download(_)
    )
}

fn is_settings_update(action: &SettingsHostAction) -> bool {
    matches!(
        action,
        SettingsHostAction::CheckForUpdates
            | SettingsHostAction::InstallUpdate
            | SettingsHostAction::OpenReleasesPage
            | SettingsHostAction::OpenProjectPage
    )
}

struct HostInner {
    services: Arc<Services>,
    sink: Arc<HostSessionSink>,
    shortcuts: Arc<dyn ShortcutRegistry + Send + Sync>,
    native: Mutex<Option<Arc<dyn NativeWindowEffects>>>,
    events: Sender<HostEvent>,
    ui: Mutex<HostUi>,
    retained: Mutex<RetainedSettings>,
    shutting_down: AtomicBool,
}

#[derive(Clone)]
pub(crate) struct Host {
    inner: Arc<HostInner>,
}

impl Host {
    pub(crate) fn new(
        services: Arc<Services>,
        sink: Arc<HostSessionSink>,
        shortcuts: Arc<dyn ShortcutRegistry + Send + Sync>,
        events: Sender<HostEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(HostInner {
                services,
                sink,
                shortcuts,
                native: Mutex::new(None),
                events,
                ui: Mutex::new(HostUi {
                    overlay: None,
                    settings: None,
                    next_settings_generation: 0,
                }),
                retained: Mutex::new(RetainedSettings::default()),
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn attach_overlay(&self, overlay: WindowHandle<OverlayView>) {
        self.inner
            .ui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .overlay = Some(overlay);
    }

    pub(crate) fn attach_native_effects(&self, native: Arc<dyn NativeWindowEffects>) {
        *self.inner.native.lock().unwrap_or_else(|e| e.into_inner()) = Some(native);
    }

    fn native(&self) -> Option<Arc<dyn NativeWindowEffects>> {
        self.inner
            .native
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn overlay(&self) -> Option<WindowHandle<OverlayView>> {
        self.inner
            .ui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .overlay
    }

    fn settings(&self) -> Option<SettingsPanel> {
        self.inner
            .ui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .settings
            .clone()
    }

    fn current_settings(&self, generation: u64) -> Option<SettingsPanel> {
        self.settings()
            .filter(|panel| panel.generation == generation)
    }

    pub(crate) fn retain_update_check_start(&self) -> bool {
        let mut retained = self
            .inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if retained.updates.checking {
            return false;
        }
        retained.updates = RetainedUpdates {
            checking: true,
            ..RetainedUpdates::default()
        };
        true
    }

    fn retain_update_check_result(
        &self,
        result: &Result<
            anycaptions_core::update::UpdateCheck,
            anycaptions_core::update::UpdateError,
        >,
    ) {
        let mut retained = self
            .inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        retained.updates.checking = false;
        retained.updates.last_check = Some(result.clone());
    }

    fn retain_install_start(&self) {
        let mut retained = self
            .inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        retained.updates.progress = Some(anycaptions_core::updater::UpdateProgress {
            phase: anycaptions_core::updater::UpdatePhase::Downloading,
            received_bytes: 0,
            total_bytes: 0,
            reason: None,
        });
        retained.updates.install_error = None;
    }

    fn retain_update_outcome(
        &self,
        progress: Option<anycaptions_core::updater::UpdateProgress>,
        error: Option<CommandError>,
    ) {
        let mut retained = self
            .inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        retained.updates.progress = progress;
        retained.updates.install_error = error;
    }

    fn retain_install_error(&self, error: CommandError) {
        self.retain_update_outcome(None, Some(error));
    }

    fn retain_update_progress(&self, progress: anycaptions_core::updater::UpdateProgress) {
        self.retain_update_outcome(Some(progress), None);
    }

    fn retain_download(&self, update: anycaptions_core::models::download::DownloadUpdate) {
        self.inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .download = Some(update);
    }

    pub(crate) fn retain_download_start(&self, model: &'static str) {
        self.retain_download(anycaptions_core::models::download::DownloadUpdate {
            model,
            phase: anycaptions_core::models::download::DownloadPhase::Downloading,
            received_bytes: 0,
            total_bytes: None,
            reason: None,
        });
    }

    fn seed_settings(&self, settings: &Entity<SettingsView>, window: &mut Window, cx: &mut App) {
        let retained = self
            .inner
            .retained
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if retained.updates.checking {
            settings.update(cx, |view, cx| {
                view.restore_pending_update_check(window, cx);
            });
        } else if let Some(result) = retained.updates.last_check {
            settings.update(cx, |view, cx| {
                view.complete_update_check(result, window, cx);
            });
        }
        if let Some(progress) = retained.updates.progress {
            settings.update(cx, |view, cx| {
                view.accept_update_progress(progress, window, cx);
            });
        } else if let Some(error) = retained.updates.install_error {
            settings.update(cx, |view, cx| {
                view.reject_install(error, window, cx);
            });
        }
        if let Some(update) = retained.download {
            settings.update(cx, |view, cx| {
                view.accept_download(update, window, cx);
            });
        }
    }

    fn session_sink(&self) -> Arc<dyn SessionSink> {
        self.inner.sink.clone()
    }

    pub(crate) fn snapshot(&self) -> AppSnapshot {
        self.inner
            .services
            .controller
            .snapshot()
            .expect("application state unavailable")
    }

    pub(crate) fn start_live_capture(&self) -> Result<AppSnapshot, CommandError> {
        self.inner
            .services
            .start(self.inner.sink.as_ref(), self.session_sink())
    }

    pub(crate) fn stop_live_capture(&self) -> Result<AppSnapshot, CommandError> {
        self.inner.services.stop(self.inner.sink.as_ref())
    }

    fn apply(&self, action: Action) -> Result<AppSnapshot, CommandError> {
        if action == Action::LockWindow {
            return self.set_window_mode(WindowMode::Locked);
        }
        self.inner.services.apply_action(
            action,
            self.inner.sink.as_ref(),
            self.session_sink(),
            self.inner.shortcuts.as_ref(),
        )
    }

    pub(crate) fn set_window_mode(&self, mode: WindowMode) -> Result<AppSnapshot, CommandError> {
        let previous = self.inner.services.controller.snapshot()?;
        let native = self.native();
        if previous.window_mode != mode
            && let Some(native) = &native
            && native
                .set_click_through(mode == WindowMode::Locked)
                .is_err()
        {
            let _ = native.set_click_through(previous.window_mode == WindowMode::Locked);
            return Err(CommandError::WindowUnavailable);
        }
        let result = self
            .inner
            .services
            .set_window_mode(mode, self.inner.sink.as_ref());
        if result.is_err() && previous.window_mode != mode {
            let committed = self
                .inner
                .services
                .controller
                .snapshot()
                .is_ok_and(|snapshot| snapshot.window_mode == mode);
            if !committed
                && let Some(native) = &native
                && native
                    .set_click_through(previous.window_mode == WindowMode::Locked)
                    .is_err()
            {
                return Err(CommandError::WindowUnavailable);
            }
        }
        result
    }

    pub(crate) fn change_shortcut(&self, shortcut: &str) -> Result<AppSnapshot, CommandError> {
        self.inner.services.set_shortcut(
            shortcut,
            self.inner.sink.as_ref(),
            self.inner.shortcuts.as_ref(),
        )
    }

    pub(crate) fn activate(&self) -> Result<AppSnapshot, CommandError> {
        self.set_window_mode(WindowMode::Edit)
    }

    fn toggle_lock(&self) -> Result<AppSnapshot, CommandError> {
        let next = match self.snapshot().window_mode {
            WindowMode::Locked => WindowMode::Edit,
            WindowMode::Edit => WindowMode::Locked,
        };
        self.set_window_mode(next)
    }

    fn toggle_capture(&self) -> Result<AppSnapshot, CommandError> {
        if self.snapshot().capture == CaptureState::Stopped {
            self.start_live_capture()
        } else {
            self.stop_live_capture()
        }
    }

    fn show_overlay(&self, cx: &mut App) {
        let editable = self
            .inner
            .services
            .controller
            .snapshot()
            .is_ok_and(|snapshot| snapshot.window_mode == WindowMode::Edit);
        if editable && let Some(overlay) = self.overlay() {
            let _ = overlay.update(cx, |view, _, cx| view.show_edit(cx));
        }
    }

    fn report(&self, error: CommandError, cx: &mut App) {
        if let Some(settings) = self.settings() {
            settings
                .entity
                .update(cx, |view, cx| view.report(error, cx));
        } else {
            eprintln!("AnyCaptions command failed: {error:?}");
        }
    }

    fn refresh_outputs(&self, generation: u64, selected: Option<String>) {
        let services = Arc::clone(&self.inner.services);
        let sink = Arc::clone(&self.inner.sink);
        let events = self.inner.events.clone();
        thread::spawn(move || {
            let is_selection = selected.is_some();
            let result = match selected {
                Some(id) => services.select_output(&id, sink.as_ref()),
                None => services.output_selection(sink.as_ref()),
            };
            let _ = events.try_send(HostEvent::Outputs {
                generation,
                selected: is_selection,
                result,
            });
        });
    }

    fn load_dictation(&self, generation: u64) {
        let services = Arc::clone(&self.inner.services);
        let events = self.inner.events.clone();
        thread::spawn(move || {
            let result = services.dictation_snapshot().map(|snapshot| DictationLoad {
                settings: snapshot.settings,
                available: snapshot.available,
                inputs: snapshot.inputs,
            });
            let _ = events.try_send(HostEvent::Dictation { generation, result });
        });
    }

    fn refresh_models(&self, generation: u64) {
        let services = Arc::clone(&self.inner.services);
        let events = self.inner.events.clone();
        thread::spawn(move || {
            let models = services.model_downloads();
            let _ = events.try_send(HostEvent::Models { generation, models });
        });
    }

    fn download_model(&self, model: &str) -> Result<(), CommandError> {
        let source = download::source_for(model).ok_or(CommandError::DownloadUnavailable)?;
        let services = Arc::clone(&self.inner.services);
        let sink = Arc::clone(&self.inner.sink);
        let on_installed = Box::new(move || {
            let _ = services.refresh_resources(sink.as_ref());
        });
        self.inner
            .services
            .downloads
            .start(
                source,
                models_root(),
                Arc::new(HostDownloadSink(self.inner.events.clone())),
                on_installed,
            )
            .map_err(|_| CommandError::DownloadBusy)
    }

    pub(crate) fn check_updates(&self, destination: UpdateDestination) {
        let events = self.inner.events.clone();
        thread::spawn(move || {
            let _ = events.try_send(HostEvent::UpdateCheck {
                destination,
                result: update::check(),
            });
        });
    }

    fn choose_color(
        &self,
        settings: &Entity<SettingsView>,
        window: &mut Window,
        cx: &mut App,
        initial: u32,
    ) {
        let result = (|| {
            let raw = raw_window_handle::HasWindowHandle::window_handle(window)
                .map_err(|_| CommandError::WindowUnavailable)?
                .as_raw();
            let effects =
                WindowEffects::from_raw_handle(raw).map_err(|_| CommandError::WindowUnavailable)?;
            SystemActions::choose_color(Some(effects.hwnd()), initial)
                .map_err(|_| CommandError::WindowUnavailable)
        })();
        settings.update(cx, |view, cx| {
            view.complete_color_choice(result, window, cx);
        });
    }

    pub(crate) fn close_settings(&self, cx: &mut App) {
        self.inner
            .ui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .settings = None;
        if let Some(overlay) = self.overlay() {
            let _ = overlay.update(cx, |view, window, cx| {
                view.set_settings_open(false, window, cx);
            });
        }
    }

    fn drain_settings(&self, panel: SettingsPanel, window: &mut Window, cx: &mut App) {
        if self.current_settings(panel.generation).is_none() {
            return;
        }
        while let Some(action) = panel.entity.update(cx, |view, _| view.take_action()) {
            if !self.perform_settings_action(panel.clone(), action, window, cx) {
                break;
            }
        }
    }

    fn perform_settings_action(
        &self,
        panel: SettingsPanel,
        action: SettingsHostAction,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if is_settings_transaction_or_service(&action) {
            self.apply_or_service_settings_action(panel, action, window, cx);
            return true;
        }
        if is_settings_update(&action) {
            self.update_settings_action(panel, action, window, cx);
            return true;
        }
        match action {
            SettingsHostAction::ChooseColor { initial, .. } => {
                self.choose_color(&panel.entity, window, cx, initial);
                true
            }
            SettingsHostAction::Close => {
                self.close_settings(cx);
                false
            }
            _ => unreachable!("settings action category"),
        }
    }

    fn apply_or_service_settings_action(
        &self,
        panel: SettingsPanel,
        action: SettingsHostAction,
        window: &mut Window,
        cx: &mut App,
    ) {
        match action {
            SettingsHostAction::Application(_) | SettingsHostAction::SaveSettings => {
                self.apply_settings_action(panel, action, window, cx);
            }
            _ => self.service_settings_action(panel, action, window, cx),
        }
    }

    fn apply_settings_action(
        &self,
        panel: SettingsPanel,
        action: SettingsHostAction,
        window: &mut Window,
        cx: &mut App,
    ) {
        match action {
            SettingsHostAction::Application(action) => {
                let result = self.apply(action);
                panel.entity.update(cx, |view, cx| {
                    view.complete_application(result, self, window, cx);
                });
            }
            SettingsHostAction::SaveSettings => {
                panel.entity.update(cx, |view, cx| {
                    let mut backend = HostSettingsSave { host: self };
                    view.save_settings(&mut backend, window, cx);
                });
            }
            _ => unreachable!("settings transaction category"),
        }
    }

    fn service_settings_action(
        &self,
        panel: SettingsPanel,
        action: SettingsHostAction,
        window: &mut Window,
        cx: &mut App,
    ) {
        match action {
            SettingsHostAction::LoadDictation => self.load_dictation(panel.generation),
            SettingsHostAction::RefreshOutputs => self.refresh_outputs(panel.generation, None),
            SettingsHostAction::SelectOutput(id) => {
                self.refresh_outputs(panel.generation, Some(id));
            }
            SettingsHostAction::RefreshModels => self.refresh_models(panel.generation),
            SettingsHostAction::Download(command) => {
                self.download_settings(command, panel.entity, window, cx);
            }
            _ => unreachable!("settings service category"),
        }
    }

    fn download_settings(
        &self,
        command: DownloadCommand,
        settings: Entity<SettingsView>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match command {
            DownloadCommand::Start(model) => {
                self.retain_download_start(model);
                if self.download_model(model).is_err() {
                    self.retain_download(anycaptions_core::models::download::DownloadUpdate {
                        model,
                        phase: anycaptions_core::models::download::DownloadPhase::Failed,
                        received_bytes: 0,
                        total_bytes: None,
                        reason: Some(anycaptions_core::models::download::DownloadFailure::Network),
                    });
                    settings.update(cx, |view, cx| {
                        view.reject_download_start(model, window, cx);
                    });
                }
            }
            DownloadCommand::Cancel => self.inner.services.downloads.cancel(),
        }
    }

    fn update_settings_action(
        &self,
        panel: SettingsPanel,
        action: SettingsHostAction,
        window: &mut Window,
        cx: &mut App,
    ) {
        match action {
            SettingsHostAction::CheckForUpdates => {
                if self.retain_update_check_start() {
                    self.check_updates(UpdateDestination::Settings(panel.generation));
                }
            }
            SettingsHostAction::InstallUpdate => {
                self.retain_install_start();
                if let Err(error) = self
                    .inner
                    .services
                    .install_update(self.inner.events.clone())
                {
                    self.retain_install_error(error);
                    panel.entity.update(cx, |view, cx| {
                        view.reject_install(error, window, cx);
                    });
                }
            }
            SettingsHostAction::OpenReleasesPage => {
                self.open_browser(panel.entity, BrowserPage::Releases, cx);
            }
            SettingsHostAction::OpenProjectPage => {
                self.open_browser(panel.entity, BrowserPage::Project, cx);
            }
            _ => unreachable!("settings update category"),
        }
    }

    fn open_browser(&self, settings: Entity<SettingsView>, page: BrowserPage, cx: &mut App) {
        if SystemActions::open_browser(page).is_err() {
            settings.update(cx, |view, cx| {
                view.report(CommandError::BrowserUnavailable, cx);
            });
        }
    }

    fn deliver_settings(
        &self,
        panel: Option<SettingsPanel>,
        window: &mut Window,
        cx: &mut App,
        update: impl FnOnce(&mut SettingsView, &mut Window, &mut Context<SettingsView>),
    ) {
        if let Some(panel) = panel {
            panel.entity.update(cx, |view, cx| update(view, window, cx));
            self.drain_settings(panel, window, cx);
        }
    }

    fn handle_snapshot(
        &self,
        snapshot: AppSnapshot,
        window: &mut Window,
        cx: &mut App,
        tray: &mut TrayService,
    ) {
        let current_revision = match self.inner.services.controller.snapshot() {
            Ok(current) => current.revision,
            Err(_) => return,
        };
        if snapshot.revision < current_revision {
            return;
        }
        let can_capture = snapshot.resources.audio_capture && snapshot.resources.transcription;
        let locked = snapshot.window_mode == WindowMode::Locked;
        let _ = tray.set_capture_enabled(can_capture);
        if let Some(settings) = self.settings() {
            settings.entity.update(cx, |view, cx| {
                view.accept_snapshot(snapshot, window, cx);
            });
            if locked {
                self.close_settings(cx);
            } else {
                self.drain_settings(settings, window, cx);
            }
        }
    }

    fn recover_window(&self, cx: &mut App) {
        let _ = self.activate();
        self.show_overlay(cx);
    }

    fn handle_tray(&self, command: TrayCommand, window: &mut Window, cx: &mut App) {
        match command {
            TrayCommand::ShowEdit => self.recover_window(cx),
            TrayCommand::ToggleCapture => {
                let _ = self.toggle_capture();
            }
            TrayCommand::ToggleLock => {
                let editing = self
                    .toggle_lock()
                    .is_ok_and(|snapshot| snapshot.window_mode == WindowMode::Edit);
                if editing {
                    self.show_overlay(cx);
                }
            }
            TrayCommand::Quit => self.shutdown_and_quit(false, window, cx),
        }
    }

    fn handle_generated_settings_event(&self, event: HostEvent, window: &mut Window, cx: &mut App) {
        match event {
            HostEvent::Outputs {
                generation,
                selected,
                result,
            } => self.deliver_settings(
                self.current_settings(generation),
                window,
                cx,
                |view, window, cx| {
                    if selected {
                        view.complete_output_selection(result, window, cx);
                    } else {
                        view.complete_outputs(result, window, cx);
                    }
                },
            ),
            HostEvent::Dictation { generation, result } => {
                self.deliver_settings(
                    self.current_settings(generation),
                    window,
                    cx,
                    |view, window, cx| {
                        view.complete_dictation(result, window, cx);
                    },
                );
            }
            HostEvent::Models { generation, models } => {
                self.deliver_settings(
                    self.current_settings(generation),
                    window,
                    cx,
                    |view, window, cx| {
                        view.accept_models(models, window, cx);
                    },
                );
            }
            _ => unreachable!("generated settings event category"),
        }
    }

    pub(crate) fn handle_global_settings_event(
        &self,
        event: HostEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        match event {
            HostEvent::Download(update) => {
                self.retain_download(update.clone());
                self.deliver_settings(self.settings(), window, cx, |view, window, cx| {
                    view.accept_download(update, window, cx);
                });
            }
            HostEvent::UpdateProgress(progress) => {
                self.retain_update_progress(progress.clone());
                self.deliver_settings(self.settings(), window, cx, |view, window, cx| {
                    view.accept_update_progress(progress, window, cx);
                });
            }
            _ => unreachable!("global settings event category"),
        }
    }

    pub(crate) fn handle_update_check(
        &self,
        destination: UpdateDestination,
        result: Result<
            anycaptions_core::update::UpdateCheck,
            anycaptions_core::update::UpdateError,
        >,
        window: &mut Window,
        cx: &mut App,
    ) {
        match destination {
            UpdateDestination::Overlay => {
                if let Some(overlay) = self.overlay() {
                    let _ = overlay.update(cx, |view, _, cx| {
                        view.complete_update_check(result, cx);
                    });
                }
            }
            UpdateDestination::Settings(_) => {
                self.retain_update_check_result(&result);
                self.deliver_settings(self.settings(), window, cx, |view, window, cx| {
                    view.complete_update_check(result, window, cx);
                });
            }
        }
    }

    fn handle_settings_event(&self, event: HostEvent, window: &mut Window, cx: &mut App) {
        if matches!(
            event,
            HostEvent::Outputs { .. } | HostEvent::Dictation { .. } | HostEvent::Models { .. }
        ) {
            self.handle_generated_settings_event(event, window, cx);
        } else {
            self.handle_global_settings_event(event, window, cx);
        }
    }

    pub(crate) fn handle_event(
        &self,
        event: HostEvent,
        window: &mut Window,
        cx: &mut App,
        tray: &mut TrayService,
    ) {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return;
        }
        match event {
            HostEvent::Snapshot(snapshot) => self.handle_snapshot(snapshot, window, cx, tray),
            HostEvent::Tray(command) => self.handle_tray(command, window, cx),
            HostEvent::Activate => self.recover_window(cx),
            HostEvent::UpdateCheck {
                destination,
                result,
            } => self.handle_update_check(destination, result, window, cx),
            HostEvent::Restart => self.shutdown_and_quit(true, window, cx),
            event => self.handle_settings_event(event, window, cx),
        }
    }

    fn shutdown_and_quit(&self, restart: bool, window: &mut Window, cx: &mut App) {
        if restart && let Err(error) = restart_after_exit() {
            self.retain_install_error(error);
            if let Some(settings) = self.settings() {
                settings.entity.update(cx, |view, cx| {
                    view.reject_install(error, window, cx);
                });
            } else {
                self.report(error, cx);
            }
            return;
        }
        if self.inner.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        let geometry = self
            .overlay()
            .and_then(|overlay| overlay.update(cx, |view, _, _| view.saved_geometry()).ok());
        if let Err(error) = self.inner.services.shutdown(geometry) {
            eprintln!("AnyCaptions shutdown preference save failed: {error:?}");
        }
        cx.quit();
    }
}

struct HostSettingsSave<'a> {
    host: &'a Host,
}

impl SnapshotRecovery for HostSettingsSave<'_> {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        self.host.inner.services.controller.snapshot()
    }
}

impl SnapshotRecovery for Host {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        self.inner.services.controller.snapshot()
    }
}

impl SettingsSave for HostSettingsSave<'_> {
    fn set_appearance(&mut self, appearance: Appearance) -> Result<AppSnapshot, CommandError> {
        self.host
            .inner
            .services
            .set_appearance(appearance, self.host.inner.sink.as_ref())
    }

    fn set_shortcut(&mut self, shortcut: &str) -> Result<AppSnapshot, CommandError> {
        self.host.change_shortcut(shortcut)
    }

    fn set_dictation(
        &mut self,
        settings: DictationSettings,
    ) -> Result<DictationSettings, CommandError> {
        self.host.inner.services.set_dictation(settings)
    }
}

impl OverlayHost for Host {
    fn execute(&self, command: OverlayCommand) -> Result<AppSnapshot, CommandError> {
        let action = match command {
            OverlayCommand::StartCapture => Action::StartCapture,
            OverlayCommand::StopCapture => Action::StopCapture,
            OverlayCommand::SetMode(mode) => Action::SetMode(mode),
            OverlayCommand::LockWindow => Action::LockWindow,
        };
        self.apply(action)
    }

    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        SnapshotRecovery::recover_snapshot(self)
    }

    fn close_to_tray(&self, geometry: SavedGeometry) -> Result<(), CommandError> {
        self.inner.services.save_geometry(geometry)
    }

    fn open_settings(&self, _snapshot: &AppSnapshot, window: &mut Window, cx: &mut App) -> AnyView {
        let snapshot = self
            .inner
            .services
            .controller
            .snapshot()
            .expect("application state unavailable");
        let settings = cx.new(|cx| SettingsView::new(snapshot, window, cx));
        let panel = {
            let mut ui = self.inner.ui.lock().unwrap_or_else(|e| e.into_inner());
            let generation = ui
                .next_settings_generation
                .checked_add(1)
                .expect("settings generation exhausted");
            ui.next_settings_generation = generation;
            let panel = SettingsPanel {
                entity: settings.clone(),
                generation,
            };
            ui.settings = Some(panel.clone());
            panel
        };
        let host = self.clone();
        let generation = panel.generation;
        window
            .observe(&settings, cx, move |entity, window, cx| {
                host.drain_settings(SettingsPanel { entity, generation }, window, cx);
            })
            .detach();
        self.seed_settings(&settings, window, cx);
        self.drain_settings(panel, window, cx);
        settings.into()
    }
}

fn restart_after_exit() -> Result<(), CommandError> {
    let executable = std::env::current_exe().map_err(|_| CommandError::UpdateUnavailable)?;
    let executable = executable
        .to_str()
        .ok_or(CommandError::UpdateUnavailable)?
        .replace('\'', "''");
    let script = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}'",
        std::process::id(),
        executable,
    );
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()
        .map(|_| ())
        .map_err(|_| CommandError::UpdateUnavailable)
}
