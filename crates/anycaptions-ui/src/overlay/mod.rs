mod accessibility;
mod caption_view;
mod toolbar;

use std::sync::{Arc, Mutex};

use crate::{
    dictation::DictationWindow,
    theme::{BORDER, ERROR, OVERLAY_EDIT_BACKGROUND, WARNING},
};
use anycaptions_core::{
    dictation::{
        indicator::IndicatorSnapshot,
        runtime::{DictationEventPayload, DictationEventSink},
    },
    domain::{AppSnapshot, CaptureState, WindowMode},
    ipc::{CaptionUpdate, CommandError, Controller, StateEvents},
    preferences::SavedGeometry,
    presentation::{
        Locale, MessageKey,
        application::{Action, ApplicationModel, SnapshotRecovery},
        dictation::DictationModel,
        error_message_key, translate,
        updates::UpdateNoticeModel,
    },
    transcription::session::SessionSink,
    update::{UpdateCheck, UpdateError},
    window_geometry::{
        Rect, WorkArea, capture_geometry, ensure_visible, initial_geometry, restored_geometry,
    },
};
use anycaptions_platform_win32::{
    ResizeEdge, WindowEffects,
    system::{DictationCue, ReducedMotion, SystemActions},
};
use async_channel::{Receiver, Sender, unbounded};
use gpui::{
    AnyView, App, Bounds, Context, CursorStyle, Entity, MouseButton, Render, Subscription, Task,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, div, point, prelude::*, px, size, transparent_black,
};

pub use caption_view::CaptionView;

#[derive(Debug, Clone)]
pub enum OverlayEvent {
    State(AppSnapshot),
    Caption(CaptionUpdate),
    Dictation(DictationEventPayload),
}

pub struct UiEventReceiver<T> {
    receiver: Receiver<T>,
}

impl<T> UiEventReceiver<T> {
    pub fn close(&self) -> bool {
        self.receiver.close()
    }

    pub fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }

    pub(crate) fn receiver(&self) -> Receiver<T> {
        Receiver::clone(&self.receiver)
    }
}

struct UiEventSinkInner {
    controller: Controller,
    previous_indicator: Mutex<Option<IndicatorSnapshot>>,
    overlay: Sender<OverlayEvent>,
    indicator: Sender<IndicatorSnapshot>,
}

#[derive(Clone)]
pub struct UiEventSink {
    inner: Arc<UiEventSinkInner>,
}

/// Creates the two UI-owned receivers. Core threads see only the clonable sink.
pub fn ui_event_channels(
    controller: Controller,
) -> (
    UiEventSink,
    UiEventReceiver<OverlayEvent>,
    UiEventReceiver<IndicatorSnapshot>,
) {
    let (overlay_tx, overlay_rx) = unbounded();
    let (indicator_tx, indicator_rx) = unbounded();
    let sink = UiEventSink {
        inner: Arc::new(UiEventSinkInner {
            controller,
            previous_indicator: Mutex::new(None),
            overlay: overlay_tx,
            indicator: indicator_tx,
        }),
    };
    (
        sink,
        UiEventReceiver {
            receiver: overlay_rx,
        },
        UiEventReceiver {
            receiver: indicator_rx,
        },
    )
}

fn send_overlay_event(
    sender: &Sender<OverlayEvent>,
    event: OverlayEvent,
) -> Result<(), CommandError> {
    sender
        .try_send(event)
        .map_err(|_| CommandError::EventUnavailable)
}

impl StateEvents for UiEventSink {
    fn state_changed(&self, state: &AppSnapshot) -> Result<(), CommandError> {
        send_overlay_event(&self.inner.overlay, OverlayEvent::State(state.clone()))
    }
}

impl SessionSink for UiEventSink {
    fn caption_updated(&self, update: &CaptionUpdate) -> Result<(), CommandError> {
        send_overlay_event(&self.inner.overlay, OverlayEvent::Caption(update.clone()))
    }
}

