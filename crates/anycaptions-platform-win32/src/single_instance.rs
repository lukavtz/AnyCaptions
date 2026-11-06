//! Named single-instance guard with registered-message activation forwarding.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    Win32::{
        Foundation::{
            CloseHandle, E_INVALIDARG, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND,
            LPARAM, LRESULT, WPARAM,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{CreateMutexW, ReleaseMutex},
        },
        UI::WindowsAndMessaging::{
            CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
            GetWindowLongPtrW, HWND_BROADCAST, PostMessageW, RegisterClassW,
            RegisterWindowMessageW, SetWindowLongPtrW, UnregisterClassW, WINDOW_STYLE, WM_NCCREATE,
            WM_NCDESTROY, WNDCLASSW,
        },
    },
    core::{Error, PCWSTR, Result},
};

static NEXT_CLASS: AtomicU32 = AtomicU32::new(1);

struct InstanceState {
    activation_message: u32,
    callback: Box<dyn Fn() + Send + Sync + 'static>,
}

struct ActivationReceiver {
    hwnd: HWND,
    instance: HINSTANCE,
    class_name: Vec<u16>,
    _state: Box<InstanceState>,
}

impl ActivationReceiver {
    fn new(
        activation_message: u32,
        callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<Self> {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
        let class_name: Vec<u16> = format!(
            "AnyCaptions.Task6.SingleInstance.{}.{}",
            std::process::id(),
            NEXT_CLASS.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let mut state = Box::new(InstanceState {
            activation_message,
            callback,
        });
        let class = WNDCLASSW {
            lpfnWndProc: Some(instance_wndproc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(Error::from_thread());
        }
        // The hidden HWND exists before mutex acquisition, closing the activation race.
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
                None,
                None,
                Some(instance),
                Some(&mut *state as *mut InstanceState as *const core::ffi::c_void),
            )
        };
        match hwnd {
            Ok(hwnd) => Ok(Self {
                hwnd,
                instance,
                class_name,
                _state: state,
            }),
            Err(error) => {
                unsafe {
                    let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
                }
                Err(error)
            }
        }
    }

    fn destroy(&self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance));
        }
    }
}

/// Keeps the primary mutex and hidden activation receiver alive.
/// `Ok(None)` means this was a secondary instance and activation was forwarded.
pub struct SingleInstance {
    mutex: HANDLE,
    receiver: ActivationReceiver,
}

impl SingleInstance {
    pub fn acquire(
        name: &str,
        callback: impl Fn() + Send + Sync + 'static,
    ) -> Result<Option<Self>> {
        let mutex_name = wide(name)?;
        let activation_message = activation_message(name)?;
        let receiver = ActivationReceiver::new(activation_message, Box::new(callback))?;
        let mutex = match unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) } {
            Ok(mutex) => mutex,
            Err(error) => {
                receiver.destroy();
                return Err(error);
            }
        };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(mutex);
            }
            receiver.destroy();
            unsafe {
                PostMessageW(
                    Some(HWND_BROADCAST),
                    activation_message,
                    WPARAM(0),
                    LPARAM(0),
                )?;
            }
            return Ok(None);
        }
        Ok(Some(Self { mutex, receiver }))
    }

    /// Forwards activation without window titles or cross-process raw-handle lookup.
    pub fn activate_existing(name: &str) -> Result<()> {
        let message = activation_message(name)?;
        unsafe { PostMessageW(Some(HWND_BROADCAST), message, WPARAM(0), LPARAM(0)) }
    }

    pub fn activation_hwnd(&self) -> HWND {
        self.receiver.hwnd
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        self.receiver.destroy();
        unsafe {
            let _ = ReleaseMutex(self.mutex);
            let _ = CloseHandle(self.mutex);
        }
    }
}

fn wide(name: &str) -> Result<Vec<u16>> {
    if name.is_empty() || name.contains('\0') {
        return Err(Error::new(
            E_INVALIDARG,
            "Single-instance name must be non-empty and contain no NUL",
        ));
    }
    Ok(name.encode_utf16().chain(Some(0)).collect())
}

fn activation_message(name: &str) -> Result<u32> {
    let name = wide(&format!("{name}.activation"))?;
    let message = unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) };
    if message == 0 {
        Err(Error::from_thread())
    } else {
        Ok(message)
    }
}

unsafe extern "system" fn instance_wndproc(
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
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const InstanceState };
    if state.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &*state };
    if message == state.activation_message {
        let _ = catch_unwind(AssertUnwindSafe(|| (state.callback)()));
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
