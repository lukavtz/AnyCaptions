//! Global hotkeys owned by a message-only window on a dedicated UI thread.

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Sender, SyncSender},
    },
    thread::{self, JoinHandle, ThreadId},
};

use anycaptions_core::{
    dictation::{DictationError, runtime::HotkeyRegistry},
    ipc::{CommandError, ShortcutRegistry},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
                RegisterHotKey, UnregisterHotKey, VK_CONTROL, VK_ESCAPE, VK_F6, VK_MENU, VK_SHIFT,
                VK_SPACE,
            },
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
                GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, KillTimer, MSG,
                PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer,
                SetWindowLongPtrW, TranslateMessage, UnregisterClassW, WINDOW_STYLE, WM_APP,
                WM_HOTKEY, WM_NCCREATE, WM_NCDESTROY, WM_TIMER, WNDCLASSW,
            },
        },
    },
    core::{Error, PCWSTR, Result},
};

const WM_APP_COMMAND: u32 = WM_APP + 0x51;
const RELEASE_TIMER: usize = 1;
const RELEASE_POLL_MS: u32 = 25;
static NEXT_CLASS: AtomicU32 = AtomicU32::new(1);

type Callback = Arc<dyn Fn(bool) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hotkey {
    modifiers: u32,
    vk: u32,
}

impl Hotkey {
    fn is_down(self) -> bool {
        let mut keys = [self.vk as u16, 0, 0, 0];
        let mut count = 1;
        for (flag, key) in [
            (MOD_CONTROL.0, VK_CONTROL.0),
            (MOD_ALT.0, VK_MENU.0),
            (MOD_SHIFT.0, VK_SHIFT.0),
        ] {
            if self.modifiers & flag != 0 {
                keys[count] = key;
                count += 1;
            }
        }
        keys[..count]
            .iter()
            .all(|key| unsafe { GetAsyncKeyState(*key as i32) as u16 & 0x8000 != 0 })
    }
}

fn parse_hotkey(shortcut: &str, allow_escape: bool) -> Option<(String, Hotkey)> {
    if allow_escape && shortcut.trim().eq_ignore_ascii_case("escape") {
        return Some((
            "Escape".into(),
            Hotkey {
                modifiers: 0,
                vk: VK_ESCAPE.0 as u32,
            },
        ));
    }
    let canonical = anycaptions_core::dictation::normalize_shortcut(shortcut).ok()?;
    let (modifier_names, key) = canonical
        .rsplit_once('+')
        .map_or(("", canonical.as_str()), |(modifiers, key)| {
            (modifiers, key)
        });
    let modifiers = modifier_names
        .split('+')
        .filter(|name| !name.is_empty())
        .try_fold(0, |modifiers, name| {
            modifier_flag(name).map(|flag| modifiers | flag)
        })?;
    let vk = virtual_key(key);
    Some((canonical, Hotkey { modifiers, vk }))
}

fn modifier_flag(name: &str) -> Option<u32> {
    match name {
        "Ctrl" => Some(MOD_CONTROL.0),
        "Alt" => Some(MOD_ALT.0),
        "Shift" => Some(MOD_SHIFT.0),
        _ => None,
    }
}

fn virtual_key(key: &str) -> u32 {
    if key == "Space" {
        return VK_SPACE.0 as u32;
    }
    if let Some(number) = key
        .strip_prefix('F')
        .and_then(|digits| digits.parse::<u16>().ok())
    {
        return (VK_F6.0 + number - 6) as u32;
    }
    key.as_bytes()[0] as u32
}

struct Registration {
    id: i32,
    key: Hotkey,
    callback: Callback,
    active: bool,
}

#[derive(Default)]
struct Registry {
    by_shortcut: BTreeMap<String, Registration>,
    next_id: i32,
}