impl DictationEventSink for UiEventSink {
    fn emit(&self, payload: &DictationEventPayload) {
        let _ = self
            .inner
            .overlay
            .try_send(OverlayEvent::Dictation(payload.clone()));
        let mut previous = self
            .inner
            .previous_indicator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let snapshot =
            IndicatorSnapshot::publish(previous.as_ref(), &self.inner.controller, payload.clone());
        if self.inner.indicator.try_send(snapshot.clone()).is_ok() {
            *previous = Some(snapshot);
        }
    }
}

pub(crate) struct EventDelivery<T> {
    receiver: UiEventReceiver<T>,
    _task: Task<()>,
}

impl<T> EventDelivery<T> {
    pub(crate) fn new(receiver: UiEventReceiver<T>, task: Task<()>) -> Self {
        Self {
            receiver,
            _task: task,
        }
    }
}

impl<T> Drop for EventDelivery<T> {
    fn drop(&mut self) {
        self.receiver.close();
    }
}

pub(crate) fn reduced_motion() -> bool {
    ReducedMotion::query().is_ok_and(|motion| motion.is_reduced())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayCommand {
    StartCapture,
    StopCapture,
    SetMode(anycaptions_core::domain::CaptionMode),
    LockWindow,
}

/// Host commands execute on the UI caller, while their emitted snapshots return
/// through `UiEventSink`. `open_settings` constructs a fresh panel and drains
/// its initial host actions before returning the panel.
pub trait OverlayHost: Send + Sync + 'static {
    /// `LockWindow` applies native click-through before returning a Locked
    /// snapshot. Failure leaves the native mode and authoritative snapshot
    /// unchanged so the view model is not committed and the action can retry.
    fn execute(&self, command: OverlayCommand) -> Result<AppSnapshot, CommandError>;
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError>;
    fn close_to_tray(&self, geometry: SavedGeometry) -> Result<(), CommandError>;
    fn open_settings(&self, snapshot: &AppSnapshot, window: &mut Window, cx: &mut App) -> AnyView;
}

struct HostRecovery<'a>(&'a dyn OverlayHost);

impl SnapshotRecovery for HostRecovery<'_> {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        self.0.recover_snapshot()
    }
}

pub fn restored_overlay_geometry(
    saved: Option<&SavedGeometry>,
    areas: &[WorkArea],
) -> Option<(Rect, WorkArea)> {
    let primary = areas.first()?;
    if let Some((geometry, monitor)) = saved.and_then(|saved| restored_geometry(saved, areas)) {
        return Some((geometry, monitor.clone()));
    }
    Some((initial_geometry(primary.area), primary.clone()))
}

fn visible_overlay_placement(
    geometry: Rect,
    current_area: &WorkArea,
    areas: &[WorkArea],
) -> (Rect, WorkArea) {
    let visible = ensure_visible(geometry, areas);
    if visible == geometry {
        return (visible, current_area.clone());
    }
    let fallback = areas
        .first()
        .cloned()
        .unwrap_or_else(|| current_area.clone());
    (visible, fallback)
}

pub(crate) fn frameless_window_options(
    geometry: Rect,
    title: &'static str,
    app_id: &'static str,
    kind: WindowKind,
    is_movable: bool,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(geometry.x as f32), px(geometry.y as f32)),
            size: size(px(geometry.width as f32), px(geometry.height as f32)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        focus: false,
        show: false,
        kind,
        is_movable,
        // Native grips preserve the non-maximizable frame.
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some(app_id.into()),
        ..Default::default()
    }
}

pub fn overlay_window_options(geometry: Rect) -> WindowOptions {
    let mut options = frameless_window_options(
        geometry,
        "AnyCaptions",
        "AnyCaptions",
        WindowKind::Normal,
        true,
    );
    options.window_min_size = Some(size(px(320.0), px(160.0)));
    options.is_resizable = true;
    options
}

