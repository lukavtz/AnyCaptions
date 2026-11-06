use anycaptions_core::{
    audio::devices::DeviceSelection,
    dictation::runtime::{DictationEventPayload, DictationEventSink},
    domain::AppSnapshot,
    ipc::{CaptionUpdate, CommandError, StateEvents},
    models::download::{DownloadSink, DownloadUpdate},
    presentation::downloads::ModelDownloadState,
    transcription::session::SessionSink,
    update::{UpdateCheck, UpdateError},
    updater::{UpdateProgress, UpdateSink},
};
use anycaptions_platform_win32::TrayCommand;
use anycaptions_ui::{overlay::UiEventSink, settings::DictationLoad};
use async_channel::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateDestination {
    Overlay,
    Settings(u64),
}

/// Plain, owned events are the only values crossing from native/core threads
/// into GPUI. No entity, window, executor, or raw HWND is captured by a sink.
pub(crate) enum HostEvent {
    Snapshot(AppSnapshot),
    Tray(TrayCommand),
    Activate,
    Outputs {
        generation: u64,
        selected: bool,
        result: Result<DeviceSelection, CommandError>,
    },
    Dictation {
        generation: u64,
        result: Result<DictationLoad, CommandError>,
    },
    Models {
        generation: u64,
        models: Vec<ModelDownloadState>,
    },
    Download(DownloadUpdate),
    UpdateCheck {
        destination: UpdateDestination,
        result: Result<UpdateCheck, UpdateError>,
    },
    UpdateProgress(UpdateProgress),
    Restart,
}

pub(crate) struct HostSessionSink {
    ui: UiEventSink,
    events: Sender<HostEvent>,
}

impl HostSessionSink {
    pub(crate) fn new(ui: UiEventSink, events: Sender<HostEvent>) -> Self {
        Self { ui, events }
    }
}

impl StateEvents for HostSessionSink {
    fn state_changed(&self, state: &AppSnapshot) -> Result<(), CommandError> {
        let delivered = self.ui.state_changed(state);
        let _ = self.events.try_send(HostEvent::Snapshot(state.clone()));
        delivered
    }
}

impl SessionSink for HostSessionSink {
    fn caption_updated(&self, update: &CaptionUpdate) -> Result<(), CommandError> {
        self.ui.caption_updated(update)
    }
}

impl DictationEventSink for HostSessionSink {
    fn emit(&self, payload: &DictationEventPayload) {
        self.ui.emit(payload);
    }
}

pub(crate) struct HostDownloadSink(pub(crate) Sender<HostEvent>);

impl DownloadSink for HostDownloadSink {
    fn download_changed(&self, update: &DownloadUpdate) {
        let _ = self.0.try_send(HostEvent::Download(update.clone()));
    }
}

pub(crate) struct HostUpdateSink(pub(crate) Sender<HostEvent>);

impl UpdateSink for HostUpdateSink {
    fn update_changed(&self, progress: &UpdateProgress) {
        let _ = self.0.try_send(HostEvent::UpdateProgress(progress.clone()));
    }
}