struct WindowState {
    registry: Mutex<Registry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistrationError {
    InvalidShortcut,
    Unavailable,
}

enum RegistryRequest {
    Register {
        callback: Callback,
        allow_escape: bool,
    },
    Unregister,
}

enum Command {
    Register {
        shortcut: String,
        callback: Callback,
        allow_escape: bool,
        response: Sender<std::result::Result<(), RegistrationError>>,
    },
    Unregister {
        shortcut: String,
        response: Option<Sender<std::result::Result<(), RegistrationError>>>,
    },
    Shutdown,
}

/// Registrations and overlay unregistration are synchronous; dictation unregistration never waits for a response.
pub struct HotkeyService {
    hwnd: usize,
    thread: Mutex<Option<JoinHandle<()>>>,
    thread_id: ThreadId,
    overlay_callback: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl HotkeyService {
    pub fn new(overlay_callback: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("anycaptions-hotkeys".into())
            .spawn(move || run_message_window(ready_tx))
            .map_err(|error| Error::new(E_FAIL, error.to_string()))?;
        let thread_id = thread.thread().id();
        let hwnd = match ready_rx.recv() {
            Ok(Ok(hwnd)) => hwnd,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(error) => {
                let _ = thread.join();
                return Err(Error::new(E_FAIL, error.to_string()));
            }
        };
        Ok(Self {
            hwnd,
            thread: Mutex::new(Some(thread)),
            thread_id,
            overlay_callback: Arc::new(overlay_callback),
        })
    }

    pub fn message_hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut core::ffi::c_void)
    }

    fn dispatch(&self, command: Command) -> Result<()> {
        if thread::current().id() == self.thread_id {
            let hwnd = self.message_hwnd();
            let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState };
            if !state.is_null() {
                unsafe {
                    if matches!(&command, Command::Register { .. }) {
                        drain_queued_commands(hwnd, &*state);
                    }
                    handle_command(hwnd, &*state, command);
                }
                return Ok(());
            }
        }
        let command = Box::into_raw(Box::new(command));
        unsafe {
            if let Err(error) = PostMessageW(
                Some(self.message_hwnd()),
                WM_APP_COMMAND,
                WPARAM(command as usize),
                LPARAM(0),
            ) {
                drop(Box::from_raw(command));
                return Err(error);
            }
        }
        Ok(())
    }

    fn register(
        &self,
        shortcut: &str,
        callback: Callback,
        allow_escape: bool,
    ) -> std::result::Result<(), RegistrationError> {
        self.request(
            shortcut,
            RegistryRequest::Register {
                callback,
                allow_escape,
            },
        )
    }

    fn unregister(&self, shortcut: &str) -> std::result::Result<(), RegistrationError> {
        self.request(shortcut, RegistryRequest::Unregister)
    }

    fn unregister_detached(&self, shortcut: &str) {
        let _ = self.dispatch(Command::Unregister {
            shortcut: shortcut.into(),
            response: None,
        });
    }

    fn request(
        &self,
        shortcut: &str,
        request: RegistryRequest,
    ) -> std::result::Result<(), RegistrationError> {
        let (response, result) = mpsc::channel();
        let shortcut = shortcut.into();
        let command = match request {
            RegistryRequest::Register {
                callback,
                allow_escape,
            } => Command::Register {
                shortcut,
                callback,
                allow_escape,
                response,
            },
            RegistryRequest::Unregister => Command::Unregister {
                shortcut,
                response: Some(response),
            },
        };
        self.dispatch(command)
            .map_err(|_| RegistrationError::Unavailable)?;
        result.recv().unwrap_or(Err(RegistrationError::Unavailable))
    }
}

impl ShortcutRegistry for HotkeyService {
    fn register(&self, shortcut: &str) -> std::result::Result<(), CommandError> {
        let overlay = Arc::clone(&self.overlay_callback);
        self.register(
            shortcut,
            Arc::new(move |pressed| {
                if pressed {
                    overlay();
                }
            }),
            false,
        )
        .map_err(|_| CommandError::ShortcutUnavailable)
    }

    fn unregister(&self, shortcut: &str) -> std::result::Result<(), CommandError> {
        HotkeyService::unregister(self, shortcut).map_err(|_| CommandError::ShortcutUnavailable)
    }
}

