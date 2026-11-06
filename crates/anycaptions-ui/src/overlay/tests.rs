//! Native click-through, drag/resize, taskbar/icon, screenshot, and DPI/work-area
//! acceptance is executable in `scripts/verify-overlay.ps1`; these tests cover
//! the GPUI lifecycle without manufacturing a native test window.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use anycaptions_core::{
    caption::{CaptionFrame, CaptionLines, CaptionState},
    dictation::{
        ActivationMode,
        indicator::{IndicatorSnapshot, place_indicator},
        runtime::{DictationEventPayload, DictationEventSink},
    },
    domain::{AppSnapshot, CaptionMode, CaptureState, PT_BR, Resources, WindowMode},
    ipc::{CaptionUpdate, CommandError, Controller, StateEvents},
    preferences::SavedGeometry,
    presentation::dictation::DictationPhase,
    transcription::session::SessionSink,
    window_geometry::{Rect, WorkArea},
};
use gpui::{AppContext as _, Entity, Task, TestAppContext, VisualTestContext, WindowKind, px};

use super::{
    CaptionView, EventDelivery, OverlayCommand, OverlayEvent, OverlayHost, OverlayView,
    caption_view::caption_layers, overlay_window_options, restored_overlay_geometry,
    ui_event_channels, visible_overlay_placement,
};
use crate::dictation::DictationWindow;

struct NoopEvents;

impl StateEvents for NoopEvents {
    fn state_changed(&self, _: &AppSnapshot) -> Result<(), CommandError> {
        Ok(())
    }
}

struct TestHost {
    recovery: AppSnapshot,
    outcomes: Mutex<VecDeque<Result<AppSnapshot, CommandError>>>,
}

impl TestHost {
    fn new(recovery: AppSnapshot, outcomes: Vec<Result<AppSnapshot, CommandError>>) -> Self {
        Self {
            recovery,
            outcomes: Mutex::new(VecDeque::from(outcomes)),
        }
    }
}

impl OverlayHost for TestHost {
    fn execute(&self, _: OverlayCommand) -> Result<AppSnapshot, CommandError> {
        self.outcomes
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected host command")
    }

    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        Ok(self.recovery.clone())
    }

    fn close_to_tray(&self, _: SavedGeometry) -> Result<(), CommandError> {
        Ok(())
    }

    fn open_settings(
        &self,
        _: &AppSnapshot,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) -> gpui::AnyView {
        unreachable!("settings are not opened by these regressions")
    }
}

fn overlay_view(
    cx: &mut TestAppContext,
    host: Arc<TestHost>,
    snapshot: AppSnapshot,
) -> (Entity<OverlayView>, &mut VisualTestContext) {
    let (_sink, events, _indicator) = ui_event_channels(Controller::default());
    let text_system = cx.text_system().clone();
    let geometry = Rect::new(0, 0, 800, 160);
    let area = WorkArea::new("primary", Rect::new(0, 0, 1920, 1040), 1.0);
    cx.add_window_view(move |window, cx| {
        OverlayView::new(
            host,
            snapshot,
            events,
            text_system,
            geometry,
            area.clone(),
            vec![area],
            window,
            cx,
        )
    })
}

fn snapshot(mode: CaptionMode, capture: CaptureState) -> AppSnapshot {
    let controller = Controller::default();
    controller
        .set_resources(
            Resources {
                audio_capture: true,
                transcription: true,
                translation: true,
                speaker_labels: true,
            },
            &NoopEvents,
        )
        .unwrap();
    controller
        .update(&NoopEvents, |state| state.set_mode(mode))
        .unwrap();
    if capture != CaptureState::Stopped {
        let started = controller.start_session(&NoopEvents).unwrap();
        if capture != CaptureState::Preparing {
            controller
                .transition_session(started.session_id, capture, &NoopEvents)
                .unwrap();
        }
    }
    controller.snapshot().unwrap()
}

