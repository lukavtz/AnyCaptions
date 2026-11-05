use std::{sync::Arc, thread};

use anycaptions_core::{
    audio::devices::{DeviceSelection, InputService, OutputService},
    dictation::{self, DictationSettings, runtime::DictationState},
    domain::{AppSnapshot, AppState, Appearance, DomainError, TranslationEngine, WindowMode},
    ipc::{
        CommandError, Controller, ShortcutRegistry, StateEvents, start_live_capture,
        stop_live_capture,
    },
    models::{
        catalog,
        download::{self, DownloadManager},
    },
    preferences::{PreferenceStore, SavedGeometry},
    presentation::{application::Action, downloads::ModelDownloadState},
    transcription::session::{CaptionSessionManager, SessionSink},
    updater::{self, UpdateInstaller},
};
use async_channel::Sender;

use crate::adapters::{HostEvent, HostUpdateSink};

pub(crate) struct DictationSnapshot {
    pub(crate) settings: DictationSettings,
    pub(crate) available: bool,
    pub(crate) inputs: DeviceSelection,
}

pub(crate) struct Services {
    pub(crate) controller: Controller,
    pub(crate) store: PreferenceStore,
    pub(crate) sessions: CaptionSessionManager,
    pub(crate) outputs: Arc<OutputService>,
    pub(crate) inputs: Arc<InputService>,
    pub(crate) dictation: Arc<DictationState>,
    pub(crate) downloads: Arc<DownloadManager>,
    pub(crate) installer: Arc<UpdateInstaller>,
}

impl Services {
    pub(crate) fn new(
        controller: Controller,
        store: PreferenceStore,
        sessions: CaptionSessionManager,
        outputs: Arc<OutputService>,
        inputs: Arc<InputService>,
        dictation: Arc<DictationState>,
    ) -> Self {
        Self {
            controller,
            store,
            sessions,
            outputs,
            inputs,
            dictation,
            downloads: Arc::new(DownloadManager::default()),
            installer: Arc::new(UpdateInstaller::default()),
        }
    }

    pub(crate) fn refresh_resources(
        &self,
        events: &impl StateEvents,
    ) -> Result<AppSnapshot, CommandError> {
        self.sessions
            .refresh_resources(&self.controller, &self.outputs, events)
    }

    pub(crate) fn start(
        &self,
        events: &impl StateEvents,
        sink: Arc<dyn SessionSink>,
    ) -> Result<AppSnapshot, CommandError> {
        start_live_capture(
            &self.sessions,
            &self.outputs,
            &self.controller,
            events,
            sink,
        )
    }

    pub(crate) fn stop(&self, events: &impl StateEvents) -> Result<AppSnapshot, CommandError> {
        stop_live_capture(&self.sessions, &self.controller, events)
    }

    fn persist(&self, snapshot: &AppSnapshot) -> Result<(), CommandError> {
        self.store
            .sync_snapshot(snapshot)
            .map_err(|_| CommandError::PreferencesUnavailable)
    }

