use std::{
    env, io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anycaptions_core::{
    audio::devices::{InputService, OutputService, WasapiCatalog},
    dictation::{self, indicator::IndicatorSnapshot, runtime::DictationState},
    ipc::{CommandError, Controller, ShortcutRegistry},
    preferences::{PreferenceStore, Preferences, config_path},
    transcription::session::CaptionSessionManager,
    updater,
    window_geometry::WorkArea,
};
use anycaptions_platform_win32::{
    HotkeyService, SingleInstance, TrayService, WindowEffects, work_areas,
};
use anycaptions_ui::{
    assets::{self, AssetSource},
    dictation::open_dictation_window,
    overlay::{
        OverlayEvent, OverlayHost, OverlayView, UiEventReceiver, link_overlay_indicator,
        open_overlay_window, ui_event_channels,
    },
    widgets,
};
use async_channel::{Receiver, Sender, unbounded};
use gpui::{App, Application, WindowHandle};
use windows::{
    Win32::{
        Foundation::HINSTANCE,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{HICON, LoadIconW},
    },
    core::PCWSTR,
};

use crate::{
    adapters::{HostEvent, HostSessionSink, UpdateDestination},
    host::{Host, Win32WindowEffects},
    services::Services,
};

const INSTANCE_NAME: &str = "Local\\AnyCaptions.GPUI.SingleInstance";
type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

struct Bootstrap {
    store: PreferenceStore,
    saved: Preferences,
    sessions: CaptionSessionManager,
    controller: Controller,
}

struct Composed {
    services: Arc<Services>,
    controller: Controller,
    sink: Arc<HostSessionSink>,
    hotkeys: Arc<HotkeyService>,
    saved: Preferences,
    overlay_events: UiEventReceiver<OverlayEvent>,
    indicator_events: UiEventReceiver<IndicatorSnapshot>,
}

struct Startup {
    services: Arc<Services>,
    host: Host,
    saved: Preferences,
    instance: SingleInstance,
    tray: TrayService,
    areas: Option<Vec<WorkArea>>,
    overlay_events: Option<UiEventReceiver<OverlayEvent>>,
    indicator_events: Option<UiEventReceiver<IndicatorSnapshot>>,
    event_rx: Receiver<HostEvent>,
}

fn acquire_instance(events: &Sender<HostEvent>) -> windows::core::Result<Option<SingleInstance>> {
    let activation = events.clone();
    SingleInstance::acquire(INSTANCE_NAME, move || {
        let _ = activation.try_send(HostEvent::Activate);
    })
}

fn cleanup_update_leftovers() {
    if let Some(directory) = env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
    {
        updater::cleanup_leftovers(&directory);
    }
}

fn config_root() -> io::Result<PathBuf> {
    env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|root| root.join("com.anycaptions.app"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA is unavailable"))
}

fn restore_bootstrap() -> AppResult<Bootstrap> {
    let store = PreferenceStore::load(config_path(&config_root()?));
    let sessions = CaptionSessionManager::default_native();
    let saved = store.snapshot()?;
    let engine = sessions.startup_engine(saved.translation_engine);
    let saved = saved.with_translation_engine(engine);
    let controller = Controller::default();
    controller
        .restore_preferences(&saved)
        .map_err(|_| io::Error::other("Cannot restore preferences"))?;
    Ok(Bootstrap {
        store,
        saved,
        sessions,
        controller,
    })
}

fn compose_services(bootstrap: Bootstrap, event_tx: &Sender<HostEvent>) -> AppResult<Composed> {
    let Bootstrap {
        store,
        saved,
        sessions,
        controller,
    } = bootstrap;
    let outputs = Arc::new(OutputService::new(
        Box::new(WasapiCatalog::outputs()),
        saved.output_id.clone(),
    ));
    let inputs = Arc::new(InputService::new(
        Box::new(WasapiCatalog::inputs()),
        saved.dictation.input_id.clone(),
    ));
    let (ui_sink, overlay_events, indicator_events) = ui_event_channels(controller.clone());
    let sink = Arc::new(HostSessionSink::new(ui_sink, event_tx.clone()));
    let hotkey_events = event_tx.clone();
    let hotkeys = Arc::new(HotkeyService::new(move || {
        let _ = hotkey_events.try_send(HostEvent::Activate);
    })?);
    let dictation = Arc::new(DictationState::new(
        sessions.factory(),
        Arc::clone(&inputs),
        saved.dictation.clone(),
        controller.clone(),
        hotkeys.clone(),
        sink.clone(),
    ));
    let services = Arc::new(Services::new(
        controller.clone(),
        store,
        sessions,
        outputs,
        inputs,
        dictation,
    ));
    Ok(Composed {
        services,
        controller,
        sink,
        hotkeys,
        saved,
        overlay_events,
        indicator_events,
    })
}

fn setup_recovery(composed: &Composed, event_tx: &Sender<HostEvent>) -> AppResult<TrayService> {
    // Resource state is established before recovery and any window can start.
    let _ = composed.services.refresh_resources(composed.sink.as_ref());
    let snapshot = composed
        .controller
        .snapshot()
        .map_err(|error| io::Error::other(format!("Cannot read application state: {error:?}")))?;
    let tray = TrayService::new(
        load_application_icon()?,
        "AnyCaptions",
        snapshot.resources.audio_capture && snapshot.resources.transcription,
        {
            let events = event_tx.clone();
            move |command| {
                let _ = events.try_send(HostEvent::Tray(command));
            }
        },
    )?;
    match composed.controller.enable_tray(composed.sink.as_ref()) {
        Ok(_) | Err(CommandError::EventUnavailable) => {}
        Err(error) => return Err(io::Error::other(format!("Cannot enable tray: {error:?}")).into()),
    }
    let shortcut = snapshot.shortcut.clone();
    let shortcut_available = composed.hotkeys.register(&shortcut).is_ok();
    match composed
        .controller
        .set_shortcut(shortcut, shortcut_available, composed.sink.as_ref())
    {
        Ok(_) | Err(CommandError::EventUnavailable) => {}
        Err(error) => {
            return Err(io::Error::other(format!("Cannot configure shortcut: {error:?}")).into());
        }
    }
    dictation::runtime::setup(&composed.services.dictation);
    Ok(tray)
}

impl Startup {
    fn new(
        instance: SingleInstance,
        event_tx: Sender<HostEvent>,
        event_rx: Receiver<HostEvent>,
    ) -> AppResult<Self> {
        let bootstrap = restore_bootstrap()?;
        let composed = compose_services(bootstrap, &event_tx)?;
        let tray = setup_recovery(&composed, &event_tx)?;
        let host = Host::new(
            Arc::clone(&composed.services),
            composed.sink,
            composed.hotkeys,
            event_tx,
        );
        let areas = work_areas()?;
        Ok(Self {
            services: composed.services,
            host,
            saved: composed.saved,
            instance,
            tray,
            areas: Some(areas),
            overlay_events: Some(composed.overlay_events),
            indicator_events: Some(composed.indicator_events),
            event_rx,
        })
    }

    fn attach_linked_windows(
        &mut self,
        cx: &mut App,
        overlay: WindowHandle<OverlayView>,
    ) -> gpui::Result<()> {
        let effects = overlay.update(cx, |_, window, _| -> gpui::Result<WindowEffects> {
            let raw = raw_window_handle::HasWindowHandle::window_handle(window)
                .map_err(|error| io::Error::other(error.to_string()))?;
            Ok(WindowEffects::from_raw_handle(raw.into())?)
        })??;
        self.host
            .attach_native_effects(Arc::new(Win32WindowEffects::new(&effects)));
        let (anchor, work_area) =
            overlay.update(cx, |view, _, _| (view.geometry(), view.work_area()))?;
        let indicator = open_dictation_window(
            cx,
            self.indicator_events
                .take()
                .expect("indicator receiver present"),
            anchor,
            work_area,
        )?;
        link_overlay_indicator(&overlay, &indicator, cx);
        Ok(())
    }

    fn open_main_window(&mut self, cx: &mut App) -> gpui::Result<WindowHandle<OverlayView>> {
        assets::register_fonts(cx)?;
        widgets::bind_keys(cx);
        let overlay_host: Arc<dyn OverlayHost> = Arc::new(self.host.clone());
        let overlay = open_overlay_window(
            cx,
            overlay_host,
            self.host.snapshot(),
            self.overlay_events
                .take()
                .expect("overlay receiver present"),
            self.saved.geometry.as_ref(),
            self.areas.take().expect("work areas present"),
        )?;
        self.host.attach_overlay(overlay);
        self.attach_linked_windows(cx, overlay)?;
        let check_updates = overlay.update(cx, |view, _, cx| view.begin_update_check(cx))?;
        if check_updates {
            self.host.check_updates(UpdateDestination::Overlay);
        }
        Ok(overlay)
    }

    fn spawn_delivery(self, overlay: WindowHandle<OverlayView>, cx: &mut App) -> gpui::Result<()> {
        let Self {
            instance,
            mut tray,
            event_rx,
            host,
            ..
        } = self;
        let delivery = overlay.update(cx, |_, window, cx| {
            cx.spawn_in(window, async move |_, cx| {
                let _instance = instance;
                while let Ok(event) = event_rx.recv().await {
                    if cx
                        .update(|window, cx| host.handle_event(event, window, cx, &mut tray))
                        .is_err()
                    {
                        break;
                    }
                }
            })
        })?;
        delivery.detach();
        Ok(())
    }

    fn start(mut self, cx: &mut App) -> gpui::Result<()> {
        let overlay = self.open_main_window(cx)?;
        self.spawn_delivery(overlay, cx)
    }
}

pub(crate) fn run() -> AppResult<()> {
    let (event_tx, event_rx) = unbounded();
    let Some(instance) = acquire_instance(&event_tx)? else {
        return Ok(());
    };
    cleanup_update_leftovers();
    let startup = Startup::new(instance, event_tx, event_rx)?;
    let startup_error = Arc::new(Mutex::new(None));
    let ui_error = Arc::clone(&startup_error);
    Application::new()
        .with_assets(AssetSource)
        .run(move |cx: &mut App| {
            let services = startup.services.clone();
            if let Err(error) = startup.start(cx) {
                *ui_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.to_string());
                let _ = services.shutdown(None);
                cx.quit();
            }
        });

    let startup_error = startup_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(error) = startup_error {
        return Err(io::Error::other(error).into());
    }
    Ok(())
}

fn load_application_icon() -> windows::core::Result<HICON> {
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
    // Resource 1 is emitted by build.rs from backend/icons/icon.ico.
    unsafe { LoadIconW(Some(instance), PCWSTR(1 as _)) }
}
