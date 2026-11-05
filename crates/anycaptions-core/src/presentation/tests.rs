use std::collections::BTreeSet;

use super::*;
use crate::{
    audio::devices::AudioDeviceError,
    dictation::DictationError,
    domain::{
        Appearance, CaptionColor, CaptionFont, CaptionMode, DomainError, SourceLanguage,
        TargetLanguage, TranslationEngine,
    },
    ipc::CommandError,
    update::UpdateError,
};

fn placeholders(message: &str) -> Vec<&str> {
    let mut result = message
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name))
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

#[test]
fn every_referenced_message_exists_in_every_locale() {
    assert_eq!(MessageKey::ALL.len(), 118);
    for locale in Locale::ALL {
        for key in MessageKey::ALL {
            let message =
                translate(*locale, *key).unwrap_or_else(|| panic!("missing {locale:?}/{key:?}"));
            assert!(!message.is_empty(), "empty {locale:?}/{key:?}");
        }
    }
}

#[test]
fn every_locale_preserves_placeholders() {
    for key in MessageKey::ALL {
        let english = translate(Locale::En, *key).unwrap();
        let expected = placeholders(english);
        for locale in Locale::ALL {
            let translated = translate(*locale, *key).unwrap();
            assert_eq!(
                placeholders(translated),
                expected,
                "placeholder mismatch {locale:?}/{key:?}"
            );
        }
    }
}

#[test]
fn message_tables_keep_upstream_wording() {
    assert_eq!(
        translate(Locale::En, MessageKey::CaptionWindow),
        Some("Caption window")
    );
    assert_eq!(translate(Locale::PtBr, MessageKey::Start), Some("Iniciar"));
    assert_eq!(translate(Locale::Es, MessageKey::Stop), Some("Detener"));
    assert_eq!(translate(Locale::Fr, MessageKey::Preview), Some("Aperçu"));
    assert_eq!(translate(Locale::Ru, MessageKey::Save), Some("Сохранить"));
    assert_eq!(translate(Locale::ZhCn, MessageKey::Settings), Some("设置"));
    assert_eq!(
        translate(Locale::En, MessageKey::ModelDownloadAction),
        Some("Download ({size} GB)")
    );
    assert_eq!(MessageKey::ALL.first(), Some(&MessageKey::CaptionWindow));
    assert_eq!(
        MessageKey::ALL.last(),
        Some(&MessageKey::ErrDictationTranslate)
    );
}

#[test]
fn ui_locales_match_the_frontend_catalog() {
    assert_eq!(Locale::ALL.len(), 6);
    assert_eq!(UI_LANGUAGES.len(), Locale::ALL.len());
    for (locale, option) in Locale::ALL.iter().copied().zip(UI_LANGUAGES) {
        assert_eq!(option.locale, locale);
        assert_eq!(option.tag, locale.tag());
        assert_eq!(option.country, locale.country());
        assert_eq!(option.name, locale.name());
        assert_eq!(Locale::from_tag(option.tag), Some(locale));
    }
    assert_eq!(Locale::En.name(), "English");
    assert_eq!(Locale::ZhCn.name(), "中文（简体）");
    assert_eq!(Locale::from_tag("missing"), None);
}

