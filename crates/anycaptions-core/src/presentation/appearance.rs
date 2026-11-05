use std::fmt;

use crate::domain::Appearance;

pub const MIN_CAPTION_SIZE: u16 = 16;
pub const MAX_CAPTION_SIZE: u16 = 80;

pub fn default_appearance() -> Appearance {
    Appearance::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptionFontOption {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
}

pub struct CaptionFontCatalog;

impl CaptionFontCatalog {
    pub fn fonts() -> &'static [CaptionFontOption] {
        &CAPTION_FONT_OPTIONS
    }

    /// Unknown ids deliberately use Inter, matching the frontend fallback.
    pub fn family(id: &str) -> &'static str {
        CAPTION_FONT_OPTIONS
            .iter()
            .find(|font| font.id == id)
            .unwrap_or(&CAPTION_FONT_OPTIONS[0])
            .family
    }
}

const CAPTION_FONT_OPTIONS: [CaptionFontOption; 9] = [
    CaptionFontOption {
        id: "inter",
        name: "Inter",
        family: "\"Inter Variable\", \"Segoe UI\", Arial, sans-serif",
    },
    CaptionFontOption {
        id: "fira-code",
        name: "Fira Code",
        family: "\"Fira Code\", Consolas, monospace",
    },
    CaptionFontOption {
        id: "segoe-ui",
        name: "Segoe UI",
        family: "\"Segoe UI\", Arial, sans-serif",
    },
    CaptionFontOption {
        id: "arial",
        name: "Arial",
        family: "Arial, Helvetica, sans-serif",
    },
    CaptionFontOption {
        id: "verdana",
        name: "Verdana",
        family: "Verdana, Geneva, sans-serif",
    },
    CaptionFontOption {
        id: "tahoma",
        name: "Tahoma",
        family: "Tahoma, Geneva, sans-serif",
    },
    CaptionFontOption {
        id: "trebuchet-ms",
        name: "Trebuchet MS",
        family: "\"Trebuchet MS\", Arial, sans-serif",
    },
    CaptionFontOption {
        id: "georgia",
        name: "Georgia",
        family: "Georgia, 'Times New Roman', serif",
    },
    CaptionFontOption {
        id: "consolas",
        name: "Consolas",
        family: "Consolas, 'Courier New', monospace",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCaptionColor;

impl fmt::Display for InvalidCaptionColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected a #RRGGBB caption color")
    }
}

impl std::error::Error for InvalidCaptionColor {}

pub fn normalize_caption_color(value: &str) -> Result<String, InvalidCaptionColor> {
    let normalized = value.trim();
    let valid = normalized.len() == 7
        && normalized.starts_with('#')
        && normalized[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        Ok(normalized.to_ascii_uppercase())
    } else {
        Err(InvalidCaptionColor)
    }
}

pub fn same_appearance(left: &Appearance, right: &Appearance) -> bool {
    left == right
}
