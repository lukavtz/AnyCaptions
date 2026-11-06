use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anycaptions_core::{
    audio::{
        capture::{CaptureBackend, CaptureError, CaptureEvent},
        convert::PcmFormat,
        devices::{
            AudioDevice, AudioDeviceCatalog, AudioDeviceError, DeviceInventory, InputService,
            OutputService,
        },
    },
    dictation::{
        DictationError,
        indicator::IndicatorSnapshot,
        runtime::{DictationState, HotkeyRegistry},
    },
    domain::{CaptureState, DomainError, Resources, UiLanguage, WindowMode},
    ipc::{CaptionUpdate, CommandError, Controller, ShortcutRegistry},
    models::{
        catalog,
        download::{self, DownloadFailure, DownloadPhase, DownloadUpdate},
    },
    pipeline::AudioProcessor,
    preferences::{PreferenceStore, SavedGeometry, config_path},
    presentation::updates::{UpdateActionState, UpdateStatus},
    transcription::session::{CaptionSessionManager, ProcessorFactory},
    update::UpdateCheck,
    updater::{UpdateInstaller, UpdatePhase, UpdateProgress},
};
use anycaptions_ui::{
    overlay::{OverlayCommand, OverlayEvent, OverlayHost, UiEventReceiver, ui_event_channels},
    settings::{DownloadRow, SettingsView},
};
use async_channel::Receiver;
use gpui::{AnyView, AppContext, Entity, TestAppContext, VisualTestContext};

use crate::{
    adapters::{HostEvent, HostSessionSink, UpdateDestination},
    host::{Host, NativeWindowEffects},
    services::{Services, ordered_shutdown, update_install_gate},
};

struct SequenceCatalog {
    inventories: Mutex<VecDeque<DeviceInventory>>,
}

impl SequenceCatalog {
    fn new(inventories: impl IntoIterator<Item = DeviceInventory>) -> Self {
        Self {
            inventories: Mutex::new(inventories.into_iter().collect()),
        }
    }
}

impl AudioDeviceCatalog for SequenceCatalog {
    fn scan(&self) -> Result<DeviceInventory, AudioDeviceError> {
        let mut inventories = self.inventories.lock().unwrap();
        if inventories.len() > 1 {
            Ok(inventories.pop_front().expect("nonempty"))
        } else {
            inventories
                .front()
                .cloned()
                .ok_or(AudioDeviceError::Unavailable)
        }
    }
}

struct FailingFactory;

impl ProcessorFactory for FailingFactory {
    fn is_available(&self) -> bool {
        true
    }

    fn create(
        &self,
        _: u32,
        _: anycaptions_core::domain::CaptionMode,
        _: Option<&str>,
        _: bool,
    ) -> Result<Box<dyn AudioProcessor<Output = Vec<CaptionUpdate>>>, DomainError> {
        Err(DomainError::TranscriptionUnavailable)
    }
}

struct UnusedCapture;

impl CaptureBackend for UnusedCapture {
    fn run(
        &self,
        _: &str,
        _: std::sync::mpsc::Receiver<()>,
        _: std::sync::mpsc::Sender<Result<PcmFormat, CaptureError>>,
        _: std::sync::mpsc::SyncSender<CaptureEvent>,
    ) -> Result<(), CaptureError> {
        Err(CaptureError::DeviceUnavailable)
    }
}

#[derive(Default)]
struct FakeHotkeys {
    registered: Mutex<HashSet<String>>,
    blocked: Mutex<Option<String>>,
}

impl ShortcutRegistry for FakeHotkeys {
    fn register(&self, shortcut: &str) -> Result<(), CommandError> {
        if self.blocked.lock().unwrap().as_deref() == Some(shortcut) {
            return Err(CommandError::ShortcutUnavailable);
        }
        self.registered.lock().unwrap().insert(shortcut.into());
        Ok(())
    }

    fn unregister(&self, shortcut: &str) -> Result<(), CommandError> {
        self.registered.lock().unwrap().remove(shortcut);
        Ok(())
    }
}

impl HotkeyRegistry for FakeHotkeys {
    fn register(
        &self,
        shortcut: &str,
        _: Box<dyn Fn(bool) + Send + Sync + 'static>,
    ) -> Result<(), DictationError> {
        if self.blocked.lock().unwrap().as_deref() == Some(shortcut) {
            return Err(DictationError::ShortcutConflict);
        }
        self.registered.lock().unwrap().insert(shortcut.into());
        Ok(())
    }

    fn unregister(&self, shortcut: &str) {
        self.registered.lock().unwrap().remove(shortcut);
    }
}

