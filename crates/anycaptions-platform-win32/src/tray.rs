//! Notification-area icon and the existing Portuguese recovery menu.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CREATESTRUCTW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, HICON,
                HMENU, HWND_MESSAGE, MF_BYCOMMAND, MF_GRAYED, MF_STRING, ModifyMenuW, PostMessageW,
                RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TPM_NONOTIFY,
                TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, UnregisterClassW, WINDOW_STYLE,
                WM_APP, WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
            },
        },
    },
    core::{Error, PCWSTR, Result},
};

const WM_APP_TRAY: u32 = WM_APP + 0x52;
const TRAY_ICON_ID: u32 = 1;
static NEXT_CLASS: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowEdit,
    ToggleCapture,
    ToggleLock,
    Quit,
}

impl TrayCommand {
    fn id(self) -> usize {
        match self {
            Self::ShowEdit => 1,
            Self::ToggleCapture => 2,
            Self::ToggleLock => 3,
            Self::Quit => 4,
        }
    }

    fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::ShowEdit),
            2 => Some(Self::ToggleCapture),
            3 => Some(Self::ToggleLock),
            4 => Some(Self::Quit),
            _ => None,
        }
    }
}

/// Menu order and enablement are pure so resource fallback can be tested natively.
pub(crate) fn menu_spec(can_capture: bool) -> [(TrayCommand, &'static str, bool); 4] {
    [
        (TrayCommand::ShowEdit, "Mostrar e editar", true),
        (
            TrayCommand::ToggleCapture,
            if can_capture {
                "Iniciar/parar legendas"
            } else {
                "Iniciar/parar (áudio e modelos indisponíveis)"
            },
            can_capture,
        ),
        (TrayCommand::ToggleLock, "Bloquear/desbloquear", true),
        (TrayCommand::Quit, "Sair", true),
    ]
}

struct TrayState {
    menu: HMENU,
    callback: Box<dyn Fn(TrayCommand) + Send + Sync + 'static>,
}

/// Owns the icon, menu, and message-only receiver; the application icon remains caller-owned.
pub struct TrayService {
    hwnd: HWND,
    menu: HMENU,
    instance: HINSTANCE,
    class_name: Vec<u16>,
    icon: NOTIFYICONDATAW,
    _state: Box<TrayState>,
}

impl TrayService {
    pub fn new(
        icon: HICON,
        tooltip: &str,
        can_capture: bool,
        callback: impl Fn(TrayCommand) + Send + Sync + 'static,
    ) -> Result<Self> {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
        let class_name: Vec<u16> = format!(
            "AnyCaptions.Task6.Tray.{}.{}",
            std::process::id(),
            NEXT_CLASS.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let menu = unsafe { CreatePopupMenu()? };
        let mut state = Box::new(TrayState {
            menu,
            callback: Box::new(callback),
        });
        if let Err(error) = unsafe { populate_menu(menu, can_capture) } {
            unsafe {
                let _ = DestroyMenu(menu);
            }
            return Err(error);
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(tray_wndproc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            unsafe {
                let _ = DestroyMenu(menu);
            }
            return Err(Error::from_thread());
        }
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                Some(&mut *state as *mut TrayState as *const core::ffi::c_void),
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(error) => {
                unsafe {
                    let _ = DestroyMenu(menu);
                    let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
                }
                return Err(error);
            }
        };
        let mut icon_data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip: Vec<u16> = tooltip.encode_utf16().take(127).collect();
        icon_data.szTip[..tip.len()].copy_from_slice(&tip);
        unsafe {
            // Legacy callbacks deliver WM_LBUTTONUP/WM_RBUTTONUP directly in lParam.
            if let Err(error) = Shell_NotifyIconW(NIM_ADD, &icon_data).ok() {
                let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data).ok();
                let _ = DestroyWindow(hwnd);
                let _ = DestroyMenu(menu);
                let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
                return Err(error);
            }
        }
        Ok(Self {
            hwnd,
            menu,
            instance,
            class_name,
            icon: icon_data,
            _state: state,
        })
    }

    pub fn set_capture_enabled(&mut self, enabled: bool) -> Result<()> {
        let label = menu_spec(enabled)[1].1;
        let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        unsafe {
            ModifyMenuW(
                self.menu,
                TrayCommand::ToggleCapture.id() as u32,
                MF_BYCOMMAND | MF_STRING | if enabled { MF_STRING } else { MF_GRAYED },
                TrayCommand::ToggleCapture.id(),
                PCWSTR(label.as_ptr()),
            )
        }
    }

    pub fn message_hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for TrayService {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.icon).ok();
            let _ = DestroyWindow(self.hwnd);
            let _ = DestroyMenu(self.menu);
            let _ = UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance));
        }
    }
}

unsafe fn populate_menu(menu: HMENU, can_capture: bool) -> Result<()> {
    for (command, label, enabled) in menu_spec(can_capture) {
        let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        unsafe {
            AppendMenuW(
                menu,
                MF_STRING | if enabled { MF_STRING } else { MF_GRAYED },
                command.id(),
                PCWSTR(label.as_ptr()),
            )?;
        }
    }
    Ok(())
}

unsafe extern "system" fn tray_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return LRESULT(1);
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const TrayState };
    if state.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &*state };
    let event = lparam.0 as u32 & 0xFFFF;
    if message == WM_APP_TRAY && matches!(event, WM_LBUTTONUP | WM_RBUTTONUP) {
        unsafe {
            show_menu(hwnd, state);
        }
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe fn show_menu(hwnd: HWND, state: &TrayState) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            state.menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        if let Some(command) = TrayCommand::from_id(selected.0 as u32) {
            let _ = catch_unwind(AssertUnwindSafe(|| (state.callback)(command)));
        }
    }
}
