pub mod appearance;
pub mod application;
pub mod dictation;
pub mod downloads;
pub mod errors;
pub mod i18n;
pub mod languages;
pub mod updates;

pub use appearance::{
    CaptionFontCatalog, CaptionFontOption, InvalidCaptionColor, MAX_CAPTION_SIZE, MIN_CAPTION_SIZE,
    default_appearance, normalize_caption_color, same_appearance,
};
pub use errors::error_message_key;
pub use i18n::{Locale, MessageKey, UI_LANGUAGES, UiLanguageOption, translate};
pub use languages::{AsrLanguage, LanguageCatalog, TranslationEngineOption, TranslationTarget};

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