#[derive(Default)]
struct FakeWindowEffects {
    fail_next: Mutex<bool>,
    click_through: Mutex<bool>,
}

impl NativeWindowEffects for FakeWindowEffects {
    fn set_click_through(&self, enabled: bool) -> Result<(), CommandError> {
        let mut fail_next = self.fail_next.lock().unwrap();
        if *fail_next {
            *fail_next = false;
            return Err(CommandError::WindowUnavailable);
        }
        *self.click_through.lock().unwrap() = enabled;
        Ok(())
    }
}

struct Fixture {
    host: Host,
    services: Arc<Services>,
    sink: Arc<HostSessionSink>,
    hotkeys: Arc<FakeHotkeys>,
    native: Arc<FakeWindowEffects>,
    overlay: UiEventReceiver<OverlayEvent>,
    _indicator: UiEventReceiver<IndicatorSnapshot>,
    _events: Receiver<HostEvent>,
    root: PathBuf,
}

impl Fixture {
    fn new(inventories: impl IntoIterator<Item = DeviceInventory>) -> Self {
        let root = isolated_root();
        let store = PreferenceStore::load(config_path(&root));
        let controller = Controller::default();
        let sessions =
            CaptionSessionManager::new(Arc::new(UnusedCapture), Arc::new(FailingFactory));
        let outputs = Arc::new(OutputService::new(
            Box::new(SequenceCatalog::new(inventories)),
            Some("test-output".into()),
        ));
        let inputs = Arc::new(InputService::new(
            Box::new(SequenceCatalog::new([empty_inventory()])),
            None,
        ));
        let (ui_sink, overlay, indicator) = ui_event_channels(controller.clone());
        let (event_tx, event_rx) = async_channel::unbounded();
        let sink = Arc::new(HostSessionSink::new(ui_sink, event_tx.clone()));
        let hotkeys = Arc::new(FakeHotkeys::default());
        let dictation = Arc::new(DictationState::new(
            sessions.factory(),
            Arc::clone(&inputs),
            Default::default(),
            controller.clone(),
            hotkeys.clone(),
            sink.clone(),
        ));
        let services = Arc::new(Services::new(
            controller, store, sessions, outputs, inputs, dictation,
        ));
        let host = Host::new(
            Arc::clone(&services),
            Arc::clone(&sink),
            hotkeys.clone(),
            event_tx,
        );
        let native = Arc::new(FakeWindowEffects::default());
        host.attach_native_effects(native.clone());
        Self {
            host,
            services,
            sink,
            hotkeys,
            native,
            overlay,
            _indicator: indicator,
            _events: event_rx,
            root,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn isolated_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("anycaptions-host-{}-{nonce}", std::process::id()))
}

fn available_inventory() -> DeviceInventory {
    DeviceInventory {
        devices: vec![AudioDevice {
            id: "test-output".into(),
            name: "Test output".into(),
        }],
        default_id: Some("test-output".into()),
    }
}

fn empty_inventory() -> DeviceInventory {
    DeviceInventory {
        devices: vec![],
        default_id: None,
    }
}

fn settings_window(
    snapshot: anycaptions_core::domain::AppSnapshot,
    cx: &mut TestAppContext,
) -> (Entity<SettingsView>, &mut VisualTestContext) {
    cx.update(anycaptions_ui::widgets::bind_keys);
    cx.add_window_view(move |window, cx| SettingsView::new(snapshot, window, cx))
}

fn open_panel(
    host: &Host,
    snapshot: &anycaptions_core::domain::AppSnapshot,
    cx: &mut VisualTestContext,
) -> (AnyView, Entity<SettingsView>) {
    cx.update(|window, cx| {
        let view = host.open_settings(snapshot, window, cx);
        let entity = view.clone().downcast::<SettingsView>().unwrap();
        (view, entity)
    })
}

fn close_panel(
    host: &Host,
    view: AnyView,
    entity: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        entity.update(cx, |view, cx| view.cancel_settings(window, cx));
        host.close_settings(cx);
    });
    drop(view);
}

fn read_panel<R>(
    entity: &Entity<SettingsView>,
    cx: &VisualTestContext,
    read: impl FnOnce(&SettingsView) -> R,
) -> R {
    cx.read_entity(entity, |view, _| read(view))
}

#[test]
fn unavailable_output_fails_the_started_session() {
    let fixture = Fixture::new([available_inventory(), empty_inventory()]);
    let result = fixture.host.start_live_capture();
    assert_eq!(
        result.unwrap_err(),
        CommandError::Domain(DomainError::CaptureUnavailable)
    );
    assert_eq!(fixture.host.snapshot().capture, CaptureState::Error);
    assert_eq!(
        fixture.host.snapshot().error,
        Some(DomainError::CaptureUnavailable)
    );
}

