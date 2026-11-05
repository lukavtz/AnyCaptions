//! Pure overlay styling and placement logic for the P0 spike.
//!
//! No FFI and no gpui types: everything here is a plain function or constant
//! so it can be tested without opening a window.

/// Extended window style: mouse input passes through to the window below.
pub const WS_EX_TRANSPARENT: u32 = 0x20;
/// Extended window style: hidden from the taskbar and Alt+Tab.
pub const WS_EX_TOOLWINDOW: u32 = 0x80;
/// Extended window style: the window may paint with per-pixel alpha.
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
/// Extended window style: clicking the window does not activate it.
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
/// Extended window style: the window sits above non-topmost windows.
pub const WS_EX_TOPMOST: u32 = 0x8;

/// Logical width of the overlay window.
pub const OVERLAY_WIDTH: f32 = 800.0;
/// Logical height of the overlay window.
pub const OVERLAY_HEIGHT: f32 = 160.0;
/// Logical gap between the overlay and the bottom of the display.
pub const OVERLAY_BOTTOM_MARGIN: f32 = 32.0;

/// How the overlay treats mouse input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Clicks fall through to the window underneath.
    #[default]
    ClickThrough,
    /// The overlay receives clicks (drag, hover).
    Interactive,
}

fn parse_mode(value: &str) -> Mode {
    match value {
        "interactive" => Mode::Interactive,
        _ => Mode::ClickThrough,
    }
}

/// Reads `--mode click-through` / `--mode interactive` / `--mode=interactive`.
///
/// Anything unrecognised, missing or malformed degrades to [`Mode::ClickThrough`].
pub fn mode_from_args<I: IntoIterator<Item = String>>(args: I) -> Mode {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--mode=") {
            return parse_mode(value);
        }
        if arg == "--mode" {
            return args
                .next()
                .map_or(Mode::default(), |value| parse_mode(&value));
        }
    }
    Mode::default()
}

/// Extended style shared by both modes.
pub fn base_ex_style() -> u32 {
    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED
}

/// Extended style for `mode`: the base bits plus click-through when enabled.
pub fn ex_style_for(mode: Mode) -> u32 {
    set_click_through(base_ex_style(), mode == Mode::ClickThrough)
}

/// Returns `ex_style` with the click-through bit set or cleared.
pub fn set_click_through(ex_style: u32, enabled: bool) -> u32 {
    if enabled {
        ex_style | WS_EX_TRANSPARENT
    } else {
        ex_style & !WS_EX_TRANSPARENT
    }
}

/// Whether mouse input currently passes through the window.
pub fn is_click_through(ex_style: u32) -> bool {
    ex_style & WS_EX_TRANSPARENT != 0
}

/// Whether the window carries the `WS_EX_TOPMOST` bit.
pub fn has_topmost(ex_style: u32) -> bool {
    ex_style & WS_EX_TOPMOST != 0
}

