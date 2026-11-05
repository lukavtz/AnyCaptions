use anycaptions_core::window_geometry::Rect;

use crate::{
    ResizeEdge, base_ex_style, indicator_ex_style, is_click_through, overlay_ex_style, resize_rect,
    set_click_through,
    tray::{TrayCommand, menu_spec},
    window::{
        WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT,
    },
};

const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

#[test]
fn overlay_style_is_normal_topmost_and_reversibly_click_through() {
    let interactive = base_ex_style();
    assert_ne!(interactive & WS_EX_APPWINDOW, 0);
    assert_ne!(interactive & WS_EX_TOPMOST, 0);
    assert_ne!(interactive & WS_EX_NOACTIVATE, 0);
    assert_ne!(interactive & WS_EX_LAYERED, 0);
    assert_eq!(interactive & WS_EX_TOOLWINDOW, 0);

    let locked = set_click_through(interactive, true);
    assert!(is_click_through(locked));
    assert_eq!(set_click_through(locked, false), interactive);

    let preserved = overlay_ex_style(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW, true);
    assert_ne!(preserved & WS_EX_NOREDIRECTIONBITMAP, 0);
    assert_eq!(preserved & WS_EX_TOOLWINDOW, 0);
    assert_ne!(preserved & WS_EX_TRANSPARENT, 0);
    assert_eq!(
        set_click_through(preserved, false),
        WS_EX_NOREDIRECTIONBITMAP | interactive
    );
}

#[test]
fn indicator_style_is_popup_and_permanently_click_through() {
    let style = indicator_ex_style(WS_EX_APPWINDOW);
    assert_ne!(style & WS_EX_TOOLWINDOW, 0);
    assert_ne!(style & WS_EX_TOPMOST, 0);
    assert_ne!(style & WS_EX_NOACTIVATE, 0);
    assert_ne!(style & WS_EX_LAYERED, 0);
    assert_ne!(style & WS_EX_TRANSPARENT, 0);
    assert_eq!(style & WS_EX_APPWINDOW, 0);
}

#[test]
fn resize_preserves_opposite_edges_and_minimums() {
    let original = Rect::new(100, 200, 800, 240);
    let west = resize_rect(original, ResizeEdge::West, -40.0, 0.0);
    assert_eq!(west.width, original.width + 40.0);
    assert_eq!(west.x, original.x - 40.0);
    assert_eq!(west.x + west.width, original.x + original.width);

    let east = resize_rect(original, ResizeEdge::East, -900.0, 0.0);
    assert_eq!(east.x, original.x);
    assert_eq!(east.width, 320.0);

    let south = resize_rect(original, ResizeEdge::South, 0.0, -200.0);
    assert_eq!(south.y, original.y);
    assert_eq!(south.height, 160.0);
}

#[test]
fn tray_menu_preserves_labels_order_and_resource_enablement() {
    let enabled = menu_spec(true);
    assert_eq!(
        enabled.map(|(command, _, _)| command),
        [
            TrayCommand::ShowEdit,
            TrayCommand::ToggleCapture,
            TrayCommand::ToggleLock,
            TrayCommand::Quit,
        ]
    );
    assert_eq!(enabled[1].1, "Iniciar/parar legendas");
    assert!(enabled[1].2);

    let disabled = menu_spec(false);
    assert_eq!(
        disabled[1].1,
        "Iniciar/parar (áudio e modelos indisponíveis)"
    );
    assert!(!disabled[1].2);
    assert!(disabled[0].2 && disabled[2].2 && disabled[3].2);
}