#[test]
fn processor_start_failure_transitions_preparing_to_error() {
    let fixture = Fixture::new([available_inventory()]);
    let started = fixture.host.start_live_capture().unwrap();
    assert_eq!(started.capture, CaptureState::Preparing);
    for _ in 0..200 {
        if fixture.host.snapshot().capture == CaptureState::Error {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let snapshot = fixture.host.snapshot();
    assert_eq!(snapshot.capture, CaptureState::Error);
    assert_eq!(snapshot.error, Some(DomainError::TranscriptionUnavailable));
}

#[test]
fn event_delivery_failure_preserves_committed_state_for_recovery() {
    let fixture = Fixture::new([available_inventory()]);
    fixture.overlay.close();
    assert_eq!(
        fixture.host.start_live_capture().unwrap_err(),
        CommandError::EventUnavailable
    );
    assert_eq!(fixture.host.snapshot().capture, CaptureState::Preparing);
    assert_eq!(
        fixture.host.stop_live_capture().unwrap_err(),
        CommandError::EventUnavailable
    );
    assert_eq!(fixture.host.snapshot().capture, CaptureState::Stopped);
}

#[test]
fn shortcut_conflict_keeps_the_previous_recovery_registration() {
    let fixture = Fixture::new([available_inventory()]);
    fixture
        .services
        .controller
        .set_shortcut("Ctrl+Alt+L".into(), true, fixture.sink.as_ref())
        .unwrap();
    fixture
        .hotkeys
        .registered
        .lock()
        .unwrap()
        .insert("Ctrl+Alt+L".into());
    *fixture.hotkeys.blocked.lock().unwrap() = Some("Ctrl+Alt+K".into());
    assert_eq!(
        fixture.host.change_shortcut("Ctrl+Alt+K").unwrap_err(),
        CommandError::ShortcutUnavailable
    );
    let snapshot = fixture.host.snapshot();
    assert_eq!(snapshot.shortcut, "Ctrl+Alt+L");
    assert!(snapshot.shortcut_available);
    let registered = fixture.hotkeys.registered.lock().unwrap();
    assert!(registered.contains("Ctrl+Alt+L"));
    assert!(!registered.contains("Ctrl+Alt+K"));
}

#[test]
fn native_lock_failure_does_not_commit_and_retry_succeeds() {
    let fixture = Fixture::new([available_inventory()]);
    fixture
        .services
        .controller
        .enable_tray(fixture.sink.as_ref())
        .unwrap();
    *fixture.native.fail_next.lock().unwrap() = true;
    assert_eq!(
        fixture
            .host
            .execute(OverlayCommand::LockWindow)
            .unwrap_err(),
        CommandError::WindowUnavailable
    );
    let snapshot = fixture.host.snapshot();
    assert_eq!(snapshot.window_mode, WindowMode::Edit);
    assert!(snapshot.can_lock);
    assert!(!*fixture.native.click_through.lock().unwrap());
    while let Ok(event) = fixture._events.try_recv() {
        if let HostEvent::Snapshot(snapshot) = event {
            assert_ne!(snapshot.window_mode, WindowMode::Locked);
        }
    }

    let locked = fixture.host.execute(OverlayCommand::LockWindow).unwrap();
    assert_eq!(locked.window_mode, WindowMode::Locked);
    assert!(*fixture.native.click_through.lock().unwrap());
}

#[test]
fn close_to_tray_persists_geometry_without_stopping_services() {
    let fixture = Fixture::new([available_inventory()]);
    let geometry = SavedGeometry {
        monitor: Some("test-monitor".into()),
        relative_x: 0.25,
        relative_y: 0.75,
        width: 800.0,
        height: 160.0,
    };
    fixture.host.close_to_tray(geometry.clone()).unwrap();
    let saved = PreferenceStore::load(config_path(&fixture.root))
        .snapshot()
        .unwrap()
        .geometry
        .unwrap();
    assert_eq!(saved.monitor, geometry.monitor);
    assert_eq!(saved.relative_x, geometry.relative_x);
    assert_eq!(saved.relative_y, geometry.relative_y);
    assert_eq!(saved.width, geometry.width);
    assert_eq!(saved.height, geometry.height);
    assert_eq!(fixture.host.snapshot().capture, CaptureState::Stopped);
}

#[test]
fn quit_orders_dictation_sessions_and_geometry_save() {
    let root = isolated_root();
    let store = PreferenceStore::load(config_path(&root));
    let order = RefCell::new(Vec::new());
    ordered_shutdown(
        || order.borrow_mut().push("dictation"),
        || order.borrow_mut().push("sessions"),
        || {
            order.borrow_mut().push("geometry");
            store.save_geometry(SavedGeometry {
                monitor: None,
                relative_x: 0.0,
                relative_y: 1.0,
                width: 640.0,
                height: 160.0,
            })
        },
    )
    .unwrap();
    assert_eq!(order.into_inner(), ["dictation", "sessions", "geometry"]);
    assert!(
        PreferenceStore::load(config_path(&root))
            .snapshot()
            .unwrap()
            .geometry
            .is_some()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn update_gates_reject_unavailable_download_busy_and_update_busy() {
    let installer = UpdateInstaller::default();
    assert_eq!(
        update_install_gate(false, true, &installer),
        Err(CommandError::UpdateUnavailable)
    );
    assert_eq!(
        update_install_gate(true, true, &installer),
        Err(CommandError::DownloadBusy)
    );
    assert!(installer.begin());
    assert_eq!(
        update_install_gate(true, false, &installer),
        Err(CommandError::UpdateBusy)
    );
    installer.finish();
    update_install_gate(true, false, &installer).unwrap();
    installer.finish();
}

#[test]
fn second_instance_activation_restores_edit_mode() {
    let fixture = Fixture::new([available_inventory()]);
    fixture
        .services
        .controller
        .enable_tray(fixture.sink.as_ref())
        .unwrap();
    fixture.host.set_window_mode(WindowMode::Locked).unwrap();
    fixture.host.activate().unwrap();
    assert_eq!(fixture.host.snapshot().window_mode, WindowMode::Edit);
}

#[test]
fn download_catalog_keeps_real_model_ids_and_gate_inputs() {
    let fixture = Fixture::new([available_inventory()]);
    fixture
        .services
        .controller
        .set_resources(
            Resources {
                audio_capture: true,
                transcription: true,
                translation: false,
                speaker_labels: false,
                active_asr_profile: None,
            },
            fixture.sink.as_ref(),
        )
        .unwrap();
    let models = fixture.services.model_downloads();
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].model, catalog::RIVA_TRANSLATE_Q4.id);
    assert_eq!(models[1].model, catalog::NEMOTRON_3_DIARIZATION_Q8.id);
    assert_eq!(models[2].model, catalog::NEMOTRON_STREAMING_EN_Q8.id);
    assert_eq!(models[2].total_bytes, download::ENGLISH_ASR_DOWNLOAD_BYTES);
    assert!(models.iter().all(|model| !model.downloading));
}

#[gpui::test]
fn settings_open_uses_authoritative_snapshot_after_overlay_race(cx: &mut TestAppContext) {
    let fixture = Fixture::new([available_inventory()]);
    let stale = fixture.host.snapshot();
    fixture
        .services
        .controller
        .update(fixture.sink.as_ref(), |state| {
            state.set_resources(Resources {
                audio_capture: true,
                transcription: true,
                translation: true,
                speaker_labels: true,
                active_asr_profile: None,
            })?;
            state.set_ui_language(UiLanguage::from_tag("pt-br").unwrap())
        })
        .unwrap();
    let expected = fixture.host.snapshot();
    let (_root, cx) = settings_window(stale.clone(), cx);
    let (_view, panel) = open_panel(&fixture.host, &stale, cx);
    let actual = read_panel(&panel, cx, |view| view.application().snapshot().clone());
    assert_eq!(actual.revision, expected.revision);
    assert_eq!(actual.ui_language, expected.ui_language);
    assert_eq!(actual.resources, expected.resources);
}

#[gpui::test]
fn settings_reopen_preserves_check_progress_failures_and_single_flight(cx: &mut TestAppContext) {
    let fixture = Fixture::new([available_inventory()]);
    let snapshot = fixture.host.snapshot();
    let (_root, cx) = settings_window(snapshot.clone(), cx);
    assert!(fixture.host.retain_update_check_start());
    let (view, panel) = open_panel(&fixture.host, &snapshot, cx);
    assert_eq!(
        read_panel(&panel, cx, |view| view.updates().status().clone()),
        UpdateStatus::Checking
    );
    close_panel(&fixture.host, view, &panel, cx);

    let (view, panel) = open_panel(&fixture.host, &snapshot, cx);
    assert_eq!(
        read_panel(&panel, cx, |view| view.updates().status().clone()),
        UpdateStatus::Checking
    );
    assert!(!fixture.host.retain_update_check_start());
    let check = UpdateCheck {
        current: "0.1.0".into(),
        latest: "9.9.9".into(),
        available: true,
        installable: true,
    };
    cx.update(|window, cx| {
        fixture
            .host
            .handle_update_check(UpdateDestination::Settings(0), Ok(check), window, cx);
    });
    assert_eq!(
        read_panel(&panel, cx, |view| {
            view.updates().available_check().unwrap().latest.clone()
        }),
        "9.9.9"
    );
    close_panel(&fixture.host, view, &panel, cx);

    cx.update(|window, cx| {
        fixture.host.handle_global_settings_event(
            HostEvent::UpdateProgress(UpdateProgress {
                phase: UpdatePhase::Downloading,
                received_bytes: 25,
                total_bytes: 100,
                reason: None,
            }),
            window,
            cx,
        );
    });
    let (view, panel) = open_panel(&fixture.host, &snapshot, cx);
    let action = read_panel(&panel, cx, |view| view.update_action_view().unwrap());
    assert!(matches!(
        action.state,
        UpdateActionState::Progress(UpdatePhase::Downloading)
    ));
    assert_eq!(action.percent, Some(25));
    close_panel(&fixture.host, view, &panel, cx);

    cx.update(|window, cx| {
        fixture.host.handle_global_settings_event(
            HostEvent::UpdateProgress(UpdateProgress {
                phase: UpdatePhase::Failed,
                received_bytes: 0,
                total_bytes: 0,
                reason: None,
            }),
            window,
            cx,
        );
    });
    let (_view, panel) = open_panel(&fixture.host, &snapshot, cx);
    let action = read_panel(&panel, cx, |view| view.update_action_view().unwrap());
    assert_eq!(action.state, UpdateActionState::OpenReleasesPage);
    assert!(action.failed);
}

#[gpui::test]
fn settings_reopen_seeds_terminal_model_download_failure(cx: &mut TestAppContext) {
    let fixture = Fixture::new([available_inventory()]);
    let snapshot = fixture.host.snapshot();
    let (_root, cx) = settings_window(snapshot.clone(), cx);
    let (view, panel) = open_panel(&fixture.host, &snapshot, cx);
    close_panel(&fixture.host, view, &panel, cx);

    cx.update(|window, cx| {
        fixture.host.handle_global_settings_event(
            HostEvent::Download(DownloadUpdate {
                model: catalog::NEMOTRON_3_DIARIZATION_Q8.id,
                phase: DownloadPhase::Failed,
                received_bytes: 0,
                total_bytes: None,
                reason: Some(DownloadFailure::Network),
            }),
            window,
            cx,
        );
    });
    let (_view, panel) = open_panel(&fixture.host, &snapshot, cx);
    cx.update(|window, cx| {
        let models = fixture.services.model_downloads();
        panel.update(cx, |view, cx| {
            view.accept_models(models, window, cx);
        });
    });
    let download = read_panel(&panel, cx, |view| {
        view.download_view(DownloadRow::Diarization).unwrap()
    });
    assert!(download.failed);
}

#[gpui::test]
fn settings_reopen_retains_successful_download_before_first_backend_event(cx: &mut TestAppContext) {
    let fixture = Fixture::new([available_inventory()]);
    let snapshot = fixture.host.snapshot();
    let (_root, cx) = settings_window(snapshot.clone(), cx);
    let (view, panel) = open_panel(&fixture.host, &snapshot, cx);
    cx.update(|window, cx| {
        fixture.host.handle_global_settings_event(
            HostEvent::Download(DownloadUpdate {
                model: catalog::NEMOTRON_3_DIARIZATION_Q8.id,
                phase: DownloadPhase::Failed,
                received_bytes: 0,
                total_bytes: None,
                reason: Some(DownloadFailure::Network),
            }),
            window,
            cx,
        );
    });
    close_panel(&fixture.host, view, &panel, cx);

    // A successful start has no backend event until the network responds.
    fixture
        .host
        .retain_download_start(catalog::NEMOTRON_3_DIARIZATION_Q8.id);
    let (_view, panel) = open_panel(&fixture.host, &snapshot, cx);
    cx.update(|window, cx| {
        let models = fixture.services.model_downloads();
        panel.update(cx, |view, cx| {
            view.accept_models(models, window, cx);
        });
    });
    let download = read_panel(&panel, cx, |view| {
        view.download_view(DownloadRow::Diarization).unwrap()
    });
    assert!(download.downloading);
    assert!(download.progress_visible);
    assert!(!download.failed);
}
