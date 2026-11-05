use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::{
    audio::devices::AudioDeviceError,
    dictation::DictationError,
    domain::{AppSnapshot, AppState, CaptureState, DomainError, Resources},
    preferences::Preferences,
    update::UpdateError,
};

/// A run of consecutive words attributed to one speaker slot. `speaker` is the
/// diarizer's 1-based generic slot ("Person 1"); `0` means the slot is not
/// known yet (the diarizer has not covered those words), and the overlay then
/// shows the words without asserting a label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerSpan {
    pub speaker: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionUpdate {
    pub session_id: u32,
    pub segment_id: u32,
    pub revision: u32,
    pub mode: crate::domain::CaptionMode,
    pub text: String,
    pub is_final: bool,
    /// Language detected by the model (BCP-47); only final captions carry it.
    pub language: Option<String>,
    /// Speaker attribution of `text`, in order; the spans' words joined by a
    /// space are the caption's words. Empty when speaker labels are off (or the
    /// diarizer is unavailable), which leaves the caption exactly as before.
    pub speakers: Vec<SpeakerSpan>,
    /// Translate mode only: a provisional translation of the utterance still
    /// being spoken. The final translation of the same segment replaces it.
    pub provisional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "code", rename_all = "kebab-case")]
pub enum CommandError {
    Domain(DomainError),
    StateUnavailable,
    EventUnavailable,
    UnauthorizedWindow,
    WindowUnavailable,
    ShortcutUnavailable,
    PreferencesUnavailable,
    AudioDevice(AudioDeviceError),
    Dictation(DictationError),
    BrowserUnavailable,
    DownloadBusy,
    DownloadUnavailable,
    UpdateBusy,
    UpdateUnavailable,
    Update(UpdateError),
}

impl From<DomainError> for CommandError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

impl From<AudioDeviceError> for CommandError {
    fn from(value: AudioDeviceError) -> Self {
        Self::AudioDevice(value)
    }
}

impl From<DictationError> for CommandError {
    fn from(value: DictationError) -> Self {
        Self::Dictation(value)
    }
}

pub trait StateEvents {
    fn state_changed(&self, state: &AppSnapshot) -> Result<(), CommandError>;
}

pub trait ShortcutRegistry {
    fn register(&self, shortcut: &str) -> Result<(), CommandError>;
    fn unregister(&self, shortcut: &str) -> Result<(), CommandError>;
}

fn replace_shortcut(
    registry: &impl ShortcutRegistry,
    previous: &AppSnapshot,
    requested: &str,
) -> Result<(), CommandError> {
    if requested == previous.shortcut && previous.shortcut_available {
        return Ok(());
    }
    registry.register(requested)?;
    if previous.shortcut_available
        && previous.shortcut != requested
        && registry.unregister(&previous.shortcut).is_err()
    {
        let _ = registry.unregister(requested);
        return Err(CommandError::ShortcutUnavailable);
    }
    Ok(())
}

#[derive(Clone, Default)]
pub struct Controller(Arc<(Mutex<AppState>, Mutex<()>)>);