impl HotkeyRegistry for HotkeyService {
    fn register(
        &self,
        shortcut: &str,
        callback: Box<dyn Fn(bool) + Send + Sync + 'static>,
    ) -> std::result::Result<(), DictationError> {
        HotkeyService::register(self, shortcut, Arc::from(callback), true).map_err(|error| {
            match error {
                RegistrationError::InvalidShortcut => DictationError::InvalidShortcut,
                RegistrationError::Unavailable => DictationError::ShortcutConflict,
            }
        })
    }

    fn unregister(&self, shortcut: &str) {
        self.unregister_detached(shortcut);
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        let _ = self.dispatch(Command::Shutdown);
        if thread::current().id() != self.thread_id
            && let Ok(mut thread) = self.thread.lock()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}

fn run_message_window(ready: SyncSender<Result<usize>>) {
    match create_message_window() {
        Ok((hwnd, class_name, instance, state)) => {
            let _ = ready.send(Ok(hwnd.0 as usize));
            let mut message = MSG::default();
            unsafe {
                loop {
                    let result = GetMessageW(&mut message, None, 0, 0);
                    if result.0 == -1 || result.0 == 0 {
                        break;
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = DestroyWindow(hwnd);
                let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
            }
            drop(state);
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

fn create_message_window() -> Result<(HWND, Vec<u16>, HINSTANCE, Box<WindowState>)> {
    let class_name: Vec<u16> = format!(
        "AnyCaptions.Task6.Hotkeys.{}.{}",
        std::process::id(),
        NEXT_CLASS.fetch_add(1, Ordering::Relaxed)
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
    let state = Box::new(WindowState {
        registry: Mutex::new(Registry::default()),
    });
    let class = WNDCLASSW {
        lpfnWndProc: Some(hotkey_wndproc),
        hInstance: instance,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
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
            Some(&*state as *const WindowState as *const core::ffi::c_void),
        )
    };
    match hwnd {
        Ok(hwnd) => Ok((hwnd, class_name, instance, state)),
        Err(error) => {
            unsafe {
                let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
            }
            Err(error)
        }
    }
}

unsafe extern "system" fn hotkey_wndproc(
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
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState };
    if state.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &*state };
    match message {
        WM_APP_COMMAND => {
            let command = unsafe { Box::from_raw(wparam.0 as *mut Command) };
            handle_command(hwnd, state, *command);
            LRESULT(0)
        }
        WM_HOTKEY => {
            handle_hotkey(hwnd, state, wparam.0 as i32);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == RELEASE_TIMER => {
            handle_release_timer(hwnd, state);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Preserves posting order when a callback registers directly on the hotkey thread.
fn drain_queued_commands(hwnd: HWND, state: &WindowState) {
    loop {
        let mut message = MSG::default();
        let queued = unsafe {
            PeekMessageW(
                &mut message,
                Some(hwnd),
                WM_APP_COMMAND,
                WM_APP_COMMAND,
                PM_REMOVE,
            )
        };
        if !queued.as_bool() {
            return;
        }
        let command = unsafe { Box::from_raw(message.wParam.0 as *mut Command) };
        handle_command(hwnd, state, *command);
    }
}

fn handle_command(hwnd: HWND, state: &WindowState, command: Command) {
    match command {
        Command::Register {
            shortcut,
            callback,
            allow_escape,
            response,
        } => {
            let _ = response.send(register_native(
                hwnd,
                state,
                &shortcut,
                callback,
                allow_escape,
            ));
        }
        Command::Unregister { shortcut, response } => {
            let result = unregister_native(hwnd, state, &shortcut);
            if let Some(response) = response {
                let _ = response.send(result);
            }
        }
        Command::Shutdown => {
            let releases = {
                let Ok(mut registry) = state.registry.lock() else {
                    unsafe { PostQuitMessage(0) };
                    return;
                };
                let registrations = std::mem::take(&mut registry.by_shortcut);
                let mut releases = Vec::new();
                for (_, registration) in registrations {
                    unsafe {
                        let _ = UnregisterHotKey(Some(hwnd), registration.id);
                    }
                    if registration.active {
                        releases.push(registration.callback);
                    }
                }
                releases
            };
            for callback in releases {
                invoke(callback, false);
            }
            unsafe {
                let _ = KillTimer(Some(hwnd), RELEASE_TIMER);
                PostQuitMessage(0);
            }
        }
    }
}

fn register_native(
    hwnd: HWND,
    state: &WindowState,
    shortcut: &str,
    callback: Callback,
    allow_escape: bool,
) -> std::result::Result<(), RegistrationError> {
    let Some((canonical, key)) = parse_hotkey(shortcut, allow_escape) else {
        return Err(RegistrationError::InvalidShortcut);
    };
    let Ok(mut registry) = state.registry.lock() else {
        return Err(RegistrationError::Unavailable);
    };
    if registry.by_shortcut.contains_key(&canonical) {
        return Err(RegistrationError::Unavailable);
    }
    let id = (1..=0xBFFF)
        .skip(registry.next_id as usize)
        .chain(1..=registry.next_id)
        .find(|id| {
            !registry
                .by_shortcut
                .values()
                .any(|registration| registration.id == *id)
        })
        .ok_or(RegistrationError::Unavailable)?;
    unsafe {
        RegisterHotKey(
            Some(hwnd),
            id,
            HOT_KEY_MODIFIERS(key.modifiers | MOD_NOREPEAT.0),
            key.vk,
        )
        .map_err(|_| RegistrationError::Unavailable)?;
    }
    registry.next_id = id;
    registry.by_shortcut.insert(
        canonical,
        Registration {
            id,
            key,
            callback,
            active: false,
        },
    );
    Ok(())
}

fn unregister_native(
    hwnd: HWND,
    state: &WindowState,
    shortcut: &str,
) -> std::result::Result<(), RegistrationError> {
    let canonical = if shortcut.trim().eq_ignore_ascii_case("escape") {
        "Escape".into()
    } else {
        anycaptions_core::dictation::normalize_shortcut(shortcut)
            .map_err(|_| RegistrationError::Unavailable)?
    };
    let release = {
        let Ok(mut registry) = state.registry.lock() else {
            return Err(RegistrationError::Unavailable);
        };
        let Some(registration) = registry.by_shortcut.get(&canonical) else {
            return Ok(());
        };
        unsafe {
            UnregisterHotKey(Some(hwnd), registration.id)
                .map_err(|_| RegistrationError::Unavailable)?;
        }
        let registration = registry.by_shortcut.remove(&canonical);
        registration.filter(|registration| registration.active)
    };
    if let Some(registration) = release {
        invoke(registration.callback, false);
    }
    Ok(())
}

fn handle_hotkey(hwnd: HWND, state: &WindowState, id: i32) {
    let callback = {
        let Ok(mut registry) = state.registry.lock() else {
            return;
        };
        let Some(registration) = registry
            .by_shortcut
            .values_mut()
            .find(|registration| registration.id == id)
        else {
            return;
        };
        if registration.active {
            return;
        }
        registration.active = true;
        registration.callback.clone()
    };
    let timer = unsafe { SetTimer(Some(hwnd), RELEASE_TIMER, RELEASE_POLL_MS, None) };
    invoke(callback.clone(), true);
    if timer == 0 {
        if let Ok(mut registry) = state.registry.lock()
            && let Some(registration) = registry
                .by_shortcut
                .values_mut()
                .find(|registration| registration.id == id)
        {
            registration.active = false;
        }
        invoke(callback, false);
    }
}

fn handle_release_timer(hwnd: HWND, state: &WindowState) {
    let (releases, active) = {
        let Ok(mut registry) = state.registry.lock() else {
            return;
        };
        let mut releases = Vec::new();
        let mut active = false;
        for registration in registry.by_shortcut.values_mut() {
            if registration.active && !registration.key.is_down() {
                registration.active = false;
                releases.push(registration.callback.clone());
            }
            active |= registration.active;
        }
        (releases, active)
    };
    if !active {
        unsafe {
            let _ = KillTimer(Some(hwnd), RELEASE_TIMER);
        }
    }
    for callback in releases {
        invoke(callback, false);
    }
}

fn invoke(callback: Callback, pressed: bool) {
    let _ = catch_unwind(AssertUnwindSafe(|| callback(pressed)));
}