/// Physical `(x, y, width, height)` for the overlay on `display_px`.
///
/// The window is centred horizontally and sits `OVERLAY_BOTTOM_MARGIN` logical
/// pixels above the bottom edge, clamped to the display.
pub fn window_rect(display_px: (f32, f32), scale: f32) -> (i32, i32, i32, i32) {
    const FALLBACK: (i32, i32, i32, i32) = (0, 0, OVERLAY_WIDTH as i32, OVERLAY_HEIGHT as i32);

    let (display_w, display_h) = display_px;
    // Rejects NaN, zero and negatives; anything that is not a usable size
    // falls back to the logical default.
    let usable = |value: f32| value.is_finite() && value > 0.0;
    if !usable(scale) || !usable(display_w) || !usable(display_h) {
        return FALLBACK;
    }

    let display_w = display_w.round() as i32;
    let display_h = display_h.round() as i32;
    let width = (OVERLAY_WIDTH * scale).min(display_w as f32).round() as i32;
    let height = (OVERLAY_HEIGHT * scale).min(display_h as f32).round() as i32;
    let margin = (OVERLAY_BOTTOM_MARGIN * scale).round() as i32;

    let x = ((display_w - width) / 2).max(0);
    let y = (display_h - height - margin).max(0);
    (x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mode_defaults_to_click_through() {
        assert_eq!(Mode::default(), Mode::ClickThrough);
        assert_eq!(mode_from_args(args(&[])), Mode::ClickThrough);
    }

    #[test]
    fn mode_parses_separate_and_equals_forms() {
        let cases = [
            (args(&["--mode", "click-through"]), Mode::ClickThrough),
            (args(&["--mode", "interactive"]), Mode::Interactive),
            (args(&["--mode=interactive"]), Mode::Interactive),
            (args(&["--mode=click-through"]), Mode::ClickThrough),
            (args(&["--mode", "bogus"]), Mode::ClickThrough),
            (args(&["--mode"]), Mode::ClickThrough),
            (args(&["--mode="]), Mode::ClickThrough),
            (
                args(&["--other", "x", "--mode=INTERACTIVE"]),
                Mode::ClickThrough,
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(mode_from_args(input), expected);
        }
    }

    #[test]
    fn base_style_has_toolwindow_noactivate_layered_and_no_transparent() {
        let style = base_ex_style();
        assert_eq!(style & WS_EX_TOOLWINDOW, WS_EX_TOOLWINDOW);
        assert_eq!(style & WS_EX_NOACTIVATE, WS_EX_NOACTIVATE);
        assert_eq!(style & WS_EX_LAYERED, WS_EX_LAYERED);
        assert!(!is_click_through(style));
    }

    #[test]
    fn ex_style_for_click_through_sets_transparent() {
        let style = ex_style_for(Mode::ClickThrough);
        assert!(is_click_through(style));
        assert_eq!(style & !WS_EX_TRANSPARENT, base_ex_style());
    }

    #[test]
    fn ex_style_for_interactive_clears_transparent() {
        let style = ex_style_for(Mode::Interactive);
        assert!(!is_click_through(style));
        assert_eq!(style, base_ex_style());
    }

    #[test]
    fn click_through_bit_toggles_both_ways() {
        let on = set_click_through(base_ex_style(), true);
        assert!(is_click_through(on));
        let off = set_click_through(on, false);
        assert!(!is_click_through(off));
        assert_eq!(off, base_ex_style());
    }

    #[test]
    fn window_rect_1080p_at_scale_1() {
        assert_eq!(window_rect((1920.0, 1080.0), 1.0), (560, 888, 800, 160));
    }

    #[test]
    fn window_rect_4k_at_scale_2() {
        assert_eq!(window_rect((3840.0, 2160.0), 2.0), (1120, 1776, 1600, 320));
    }

    #[test]
    fn window_rect_keeps_logical_bottom_margin() {
        let (_, y, _, height) = window_rect((3840.0, 2160.0), 2.0);
        assert_eq!(2160 - (y + height), (OVERLAY_BOTTOM_MARGIN * 2.0) as i32);
    }

    #[test]
    fn window_rect_clamps_to_small_display() {
        let (x, y, w, h) = window_rect((640.0, 120.0), 2.0);
        assert_eq!((w, h), (640, 120));
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn window_rect_degenerate_inputs_fall_back() {
        let fallback = (0, 0, 800, 160);
        assert_eq!(window_rect((1920.0, 1080.0), 0.0), fallback);
        assert_eq!(window_rect((1920.0, 1080.0), -1.0), fallback);
        assert_eq!(window_rect((0.0, 1080.0), 1.0), fallback);
        assert_eq!(window_rect((1920.0, 0.0), 1.0), fallback);
        assert_eq!(window_rect((f32::NAN, 1080.0), 1.0), fallback);
        assert_eq!(window_rect((1920.0, 1080.0), f32::NAN), fallback);
    }
}