macro_rules! forward_update {
    ($(#[$meta:meta])* $name:ident($($arg:ident: $ty:ty),* $(,)?), $action:expr) => {
        $(#[$meta])*
        pub fn $name(
            &self,
            $($arg: $ty,)*
            events: &(impl StateEvents + ?Sized),
        ) -> Result<AppSnapshot, CommandError> {
            self.update(events, $action)
        }
    };
}

impl Controller {
    pub fn restore_preferences(&self, preferences: &Preferences) -> Result<(), CommandError> {
        let mut state = self
            .0
            .0
            .lock()
            .map_err(|_| CommandError::StateUnavailable)?;
        // Engine first: a saved translate mode is only valid under its engine.
        state.set_translation_engine(preferences.translation_engine)?;
        state.set_mode(preferences.mode)?;
        state.set_source_language(preferences.source_language)?;
        state.set_speaker_labels(preferences.speaker_labels)?;
        state.set_update_check(preferences.update_check)?;
        state.set_ui_language(preferences.ui_language)?;
        state.set_appearance(preferences.appearance)?;
        state.set_shortcut(preferences.shortcut.clone(), false)?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<AppSnapshot, CommandError> {
        Ok(self
            .0
            .0
            .lock()
            .map_err(|_| CommandError::StateUnavailable)?
            .snapshot())
    }

    pub fn update(
        &self,
        events: &(impl StateEvents + ?Sized),
        action: impl FnOnce(&mut AppState) -> Result<(), DomainError>,
    ) -> Result<AppSnapshot, CommandError> {
        let (snapshot, changed) = {
            let mut state = self
                .0
                .0
                .lock()
                .map_err(|_| CommandError::StateUnavailable)?;
            let previous_revision = state.snapshot().revision;
            action(&mut state)?;
            let snapshot = state.snapshot();
            let changed = snapshot.revision != previous_revision;
            (snapshot, changed)
        };
        // The authoritative state is committed even if delivery fails; callers can re-query it.
        if changed {
            events.state_changed(&snapshot)?;
        }
        Ok(snapshot)
    }

    forward_update!(
        set_resources(resources: Resources),
        |state| state.set_resources(resources)
    );

    forward_update!(
        /// Records whether the running session can really label speakers (the
        /// coordinator learns it only when the recognizer loads).
        set_speaker_labels_resource(available: bool),
        |state| state.set_speaker_labels_resource(available)
    );

    forward_update!(start_session(), AppState::start);
    forward_update!(stop_session(), AppState::stop);
    forward_update!(
        transition_session(session_id: u32, next: CaptureState),
        |state| state.transition(session_id, next)
    );
    forward_update!(
        fail_session(session_id: u32, error: DomainError),
        |state| state.fail(session_id, error)
    );

    pub fn is_session_active(&self, session_id: u32) -> bool {
        self.snapshot().is_ok_and(|snapshot| {
            snapshot.session_id == session_id
                && matches!(
                    snapshot.capture,
                    CaptureState::Preparing | CaptureState::Listening | CaptureState::Processing
                )
        })
    }

    pub fn enable_tray(&self, window: &impl StateEvents) -> Result<AppSnapshot, CommandError> {
        self.update(window, |state| state.set_recovery(true, false))
    }

    pub fn set_shortcut(
        &self,
        shortcut: String,
        available: bool,
        window: &impl StateEvents,
    ) -> Result<AppSnapshot, CommandError> {
        self.update(window, |state| state.set_shortcut(shortcut, available))
    }

    pub fn change_shortcut(
        &self,
        registry: &impl ShortcutRegistry,
        window: &impl StateEvents,
        requested: &str,
    ) -> Result<AppSnapshot, CommandError> {
        let normalized = AppState::normalize_shortcut(requested)?;
        let _change = self
            .0
            .1
            .lock()
            .map_err(|_| CommandError::StateUnavailable)?;
        let previous = self.snapshot()?;
        if normalized == previous.shortcut && previous.shortcut_available {
            return Ok(previous);
        }
        replace_shortcut(registry, &previous, &normalized)?;
        self.set_shortcut(normalized, true, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{CaptionColor, CaptionMode},
        models::download::DownloadUpdate,
        update::UpdateCheck,
    };
    use std::{cell::RefCell, collections::HashSet};

    #[derive(Default)]
    struct Events(RefCell<Vec<AppSnapshot>>);

    impl StateEvents for Events {
        fn state_changed(&self, state: &AppSnapshot) -> Result<(), CommandError> {
            self.0.borrow_mut().push(state.clone());
            Ok(())
        }
    }

    #[test]
    fn successful_changes_emit_the_committed_state() {
        let controller = Controller::default();
        let events = Events::default();
        let response = controller
            .update(&events, |state| {
                state.set_mode(CaptionMode::Translate(crate::domain::PT_BR))
            })
            .unwrap();
        let recorded = events.0.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0].mode,
            CaptionMode::Translate(crate::domain::PT_BR)
        );
        assert_eq!(recorded[0].session_id, response.session_id);
        assert_eq!(
            recorded[0].revision,
            controller.snapshot().unwrap().revision
        );
    }

    #[test]
    fn rejected_or_unchanged_requests_emit_nothing() {
        let controller = Controller::default();
        let events = Events::default();
        controller.update(&events, AppState::stop).unwrap();
        assert!(controller.update(&events, AppState::start).is_err());
        assert!(
            controller
                .update(&events, |state| state.set_appearance(
                    crate::domain::Appearance {
                        font_size: 100,
                        color: CaptionColor::WHITE,
                        partial_color: CaptionColor::DEFAULT_PARTIAL,
                        background: false,
                        font: crate::domain::CaptionFont::default(),
                    }
                ))
                .is_err()
        );
        assert!(events.0.borrow().is_empty());
        assert_eq!(controller.snapshot().unwrap().revision, 0);
    }

    #[test]
    fn an_event_failure_can_be_recovered_by_reading_state() {
        struct BrokenEvents;

        impl StateEvents for BrokenEvents {
            fn state_changed(&self, _: &AppSnapshot) -> Result<(), CommandError> {
                Err(CommandError::EventUnavailable)
            }
        }

        let controller = Controller::default();
        let result = controller.update(&BrokenEvents, |state| {
            state.set_mode(CaptionMode::Translate(crate::domain::PT_BR))
        });
        assert_eq!(result.unwrap_err(), CommandError::EventUnavailable);
        assert_eq!(
            controller.snapshot().unwrap().mode,
            CaptionMode::Translate(crate::domain::PT_BR)
        );
    }

    #[test]
    fn event_contracts_use_frontend_field_names() {
        let update = CaptionUpdate {
            session_id: 1,
            segment_id: 2,
            revision: 3,
            mode: CaptionMode::Translate(crate::domain::PT_BR),
            text: "<b>texto</b>".into(),
            is_final: true,
            language: Some("en-US".into()),
            speakers: vec![SpeakerSpan {
                speaker: 2,
                text: "<b>texto</b>".into(),
            }],
            provisional: false,
        };
        let json = serde_json::to_value(update).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"sessionId":1,"segmentId":2,"revision":3,"mode":"translate-pt-br","text":"<b>texto</b>","isFinal":true,"language":"en-US","speakers":[{"speaker":2,"text":"<b>texto</b>"}],"provisional":false})
        );
        let snapshot = serde_json::to_value(AppState::default().snapshot()).unwrap();
        assert_eq!(snapshot["windowMode"], "edit");
        assert_eq!(snapshot["translationEngine"], "opus");
        assert_eq!(snapshot["uiLanguage"], "en");
        assert_eq!(snapshot["speakerLabels"], false);
        assert_eq!(snapshot["resources"]["speakerLabels"], false);
        assert_eq!(snapshot["appearance"]["font"], "inter");
        assert_eq!(snapshot["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(snapshot["updateCheck"], false);
        assert_eq!(
            serde_json::to_value(CommandError::Update(UpdateError::Network)).unwrap(),
            serde_json::json!({"kind":"update","code":"network"})
        );
        assert_eq!(
            serde_json::to_value(UpdateCheck {
                current: "0.1.1".into(),
                latest: "0.2.0".into(),
                available: true,
                installable: true,
            })
            .unwrap(),
            serde_json::json!({"current":"0.1.1","latest":"0.2.0","available":true,"installable":true})
        );
        assert_eq!(snapshot["appearance"]["fontSize"], 32);
        assert_eq!(snapshot["appearance"]["color"], "#FFFFFF");
        assert_eq!(snapshot["appearance"]["partialColor"], "#B8B8B8");

        let download = serde_json::to_value(DownloadUpdate {
            model: "riva-translate-4b-instruct-v2-q4_k_m",
            phase: crate::models::download::DownloadPhase::Failed,
            received_bytes: 10,
            total_bytes: Some(20),
            reason: Some(crate::models::download::DownloadFailure::Network),
        })
        .unwrap();
        assert_eq!(
            download,
            serde_json::json!({"model":"riva-translate-4b-instruct-v2-q4_k_m","phase":"failed","receivedBytes":10,"totalBytes":20,"reason":"network"})
        );
    }

    #[test]
    fn shortcut_conflict_keeps_the_previous_recovery_chord() {
        struct FakeRegistry {
            registered: RefCell<HashSet<String>>,
            blocked: Option<&'static str>,
            stuck: Option<&'static str>,
        }

        impl ShortcutRegistry for FakeRegistry {
            fn register(&self, key: &str) -> Result<(), CommandError> {
                if self.blocked == Some(key) {
                    return Err(CommandError::ShortcutUnavailable);
                }
                self.registered.borrow_mut().insert(key.into());
                Ok(())
            }

            fn unregister(&self, key: &str) -> Result<(), CommandError> {
                if self.stuck == Some(key) {
                    return Err(CommandError::ShortcutUnavailable);
                }
                self.registered.borrow_mut().remove(key);
                Ok(())
            }
        }

        let mut previous = AppState::default().snapshot();
        previous.shortcut_available = true;
        let registry = FakeRegistry {
            registered: RefCell::new(HashSet::from([previous.shortcut.clone()])),
            blocked: Some("Ctrl+Alt+K"),
            stuck: None,
        };
        assert_eq!(
            replace_shortcut(&registry, &previous, "Ctrl+Alt+K"),
            Err(CommandError::ShortcutUnavailable)
        );
        assert_eq!(registry.registered.borrow().len(), 1);
        assert!(registry.registered.borrow().contains("Ctrl+Alt+L"));

        let registry = FakeRegistry {
            registered: RefCell::new(HashSet::from([previous.shortcut.clone()])),
            blocked: None,
            stuck: None,
        };
        replace_shortcut(&registry, &previous, "Ctrl+Alt+K").unwrap();
        assert_eq!(registry.registered.borrow().len(), 1);
        assert!(registry.registered.borrow().contains("Ctrl+Alt+K"));

        let registry = FakeRegistry {
            registered: RefCell::new(HashSet::from([previous.shortcut.clone()])),
            blocked: None,
            stuck: Some("Ctrl+Alt+L"),
        };
        assert_eq!(
            replace_shortcut(&registry, &previous, "Ctrl+Alt+K"),
            Err(CommandError::ShortcutUnavailable)
        );
        assert_eq!(registry.registered.borrow().len(), 1);
        assert!(registry.registered.borrow().contains("Ctrl+Alt+L"));
    }
}