pub(crate) fn native_effects(window: &Window) -> gpui::Result<WindowEffects> {
    let raw = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|error| std::io::Error::other(error.to_string()))?
        .as_raw();
    Ok(WindowEffects::from_raw_handle(raw)?)
}

fn open_overlay_root(
    cx: &mut App,
    host: Arc<dyn OverlayHost>,
    snapshot: AppSnapshot,
    events: UiEventReceiver<OverlayEvent>,
    saved_geometry: Option<&SavedGeometry>,
    areas: Vec<WorkArea>,
) -> gpui::Result<WindowHandle<OverlayView>> {
    let (geometry, work_area) = restored_overlay_geometry(saved_geometry, &areas)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No monitor work area"))?;
    let text_system = cx.text_system().clone();
    let mut attach_error = None;
    let handle = cx.open_window(overlay_window_options(geometry), |window, cx| {
        let view = cx.new(|view_cx| {
            OverlayView::new(
                host,
                snapshot,
                events,
                text_system,
                geometry,
                work_area,
                areas,
                window,
                view_cx,
            )
        });
        if let Err(error) = accessibility::OverlayAccessibility::attach(&view, window, cx) {
            attach_error = Some(error);
        }
        view
    })?;
    if let Some(error) = attach_error {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return Err(error);
    }
    Ok(handle)
}

fn overlay_native_step(
    handle: &WindowHandle<OverlayView>,
    cx: &mut App,
    step: impl FnOnce(&mut OverlayView, &Window) -> gpui::Result<()>,
) -> gpui::Result<()> {
    let result = handle.update(cx, |view, window, _| step(view, window))?;
    if let Err(error) = result {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return Err(error);
    }
    Ok(())
}

fn attach_overlay_native(view: &mut OverlayView, window: &Window) -> gpui::Result<()> {
    let effects = native_effects(window)?;
    effects.apply_overlay(view.model.snapshot().window_mode == WindowMode::Locked)?;
    effects.move_resize(view.geometry, &view.work_area)?;
    let geometry = effects.geometry()?;
    view.toolbar
        .paint_bounds()
        .set_native_origin((geometry.x, geometry.y));
    view.native = Some(effects);
    Ok(())
}

fn show_overlay_native(view: &mut OverlayView, _: &Window) -> gpui::Result<()> {
    view.native
        .as_ref()
        .expect("native effects attached")
        .show()?;
    Ok(())
}

pub fn open_overlay_window(
    cx: &mut App,
    host: Arc<dyn OverlayHost>,
    snapshot: AppSnapshot,
    events: UiEventReceiver<OverlayEvent>,
    saved_geometry: Option<&SavedGeometry>,
    areas: Vec<WorkArea>,
) -> gpui::Result<WindowHandle<OverlayView>> {
    let handle = open_overlay_root(cx, host, snapshot, events, saved_geometry, areas)?;
    overlay_native_step(&handle, cx, attach_overlay_native)?;
    overlay_native_step(&handle, cx, show_overlay_native)?;
    Ok(handle)
}

pub struct OverlayView {
    model: ApplicationModel,
    host: Arc<dyn OverlayHost>,
    captions: Entity<CaptionView>,
    toolbar: toolbar::Toolbar,
    native: Option<WindowEffects>,
    geometry: Rect,
    work_area: WorkArea,
    areas: Vec<WorkArea>,
    settings_open: bool,
    settings_view: Option<AnyView>,
    update_notice: UpdateNoticeModel,
    dictation: DictationModel,
    dictation_error: Option<MessageKey>,
    indicator: Option<WindowHandle<DictationWindow>>,
    _delivery: EventDelivery<OverlayEvent>,
    subscriptions: Vec<Subscription>,
}