fn update(snapshot: &AppSnapshot, segment: u32, revision: u32, text: &str) -> CaptionUpdate {
    CaptionUpdate {
        session_id: snapshot.session_id,
        segment_id: segment,
        revision,
        mode: snapshot.mode,
        text: text.into(),
        is_final: true,
        language: None,
        speakers: Vec::new(),
        provisional: false,
    }
}

fn caption_view(
    cx: &mut TestAppContext,
    snapshot: AppSnapshot,
) -> (Entity<CaptionView>, &mut VisualTestContext) {
    let text_system = cx.text_system().clone();
    cx.add_window_view(move |window, cx| {
        CaptionView::new(text_system, snapshot, px(2000.0), window, cx)
    })
}

struct CaptionFixture {
    state: AppSnapshot,
    view: Entity<CaptionView>,
    timers: std::rc::Rc<std::cell::Cell<usize>>,
}

fn caption_fixture<'a>(
    cx: &'a mut TestAppContext,
    text: &'a str,
) -> (CaptionFixture, &'a mut VisualTestContext) {
    let state = snapshot(CaptionMode::Auto, CaptureState::Listening);
    let (view, cx) = caption_view(cx, state.clone());
    view.update(cx, |view, cx| {
        assert!(view.accept(update(&state, 1, 1, text), cx));
    });
    let timers = read(&view, cx, CaptionView::timer_count);
    (
        CaptionFixture {
            state,
            view,
            timers,
        },
        cx,
    )
}

fn read<V: 'static, R>(view: &Entity<V>, cx: &VisualTestContext, read: impl FnOnce(&V) -> R) -> R {
    cx.read_entity(view, |view, _| read(view))
}

#[gpui::test]
fn final_caption_has_no_self_clear_timer_after_60_seconds(cx: &mut TestAppContext) {
    let (CaptionFixture { view, timers, .. }, cx) = caption_fixture(cx, "Still visible");
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(
        read(&view, cx, |view| view.frame().unwrap().text.clone()),
        "Still visible"
    );
    assert_eq!(read(&view, cx, |view| view.frame().unwrap().opacity), 1.0);
    assert_eq!(timers.get(), 0);
    assert!(!read(&view, cx, CaptionView::fade_pending));
}

#[test]
fn only_newest_translated_sentence_animates() {
    let frame = CaptionFrame {
        text: "First Second Third".into(),
        committed_text: None,
        live_partial_text: None,
        sentences: vec!["First".into(), "Second".into(), "Third".into()],
        parts: None,
        sentence_speakers: None,
        provisional_text: None,
        provisional_speaker: None,
        animate_newest: true,
        lines: CaptionLines::Multi,
        state: CaptionState::Final,
        opacity: 1.0,
    };
    let layers = caption_layers(&frame, &Default::default());
    assert_eq!(layers.len(), 3);
    assert_eq!(
        layers.iter().map(|layer| layer.animate).collect::<Vec<_>>(),
        [false, false, true]
    );
    assert_eq!(layers[0].text, "First ");
    assert_eq!(layers[1].text, "Second ");
    assert_eq!(layers[2].text, "Third");
}

#[gpui::test]
fn state_after_translation_keeps_newest_sentence_animation_until_render(cx: &mut TestAppContext) {
    let state = snapshot(CaptionMode::Translate(PT_BR), CaptureState::Listening);
    let host = Arc::new(TestHost::new(state.clone(), Vec::new()));
    let (view, cx) = overlay_view(cx, host, state.clone());
    let captions = read(&view, cx, |overlay| overlay.captions().clone());
    cx.update(|window, cx| {
        view.update(cx, |overlay, cx| {
            overlay.accept_ui_event(
                OverlayEvent::Caption(update(&state, 1, 1, "Primeira frase.")),
                window,
                cx,
            );
        });
    });
    assert!(read(&captions, cx, |caption| {
        caption.frame().is_some_and(|frame| frame.animate_newest)
    }));

    let mut listening = state;
    listening.revision += 1;
    cx.update(|window, cx| {
        view.update(cx, |overlay, cx| {
            overlay.accept_ui_event(OverlayEvent::State(listening), window, cx);
        });
    });
    assert!(read(&captions, cx, |caption| {
        caption.frame().is_some_and(|frame| frame.animate_newest)
    }));
    assert_eq!(
        read(&captions, cx, |caption| {
            caption_layers(caption.frame().unwrap(), &Default::default())
                .into_iter()
                .filter(|layer| layer.animate)
                .count()
        }),
        1
    );
}