#[test]
fn caption_colors_normalize_exactly_like_the_frontend() {
    assert_eq!(normalize_caption_color(" #a1b2c3 ").unwrap(), "#A1B2C3");
    assert_eq!(normalize_caption_color("#A1B2C3").unwrap(), "#A1B2C3");
    assert_eq!(normalize_caption_color("\n#00ff99\t").unwrap(), "#00FF99");
    for invalid in [
        "red", "", "#fff", "#A1B2C34", "#GG0000", "A1B2C3", "##A1B2C3",
    ] {
        assert!(
            normalize_caption_color(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn appearance_defaults_and_limits_match_the_domain() {
    let appearance = default_appearance();
    assert_eq!(appearance, Appearance::default());
    assert_eq!(appearance.font_size, 32);
    assert_eq!(appearance.color, CaptionColor::WHITE);
    assert_eq!(appearance.partial_color, CaptionColor::DEFAULT_PARTIAL);
    assert!(!appearance.background);
    assert_eq!(appearance.font.id(), "inter");
    assert_eq!(MIN_CAPTION_SIZE, 16);
    assert_eq!(MAX_CAPTION_SIZE, 80);
}

#[test]
fn appearance_comparison_includes_every_field() {
    let base = default_appearance();
    assert!(same_appearance(&base, &base));
    assert!(!same_appearance(
        &base,
        &Appearance {
            font_size: 33,
            ..base
        }
    ));
    assert!(!same_appearance(
        &base,
        &Appearance {
            color: CaptionColor::YELLOW,
            ..base
        }
    ));
    assert!(!same_appearance(
        &base,
        &Appearance {
            partial_color: CaptionColor::WHITE,
            ..base
        }
    ));
    assert!(!same_appearance(
        &base,
        &Appearance {
            background: true,
            ..base
        }
    ));
    assert!(!same_appearance(
        &base,
        &Appearance {
            font: CaptionFont::from_id("georgia").unwrap(),
            ..base
        }
    ));
}

#[test]
fn caption_font_catalog_is_complete_and_falls_back_to_inter() {
    let expected = [
        (
            "inter",
            "Inter",
            "\"Inter Variable\", \"Segoe UI\", Arial, sans-serif",
        ),
        (
            "fira-code",
            "Fira Code",
            "\"Fira Code Variable\", Consolas, monospace",
        ),
        ("segoe-ui", "Segoe UI", "\"Segoe UI\", Arial, sans-serif"),
        ("arial", "Arial", "Arial, Helvetica, sans-serif"),
        ("verdana", "Verdana", "Verdana, Geneva, sans-serif"),
        ("tahoma", "Tahoma", "Tahoma, Geneva, sans-serif"),
        (
            "trebuchet-ms",
            "Trebuchet MS",
            "\"Trebuchet MS\", Arial, sans-serif",
        ),
        ("georgia", "Georgia", "Georgia, 'Times New Roman', serif"),
        ("consolas", "Consolas", "Consolas, 'Courier New', monospace"),
    ];
    assert_eq!(CaptionFontCatalog::fonts().len(), 9);
    for (font, expected) in CaptionFontCatalog::fonts().iter().zip(expected) {
        assert_eq!((font.id, font.name, font.family), expected);
        assert!(CaptionFont::from_id(font.id).is_some());
    }
    assert_eq!(CaptionFontCatalog::family("fira-code"), expected[1].2);
    assert_eq!(CaptionFontCatalog::family("unknown"), expected[0].2);
}

fn assert_catalog_entries<T>(
    entries: &[T],
    expected_len: usize,
    expected_start: [&str; 2],
    expected_last: &str,
    id: impl Fn(&T) -> &'static str,
    details: impl Fn(&T) -> (&'static str, &'static str),
    validate: impl Fn(&T),
) {
    assert_eq!(entries.len(), expected_len);
    let unique = entries.iter().map(&id).collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), expected_len);
    for (entry, expected) in entries.iter().zip(expected_start) {
        assert_eq!(id(entry), expected);
    }
    assert_eq!(id(entries.last().unwrap()), expected_last);
    for entry in entries {
        let (country, name) = details(entry);
        assert!(!country.is_empty());
        assert!(!name.is_empty());
        validate(entry);
    }
}

#[test]
fn language_catalogs_have_all_valid_unique_entries() {
    assert_catalog_entries(
        LanguageCatalog::translation_targets(),
        37,
        ["translate-pt-br", "translate-en"],
        "translate-vi",
        |target| target.mode,
        |target| (target.country, target.name),
        |target| {
            let tag = target.mode.strip_prefix("translate-").unwrap();
            let domain_target =
                TargetLanguage::from_tag(tag).unwrap_or_else(|| panic!("invalid target {tag}"));
            assert_eq!(target.caption_mode(), CaptionMode::Translate(domain_target));
            assert!(!target.short.is_empty());
        },
    );
    assert_catalog_entries(
        LanguageCatalog::asr_languages(),
        83,
        ["pt-BR", "en-US"],
        "zu-ZA",
        |language| language.tag,
        |language| (language.country, language.name),
        |language| {
            let domain_language = SourceLanguage::from_tag(language.tag)
                .unwrap_or_else(|| panic!("invalid ASR tag {}", language.tag));
            assert_eq!(language.source_language(), domain_language);
        },
    );
}

#[test]
fn required_flags_cover_every_language_catalog_without_duplicates() {
    let expected = Locale::ALL
        .iter()
        .map(|locale| locale.country())
        .chain(
            LanguageCatalog::translation_targets()
                .iter()
                .map(|target| target.country),
        )
        .chain(
            LanguageCatalog::asr_languages()
                .iter()
                .map(|language| language.country),
        )
        .collect::<BTreeSet<_>>();
    let flags = LanguageCatalog::required_flags();
    assert_eq!(flags.len(), 67);
    assert_eq!(flags.iter().copied().collect::<BTreeSet<_>>(), expected);
    assert!(flags.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn engine_filtering_matches_frontend_modes() {
    let engines = LanguageCatalog::translation_engines();
    assert_eq!(engines.len(), 2);
    assert_eq!(engines[0].id, TranslationEngine::Opus);
    assert_eq!(engines[0].label_key, MessageKey::EngineOpus);
    assert_eq!(engines[1].id, TranslationEngine::Riva);
    assert_eq!(engines[1].label_key, MessageKey::EngineRiva);
    assert!(!LanguageCatalog::engine_serves(
        TranslationEngine::Opus,
        CaptionMode::Auto
    ));
    assert!(LanguageCatalog::engine_serves(
        TranslationEngine::Riva,
        CaptionMode::Auto
    ));
    for target in LanguageCatalog::translation_targets() {
        let mode = target.caption_mode();
        let opus_expected = matches!(
            target.mode,
            "translate-en" | "translate-pt-br" | "translate-pt-pt"
        );
        assert_eq!(
            LanguageCatalog::engine_serves(TranslationEngine::Opus, mode),
            opus_expected,
            "OPUS {}",
            target.mode
        );
        assert!(
            LanguageCatalog::engine_serves(TranslationEngine::Riva, mode),
            "Riva {}",
            target.mode
        );
    }
}

#[test]
fn command_errors_keep_frontend_message_precedence() {
    let cases = [
        (
            CommandError::Update(UpdateError::Network),
            MessageKey::UpdateFailed,
        ),
        (
            CommandError::Update(UpdateError::Malformed),
            MessageKey::UpdateFailed,
        ),
        (
            CommandError::Dictation(DictationError::InvalidShortcut),
            MessageKey::ErrDictationShortcutFormat,
        ),
        (
            CommandError::Dictation(DictationError::ShortcutConflict),
            MessageKey::ErrDictationShortcutConflict,
        ),
        (
            CommandError::Dictation(DictationError::InvalidDuration),
            MessageKey::ErrDictationDuration,
        ),
        (
            CommandError::Dictation(DictationError::InvalidLanguage),
            MessageKey::ErrDictationLanguage,
        ),
        (
            CommandError::Domain(DomainError::CaptureUnavailable),
            MessageKey::ErrCapture,
        ),
        (
            CommandError::Domain(DomainError::TranscriptionUnavailable),
            MessageKey::ErrTranscription,
        ),
        (
            CommandError::Domain(DomainError::TranslationUnavailable),
            MessageKey::ErrTranslation,
        ),
        (
            CommandError::Domain(DomainError::InvalidFontSize),
            MessageKey::ErrFontSize,
        ),
        (
            CommandError::Domain(DomainError::RecoveryUnavailable),
            MessageKey::ErrRecovery,
        ),
        (
            CommandError::Domain(DomainError::InvalidShortcut),
            MessageKey::ErrShortcutFormat,
        ),
        (
            CommandError::ShortcutUnavailable,
            MessageKey::ErrShortcutTaken,
        ),
        (
            CommandError::AudioDevice(AudioDeviceError::NotFound),
            MessageKey::ErrOutputGone,
        ),
        (
            CommandError::AudioDevice(AudioDeviceError::InvalidId),
            MessageKey::ErrOutputInvalid,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(
            error_message_key(&error),
            expected,
            "wrong mapping for {error:?}"
        );
    }
}

#[test]
fn unmapped_command_errors_use_the_generic_message() {
    let cases = [
        CommandError::Domain(DomainError::InvalidTransition),
        CommandError::Domain(DomainError::CounterExhausted),
        CommandError::StateUnavailable,
        CommandError::EventUnavailable,
        CommandError::UnauthorizedWindow,
        CommandError::WindowUnavailable,
        CommandError::PreferencesUnavailable,
        CommandError::AudioDevice(AudioDeviceError::Unavailable),
        CommandError::BrowserUnavailable,
        CommandError::DownloadBusy,
        CommandError::DownloadUnavailable,
        CommandError::UpdateBusy,
        CommandError::UpdateUnavailable,
    ];
    for error in cases {
        assert_eq!(
            error_message_key(&error),
            MessageKey::ErrGeneric,
            "wrong fallback for {error:?}"
        );
    }
}
