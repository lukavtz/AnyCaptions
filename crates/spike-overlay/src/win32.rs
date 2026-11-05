//! Win32 window discovery and style manipulation for the overlay spike.
//!
//! Finds the overlay window by its title and applies click-through styles.

#![allow(dead_code)]

use std::ffi::c_void;

use spike_overlay::style::{self, Mode};

/// Win32 `HWND` handle.
pub type Hwnd = *mut c_void;

const GWL_EXSTYLE: i32 = -20;
const HWND_TOPMOST: Hwnd = -1_isize as Hwnd;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;

#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> Hwnd;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
    fn SetWindowPos(
        hwnd: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

/// Finds a window by its exact title.
///
/// Returns `None` if no window matches or the title is empty.
pub fn find_window_by_title(title: &str) -> Option<Hwnd> {
    if title.is_empty() {
        return None;
    }
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a null-terminated UTF-16 string; FindWindowW only reads it.
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    if hwnd.is_null() { None } else { Some(hwnd) }
}

/// Extended style bits currently set on the window.
pub fn ex_style(hwnd: Hwnd) -> u32 {
    // SAFETY: `hwnd` is a valid window handle from FindWindowW.
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

fn set_ex_style(hwnd: Hwnd, ex_style: u32) {
    // SAFETY: valid handle; GWL_EXSTYLE is the documented index.
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
    }
}

fn raise_topmost(hwnd: Hwnd) {
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    // SAFETY: valid handle; call only changes z-order.
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
    }
}

/// Applies overlay bits on top of whatever GPUI already set.
///
/// Returns `(previous, applied)` extended styles.
pub fn apply_overlay(hwnd: Hwnd, mode: Mode) -> (u32, u32) {
    let previous = ex_style(hwnd);
    let applied = previous | style::ex_style_for(mode);
    if applied != previous {
        set_ex_style(hwnd, applied);
    }
    raise_topmost(hwnd);
    (previous, ex_style(hwnd))
}