//! Raw-HWND overlay styling and geometry.

use anycaptions_core::window_geometry::{Rect, WorkArea};
use raw_window_handle::RawWindowHandle;
use windows::{
    Win32::{
        Foundation::{
            E_HANDLE, GetLastError, HWND, LPARAM, RECT, SetLastError, WIN32_ERROR, WPARAM,
        },
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST,
            MONITORINFOEXW, MonitorFromWindow,
        },
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::ReleaseCapture,
            WindowsAndMessaging::{
                GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HTCAPTION, HWND_TOPMOST,
                IsWindowVisible, MONITORINFOF_PRIMARY, SC_SIZE, SW_HIDE, SW_SHOWNOACTIVATE,
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW,
                SetWindowLongPtrW, SetWindowPos, ShowWindowAsync, WM_NCLBUTTONDOWN, WM_SYSCOMMAND,
                WMSZ_BOTTOM, WMSZ_LEFT, WMSZ_RIGHT,
            },
        },
    },
    core::{BOOL, Error, Result},
};

pub const WS_EX_TRANSPARENT: u32 = 0x20;
pub const WS_EX_TOOLWINDOW: u32 = 0x80;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const WS_EX_TOPMOST: u32 = 0x8;

pub const MIN_WINDOW_WIDTH: f64 = 320.0;
pub const MIN_WINDOW_HEIGHT: f64 = 160.0;

/// Normal overlay style: present in the taskbar, layered, topmost, and never activated.
pub fn base_ex_style() -> u32 {
    WS_EX_APPWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED
}

pub fn set_click_through(ex_style: u32, enabled: bool) -> u32 {
    if enabled {
        ex_style | WS_EX_TRANSPARENT
    } else {
        ex_style & !WS_EX_TRANSPARENT
    }
}

pub fn is_click_through(ex_style: u32) -> bool {
    ex_style & WS_EX_TRANSPARENT != 0
}

/// Preserves unrelated GPUI bits while enforcing the Normal overlay contract.
pub fn overlay_ex_style(existing: u32, locked: bool) -> u32 {
    set_click_through((existing | base_ex_style()) & !WS_EX_TOOLWINDOW, locked)
}

/// Dictation uses a non-taskbar popup and always passes mouse input through.
pub fn indicator_ex_style(existing: u32) -> u32 {
    (existing
        | WS_EX_TOOLWINDOW
        | WS_EX_TOPMOST
        | WS_EX_NOACTIVATE
        | WS_EX_LAYERED
        | WS_EX_TRANSPARENT)
        & !WS_EX_APPWINDOW
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    West,
    East,
    South,
}

impl ResizeEdge {
    fn sys_command(self) -> u32 {
        match self {
            Self::West => WMSZ_LEFT,
            Self::East => WMSZ_RIGHT,
            Self::South => WMSZ_BOTTOM,
        }
    }
}

/// Resizes around the opposite fixed edge, preserving the upstream minimum size.
pub fn resize_rect(original: Rect, edge: ResizeEdge, delta_x: f64, delta_y: f64) -> Rect {
    let right = original.x + original.width;
    let mut resized = original;
    match edge {
        ResizeEdge::West => {
            resized.x = (original.x + delta_x).min(right - MIN_WINDOW_WIDTH.min(original.width));
            resized.width = right - resized.x;
        }
        ResizeEdge::East => {
            resized.width = (original.width + delta_x).max(MIN_WINDOW_WIDTH.min(original.width));
        }
        ResizeEdge::South => {
            resized.height =
                (original.height + delta_y).max(MIN_WINDOW_HEIGHT.min(original.height));
        }
    }
    resized
}

/// Native effects for a GPUI-owned window. The HWND is borrowed, never destroyed here.
pub struct WindowEffects {
    hwnd: HWND,
}

impl WindowEffects {
    pub fn new(hwnd: HWND) -> Result<Self> {
        if hwnd.is_invalid() {
            return Err(Error::new(E_HANDLE, "Invalid AnyCaptions window handle"));
        }
        Ok(Self { hwnd })
    }