#[gpui::test]
fn lock_failure_preserves_edit_model_and_allows_retry(cx: &mut TestAppContext) {
    let mut state = snapshot(CaptionMode::Auto, CaptureState::Listening);
    state.can_lock = true;
    let mut locked = state.clone();
    locked.revision += 1;
    locked.window_mode = WindowMode::Locked;
    let host = Arc::new(TestHost::new(
        state.clone(),
        vec![Err(CommandError::WindowUnavailable), Ok(locked)],
    ));
    let (view, cx) = overlay_view(cx, host, state);

    cx.update(|window, cx| {
        view.update(cx, |overlay, cx| {
            overlay.perform_action(OverlayCommand::LockWindow, window, cx);
        });
    });
    assert_eq!(
        read(&view, cx, |overlay| overlay.model.snapshot().window_mode),
        WindowMode::Edit
    );
    assert!(!read(&view, cx, |overlay| overlay.model.busy()));
    assert_eq!(
        read(&view, cx, |overlay| overlay.model.error()),
        Some(CommandError::WindowUnavailable)
    );

    cx.update(|window, cx| {
        view.update(cx, |overlay, cx| {
            overlay.perform_action(OverlayCommand::LockWindow, window, cx);
        });
    });
    assert_eq!(
        read(&view, cx, |overlay| overlay.model.snapshot().window_mode),
        WindowMode::Locked
    );
    assert!(!read(&view, cx, |overlay| overlay.model.busy()));
    assert_eq!(read(&view, cx, |overlay| overlay.model.error()), None);
}

#[gpui::test]
fn clear_hides_after_120ms_and_new_input_invalidates_old_timer(cx: &mut TestAppContext) {
    let (
        CaptionFixture {
            state,
            view,
            timers,
        },
        cx,
    ) = caption_fixture(cx, "Old");
    view.update(cx, |view, cx| view.clear(false, cx));
    assert_eq!(timers.get(), 1);
    assert_eq!(read(&view, cx, |view| view.frame().unwrap().opacity), 0.0);
    cx.executor().advance_clock(Duration::from_millis(119));
    assert!(read(&view, cx, |view| view.frame().is_some()));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert!(read(&view, cx, CaptionView::accessible_text).is_empty());
    assert_eq!(timers.get(), 0);

    let mut before = update(&state, 2, 1, "Before clear");
    before.is_final = false;
    let mut newest = update(&state, 2, 2, "New input");
    newest.is_final = false;
    view.update(cx, |view, cx| {
        view.accept(before, cx);
        view.clear(false, cx);
        assert!(view.accept(newest, cx));
    });
    assert_eq!(timers.get(), 0);
    cx.executor().advance_clock(Duration::from_millis(120));
    cx.run_until_parked();
    assert_eq!(
        read(&view, cx, |view| view.frame().unwrap().text.clone()),
        "New input"
    );
    assert_eq!(read(&view, cx, |view| view.frame().unwrap().opacity), 1.0);
    assert_eq!(timers.get(), 0);
}

#[gpui::test]
fn disposal_cancels_pending_clear_and_window_listeners(cx: &mut TestAppContext) {
    let (CaptionFixture { view, timers, .. }, cx) = caption_fixture(cx, "Disposable");
    view.update(cx, |view, cx| view.clear(false, cx));
    assert_eq!(read(&view, cx, CaptionView::listener_count), 1);
    let weak = view.downgrade();
    drop(view);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert_eq!(timers.get(), 0);
    cx.executor().advance_clock(Duration::from_millis(120));
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}

