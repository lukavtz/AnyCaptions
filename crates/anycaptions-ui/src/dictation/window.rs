use std::time::{Duration, Instant};

use anycaptions_core::{
    dictation::indicator::{IndicatorSnapshot, place_indicator},
    presentation::{
        Locale,
        dictation::{DictationModel, DictationPhase},
    },
    window_geometry::{Rect, WorkArea},
};
use anycaptions_platform_win32::WindowEffects;
use gpui::{
    Animation, AnimationExt as _, App, Context, Render, ScrollHandle, Window, WindowHandle,
    WindowKind, WindowOptions, div, prelude::*, pulsating_between, px,
};

use crate::{
    overlay::{EventDelivery, UiEventReceiver, native_effects, reduced_motion},
    theme::{
        BASE_CONTENT, DICTATION_PULSE_DURATION, DICTATION_RECORDING, MUTED,
        OVERLAY_SETTINGS_BACKGROUND,
    },
};

pub fn dictation_window_options(geometry: Rect) -> WindowOptions {
    crate::overlay::frameless_window_options(
        geometry,
        "AnyCaptions Dictation",
        "AnyCaptions.Dictation",
        WindowKind::PopUp,
        false,
    )
}

pub fn open_dictation_window(
    cx: &mut App,
    events: UiEventReceiver<IndicatorSnapshot>,
    anchor: Rect,
    work_area: WorkArea,
) -> gpui::Result<WindowHandle<DictationWindow>> {
    let initial = place_indicator(anchor, work_area.area, false);
    let handle = cx.open_window(dictation_window_options(initial), |window, cx| {
        cx.new(|view_cx| DictationWindow::new(events, anchor, work_area, window, view_cx))
    })?;
    let native = handle.update(cx, |view, window, _| -> gpui::Result<()> {
        let effects = native_effects(window)?;
        effects.apply_indicator()?;
        effects.move_resize(view.placement, &view.work_area)?;
        view.native = Some(effects);
        Ok(())
    })?;
    if let Err(error) = native {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return Err(error);
    }
    Ok(handle)
}

pub struct DictationWindow {
    model: DictationModel,
    native: Option<WindowEffects>,
    anchor: Rect,
    work_area: WorkArea,
    placement: Rect,
    visible: bool,
    started: Option<Instant>,
    elapsed_task: Option<gpui::Task<()>>,
    scroll_handle: ScrollHandle,
    _delivery: EventDelivery<IndicatorSnapshot>,
    native_error: Option<String>,
}

impl DictationWindow {
    fn new(
        events: UiEventReceiver<IndicatorSnapshot>,
        anchor: Rect,
        work_area: WorkArea,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let receiver = events.receiver();
        let task = cx.spawn_in(window, async move |view, cx| {
            while let Ok(snapshot) = receiver.recv().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        view.accept_snapshot(&snapshot, window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let placement = place_indicator(anchor, work_area.area, false);
        Self {
            model: DictationModel::default(),
            native: None,
            anchor,
            work_area,
            placement,
            visible: false,
            started: None,
            elapsed_task: None,
            scroll_handle: ScrollHandle::new(),
            _delivery: EventDelivery::new(events, task),
            native_error: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn headless(
        events: UiEventReceiver<IndicatorSnapshot>,
        anchor: Rect,
        work_area: WorkArea,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(events, anchor, work_area, window, cx)
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn placement(&self) -> Rect {
        self.placement
    }

    pub fn frame(&self) -> &anycaptions_core::presentation::dictation::DictationFrame {
        self.model.frame()
    }

    pub fn native_error(&self) -> Option<&str> {
        self.native_error.as_deref()
    }

    pub fn accept_snapshot(
        &mut self,
        snapshot: &IndicatorSnapshot,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.model.accept_snapshot(snapshot).is_none() {
            return false;
        }
        if self.model.frame().hidden() {
            self.started = None;
            self.elapsed_task = None;
            self.set_visible(false);
        } else {
            if snapshot.event.phase == "recording" && self.started.is_none() {
                self.started = Some(cx.background_executor().now());
                self.model.tick(0);
                self.start_elapsed_timer(cx);
            }
            self.placement =
                place_indicator(self.anchor, self.work_area.area, self.model.frame().compact);
            if let Some(native) = &self.native
                && let Err(error) = native.move_resize(self.placement, &self.work_area)
            {
                self.native_error = Some(error.to_string());
            }
            self.set_visible(true);
            self.scroll_handle.scroll_to_bottom();
        }
        cx.notify();
        true
    }

    fn set_visible(&mut self, visible: bool) {
        let result = self.native.as_ref().map(|native| {
            if visible {
                native.show()
            } else {
                native.hide()
            }
        });
        if let Some(Err(error)) = result {
            self.native_error = Some(error.to_string());
        } else {
            self.native_error = None;
            self.visible = visible;
        }
    }

    fn start_elapsed_timer(&mut self, cx: &mut Context<Self>) {
        self.elapsed_task = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let running = view
                    .update(cx, |view, cx| {
                        let Some(started) = view.started else {
                            return false;
                        };
                        if view.model.frame().hidden() {
                            return false;
                        }
                        let elapsed = cx
                            .background_executor()
                            .now()
                            .saturating_duration_since(started);
                        view.model.tick(elapsed.as_millis() as u64);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        }));
    }

    pub fn follow_overlay(
        &mut self,
        anchor: Rect,
        work_area: WorkArea,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.anchor = anchor;
        self.work_area = work_area;
        self.placement = place_indicator(anchor, self.work_area.area, self.model.frame().compact);
        if self.visible {
            if let Some(native) = &self.native
                && let Err(error) = native.move_resize(self.placement, &self.work_area)
            {
                self.native_error = Some(error.to_string());
            }
            cx.notify();
        }
    }
}

impl Render for DictationWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let locale = self
            .model
            .indicator_language()
            .and_then(Locale::from_tag)
            .unwrap_or_default();
        let frame = self.model.frame();
        let recording = frame.phase == DictationPhase::Recording;
        let dot = div()
            .id("dictation-dot")
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .bg(if recording {
                DICTATION_RECORDING
            } else {
                MUTED
            });
        let dot = if recording && !reduced_motion() {
            dot.with_animation(
                "dictation-pulse",
                Animation::new(DICTATION_PULSE_DURATION)
                    .repeat()
                    .with_easing(pulsating_between(0.35, 1.0)),
                |dot, delta| dot.opacity(delta),
            )
            .into_any_element()
        } else {
            dot.into_any_element()
        };
        let header = div().flex().items_center().gap_2().child(dot).child(
            div()
                .id("dictation-elapsed")
                .text_sm()
                .text_color(BASE_CONTENT)
                .child(frame.elapsed.clone()),
        );
        let text = frame.display_text(locale).to_owned();
        div()
            .id("dictation-indicator")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .bg(OVERLAY_SETTINGS_BACKGROUND)
            .text_color(BASE_CONTENT)
            .when(!self.visible, |indicator| indicator.hidden())
            .child(header)
            .when(!frame.compact, |indicator| {
                indicator.child(
                    div()
                        .id("dictation-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle)
                        .child(text),
                )
            })
    }
}

impl Drop for DictationWindow {
    fn drop(&mut self) {
        self.elapsed_task = None;
        if let Some(native) = &self.native {
            let _ = native.hide();
        }
    }
}
