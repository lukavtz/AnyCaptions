use crate::{
    dictation::DictationSettings,
    domain::{
        AppSnapshot, Appearance, CaptionMode, CaptureState, SourceLanguage, TranslationEngine,
        UiLanguage, WindowMode,
    },
    ipc::{CommandError, Controller},
};

use super::appearance::same_appearance;

/// Snapshot-producing overlay actions. UI-only gates stay here so every host
/// gets the same single-flight behavior as the React application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    StartCapture,
    StopCapture,
    LockWindow,
    SetMode(CaptionMode),
    SetTranslationEngine(TranslationEngine),
    SetSourceLanguage(Option<SourceLanguage>),
    SetSpeakerLabels(bool),
    SetUpdateCheck(bool),
    SetUiLanguage(UiLanguage),
    SaveSettings,
}

/// Reads the authoritative state after a command rejection.
pub trait SnapshotRecovery {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError>;
}

impl SnapshotRecovery for Controller {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        self.snapshot()
    }
}

/// The three ordered writes used by Settings → Save.
pub trait SettingsSave: SnapshotRecovery {
    fn set_appearance(&mut self, appearance: Appearance) -> Result<AppSnapshot, CommandError>;
    fn set_shortcut(&mut self, shortcut: &str) -> Result<AppSnapshot, CommandError>;
    fn set_dictation(
        &mut self,
        settings: DictationSettings,
    ) -> Result<DictationSettings, CommandError>;
}

/// Drafts are independent from incoming snapshots until Save succeeds or the
/// panel is cancelled. The confirmed dictation value is the comparison base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDraft {
    pub appearance: Appearance,
    pub shortcut: String,
    pub dictation: Option<DictationSettings>,
    confirmed_dictation: Option<DictationSettings>,
}

impl SettingsDraft {
    fn new(snapshot: &AppSnapshot) -> Self {
        Self {
            appearance: snapshot.appearance,
            shortcut: snapshot.shortcut.clone(),
            dictation: None,
            confirmed_dictation: None,
        }
    }
}

pub struct ApplicationModel {
    snapshot: AppSnapshot,
    pending: Option<Action>,
    error: Option<CommandError>,
    draft: Option<SettingsDraft>,
}

impl ApplicationModel {
    pub fn new(snapshot: AppSnapshot) -> Self {
        Self {
            snapshot,
            pending: None,
            error: None,
            draft: None,
        }
    }

    pub fn snapshot(&self) -> &AppSnapshot {
        &self.snapshot
    }

    /// Equal revisions are intentionally accepted; only older snapshots lose.
    pub fn accept(&mut self, incoming: AppSnapshot) -> bool {
        if incoming.revision < self.snapshot.revision {
            return false;
        }
        let entering_locked = self.snapshot.window_mode != WindowMode::Locked
            && incoming.window_mode == WindowMode::Locked;
        self.snapshot = incoming;
        if entering_locked {
            self.draft = None;
        }
        true
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn pending(&self) -> Option<&Action> {
        self.pending.as_ref()
    }

    pub fn error(&self) -> Option<CommandError> {
        self.error
    }

    pub fn report(&mut self, error: CommandError) {
        self.error = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn try_begin(&mut self, action: Action) -> bool {
        if self.pending.is_some() || !self.action_allowed(&action) {
            return false;
        }
        self.pending = Some(action);
        self.error = None;
        true
    }

    fn action_allowed(&self, action: &Action) -> bool {
        match action {
            Action::StartCapture | Action::StopCapture | Action::LockWindow => {
                self.window_action_allowed(action)
            }
            Action::SetMode(mode) => self.mode_allowed(*mode),
            _ => self.setting_action_allowed(action),
        }
    }

    fn window_action_allowed(&self, action: &Action) -> bool {
        let state = &self.snapshot;
        match action {
            Action::StartCapture => state.capture == CaptureState::Stopped && state.can_start,
            Action::StopCapture => state.capture != CaptureState::Stopped,
            Action::LockWindow => state.can_lock,
            _ => false,
        }
    }

    fn mode_allowed(&self, mode: CaptionMode) -> bool {
        let state = &self.snapshot;
        if state.window_mode == WindowMode::Locked || mode == state.mode {
            return false;
        }
        match mode {
            CaptionMode::Auto => true,
            CaptionMode::Translate(target) => {
                state.resources.translation && state.translation_engine.supports(target)
            }
        }
    }

    fn setting_action_allowed(&self, action: &Action) -> bool {
        let state = &self.snapshot;
        match action {
            Action::SetSourceLanguage(language) => *language != state.source_language,
            Action::SetSpeakerLabels(enabled) => *enabled != state.speaker_labels,
            Action::SetUpdateCheck(enabled) => *enabled != state.update_check,
            Action::SetUiLanguage(language) => *language != state.ui_language,
            Action::SetTranslationEngine(_) | Action::SaveSettings => true,
            _ => false,
        }
    }

    /// Finishes the pending action. A rejection re-queries authoritative state;
    /// recovery failure preserves the last confirmed snapshot and never hides
    /// the original command error.
    pub fn complete(
        &mut self,
        result: Result<AppSnapshot, CommandError>,
        recovery: &impl SnapshotRecovery,
    ) -> bool {
        if self.pending.take().is_none() {
            return false;
        }
        match result {
            Ok(snapshot) => {
                self.accept(snapshot);
                true
            }
            Err(error) => {
                if let Ok(snapshot) = recovery.recover_snapshot() {
                    self.accept(snapshot);
                }
                self.error = Some(error);
                false
            }
        }
    }

    pub fn open_settings(&mut self) {
        self.draft = Some(SettingsDraft::new(&self.snapshot));
    }

    pub fn cancel_settings(&mut self) {
        self.draft = None;
    }

    pub fn draft(&self) -> Option<&SettingsDraft> {
        self.draft.as_ref()
    }

    pub fn draft_mut(&mut self) -> Option<&mut SettingsDraft> {
        self.draft.as_mut()
    }

    pub fn load_dictation(&mut self, settings: DictationSettings) {
        if let Some(draft) = &mut self.draft {
            draft.confirmed_dictation = Some(settings.clone());
            draft.dictation = Some(settings);
        }
    }

    /// Saves in Appearance → Shortcut → Dictation order. Earlier successful
    /// writes are not rolled back. Any rejection keeps every draft for retry;
    /// only complete success closes the draft session.
    pub fn save_settings(&mut self, backend: &mut impl SettingsSave) -> bool {
        if self.draft.is_none() || !self.try_begin(Action::SaveSettings) {
            return false;
        }
        let result = save_draft(
            self.snapshot.clone(),
            self.draft.as_mut().expect("draft checked above"),
            backend,
        );
        let saved = self.complete(result, backend);
        if saved {
            self.draft = None;
        }
        saved
    }
}

fn save_draft(
    mut saved: AppSnapshot,
    draft: &mut SettingsDraft,
    backend: &mut impl SettingsSave,
) -> Result<AppSnapshot, CommandError> {
    if !same_appearance(&draft.appearance, &saved.appearance) {
        saved = backend.set_appearance(draft.appearance)?;
    }
    if draft.shortcut != saved.shortcut {
        saved = backend.set_shortcut(&draft.shortcut)?;
    }
    if let (Some(requested), Some(confirmed)) = (&draft.dictation, &draft.confirmed_dictation)
        && requested != confirmed
    {
        let mut requested = requested.clone();
        requested.shortcut = requested.shortcut.trim().to_owned();
        let result = backend.set_dictation(requested)?;
        draft.confirmed_dictation = Some(result.clone());
        draft.dictation = Some(result);
    }
    Ok(saved)
}