impl OverlayView {
    #[allow(clippy::too_many_arguments)]
    fn new(
        host: Arc<dyn OverlayHost>,
        snapshot: AppSnapshot,
        events: UiEventReceiver<OverlayEvent>,
        text_system: Arc<gpui::TextSystem>,
        geometry: Rect,
        work_area: WorkArea,
        areas: Vec<WorkArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = ApplicationModel::new(snapshot.clone());
        let width = px((f32::from(window.viewport_size().width) - 32.0).max(1.0));
        let captions = cx.new(|caption_cx| {
            CaptionView::new(text_system, snapshot.clone(), width, window, caption_cx)
        });
        let toolbar = toolbar::Toolbar::new(&snapshot, false, window, cx);
        let delivery = Self::start_delivery(events, window, cx);
        let subscriptions = Self::observe_lifecycle(window, cx);
        Self {
            model,
            host,
            captions,
            toolbar,
            native: None,
            geometry,
            work_area,
            areas,
            settings_open: false,
            settings_view: None,
            update_notice: UpdateNoticeModel::default(),
            dictation: DictationModel::default(),
            dictation_error: None,
            indicator: None,
            _delivery: delivery,
            subscriptions,
        }
    }

    fn start_delivery(
        events: UiEventReceiver<OverlayEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EventDelivery<OverlayEvent> {
        let receiver = events.receiver();
        let task = cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = receiver.recv().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        view.accept_ui_event(event, window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        EventDelivery::new(events, task)
    }

    fn observe_lifecycle(window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = weak.update(cx, |view, cx| view.close_to_tray(window, cx));
            false
        });
        vec![cx.observe_window_bounds(window, |view, window, cx| {
            view.geometry_changed(window, cx);
        })]
    }

    pub fn captions(&self) -> &Entity<CaptionView> {
        &self.captions
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    pub fn geometry(&self) -> Rect {
        self.geometry
    }

    pub fn work_area(&self) -> WorkArea {
        self.work_area.clone()
    }

    pub fn apply_snapshot(
        &mut self,
        snapshot: AppSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.accept(snapshot) {
            self.sync_from_model(window, cx);
        }
    }

    fn sync_from_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.model.snapshot().clone();
        let locked = snapshot.window_mode == WindowMode::Locked;
        if locked && !self.settings_open && self.settings_view.is_some() {
            self.settings_view = None;
        }
        if locked && self.settings_open {
            self.set_settings_open(false, window, cx);
        }
        self.captions.update(cx, |captions, cx| {
            captions.apply_snapshot(&snapshot, reduced_motion(), cx);
        });
        self.toolbar.sync(&snapshot, self.model.busy(), window, cx);
        cx.notify();
    }

    pub fn perform_action(
        &mut self,
        command: OverlayCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match command {
            OverlayCommand::StartCapture => Action::StartCapture,
            OverlayCommand::StopCapture => Action::StopCapture,
            OverlayCommand::SetMode(mode) => Action::SetMode(mode),
            OverlayCommand::LockWindow => Action::LockWindow,
        };
        if !self.model.try_begin(action) {
            return;
        }
        self.toolbar.sync(self.model.snapshot(), true, window, cx);
        let result = self.host.execute(command);
        self.model
            .complete(result, &HostRecovery(self.host.as_ref()));
        self.sync_from_model(window, cx);
    }

