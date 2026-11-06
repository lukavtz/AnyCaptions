//! Shared GPUI theme values ported from the existing UI.

use gpui::{FontWeight, Rgba};
use std::time::Duration;

pub const INTER_FONT_FAMILY: &str = "Inter Variable";
pub const FIRA_CODE_FONT_FAMILY: &str = "Fira Code";
pub const SYSTEM_FONT_FAMILY: &str = "Segoe UI";
pub const MONOSPACE_FONT_FAMILY: &str = "Consolas";
pub const CAPTION_FONT_WEIGHT: FontWeight = FontWeight::BOLD;
pub const CAPTION_LINE_HEIGHT: f32 = 1.25;
pub const CAPTION_FADE_DURATION: Duration = Duration::from_millis(120);
pub const DICTATION_PULSE_DURATION: Duration = Duration::from_millis(1_200);
pub const CONTROL_RADIUS: f32 = 8.0;
pub const ICON_BUTTON_SIZE: f32 = 32.0;
pub const FLAG_WIDTH: f32 = 18.0;
pub const FLAG_HEIGHT: f32 = 13.5;

const fn rgb(red: u8, green: u8, blue: u8) -> Rgba {
    rgba(red, green, blue, u8::MAX)
}

const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Rgba {
    Rgba {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: alpha as f32 / 255.0,
    }
}

pub const BASE_100: Rgba = rgb(0x25, 0x25, 0x25);
pub const BASE_200: Rgba = BASE_100;
pub const BASE_300: Rgba = rgb(0x38, 0x38, 0x38);
pub const BASE_CONTENT: Rgba = rgb(0xf5, 0xf5, 0xf5);
pub const PRIMARY: Rgba = BASE_CONTENT;
pub const PRIMARY_CONTENT: Rgba = rgb(0x15, 0x15, 0x15);
pub const SECONDARY: Rgba = BASE_300;
pub const SECONDARY_CONTENT: Rgba = BASE_CONTENT;
pub const ACCENT: Rgba = rgb(0xff, 0xff, 0xff);
pub const ACCENT_CONTENT: Rgba = PRIMARY_CONTENT;
pub const INFO: Rgba = rgb(0x60, 0xa5, 0xfa);
pub const SUCCESS: Rgba = rgb(0x4a, 0xde, 0x80);
pub const WARNING: Rgba = rgb(0xfb, 0xbf, 0x24);
pub const ERROR: Rgba = rgb(0xf8, 0x71, 0x71);
pub const CLOSE_HOVER: Rgba = rgb(0x5c, 0x29, 0x29);
pub const DICTATION_RECORDING: Rgba = rgb(0xef, 0x44, 0x44);

pub const BORDER: Rgba = rgba(0xff, 0xff, 0xff, 0x24);
pub const BORDER_SUBTLE: Rgba = rgba(0xff, 0xff, 0xff, 0x1f);
pub const BORDER_STRONG: Rgba = rgba(0xff, 0xff, 0xff, 0x80);
pub const OVERLAY_EDIT_BACKGROUND: Rgba = rgba(0x0f, 0x0f, 0x0f, 0xa6);
pub const OVERLAY_SETTINGS_BACKGROUND: Rgba = rgba(0x0f, 0x0f, 0x0f, 0xf5);
pub const CAPTION_BACKGROUND: Rgba = rgba(0x00, 0x00, 0x00, 0x99);
pub const PREVIEW_BACKGROUND: Rgba = rgb(0x09, 0x09, 0x09);
pub const PICKER_BACKGROUND: Rgba = rgb(0x1d, 0x1d, 0x1d);
pub const MUTED: Rgba = rgb(0xc4, 0xc4, 0xc4);
pub const SUBTLE: Rgba = rgb(0x8a, 0x8a, 0x8a);
pub const CAPTION_PARTIAL: Rgba = rgb(0xb8, 0xb8, 0xb8);

pub const SPEAKER_COLORS: [Rgba; 8] = [
    rgb(0x7c, 0xc4, 0xff),
    rgb(0xff, 0xb3, 0x6b),
    rgb(0x7f, 0xe0, 0xa0),
    rgb(0xd6, 0xa6, 0xff),
    rgb(0xff, 0x9b, 0xb8),
    rgb(0x6f, 0xe6, 0xe0),
    rgb(0xff, 0x8a, 0x80),
    rgb(0xb8, 0xc2, 0xff),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_preserves_caption_motion_and_speaker_palette() {
        assert_eq!(CAPTION_FADE_DURATION, Duration::from_millis(120));
        assert_eq!(SPEAKER_COLORS.len(), 8);
        assert_eq!(SPEAKER_COLORS[0].r, 124.0 / 255.0);
        assert_eq!(CAPTION_FONT_WEIGHT, FontWeight::BOLD);
        assert_eq!(INTER_FONT_FAMILY, "Inter Variable");
        assert_eq!(FIRA_CODE_FONT_FAMILY, "Fira Code");
        assert_eq!(FLAG_WIDTH / FLAG_HEIGHT, 4.0 / 3.0);
    }
}
