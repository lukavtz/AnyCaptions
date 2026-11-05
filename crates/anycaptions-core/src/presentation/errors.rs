use crate::{
    audio::devices::AudioDeviceError, dictation::DictationError, domain::DomainError,
    ipc::CommandError,
};

use super::i18n::MessageKey;

static DICTATION_MESSAGES: &[(DictationError, MessageKey)] = &[
    (
        DictationError::InvalidShortcut,
        MessageKey::ErrDictationShortcutFormat,
    ),
    (
        DictationError::ShortcutConflict,
        MessageKey::ErrDictationShortcutConflict,
    ),
    (
        DictationError::InvalidDuration,
        MessageKey::ErrDictationDuration,
    ),
    (
        DictationError::InvalidLanguage,
        MessageKey::ErrDictationLanguage,
    ),
];

static DOMAIN_MESSAGES: &[(DomainError, MessageKey)] = &[
    (DomainError::CaptureUnavailable, MessageKey::ErrCapture),
    (
        DomainError::TranscriptionUnavailable,
        MessageKey::ErrTranscription,
    ),
    (
        DomainError::TranslationUnavailable,
        MessageKey::ErrTranslation,
    ),
    (DomainError::InvalidFontSize, MessageKey::ErrFontSize),
    (DomainError::RecoveryUnavailable, MessageKey::ErrRecovery),
    (DomainError::InvalidShortcut, MessageKey::ErrShortcutFormat),
];

static AUDIO_DEVICE_MESSAGES: &[(AudioDeviceError, MessageKey)] = &[
    (AudioDeviceError::NotFound, MessageKey::ErrOutputGone),
    (AudioDeviceError::InvalidId, MessageKey::ErrOutputInvalid),
];

fn lookup<T: PartialEq + Copy>(table: &[(T, MessageKey)], error: T) -> Option<MessageKey> {
    table
        .iter()
        .find(|(candidate, _)| *candidate == error)
        .map(|(_, message)| *message)
}

pub fn error_message_key(error: &CommandError) -> MessageKey {
    // Specialized kinds take precedence over code-based fallback mapping.
    if matches!(error, CommandError::Update(_)) {
        return MessageKey::UpdateFailed;
    }

    match error {
        CommandError::Dictation(error) => {
            lookup(DICTATION_MESSAGES, *error).unwrap_or(MessageKey::ErrDictation)
        }
        CommandError::Domain(error) => {
            lookup(DOMAIN_MESSAGES, *error).unwrap_or(MessageKey::ErrGeneric)
        }
        CommandError::AudioDevice(error) => {
            lookup(AUDIO_DEVICE_MESSAGES, *error).unwrap_or(MessageKey::ErrGeneric)
        }
        CommandError::ShortcutUnavailable => MessageKey::ErrShortcutTaken,
        _ => MessageKey::ErrGeneric,
    }
}
