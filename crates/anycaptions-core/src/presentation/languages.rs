use crate::domain::{CaptionMode, SourceLanguage, TargetLanguage, TranslationEngine};

use super::i18n::MessageKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranslationEngineOption {
    pub id: TranslationEngine,
    pub label_key: MessageKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranslationTarget {
    /// Wire id (`translate-<target tag>`), matching the frontend catalog.
    pub mode: &'static str,
    pub short: &'static str,
    pub country: &'static str,
    pub name: &'static str,
}

impl TranslationTarget {
    pub fn caption_mode(&self) -> CaptionMode {
        let tag = self
            .mode
            .strip_prefix("translate-")
            .and_then(TargetLanguage::from_tag)
            .expect("presentation target is validated by the domain");
        CaptionMode::Translate(tag)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsrLanguage {
    pub tag: &'static str,
    pub country: &'static str,
    pub name: &'static str,
}

impl AsrLanguage {
    pub fn source_language(&self) -> SourceLanguage {
        SourceLanguage::from_tag(self.tag)
            .expect("presentation ASR language is validated by the domain")
    }
}

pub struct LanguageCatalog;

impl LanguageCatalog {
    pub fn translation_engines() -> &'static [TranslationEngineOption] {
        &TRANSLATION_ENGINE_OPTIONS
    }

    pub fn translation_targets() -> &'static [TranslationTarget] {
        &TRANSLATION_TARGET_OPTIONS
    }

    pub fn asr_languages() -> &'static [AsrLanguage] {
        &ASR_LANGUAGE_OPTIONS
    }

    /// Every lowercase flag code used by UI, target, and ASR language options.
    pub fn required_flags() -> &'static [&'static str] {
        &REQUIRED_FLAGS
    }

    pub fn engine_serves(engine: TranslationEngine, mode: CaptionMode) -> bool {
        match mode {
            CaptionMode::Auto => matches!(engine, TranslationEngine::Riva),
            CaptionMode::Translate(target) => engine.supports(target),
        }
    }
}

const TRANSLATION_ENGINE_OPTIONS: [TranslationEngineOption; 2] = [
    TranslationEngineOption {
        id: TranslationEngine::Opus,
        label_key: MessageKey::EngineOpus,
    },
    TranslationEngineOption {
        id: TranslationEngine::Riva,
        label_key: MessageKey::EngineRiva,
    },
];

