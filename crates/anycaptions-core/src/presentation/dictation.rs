use crate::dictation::{
    ActivationMode, indicator::IndicatorSnapshot, runtime::DictationEventPayload,
};

use super::i18n::{Locale, MessageKey, translate};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DictationPhase {
    #[default]
    Idle,
    Recording,
    Finalizing,
    Other(&'static str),
}

impl DictationPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Finalizing => "finalizing",
            Self::Other(phase) => phase,
        }
    }

    fn from_event(phase: &'static str) -> Self {
        match phase {
            "idle" => Self::Idle,
            "recording" => Self::Recording,
            "finalizing" => Self::Finalizing,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DictationFrame {
    visible: bool,
    pub phase: DictationPhase,
    pub compact: bool,
    /// Recognized literal text. When empty in non-compact mode, render `hint`.
    pub text: String,
    pub hint: Option<MessageKey>,
    pub elapsed: String,
}

impl DictationFrame {
    pub fn hidden(&self) -> bool {
        !self.visible
    }

    /// Literal recognized text wins; an empty live-text frame shows the
    /// localized hint, while compact mode deliberately shows neither.
    pub fn display_text(&self, locale: Locale) -> &str {
        if !self.text.is_empty() {
            &self.text
        } else if self.compact {
            ""
        } else {
            self.hint
                .and_then(|key| translate(locale, key))
                .unwrap_or("")
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DictationEffects {
    /// The adapter owns audio; 880 Hz starts and 440 Hz ends a recording.
    pub sound_cue_hz: Option<u16>,
    /// Errors are emitted only on the idle event, after the frame hides.
    pub error: Option<MessageKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    Start,
    Stop,
    None,
}

#[derive(Default)]
pub struct DictationModel {
    frame: DictationFrame,
    last_phase: DictationPhase,
    timer_running: bool,
    indicator_revision: Option<u64>,
    indicator_language: Option<String>,
}

impl DictationModel {
    pub fn frame(&self) -> &DictationFrame {
        &self.frame
    }

    pub fn accept(&mut self, event: &DictationEventPayload) -> DictationEffects {
        let phase = DictationPhase::from_event(event.phase);
        let transition = phase_transition(self.last_phase, phase);
        self.last_phase = phase;
        match phase {
            DictationPhase::Idle => self.accept_idle(event, transition),
            _ => self.accept_active(event, phase, transition),
        }
    }

    fn accept_idle(
        &mut self,
        event: &DictationEventPayload,
        transition: Transition,
    ) -> DictationEffects {
        self.timer_running = false;
        self.frame.elapsed.clear();
        self.frame.visible = false;
        self.frame.phase = DictationPhase::Idle;
        DictationEffects {
            sound_cue_hz: cue_frequency(transition, event.sound_cue),
            error: event.error.map(dictation_error_key),
        }
    }

    fn accept_active(
        &mut self,
        event: &DictationEventPayload,
        phase: DictationPhase,
        transition: Transition,
    ) -> DictationEffects {
        if transition == Transition::Start {
            self.timer_running = true;
            self.frame.elapsed = "0:00".into();
        }
        self.frame.visible = true;
        self.frame.phase = phase;
        self.frame.compact = !event.show_live_text;
        self.frame.text = if event.show_live_text {
            event.text.clone()
        } else {
            String::new()
        };
        self.frame.hint = Some(active_hint(event, phase));
        DictationEffects {
            sound_cue_hz: cue_frequency(transition, event.sound_cue),
            error: None,
        }
    }

    /// Advances the running timer with an externally measured duration. Both
    /// hold and toggle modes use rounded seconds and unrestricted `m:ss`.
    pub fn tick(&mut self, elapsed_millis: u64) {
        if !self.timer_running {
            return;
        }
        let total = elapsed_millis / 1_000 + u64::from(elapsed_millis % 1_000 >= 500);
        self.frame.elapsed = format!("{}:{:02}", total / 60, total % 60);
    }

    /// Native indicator snapshots are strictly revisioned and display-only:
    /// the main overlay remains solely responsible for sound and errors.
    pub fn accept_snapshot(&mut self, snapshot: &IndicatorSnapshot) -> Option<DictationEffects> {
        if self
            .indicator_revision
            .is_some_and(|revision| snapshot.revision <= revision)
        {
            return None;
        }
        self.indicator_revision = Some(snapshot.revision);
        self.indicator_language = Some(snapshot.ui_language.clone());
        let mut event = snapshot.event.clone();
        event.sound_cue = false;
        event.error = None;
        Some(self.accept(&event))
    }

    pub fn indicator_language(&self) -> Option<&str> {
        self.indicator_language.as_deref()
    }
}

fn phase_transition(current: DictationPhase, next: DictationPhase) -> Transition {
    match (current, next) {
        (DictationPhase::Idle, DictationPhase::Recording) => Transition::Start,
        (DictationPhase::Idle, _) => Transition::None,
        (_, DictationPhase::Idle) => Transition::Stop,
        _ => Transition::None,
    }
}

fn cue_frequency(transition: Transition, requested: bool) -> Option<u16> {
    match (transition, requested) {
        (Transition::Start, true) => Some(880),
        (Transition::Stop, true) => Some(440),
        _ => None,
    }
}

fn active_hint(event: &DictationEventPayload, phase: DictationPhase) -> MessageKey {
    if phase == DictationPhase::Finalizing {
        return MessageKey::DictationFinalizing;
    }
    match event.activation {
        ActivationMode::Hold => MessageKey::DictationListening,
        ActivationMode::Toggle => MessageKey::DictationListeningToggle,
    }
}

pub fn dictation_error_key(code: &str) -> MessageKey {
    match code {
        "dictation-capture" => MessageKey::ErrDictationMic,
        "dictation-model" => MessageKey::ErrDictationModel,
        "dictation-paste" => MessageKey::ErrDictationPaste,
        "dictation-translate" => MessageKey::ErrDictationTranslate,
        _ => MessageKey::ErrDictation,
    }
}