    fn persist_update(
        &self,
        events: &impl StateEvents,
        action: impl FnOnce(&mut AppState) -> Result<(), DomainError>,
    ) -> Result<AppSnapshot, CommandError> {
        let snapshot = self.controller.update(events, action)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn change_session(
        &self,
        events: &impl StateEvents,
        refresh_resources: bool,
        action: impl FnOnce(&mut AppState) -> Result<(), DomainError>,
    ) -> Result<AppSnapshot, CommandError> {
        self.sessions.stop_session();
        let snapshot = self.persist_update(events, action)?;
        if refresh_resources {
            self.refresh_resources(events)
        } else {
            Ok(snapshot)
        }
    }

    fn apply_overlay_action(
        &self,
        action: Action,
        events: &impl StateEvents,
        sink: Arc<dyn SessionSink>,
    ) -> Result<AppSnapshot, CommandError> {
        match action {
            Action::StartCapture => self.start(events, sink),
            Action::StopCapture => self.stop(events),
            Action::LockWindow => self
                .controller
                .change_window_mode(WindowMode::Locked, events),
            _ => unreachable!("overlay action category"),
        }
    }

    fn apply_service_action(
        &self,
        action: Action,
        events: &impl StateEvents,
    ) -> Result<AppSnapshot, CommandError> {
        match action {
            Action::SetMode(mode) => {
                self.change_session(events, false, |state| state.set_mode(mode))
            }
            Action::SetTranslationEngine(engine) => {
                self.change_session(events, true, |state| state.set_translation_engine(engine))
            }
            Action::SetSourceLanguage(language) => {
                self.change_session(events, false, |state| state.set_source_language(language))
            }
            Action::SetSpeakerLabels(enabled) => {
                self.change_session(events, true, |state| state.set_speaker_labels(enabled))
            }
            Action::SetUpdateCheck(enabled) => {
                self.persist_update(events, |state| state.set_update_check(enabled))
            }
            Action::SetUiLanguage(language) => {
                self.persist_update(events, |state| state.set_ui_language(language))
            }
            Action::SaveSettings => Err(CommandError::Domain(DomainError::InvalidTransition)),
            _ => unreachable!("settings service action category"),
        }
    }

    pub(crate) fn apply_action(
        &self,
        action: Action,
        events: &impl StateEvents,
        sink: Arc<dyn SessionSink>,
        _shortcuts: &dyn ShortcutRegistry,
    ) -> Result<AppSnapshot, CommandError> {
        match action {
            Action::StartCapture | Action::StopCapture | Action::LockWindow => {
                self.apply_overlay_action(action, events, sink)
            }
            action => self.apply_service_action(action, events),
        }
    }

    pub(crate) fn set_window_mode(
        &self,
        mode: WindowMode,
        events: &impl StateEvents,
    ) -> Result<AppSnapshot, CommandError> {
        self.controller.change_window_mode(mode, events)
    }

    pub(crate) fn set_appearance(
        &self,
        appearance: Appearance,
        events: &impl StateEvents,
    ) -> Result<AppSnapshot, CommandError> {
        self.persist_update(events, |state| state.set_appearance(appearance))
    }

    pub(crate) fn set_shortcut(
        &self,
        requested: &str,
        events: &impl StateEvents,
        shortcuts: &dyn ShortcutRegistry,
    ) -> Result<AppSnapshot, CommandError> {
        let normalized = AppState::normalize_shortcut(requested)?;
        if normalized.eq_ignore_ascii_case(&self.dictation.settings().shortcut) {
            return Err(CommandError::ShortcutUnavailable);
        }
        let snapshot = self
            .controller
            .change_shortcut(shortcuts, events, &normalized)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    pub(crate) fn set_dictation(
        &self,
        mut settings: DictationSettings,
    ) -> Result<DictationSettings, CommandError> {
        settings.shortcut = dictation::normalize_shortcut(&settings.shortcut)?;
        settings.validate(&self.controller.snapshot()?.shortcut)?;
        dictation::runtime::apply_settings(&self.dictation, settings.clone())?;
        self.store
            .save_dictation(settings.clone())
            .map_err(|_| CommandError::PreferencesUnavailable)?;
        Ok(settings)
    }

    pub(crate) fn output_selection(
        &self,
        events: &impl StateEvents,
    ) -> Result<DeviceSelection, CommandError> {
        let selection = self.outputs.selection()?;
        let _ = self.refresh_resources(events);
        Ok(selection)
    }

    pub(crate) fn select_output(
        &self,
        id: &str,
        events: &impl StateEvents,
    ) -> Result<DeviceSelection, CommandError> {
        let selection = self.outputs.select(id)?;
        self.store
            .save_output_id(id.to_owned())
            .map_err(|_| CommandError::PreferencesUnavailable)?;
        let _ = self.stop(events);
        let _ = self.refresh_resources(events);
        Ok(selection)
    }

    pub(crate) fn dictation_snapshot(&self) -> Result<DictationSnapshot, CommandError> {
        Ok(DictationSnapshot {
            settings: self.dictation.settings(),
            available: self.dictation.available(),
            inputs: self.inputs.selection()?,
        })
    }

    pub(crate) fn model_downloads(&self) -> Vec<ModelDownloadState> {
        vec![
            ModelDownloadState {
                model: catalog::RIVA_TRANSLATE_Q4.id,
                kind: catalog::RIVA_TRANSLATE_Q4.kind,
                engine: Some(TranslationEngine::Riva),
                installed: self.sessions.translation_available(TranslationEngine::Riva),
                downloadable: download::riva_source_configured(),
                downloading: self.downloads.active_model() == Some(catalog::RIVA_TRANSLATE_Q4.id),
                total_bytes: download::RIVA_DOWNLOAD_BYTES,
            },
            ModelDownloadState {
                model: catalog::NEMOTRON_3_DIARIZATION_Q8.id,
                kind: catalog::NEMOTRON_3_DIARIZATION_Q8.kind,
                engine: None,
                installed: self.sessions.speaker_labels_available(),
                downloadable: download::source_configured(catalog::NEMOTRON_3_DIARIZATION_Q8.id),
                downloading: self.downloads.active_model()
                    == Some(catalog::NEMOTRON_3_DIARIZATION_Q8.id),
                total_bytes: download::DIARIZATION_DOWNLOAD_BYTES,
            },
        ]
    }

    pub(crate) fn install_update(&self, events: Sender<HostEvent>) -> Result<(), CommandError> {
        update_install_gate(
            updater::installable(),
            self.downloads.active_model().is_some(),
            &self.installer,
        )?;
        let installer = Arc::clone(&self.installer);
        thread::spawn(move || {
            let installed = updater::run(&HostUpdateSink(events.clone())).is_ok();
            installer.finish();
            if installed {
                let _ = events.try_send(HostEvent::Restart);
            }
        });
        Ok(())
    }

    pub(crate) fn save_geometry(&self, geometry: SavedGeometry) -> Result<(), CommandError> {
        self.store
            .save_geometry(geometry)
            .map_err(|_| CommandError::PreferencesUnavailable)
    }

    pub(crate) fn shutdown(&self, geometry: Option<SavedGeometry>) -> Result<(), CommandError> {
        ordered_shutdown(
            || dictation::runtime::shutdown(&self.dictation),
            || self.sessions.stop_and_join(),
            || match geometry {
                Some(geometry) => self.save_geometry(geometry),
                None => Ok(()),
            },
        )
    }
}

pub(crate) fn update_install_gate(
    installable: bool,
    download_active: bool,
    installer: &UpdateInstaller,
) -> Result<(), CommandError> {
    if !installable {
        return Err(CommandError::UpdateUnavailable);
    }
    if download_active {
        return Err(CommandError::DownloadBusy);
    }
    if !installer.begin() {
        return Err(CommandError::UpdateBusy);
    }
    Ok(())
}

pub(crate) fn ordered_shutdown<E>(
    stop_dictation: impl FnOnce(),
    stop_sessions: impl FnOnce(),
    save_geometry: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    stop_dictation();
    stop_sessions();
    save_geometry()
}