#[cfg(windows)]
mod native {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicU32, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use anycaptions_core::{
        dictation::{DictationError, runtime::HotkeyRegistry},
        ipc::{CommandError, ShortcutRegistry},
        window_geometry::{Rect, WorkArea},
    };
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetWindowLongPtrW,
                GetWindowRect, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
                TranslateMessage, WINDOW_EX_STYLE, WM_HOTKEY, WS_OVERLAPPED,
            },
        },
        core::{PCWSTR, Result, w},
    };

    use crate::{
        HotkeyService, SingleInstance, WindowEffects,
        window::{
            WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
            WS_EX_TRANSPARENT,
        },
    };

    static NEXT_ID: AtomicU32 = AtomicU32::new(1);

    fn unique_name(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!(
            r"Local\AnyCaptions.Task6.Test.{label}.{}.{}.{}",
            std::process::id(),
            nanos,
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        )
    }

    struct WindowGuard(HWND);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    fn native_window() -> Result<(WindowGuard, WindowEffects)> {
        let title: Vec<u16> = unique_name("window")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0x0020_0000),
                w!("STATIC"),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPED,
                0,
                0,
                80,
                40,
                None,
                None,
                None,
                None,
            )?
        };
        let guard = WindowGuard(hwnd);
        let effects = WindowEffects::new(hwnd)?;
        Ok((guard, effects))
    }

    fn pump_messages() {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    #[test]
    fn raw_window_styles_preserve_gpui_bits_and_toggle_click_through() -> Result<()> {
        let (_guard, effects) = native_window()?;
        effects.apply_overlay(false)?;
        unsafe {
            let style = GetWindowLongPtrW(effects.hwnd(), GWL_EXSTYLE) as u32;
            assert_ne!(style & WS_EX_APPWINDOW, 0);
            assert_ne!(
                style & WS_EX_TOPMOST,
                0,
                "extended style missing topmost: {style:#010x}"
            );
            assert_ne!(style & WS_EX_NOACTIVATE, 0);
            assert_ne!(style & WS_EX_LAYERED, 0);
            assert_ne!(style & 0x0020_0000, 0);
            assert_eq!(style & WS_EX_TOOLWINDOW, 0);
            assert_eq!(style & WS_EX_TRANSPARENT, 0);
        }
        effects.set_click_through(true)?;
        assert!(effects.click_through()?);
        effects.set_click_through(false)?;
        assert!(!effects.click_through()?);
        Ok(())
    }

    #[test]
    fn raw_window_geometry_moves_and_enumerates_work_areas() -> Result<()> {
        let (_guard, effects) = native_window()?;
        let area = WorkArea::new("task6-test", Rect::new(-30000, -30000, 2000, 2000), 1.0);
        let expected = Rect::new(-30000, -30000, 320, 160);
        effects.move_resize(expected, &area)?;
        let native = unsafe {
            let mut native = Default::default();
            GetWindowRect(effects.hwnd(), &mut native)?;
            native
        };
        assert_eq!((native.left, native.top), (-30000, -30000));
        assert_eq!(
            (native.right - native.left, native.bottom - native.top),
            (320, 160)
        );
        assert!(effects.current_work_area()?.scale.is_finite());
        let areas = crate::work_areas()?;
        assert!(!areas.is_empty());
        assert!(
            areas.iter().all(|area| {
                area.scale.is_finite() && area.scale > 0.0 && area.area.width > 0.0
            })
        );
        Ok(())
    }

    #[test]
    fn raw_window_hide_and_show_round_trip_visibility() -> Result<()> {
        let (_guard, effects) = native_window()?;
        effects.hide()?;
        pump_messages();
        assert!(!unsafe { IsWindowVisible(effects.hwnd()) }.as_bool());
        effects.show()?;
        pump_messages();
        assert!(unsafe { IsWindowVisible(effects.hwnd()) }.as_bool());
        Ok(())
    }

    #[test]
    fn unique_mutex_conflicts_and_forwards_activation() -> Result<()> {
        let name = unique_name("mutex");
        let (activated_tx, activated_rx) = mpsc::channel();
        let primary = SingleInstance::acquire(&name, move || {
            let _ = activated_tx.send(());
        })?
        .expect("first unique instance must be primary");

        let secondary = SingleInstance::acquire(&name, || {
            panic!("secondary callback must not become primary");
        })?;
        assert!(secondary.is_none(), "second acquisition must conflict");
        pump_messages();
        activated_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("secondary must activate primary raw HWND");

        SingleInstance::activate_existing(&name)?;
        pump_messages();
        activated_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("explicit activation must use the registered message");
        drop(primary);
        Ok(())
    }

    #[test]
    fn hotkey_registration_conflict_leaves_no_stale_registration() -> Result<()> {
        const CHORD: &str = "F24";
        let first = HotkeyService::new(|| {})?;
        let second = HotkeyService::new(|| {})?;
        ShortcutRegistry::register(&first, CHORD)
            .expect("unique test chord must register on first service");
        assert_eq!(
            ShortcutRegistry::register(&second, CHORD),
            Err(CommandError::ShortcutUnavailable),
            "OS conflict must fail without replacing registry state"
        );
        ShortcutRegistry::unregister(&first, CHORD)
            .expect("original registration must survive the conflict");
        ShortcutRegistry::register(&second, CHORD)
            .expect("failed registration must roll back its ID and registry entry");
        ShortcutRegistry::unregister(&second, CHORD)
            .expect("rolled-back service must register and unregister cleanly");
        Ok(())
    }

    #[test]
    fn message_only_hotkeys_emit_press_release_and_scope_escape() -> Result<()> {
        let service = HotkeyService::new(|| {})?;
        let (events_tx, events_rx) = mpsc::channel();
        HotkeyRegistry::register(
            &service,
            "F23",
            Box::new(move |pressed| {
                let _ = events_tx.send(pressed);
            }),
        )
        .expect("F23 must be available for the isolated native test");
        unsafe {
            PostMessageW(
                Some(service.message_hwnd()),
                WM_HOTKEY,
                WPARAM(1),
                LPARAM(0),
            )?;
        }
        assert!(events_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(!events_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        HotkeyRegistry::unregister(&service, "F23");

        assert_eq!(
            ShortcutRegistry::register(&service, "Escape"),
            Err(CommandError::ShortcutUnavailable),
            "overlay registry must never capture recording-only Escape"
        );
        HotkeyRegistry::register(&service, "Escape", Box::new(|_| {}))
            .expect("dictation registry may register Escape only while recording");
        HotkeyRegistry::unregister(&service, "Escape");
        Ok(())
    }

    struct HotkeyCallbackFixture {
        service: Arc<HotkeyService>,
        chord: &'static str,
        done_tx: mpsc::Sender<()>,
        done_rx: mpsc::Receiver<()>,
    }

    impl HotkeyCallbackFixture {
        fn new(chord: &'static str) -> Result<Self> {
            let (done_tx, done_rx) = mpsc::channel();
            Ok(Self {
                service: Arc::new(HotkeyService::new(|| {})?),
                chord,
                done_tx,
                done_rx,
            })
        }

        fn register<F>(&self, callback: F)
        where
            F: Fn(bool, &std::sync::Weak<HotkeyService>, &mpsc::Sender<()>) + Send + Sync + 'static,
        {
            let service = Arc::downgrade(&self.service);
            let done = self.done_tx.clone();
            HotkeyRegistry::register(
                &*self.service,
                self.chord,
                Box::new(move |pressed| callback(pressed, &service, &done)),
            )
            .expect("native callback fixture must register its hotkey");
        }

        fn trigger(&self) -> Result<()> {
            unsafe {
                PostMessageW(
                    Some(self.service.message_hwnd()),
                    WM_HOTKEY,
                    WPARAM(1),
                    LPARAM(0),
                )
            }
        }

        fn wait_done(&self, message: &str) {
            wait_event(&self.done_rx, message);
        }

        fn register_again(&self, message: &str) {
            HotkeyRegistry::register(&*self.service, self.chord, Box::new(|_| {})).expect(message);
        }

        fn unregister_sync(&self, message: &str) {
            ShortcutRegistry::unregister(&*self.service, self.chord).expect(message);
        }
    }

    fn wait_event(receiver: &mpsc::Receiver<()>, message: &str) {
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect(message);
    }

    #[test]
    fn hotkey_callbacks_can_unregister_outside_registry_locks() -> Result<()> {
        let fixture = HotkeyCallbackFixture::new("F22")?;
        fixture.register(|pressed, service, done| {
            if pressed {
                if let Some(service) = service.upgrade() {
                    HotkeyRegistry::unregister(&*service, "F22");
                }
                let _ = done.send(());
            }
        });
        fixture.trigger()?;
        fixture.wait_done("callback must unregister synchronously without deadlocking");
        fixture.register_again("reentrant callback must actually remove its registration");
        fixture.unregister_sync("reentrant callback registration must be removable");
        Ok(())
    }

    #[test]
    fn dictation_unregister_allows_hotkey_thread_to_join_previous_worker() -> Result<()> {
        let fixture = HotkeyCallbackFixture::new("F21")?;
        let (start_tx, start_rx) = mpsc::channel();
        let (returned_tx, returned_rx) = mpsc::channel();
        let (registered_tx, registered_rx) = mpsc::channel();
        let worker_service = Arc::downgrade(&fixture.service);
        let worker = thread::spawn(move || {
            start_rx.recv().expect("callback must start the worker");
            let service = worker_service
                .upgrade()
                .expect("fixture must retain its service");
            HotkeyRegistry::unregister(&*service, "F21");
            let _ = returned_tx.send(());
        });
        let worker = Mutex::new(Some(worker));
        fixture.register(move |pressed, service, joined| {
            if !pressed {
                return;
            }
            let _ = start_tx.send(());
            let worker = worker.lock().unwrap().take();
            if let Some(worker) = worker {
                worker.join().expect("simulated worker must finish");
            }
            let service = service.upgrade().expect("fixture must retain its service");
            let registered = HotkeyRegistry::register(&*service, "F21", Box::new(|_| {}));
            let _ = registered_tx.send(registered);
            let _ = joined.send(());
        });
        fixture.trigger()?;

        wait_event(
            &returned_rx,
            "dictation unregister must return without hotkey-thread response processing",
        );
        fixture.wait_done("hotkey callback must finish joining the previous worker");
        registered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("in-callback registration must return a result")
            .expect("direct in-callback registration must drain older unregister first");
        assert_eq!(
            HotkeyRegistry::register(&*fixture.service, "F21", Box::new(|_| {})),
            Err(DictationError::ShortcutConflict),
            "drained unregister must not remove the new in-callback registration"
        );
        fixture.unregister_sync("synchronous overlay unregistration must remain available");
        Ok(())
    }
}