#[gpui::test]
fn reduced_motion_routes_to_immediate_clear_without_timer(cx: &mut TestAppContext) {
    let (CaptionFixture { view, timers, .. }, cx) = caption_fixture(cx, "Motion");
    view.update(cx, |view, cx| view.clear(true, cx));
    assert!(read(&view, cx, CaptionView::accessible_text).is_empty());
    assert!(!read(&view, cx, CaptionView::fade_pending));
    assert_eq!(timers.get(), 0);
}

#[gpui::test]
fn html_text_is_literal_and_replacement_has_no_previous_layer(cx: &mut TestAppContext) {
    let state = snapshot(CaptionMode::Auto, CaptureState::Listening);
    let (view, cx) = caption_view(cx, state.clone());
    let html = "<img src=x onerror=alert(1)> literal & text";
    let mut first = update(&state, 1, 1, html);
    first.is_final = false;
    view.update(cx, |view, cx| {
        view.accept(first, cx);
    });
    assert_eq!(read(&view, cx, CaptionView::accessible_text), html);
    let layers = read(&view, cx, |view| {
        caption_layers(view.frame().unwrap(), &Default::default())
    });
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].text, html);

    let mut replacement = update(&state, 1, 2, "Replacement only");
    replacement.is_final = false;
    view.update(cx, |view, cx| {
        view.accept(replacement, cx);
    });
    let layers = read(&view, cx, |view| {
        caption_layers(view.frame().unwrap(), &Default::default())
    });
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].text, "Replacement only");
    assert!(!layers[0].text.contains(html));
}

fn dictation_event(phase: &'static str, text: &str) -> DictationEventPayload {
    DictationEventPayload {
        phase,
        text: text.into(),
        error: None,
        sound_cue: true,
        activation: ActivationMode::Hold,
        show_live_text: true,
        max_duration_secs: 30,
    }
}

#[gpui::test]
fn indicator_rejects_stale_revisions_follows_and_hides_on_idle(cx: &mut TestAppContext) {
    let controller = Controller::default();
    let (_sink, _overlay, indicator) = ui_event_channels(controller.clone());
    let anchor = Rect::new(100, 400, 800, 160);
    let area = WorkArea::new("primary", Rect::new(0, 0, 1920, 1040), 1.0);
    let (view, cx) = cx.add_window_view(move |window, cx| {
        DictationWindow::headless(indicator, anchor, area, window, cx)
    });
    let recording = IndicatorSnapshot::publish(
        None,
        &controller,
        dictation_event("recording", "Newest dictated text"),
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.accept_snapshot(&recording, window, cx));
        });
    });
    assert!(read(&view, cx, DictationWindow::visible));
    assert_eq!(
        read(&view, cx, |view| view.frame().phase),
        DictationPhase::Recording
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.accept_snapshot(&recording, window, cx));
        });
    });
    assert_eq!(
        read(&view, cx, |view| view.frame().text.clone()),
        "Newest dictated text"
    );

    let new_anchor = Rect::new(300, 500, 640, 160);
    let new_area = WorkArea::new("primary", Rect::new(0, 0, 1920, 1080), 1.0);
    let expected = place_indicator(new_anchor, new_area.area, false);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.follow_overlay(new_anchor, new_area, window, cx);
        });
    });
    assert_eq!(read(&view, cx, DictationWindow::placement), expected);

    let idle =
        IndicatorSnapshot::publish(Some(&recording), &controller, dictation_event("idle", ""));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.accept_snapshot(&idle, window, cx));
        });
    });
    assert!(!read(&view, cx, DictationWindow::visible));
    assert!(read(&view, cx, |view| view.frame().elapsed.is_empty()));
}

