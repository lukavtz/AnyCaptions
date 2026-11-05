//! Compile-time UI assets. No runtime filesystem or network lookup is used.

use anycaptions_core::presentation::Locale;
use gpui::{App, AssetSource as GpuiAssetSource, Result, SharedString};
use std::borrow::Cow;

pub const INTER_FONT: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");
pub const FIRA_CODE_FONT: &[u8] = include_bytes!("../assets/fonts/FiraCode-VF.ttf");
pub const INTER_OFL_NOTICE: &[u8] = include_bytes!("../assets/fonts/OFL-Inter.txt");
pub const FIRA_CODE_OFL_NOTICE: &[u8] = include_bytes!("../assets/fonts/OFL-FiraCode.txt");
pub const FLAG_ICONS_MIT_NOTICE: &[u8] = include_bytes!("../assets/flags/MIT-flag-icons.txt");

const FLAGS: &[(&str, &[u8])] = &[
    ("am", include_bytes!("../assets/flags/am.png")),
    ("az", include_bytes!("../assets/flags/az.png")),
    ("bg", include_bytes!("../assets/flags/bg.png")),
    ("bo", include_bytes!("../assets/flags/bo.png")),
    ("br", include_bytes!("../assets/flags/br.png")),
    ("ca", include_bytes!("../assets/flags/ca.png")),
    ("cd", include_bytes!("../assets/flags/cd.png")),
    ("cn", include_bytes!("../assets/flags/cn.png")),
    ("cz", include_bytes!("../assets/flags/cz.png")),
    ("de", include_bytes!("../assets/flags/de.png")),
    ("dk", include_bytes!("../assets/flags/dk.png")),
    ("ee", include_bytes!("../assets/flags/ee.png")),
    ("es", include_bytes!("../assets/flags/es.png")),
    ("et", include_bytes!("../assets/flags/et.png")),
    ("fi", include_bytes!("../assets/flags/fi.png")),
    ("fr", include_bytes!("../assets/flags/fr.png")),
    ("gb", include_bytes!("../assets/flags/gb.png")),
    ("ge", include_bytes!("../assets/flags/ge.png")),
    ("gr", include_bytes!("../assets/flags/gr.png")),
    ("hr", include_bytes!("../assets/flags/hr.png")),
    ("hu", include_bytes!("../assets/flags/hu.png")),
    ("id", include_bytes!("../assets/flags/id.png")),
    ("il", include_bytes!("../assets/flags/il.png")),
    ("in", include_bytes!("../assets/flags/in.png")),
    ("ir", include_bytes!("../assets/flags/ir.png")),
    ("it", include_bytes!("../assets/flags/it.png")),
    ("jp", include_bytes!("../assets/flags/jp.png")),
    ("ke", include_bytes!("../assets/flags/ke.png")),
    ("kg", include_bytes!("../assets/flags/kg.png")),
    ("kh", include_bytes!("../assets/flags/kh.png")),
    ("kr", include_bytes!("../assets/flags/kr.png")),
    ("lk", include_bytes!("../assets/flags/lk.png")),
    ("lt", include_bytes!("../assets/flags/lt.png")),
    ("lv", include_bytes!("../assets/flags/lv.png")),
    ("mt", include_bytes!("../assets/flags/mt.png")),
    ("mw", include_bytes!("../assets/flags/mw.png")),
    ("mx", include_bytes!("../assets/flags/mx.png")),
    ("my", include_bytes!("../assets/flags/my.png")),
    ("ng", include_bytes!("../assets/flags/ng.png")),
    ("nl", include_bytes!("../assets/flags/nl.png")),
    ("no", include_bytes!("../assets/flags/no.png")),
    ("np", include_bytes!("../assets/flags/np.png")),
    ("nz", include_bytes!("../assets/flags/nz.png")),
    ("pe", include_bytes!("../assets/flags/pe.png")),
    ("pk", include_bytes!("../assets/flags/pk.png")),
    ("pl", include_bytes!("../assets/flags/pl.png")),
    ("pt", include_bytes!("../assets/flags/pt.png")),
    ("py", include_bytes!("../assets/flags/py.png")),
    ("ro", include_bytes!("../assets/flags/ro.png")),
    ("ru", include_bytes!("../assets/flags/ru.png")),
    ("rw", include_bytes!("../assets/flags/rw.png")),
    ("sa", include_bytes!("../assets/flags/sa.png")),
    ("se", include_bytes!("../assets/flags/se.png")),
    ("si", include_bytes!("../assets/flags/si.png")),
    ("sk", include_bytes!("../assets/flags/sk.png")),
    ("so", include_bytes!("../assets/flags/so.png")),
    ("th", include_bytes!("../assets/flags/th.png")),
    ("tj", include_bytes!("../assets/flags/tj.png")),
    ("to", include_bytes!("../assets/flags/to.png")),
    ("tr", include_bytes!("../assets/flags/tr.png")),
    ("tw", include_bytes!("../assets/flags/tw.png")),
    ("ua", include_bytes!("../assets/flags/ua.png")),
    ("us", include_bytes!("../assets/flags/us.png")),
    ("uz", include_bytes!("../assets/flags/uz.png")),
    ("vn", include_bytes!("../assets/flags/vn.png")),
    ("ws", include_bytes!("../assets/flags/ws.png")),
    ("za", include_bytes!("../assets/flags/za.png")),
];