const TRANSLATION_TARGET_OPTIONS: [TranslationTarget; 37] = [
    TranslationTarget {
        mode: "translate-pt-br",
        short: "PT",
        country: "br",
        name: "Português (Brasil)",
    },
    TranslationTarget {
        mode: "translate-en",
        short: "EN",
        country: "us",
        name: "Inglês",
    },
    TranslationTarget {
        mode: "translate-de",
        short: "DE",
        country: "de",
        name: "Alemão",
    },
    TranslationTarget {
        mode: "translate-ar",
        short: "AR",
        country: "sa",
        name: "Árabe",
    },
    TranslationTarget {
        mode: "translate-bg",
        short: "BG",
        country: "bg",
        name: "Búlgaro",
    },
    TranslationTarget {
        mode: "translate-zh-cn",
        short: "ZH",
        country: "cn",
        name: "Chinês (Simplificado)",
    },
    TranslationTarget {
        mode: "translate-zh-tw",
        short: "ZH",
        country: "tw",
        name: "Chinês (Tradicional)",
    },
    TranslationTarget {
        mode: "translate-ko",
        short: "KO",
        country: "kr",
        name: "Coreano",
    },
    TranslationTarget {
        mode: "translate-hr",
        short: "HR",
        country: "hr",
        name: "Croata",
    },
    TranslationTarget {
        mode: "translate-da",
        short: "DA",
        country: "dk",
        name: "Dinamarquês",
    },
    TranslationTarget {
        mode: "translate-sk",
        short: "SK",
        country: "sk",
        name: "Eslovaco",
    },
    TranslationTarget {
        mode: "translate-sl",
        short: "SL",
        country: "si",
        name: "Esloveno",
    },
    TranslationTarget {
        mode: "translate-es-us",
        short: "ES",
        country: "mx",
        name: "Espanhol (América Latina)",
    },
    TranslationTarget {
        mode: "translate-es-es",
        short: "ES",
        country: "es",
        name: "Espanhol (Espanha)",
    },
    TranslationTarget {
        mode: "translate-et",
        short: "ET",
        country: "ee",
        name: "Estoniano",
    },
    TranslationTarget {
        mode: "translate-fi",
        short: "FI",
        country: "fi",
        name: "Finlandês",
    },
    TranslationTarget {
        mode: "translate-fr",
        short: "FR",
        country: "fr",
        name: "Francês",
    },
    TranslationTarget {
        mode: "translate-el",
        short: "EL",
        country: "gr",
        name: "Grego",
    },
    TranslationTarget {
        mode: "translate-hi",
        short: "HI",
        country: "in",
        name: "Hindi",
    },
    TranslationTarget {
        mode: "translate-nl",
        short: "NL",
        country: "nl",
        name: "Holandês",
    },
    TranslationTarget {
        mode: "translate-hu",
        short: "HU",
        country: "hu",
        name: "Húngaro",
    },
    TranslationTarget {
        mode: "translate-id",
        short: "ID",
        country: "id",
        name: "Indonésio",
    },
    TranslationTarget {
        mode: "translate-it",
        short: "IT",
        country: "it",
        name: "Italiano",
    },
    TranslationTarget {
        mode: "translate-ja",
        short: "JA",
        country: "jp",
        name: "Japonês",
    },
    TranslationTarget {
        mode: "translate-lv",
        short: "LV",
        country: "lv",
        name: "Letão",
    },
    TranslationTarget {
        mode: "translate-lt",
        short: "LT",
        country: "lt",
        name: "Lituano",
    },
    TranslationTarget {
        mode: "translate-no",
        short: "NO",
        country: "no",
        name: "Norueguês",
    },
    TranslationTarget {
        mode: "translate-pl",
        short: "PL",
        country: "pl",
        name: "Polonês",
    },
    TranslationTarget {
        mode: "translate-pt-pt",
        short: "PT",
        country: "pt",
        name: "Português (Portugal)",
    },
    TranslationTarget {
        mode: "translate-ro",
        short: "RO",
        country: "ro",
        name: "Romeno",
    },
    TranslationTarget {
        mode: "translate-ru",
        short: "RU",
        country: "ru",
        name: "Russo",
    },
    TranslationTarget {
        mode: "translate-sv",
        short: "SV",
        country: "se",
        name: "Sueco",
    },
    TranslationTarget {
        mode: "translate-th",
        short: "TH",
        country: "th",
        name: "Tailandês",
    },
    TranslationTarget {
        mode: "translate-cs",
        short: "CS",
        country: "cz",
        name: "Tcheco",
    },
    TranslationTarget {
        mode: "translate-tr",
        short: "TR",
        country: "tr",
        name: "Turco",
    },
    TranslationTarget {
        mode: "translate-uk",
        short: "UK",
        country: "ua",
        name: "Ucraniano",
    },
    TranslationTarget {
        mode: "translate-vi",
        short: "VI",
        country: "vn",
        name: "Vietnamita",
    },
];