    fn accept_ui_event(
        &mut self,
        event: OverlayEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            OverlayEvent::State(snapshot) => self.apply_snapshot(snapshot, window, cx),
            OverlayEvent::Caption(update) => {
                self.captions.update(cx, |captions, cx| {
                    captions.accept(update, cx);
                });
            }
            OverlayEvent::Dictation(event) => self.accept_dictation(&event, cx),
        }
    }

    fn accept_dictation(&mut self, event: &DictationEventPayload, cx: &mut Context<Self>) {
        let effects = self.dictation.accept(event);
        if event.phase == "idle" {
            self.dictation_error = effects.error;
        } else {
            self.dictation_error = None;
        }
        match effects.sound_cue_hz {
            Some(880) => SystemActions::beep(DictationCue::Start),
            Some(440) => SystemActions::beep(DictationCue::Stop),
            _ => {}
        }
        cx.notify();
    }

    pub fn begin_update_check(&mut self, cx: &mut Context<Self>) -> bool {
        let started = self
            .update_notice
            .begin_check(self.model.snapshot().update_check);
        cx.notify();
        started
    }

    pub fn complete_update_check(
        &mut self,
        result: Result<UpdateCheck, UpdateError>,
        cx: &mut Context<Self>,
    ) -> bool {
        let completed = self.update_notice.complete_check(result);
        cx.notify();
        completed
    }

    fn dismiss_update_notice(&mut self, cx: &mut Context<Self>) {
        self.update_notice.dismiss();
        cx.notify();
    }

    pub fn set_settings_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open == open && (!open || self.settings_view.is_some()) {
            return;
        }
        if open {
            let view = self.host.open_settings(self.model.snapshot(), window, cx);
            self.settings_view = Some(view);
            self.settings_open = true;
        } else {
            self.settings_view = None;
            self.settings_open = false;
            let focus = self.toolbar.settings_focus(cx);
            window.focus(&focus);
        }
        self.captions.update(cx, |captions, cx| {
            captions.set_settings_hidden(open, cx);
        });
        cx.notify();
    }

    pub fn close_to_tray(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(native) = &self.native {
            if let Ok(geometry) = native.geometry() {
                self.geometry = geometry;
            }
            if let Ok(area) = native.current_work_area() {
                self.work_area = area;
            }
        }
        let geometry = capture_geometry(self.geometry, &self.work_area);
        match self.host.close_to_tray(geometry) {
            Ok(()) => {
                if let Some(native) = &self.native
                    && native.hide().is_err()
                {
                    self.model.report(CommandError::WindowUnavailable);
                }
            }
            Err(error) => self.model.report(error),
        }
        cx.notify();
    }

    pub fn show_edit(&mut self, cx: &mut Context<Self>) {
        (self.geometry, self.work_area) =
            visible_overlay_placement(self.geometry, &self.work_area, &self.areas);
        if let Some(native) = &self.native {
            let result = native
                .set_click_through(false)
                .and_then(|_| native.move_resize(self.geometry, &self.work_area))
                .and_then(|_| native.show());
            if result.is_err() {
                self.model.report(CommandError::WindowUnavailable);
            }
        }
        cx.notify();
    }

    pub fn saved_geometry(&self) -> SavedGeometry {
        capture_geometry(self.geometry, &self.work_area)
    }

    pub fn set_indicator(&mut self, indicator: WindowHandle<DictationWindow>, cx: &mut App) {
        self.indicator = Some(indicator);
        self.follow_indicator(cx);
    }

    fn geometry_changed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(native) = &self.native {
            if let Ok(geometry) = native.geometry() {
                self.geometry = geometry;
                self.toolbar
                    .paint_bounds()
                    .set_native_origin((geometry.x, geometry.y));
            }
            if let Ok(area) = native.current_work_area() {
                self.work_area = area;
            }
        }
        self.follow_indicator(cx);
        cx.notify();
    }

    fn follow_indicator(&mut self, cx: &mut App) {
        if let Some(indicator) = &self.indicator {
            let anchor = self.geometry;
            let area = self.work_area.clone();
            let _ = indicator.update(cx, |indicator, window, cx| {
                indicator.follow_overlay(anchor, area, window, cx);
            });
        }
    }

    fn begin_resize(&mut self, edge: ResizeEdge, cx: &mut Context<Self>) {
        if self.model.snapshot().window_mode == WindowMode::Locked {
            return;
        }
        if let Some(native) = &self.native
            && native.begin_resize(edge).is_err()
        {
            self.model.report(CommandError::WindowUnavailable);
            cx.notify();
        }
    }

    fn error_text(&self, locale: Locale) -> Option<String> {
        let key = if let Some(error) = self.model.error() {
            error_message_key(&error)
        } else if let Some(error) = self.model.snapshot().error {
            error_message_key(&CommandError::Domain(error))
        } else {
            self.dictation_error?
        };
        translate(locale, key).map(str::to_owned)
    }

    fn render_notices(&self, locale: Locale, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let mut notices = Vec::new();
        let snapshot = self.model.snapshot();
        if snapshot.speaker_labels && !snapshot.resources.speaker_labels {
            notices.push(
                div()
                    .id("notice")
                    .text_color(WARNING)
                    .child(
                        translate(locale, MessageKey::SpeakerLabelsUnavailable)
                            .unwrap_or("")
                            .to_owned(),
                    )
                    .into_any_element(),
            );
        }
        if let Some(check) = self.update_notice.visible_check() {
            let message = translate(locale, MessageKey::UpdateAvailable)
                .unwrap_or("New version {version} available.")
                .replace("{version}", &check.latest);
            notices.push(
                div()
                    .id("update-notice")
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .text_color(WARNING)
                    .child(message)
                    .child(
                        div()
                            .id("update-dismiss")
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_update_notice(cx);
                            })),
                    )
                    .into_any_element(),
            );
        }
        if let Some(error) = self.error_text(locale) {
            notices.push(
                div()
                    .id("error")
                    .text_color(ERROR)
                    .child(error)
                    .into_any_element(),
            );
        }
        notices
    }

    fn resize_grip(
        &self,
        edge: ResizeEdge,
        id: &'static str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let grip = div()
            .id(id)
            .debug_selector(|| id.into())
            .absolute()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if event.button == MouseButton::Left {
                        this.begin_resize(edge, cx);
                    }
                }),
            );
        match edge {
            ResizeEdge::West | ResizeEdge::East => {
                let grip = grip.top(px(48.0)).bottom_0().w(px(7.0));
                let grip = if edge == ResizeEdge::West {
                    grip.left_0()
                } else {
                    grip.right_0()
                };
                grip.cursor(CursorStyle::ResizeLeftRight)
            }
            ResizeEdge::South => grip
                .bottom_0()
                .left(px(48.0))
                .right(px(48.0))
                .h(px(7.0))
                .cursor(CursorStyle::ResizeUpDown),
        }
    }
}

