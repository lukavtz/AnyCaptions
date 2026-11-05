//! P0 spike: a GPUI window shaped like AnyCaptions' caption overlay.
//!
//! Proves the four things the port depends on: transparent window, always on
//! top, click-through via `WS_EX_TRANSPARENT`, and GPUI text rendering on top
//! of the desktop. Not production code.

mod win32;

use gpui::{
    App, Application, Bounds, Context, MouseButton, Pixels, Render, SharedString, Styled,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    div, point, prelude::*, px, rgb, size,
};
use spike_overlay::style::{self, Mode, OVERLAY_BOTTOM_MARGIN, OVERLAY_HEIGHT, OVERLAY_WIDTH};

struct Overlay {
    mode: Mode,
    title_set: bool,
}

impl Overlay {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            title_set: false,
        }
    }

    fn drag(&mut self, _: &gpui::MouseDownEvent, window: &mut Window, _: &mut Context<Self>) {
        window.start_window_move();
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Set title here, when the window is definitely ready.
        if !self.title_set {
            window.set_window_title("AnyCaptions Spike");
            self.title_set = true;

            // Apply Win32 styles after the title is set and window is ready.
            apply_native_by_title("AnyCaptions Spike", self.mode);
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_end()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
            .border_1()
            .border_color(gpui::hsla(0.0, 0.0, 1.0, 0.18))
            .text_color(rgb(0xffffff))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::drag))
            .child(
                div()
                    .text_2xl()
                    .child(SharedString::from("Субтитры поверх всех окон")),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(0xc8c8d0))
                    .child(SharedString::from(match self.mode {
                        Mode::ClickThrough => "режим: click-through — клики проходят насквозь",
                        Mode::Interactive => "режим: interactive — клик и перетаскивание",
                    })),
            )
    }
}

/// Bottom-centre placement of an 800×160 logical window on the primary display.
///
/// GPUI's `Pixels` deliberately has no arithmetic operators, so the geometry is
/// computed in `f32` and converted back once.
fn bottom_bounds(cx: &App) -> Bounds<Pixels> {
    let Some(display) = cx.primary_display() else {
        return Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(OVERLAY_WIDTH), px(OVERLAY_HEIGHT)),
        );
    };
    let area = display.bounds();
    let ax = f32::from(area.origin.x);
    let ay = f32::from(area.origin.y);
    let aw = f32::from(area.size.width);
    let ah = f32::from(area.size.height);
    // GPUI adds a border offset; subtract it from the desired content size.
    let width = (OVERLAY_WIDTH - 16.0).min(aw);
    let height = (OVERLAY_HEIGHT - 9.0).min(ah);
    let x = ax + (aw - width) / 2.0;
    let y = (ay + ah - height - OVERLAY_BOTTOM_MARGIN).max(ay);
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

/// Finds the overlay window by title and applies native styles.
fn apply_native_by_title(title: &str, mode: Mode) {
    match win32::find_window_by_title(title) {
        Some(hwnd) => {
            let (previous, applied) = win32::apply_overlay(hwnd, mode);
            println!(
                "overlay: hwnd={:#x} mode={mode:?} exstyle={applied:#010x} previous={previous:#010x} click_through={} topmost={}",
                hwnd as isize,
                style::is_click_through(applied),
                style::has_topmost(applied),
            );
        }
        None => eprintln!("overlay: window '{title}' not found after creation"),
    }
}

fn main() {
    let mode = style::mode_from_args(std::env::args().skip(1));
    let title = "AnyCaptions Spike";
    println!("overlay: starting in {mode:?}, title='{title}'");

    Application::new().run(move |cx: &mut App| {
        let bounds = bottom_bounds(cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(title.into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    focus: false,
                    show: true,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |_window, cx| cx.new(|_| Overlay::new(mode)),
            )
            .expect("open overlay window");

        // Styles are applied in render when the window is ready.

        // Window handle is Copy; GPUI keeps it alive internally.
        let _ = handle;
    });
}