const ASR_LANGUAGE_OPTIONS: [AsrLanguage; 83] = [
    AsrLanguage {
        tag: "pt-BR",
        country: "br",
        name: "Português (Brasil)",
    },
    AsrLanguage {
        tag: "en-US",
        country: "us",
        name: "Inglês (EUA)",
    },
    AsrLanguage {
        tag: "af-ZA",
        country: "za",
        name: "Africâner",
    },
    AsrLanguage {
        tag: "ay-BO",
        country: "bo",
        name: "Aimará",
    },
    AsrLanguage {
        tag: "de",
        country: "de",
        name: "Alemão",
    },
    AsrLanguage {
        tag: "am-ET",
        country: "et",
        name: "Amárico",
    },
    AsrLanguage {
        tag: "ar",
        country: "sa",
        name: "Árabe",
    },
    AsrLanguage {
        tag: "hy-AM",
        country: "am",
        name: "Armênio",
    },
    AsrLanguage {
        tag: "az-AZ",
        country: "az",
        name: "Azerbaijano",
    },
    AsrLanguage {
        tag: "bn-IN",
        country: "in",
        name: "Bengali",
    },
    AsrLanguage {
        tag: "bg",
        country: "bg",
        name: "Búlgaro",
    },
    AsrLanguage {
        tag: "kn-IN",
        country: "in",
        name: "Canarês",
    },
    AsrLanguage {
        tag: "ny-MW",
        country: "mw",
        name: "Chichewa",
    },
    AsrLanguage {
        tag: "zh-CN",
        country: "cn",
        name: "Chinês (Simplificado)",
    },
    AsrLanguage {
        tag: "zh-TW",
        country: "tw",
        name: "Chinês (Tradicional)",
    },
    AsrLanguage {
        tag: "si-LK",
        country: "lk",
        name: "Cingalês",
    },
    AsrLanguage {
        tag: "ko",
        country: "kr",
        name: "Coreano",
    },
    AsrLanguage {
        tag: "hr",
        country: "hr",
        name: "Croata",
    },
    AsrLanguage {
        tag: "ku-TR",
        country: "tr",
        name: "Curdo",
    },
    AsrLanguage {
        tag: "da",
        country: "dk",
        name: "Dinamarquês",
    },
    AsrLanguage {
        tag: "sk",
        country: "sk",
        name: "Eslovaco",
    },
    AsrLanguage {
        tag: "sl",
        country: "si",
        name: "Esloveno",
    },
    AsrLanguage {
        tag: "es-US",
        country: "mx",
        name: "Espanhol (América Latina)",
    },
    AsrLanguage {
        tag: "es-ES",
        country: "es",
        name: "Espanhol (Espanha)",
    },
    AsrLanguage {
        tag: "et",
        country: "ee",
        name: "Estoniano",
    },
    AsrLanguage {
        tag: "fi",
        country: "fi",
        name: "Finlandês",
    },
    AsrLanguage {
        tag: "fr",
        country: "fr",
        name: "Francês",
    },
    AsrLanguage {
        tag: "fr-CA",
        country: "ca",
        name: "Francês (Canadá)",
    },
    AsrLanguage {
        tag: "ka-GE",
        country: "ge",
        name: "Georgiano",
    },
    AsrLanguage {
        tag: "el",
        country: "gr",
        name: "Grego",
    },
    AsrLanguage {
        tag: "gn-PY",
        country: "py",
        name: "Guarani",
    },
    AsrLanguage {
        tag: "gu-IN",
        country: "in",
        name: "Guzerate",
    },
    AsrLanguage {
        tag: "ha-NG",
        country: "ng",
        name: "Hauçá",
    },
    AsrLanguage {
        tag: "haw-US",
        country: "us",
        name: "Havaiano",
    },
    AsrLanguage {
        tag: "he-IL",
        country: "il",
        name: "Hebraico",
    },
    AsrLanguage {
        tag: "hi",
        country: "in",
        name: "Hindi",
    },
    AsrLanguage {
        tag: "nl",
        country: "nl",
        name: "Holandês",
    },
    AsrLanguage {
        tag: "hu",
        country: "hu",
        name: "Húngaro",
    },
    AsrLanguage {
        tag: "ig-NG",
        country: "ng",
        name: "Igbo",
    },
    AsrLanguage {
        tag: "id-ID",
        country: "id",
        name: "Indonésio",
    },
    AsrLanguage {
        tag: "en-GB",
        country: "gb",
        name: "Inglês (Reino Unido)",
    },
    AsrLanguage {
        tag: "yo-NG",
        country: "ng",
        name: "Iorubá",
    },
    AsrLanguage {
        tag: "it",
        country: "it",
        name: "Italiano",
    },
    AsrLanguage {
        tag: "ja-JP",
        country: "jp",
        name: "Japonês",
    },
    AsrLanguage {
        tag: "km-KH",
        country: "kh",
        name: "Khmer",
    },
    AsrLanguage {
        tag: "lv",
        country: "lv",
        name: "Letão",
    },
    AsrLanguage {
        tag: "ln-CD",
        country: "cd",
        name: "Lingala",
    },
    AsrLanguage {
        tag: "lt",
        country: "lt",
        name: "Lituano",
    },
    AsrLanguage {
        tag: "ml-IN",
        country: "in",
        name: "Malaiala",
    },
    AsrLanguage {
        tag: "ms-MY",
        country: "my",
        name: "Malaio",
    },
    AsrLanguage {
        tag: "mt-MT",
        country: "mt",
        name: "Maltês",
    },
    AsrLanguage {
        tag: "mi-NZ",
        country: "nz",
        name: "Maori",
    },
    AsrLanguage {
        tag: "mr-IN",
        country: "in",
        name: "Marati",
    },
    AsrLanguage {
        tag: "nah-MX",
        country: "mx",
        name: "Náuatle",
    },
    AsrLanguage {
        tag: "ne-NP",
        country: "np",
        name: "Nepalês",
    },
    AsrLanguage {
        tag: "no",
        country: "no",
        name: "Norueguês",
    },
    AsrLanguage {
        tag: "nb",
        country: "no",
        name: "Norueguês (Bokmål)",
    },
    AsrLanguage {
        tag: "nn",
        country: "no",
        name: "Norueguês (Nynorsk)",
    },
    AsrLanguage {
        tag: "or-KE",
        country: "ke",
        name: "Oromo",
    },
    AsrLanguage {
        tag: "fa-IR",
        country: "ir",
        name: "Persa",
    },
    AsrLanguage {
        tag: "pl",
        country: "pl",
        name: "Polonês",
    },
    AsrLanguage {
        tag: "pt-PT",
        country: "pt",
        name: "Português (Portugal)",
    },
    AsrLanguage {
        tag: "qu-PE",
        country: "pe",
        name: "Quéchua",
    },
    AsrLanguage {
        tag: "rw-RW",
        country: "rw",
        name: "Quiniaruanda",
    },
    AsrLanguage {
        tag: "ky-KG",
        country: "kg",
        name: "Quirguiz",
    },
    AsrLanguage {
        tag: "ro",
        country: "ro",
        name: "Romeno",
    },
    AsrLanguage {
        tag: "ru",
        country: "ru",
        name: "Russo",
    },
    AsrLanguage {
        tag: "sm-WS",
        country: "ws",
        name: "Samoano",
    },
    AsrLanguage {
        tag: "so-SO",
        country: "so",
        name: "Somali",
    },
    AsrLanguage {
        tag: "sw-KE",
        country: "ke",
        name: "Suaíli",
    },
    AsrLanguage {
        tag: "sv",
        country: "se",
        name: "Sueco",
    },
    AsrLanguage {
        tag: "tg-TJ",
        country: "tj",
        name: "Tadjique",
    },
    AsrLanguage {
        tag: "th-TH",
        country: "th",
        name: "Tailandês",
    },
    AsrLanguage {
        tag: "ta-IN",
        country: "in",
        name: "Tâmil",
    },
    AsrLanguage {
        tag: "cs",
        country: "cz",
        name: "Tcheco",
    },
    AsrLanguage {
        tag: "te-IN",
        country: "in",
        name: "Télugo",
    },
    AsrLanguage {
        tag: "to-TO",
        country: "to",
        name: "Tonganês",
    },
    AsrLanguage {
        tag: "tr",
        country: "tr",
        name: "Turco",
    },
    AsrLanguage {
        tag: "uk",
        country: "ua",
        name: "Ucraniano",
    },
    AsrLanguage {
        tag: "ur-PK",
        country: "pk",
        name: "Urdu",
    },
    AsrLanguage {
        tag: "uz-UZ",
        country: "uz",
        name: "Uzbeque",
    },
    AsrLanguage {
        tag: "vi-VN",
        country: "vn",
        name: "Vietnamita",
    },
    AsrLanguage {
        tag: "zu-ZA",
        country: "za",
        name: "Zulu",
    },
];

const REQUIRED_FLAGS: [&str; 67] = [
    "am", "az", "bg", "bo", "br", "ca", "cd", "cn", "cz", "de", "dk", "ee", "es", "et", "fi", "fr",
    "gb", "ge", "gr", "hr", "hu", "id", "il", "in", "ir", "it", "jp", "ke", "kg", "kh", "kr", "lk",
    "lt", "lv", "mt", "mw", "mx", "my", "ng", "nl", "no", "np", "nz", "pe", "pk", "pl", "pt", "py",
    "ro", "ru", "rw", "sa", "se", "si", "sk", "so", "th", "tj", "to", "tr", "tw", "ua", "us", "uz",
    "vn", "ws", "za",
];