    pub fn from_raw_handle(handle: RawWindowHandle) -> Result<Self> {
        match handle {
            RawWindowHandle::Win32(handle) => {
                Self::new(HWND(handle.hwnd.get() as *mut core::ffi::c_void))
            }
            _ => Err(Error::new(E_HANDLE, "Expected a Win32 window handle")),
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn apply_overlay(&self, locked: bool) -> Result<()> {
        let existing = self.ex_style()?;
        self.apply_style(overlay_ex_style(existing, locked))
    }

    pub fn set_click_through(&self, enabled: bool) -> Result<()> {
        self.apply_overlay(enabled)
    }

    pub fn click_through(&self) -> Result<bool> {
        self.ex_style().map(is_click_through)
    }

    pub fn apply_indicator(&self) -> Result<()> {
        let existing = self.ex_style()?;
        self.apply_style(indicator_ex_style(existing))
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn hide(&self) -> Result<()> {
        unsafe { ShowWindowAsync(self.hwnd, SW_HIDE).ok() }
    }

    /// Shows without stealing foreground focus and restores topmost ordering.
    pub fn show(&self) -> Result<()> {
        unsafe {
            ShowWindowAsync(self.hwnd, SW_SHOWNOACTIVATE).ok()?;
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
    }

    /// Converts logical geometry at the final native boundary.
    pub fn move_resize(&self, geometry: Rect, area: &WorkArea) -> Result<()> {
        let scale = area.scale;
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                (geometry.x * scale).round() as i32,
                (geometry.y * scale).round() as i32,
                (geometry.width * scale).round() as i32,
                (geometry.height * scale).round() as i32,
                SWP_NOACTIVATE,
            )
        }
    }

    pub fn geometry(&self) -> Result<Rect> {
        let area = self.current_work_area()?;
        self.geometry_at_scale(area.scale)
    }

    fn geometry_at_scale(&self, scale: f64) -> Result<Rect> {
        let mut native = RECT::default();
        unsafe { GetWindowRect(self.hwnd, &mut native)? };
        Ok(Rect::new(
            native.left as f64 / scale,
            native.top as f64 / scale,
            (native.right - native.left) as f64 / scale,
            (native.bottom - native.top) as f64 / scale,
        ))
    }

    /// Applies a physical-pixel drag delta with logical minimum dimensions.
    pub fn resize(&self, edge: ResizeEdge, delta_x: i32, delta_y: i32) -> Result<()> {
        let area = self.current_work_area()?;
        let geometry = self.geometry_at_scale(area.scale)?;
        let resized = resize_rect(
            geometry,
            edge,
            delta_x as f64 / area.scale,
            delta_y as f64 / area.scale,
        );
        self.move_resize(resized, &area)
    }

    pub fn begin_move(&self) -> Result<()> {
        unsafe {
            ReleaseCapture()?;
            SendMessageW(
                self.hwnd,
                WM_NCLBUTTONDOWN,
                Some(WPARAM(HTCAPTION as usize)),
                Some(LPARAM(0)),
            );
        }
        Ok(())
    }

    pub fn begin_resize(&self, edge: ResizeEdge) -> Result<()> {
        unsafe {
            ReleaseCapture()?;
            SendMessageW(
                self.hwnd,
                WM_SYSCOMMAND,
                Some(WPARAM((SC_SIZE | edge.sys_command()) as usize)),
                Some(LPARAM(0)),
            );
        }
        Ok(())
    }

    pub fn current_work_area(&self) -> Result<WorkArea> {
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_invalid() {
            return Err(Error::new(E_HANDLE, "Window has no nearest monitor"));
        }
        work_area_from_monitor(monitor)
    }

    fn ex_style(&self) -> Result<u32> {
        unsafe { window_long(self.hwnd) }
    }

    fn apply_style(&self, style: u32) -> Result<()> {
        unsafe {
            // SetWindowPos owns topmost Z-order. Apply every other style first,
            // then mirror TOPMOST so the style cache cannot clear it afterward.
            set_window_ex_style(self.hwnd, style & !WS_EX_TOPMOST)?;
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )?;
            set_window_ex_style(self.hwnd, style)
        }
    }
}

/// Enumerates monitor work areas in logical coordinates, with the primary monitor first.
pub fn work_areas() -> Result<Vec<WorkArea>> {
    struct Context {
        areas: Vec<WorkArea>,
        error: Option<Error>,
    }

    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _hdc: windows::Win32::Graphics::Gdi::HDC,
        _clip: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let context = unsafe { &mut *(data.0 as *mut Context) };
        match monitor_work_area(monitor) {
            Ok((area, primary)) => {
                if primary {
                    context.areas.insert(0, area);
                } else {
                    context.areas.push(area);
                }
                true.into()
            }
            Err(error) => {
                context.error = Some(error);
                false.into()
            }
        }
    }

    let mut context = Context {
        areas: Vec::new(),
        error: None,
    };
    unsafe {
        let result = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut context as *mut Context as isize),
        );
        if let Some(error) = context.error {
            return Err(error);
        }
        if !result.as_bool() {
            return Err(Error::from_thread());
        }
    }
    Ok(context.areas)
}

fn work_area_from_monitor(monitor: HMONITOR) -> Result<WorkArea> {
    monitor_work_area(monitor).map(|(area, _)| area)
}

fn monitor_work_area(monitor: HMONITOR) -> Result<(WorkArea, bool)> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe {
        GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut _).ok()?;
    }
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)? };
    let scale = dpi_x as f64 / 96.0;
    let work = info.monitorInfo.rcWork;
    let name_end = info
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szDevice.len());
    let name = String::from_utf16_lossy(&info.szDevice[..name_end]);
    let area = WorkArea::new(
        name,
        Rect::new(
            work.left as f64 / scale,
            work.top as f64 / scale,
            (work.right - work.left) as f64 / scale,
            (work.bottom - work.top) as f64 / scale,
        ),
        scale,
    );
    Ok((area, info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0))
}

unsafe fn set_window_ex_style(hwnd: HWND, style: u32) -> Result<()> {
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        let error = GetLastError();
        if previous == 0 && error.0 != 0 {
            Err(Error::from_thread())
        } else {
            Ok(())
        }
    }
}

unsafe fn window_long(hwnd: HWND) -> Result<u32> {
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let value = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let error = GetLastError();
        if value == 0 && error.0 != 0 {
            Err(Error::from_thread())
        } else {
            Ok(value as u32)
        }
    }
}
