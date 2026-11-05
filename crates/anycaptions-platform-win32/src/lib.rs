//! Native Win32 adapters for AnyCaptions windows and operating-system services.
//!
//! Pure geometry and presenter logic remain in `anycaptions-core`.

pub mod accessibility;
pub mod hotkeys;
pub mod single_instance;
pub mod system;
pub mod tray;
pub mod window;

pub use hotkeys::HotkeyService;
pub use single_instance::SingleInstance;
pub use tray::{TrayCommand, TrayService};
pub use window::{
    ResizeEdge, WindowEffects, base_ex_style, indicator_ex_style, is_click_through,
    overlay_ex_style, resize_rect, set_click_through, work_areas,
};

#[cfg(test)]
mod accessibility_tests;
#[cfg(test)]
mod tests;