impl Render for OverlayView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.model.snapshot();
        let locked = snapshot.window_mode == WindowMode::Locked;
        let locale = Locale::from_tag(snapshot.ui_language.tag()).unwrap_or_default();
        let stopped = snapshot.capture == CaptureState::Stopped;
        let notices = (!locked).then(|| self.render_notices(locale, cx));
        let settings = self.settings_view.clone();
        let border = if locked {
            transparent_black()
        } else {
            BORDER.into()
        };
        let background = if locked {
            transparent_black()
        } else {
            OVERLAY_EDIT_BACKGROUND.into()
        };

        div()
            .id("overlay")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(background)
            .when(!locked, |root| root.child(self.toolbar.render(stopped)))
            .child(
                div()
                    .id("overlay-content")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(self.captions.clone())
                    .when_some(settings, |content, settings| content.child(settings)),
            )
            .when_some(notices, |root, notices| {
                root.child(
                    div()
                        .px_3()
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_xs()
                        .children(notices),
                )
            })
            .when(!locked, |root| {
                root.child(self.resize_grip(ResizeEdge::West, "resize-left", cx))
                    .child(self.resize_grip(ResizeEdge::East, "resize-right", cx))
                    .child(self.resize_grip(ResizeEdge::South, "resize-bottom", cx))
            })
    }
}

impl Drop for OverlayView {
    fn drop(&mut self) {
        self.subscriptions.clear();
        self.settings_view = None;
    }
}

pub fn link_overlay_indicator(
    overlay: &WindowHandle<OverlayView>,
    indicator: &WindowHandle<DictationWindow>,
    cx: &mut App,
) {
    let _ = overlay.update(cx, |overlay, _, cx| {
        overlay.set_indicator(*indicator, cx);
    });
}

#[cfg(test)]
mod tests;