#[test]
fn core_sink_uses_async_plain_events_and_disposal_closes_delivery() {
    let controller = Controller::default();
    let (sink, overlay, indicator) = ui_event_channels(controller.clone());
    let state = snapshot(CaptionMode::Auto, CaptureState::Listening);
    sink.caption_updated(&update(&state, 1, 1, "Channel"))
        .unwrap();
    assert!(matches!(
        overlay.receiver().try_recv(),
        Ok(OverlayEvent::Caption(CaptionUpdate { text, .. })) if text == "Channel"
    ));
    sink.emit(&dictation_event("recording", "One"));
    let first = indicator.receiver().try_recv().unwrap();
    assert_eq!(first.revision, 1);
    sink.emit(&dictation_event("recording", "Two"));
    let second = indicator.receiver().try_recv().unwrap();
    assert_eq!(second.revision, 2);
    assert!(matches!(
        overlay.receiver().try_recv(),
        Ok(OverlayEvent::Dictation(_))
    ));

    let delivery = EventDelivery::new(overlay, Task::ready(()));
    drop(delivery);
    assert_eq!(
        sink.caption_updated(&update(&state, 1, 2, "Closed")),
        Err(CommandError::EventUnavailable)
    );
}

#[test]
fn edit_recovery_fallback_uses_primary_monitor_work_area() {
    let primary = WorkArea::new("primary", Rect::new(0, 0, 1920, 1040), 1.5);
    let disconnected = WorkArea::new("disconnected", Rect::new(1920, 0, 1280, 720), 2.0);
    let offscreen = Rect::new(5000, 5000, 800, 160);

    let (geometry, area) =
        visible_overlay_placement(offscreen, &disconnected, std::slice::from_ref(&primary));

    assert_eq!(geometry, Rect::new(560, 848, 800, 160));
    assert_eq!(area, primary);
    assert_eq!(area.scale, 1.5);
}

#[test]
fn production_window_is_normal_and_restores_work_area_geometry() {
    let areas = vec![WorkArea::new("primary", Rect::new(0, 0, 1920, 1040), 1.5)];
    let saved = SavedGeometry {
        monitor: Some("primary".into()),
        relative_x: 0.5,
        relative_y: 1.0,
        width: 800.0,
        height: 160.0,
    };
    let (geometry, area) = restored_overlay_geometry(Some(&saved), &areas).unwrap();
    assert_eq!(geometry.width, 800.0);
    assert_eq!(geometry.height, 160.0);
    assert_eq!(area.name.as_deref(), Some("primary"));
    let options = overlay_window_options(geometry);
    assert_eq!(options.kind, WindowKind::Normal);
    assert!(options.is_movable);
    assert!(options.is_resizable);
    assert!(!options.focus);
    assert!(!options.show);
    assert_eq!(options.window_min_size.unwrap().width, px(320.0));
    assert_eq!(options.window_min_size.unwrap().height, px(160.0));
}

#[test]
fn lock_style_preserves_taskbar_and_enables_click_through() {
    use anycaptions_platform_win32::window::{
        WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, overlay_ex_style,
    };
    let style = overlay_ex_style(0, true);
    assert_ne!(style & WS_EX_APPWINDOW, 0);
    assert_eq!(style & WS_EX_TOOLWINDOW, 0);
    assert_ne!(style & WS_EX_TOPMOST, 0);
    assert_ne!(style & WS_EX_NOACTIVATE, 0);
    assert_ne!(style & WS_EX_LAYERED, 0);
    assert_ne!(style & WS_EX_TRANSPARENT, 0);
    assert_eq!(overlay_ex_style(style, false) & WS_EX_TRANSPARENT, 0);
}

#[gpui::test]
fn resize_grips_have_nonzero_hitboxes(cx: &mut TestAppContext) {
    let state = snapshot(CaptionMode::Auto, CaptureState::Stopped);
    let host = Arc::new(TestHost::new(state.clone(), Vec::new()));
    let (_view, cx) = overlay_view(cx, host, state);
    for id in ["resize-left", "resize-right", "resize-bottom"] {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"));
        assert!(bounds.size.width > px(0.0), "{id} has zero width");
        assert!(bounds.size.height > px(0.0), "{id} has zero height");
    }
}