const FONT_ASSETS: &[(&str, &[u8])] = &[
    ("InterVariable.ttf", INTER_FONT),
    ("FiraCode-VF.ttf", FIRA_CODE_FONT),
    ("OFL-Inter.txt", INTER_OFL_NOTICE),
    ("OFL-FiraCode.txt", FIRA_CODE_OFL_NOTICE),
];

/// GPUI asset source backed exclusively by bytes embedded in the executable.
#[derive(Debug, Default, Clone, Copy)]
pub struct AssetSource;

pub fn flag(country: &str) -> Option<&'static [u8]> {
    FLAGS
        .iter()
        .find_map(|(code, bytes)| (*code == country).then_some(*bytes))
}

pub fn has_flag(country: &str) -> bool {
    flag(country).is_some()
}

pub const fn ui_language_name(locale: Locale) -> Option<&'static str> {
    Some(match locale {
        Locale::PtBr => "Português (Brasil)",
        Locale::En => "English",
        Locale::Es => "Español",
        Locale::ZhCn => "中文（简体）",
        Locale::Fr => "Français",
        Locale::Ru => "Русский",
    })
}

pub fn bundled_fonts() -> [Cow<'static, [u8]>; 2] {
    [Cow::Borrowed(INTER_FONT), Cow::Borrowed(FIRA_CODE_FONT)]
}

/// Register bundled fonts during application startup, before creating windows.
pub fn register_fonts(cx: &App) -> Result<()> {
    cx.text_system().add_fonts(Vec::from(bundled_fonts()))
}

impl GpuiAssetSource for AssetSource {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(country) = path
            .strip_prefix("flags/")
            .and_then(|p| p.strip_suffix(".png"))
        {
            return Ok(flag(country).map(Cow::Borrowed));
        }
        if let Some(name) = path.strip_prefix("fonts/") {
            return Ok(FONT_ASSETS
                .iter()
                .find_map(|(asset, bytes)| (*asset == name).then_some(Cow::Borrowed(*bytes))));
        }
        if path == "flags/MIT-flag-icons.txt" {
            return Ok(Some(Cow::Borrowed(FLAG_ICONS_MIT_NOTICE)));
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let paths: Vec<SharedString> = match path.trim_end_matches('/') {
            "flags" => FLAGS
                .iter()
                .map(|(country, _)| SharedString::from(format!("flags/{country}.png")))
                .chain(std::iter::once(SharedString::new_static(
                    "flags/MIT-flag-icons.txt",
                )))
                .collect(),
            "fonts" => FONT_ASSETS
                .iter()
                .map(|(name, _)| SharedString::from(format!("fonts/{name}")))
                .collect(),
            _ => Vec::new(),
        };
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anycaptions_core::presentation::{LanguageCatalog, Locale};

    #[test]
    fn every_ui_language_has_a_native_display_name() {
        for locale in Locale::ALL {
            assert!(ui_language_name(*locale).is_some(), "missing {locale:?}");
        }
        assert_eq!(ui_language_name(Locale::PtBr), Some("Português (Brasil)"));
        assert_eq!(ui_language_name(Locale::ZhCn), Some("中文（简体）"));
    }

    #[test]
    fn every_required_flag_is_an_embedded_color_png() {
        let assets = AssetSource;
        for flag in LanguageCatalog::required_flags() {
            assert!(has_flag(flag), "missing flag {flag}");
            let path = format!("flags/{flag}.png");
            let bytes = assets
                .load(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("missing {path}"));
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{path} is not PNG");
            assert!(bytes.len() > 33, "{path} is truncated");
            assert_eq!(&bytes[12..16], b"IHDR", "{path} has no IHDR");
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 80);
            assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 60);
            let color_type = bytes[25];
            assert!(matches!(color_type, 2 | 3 | 6), "{path} is grayscale");
        }
        assert!(!has_flag("unknown"));
    }

    #[test]
    fn asset_source_is_local_and_exact() {
        let assets = AssetSource;
        assert!(assets.load("flags/br.png").unwrap().is_some());
        assert!(assets.load("fonts/InterVariable.ttf").unwrap().is_some());
        assert!(assets.load("fonts/FiraCode-VF.ttf").unwrap().is_some());
        assert!(assets.load("fonts/OFL-Inter.txt").unwrap().is_some());
        assert!(assets.load("fonts/OFL-FiraCode.txt").unwrap().is_some());
        assert!(assets.load("flags/MIT-flag-icons.txt").unwrap().is_some());
        assert!(assets.load("../flags/br.png").unwrap().is_none());
        assert!(assets.load("https://example.com/br.png").unwrap().is_none());
        assert!(assets.load("flags/br.svg").unwrap().is_none());
    }

    #[test]
    fn bundled_fonts_are_sfnt_files() {
        for bytes in [INTER_FONT, FIRA_CODE_FONT] {
            assert!(bytes.len() > 12);
            assert!(matches!(&bytes[..4], b"\x00\x01\x00\x00" | b"OTTO"));
        }
        assert_eq!(bundled_fonts().len(), 2);
    }
}
