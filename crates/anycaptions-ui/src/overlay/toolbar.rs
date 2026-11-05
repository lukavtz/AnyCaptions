use std::{cell::RefCell, rc::Rc};

use anycaptions_core::{
    domain::{AppSnapshot, CaptionMode, TranslationEngine, WindowMode},
    presentation::{LanguageCatalog, Locale, MessageKey, translate},
};
use anycaptions_platform_win32::{accessibility::UiaBounds, accessibility_tree::UiaFragment};
use gpui::{
    App, Bounds, Context, Entity, Focusable, Pixels, WeakEntity, Window, WindowControlArea, div,
    prelude::*, px,
};

use crate::{
    theme::{BASE_CONTENT, BORDER_STRONG, CLOSE_HOVER, MUTED},
    widgets::{Button, Picker, PickerFooter, PickerOption},
};

use super::{OverlayCommand, OverlayView};

fn text(locale: Locale, key: MessageKey) -> &'static str {
    translate(locale, key).unwrap_or("")
}

fn selected_mode(mode: CaptionMode) -> String {
    match mode {
        CaptionMode::Auto => "auto".into(),
        CaptionMode::Translate(target) => format!("translate-{}", target.tag()),
    }
}

#[derive(Default)]
pub(crate) struct ToolbarPaintBounds {
    capture: RefCell<Option<UiaFragment>>,
    language: RefCell<Option<UiaFragment>>,
    settings: RefCell<Option<UiaFragment>>,
    lock: RefCell<Option<UiaFragment>>,
    close: RefCell<Option<UiaFragment>>,
    origin: RefCell<Option<(f64, f64)>>,
}

impl ToolbarPaintBounds {
    pub(crate) fn attach_nodes(
        &self,
        capture: &UiaFragment,
        language: &UiaFragment,
        settings: &UiaFragment,
        lock: &UiaFragment,
        close: &UiaFragment,
    ) {
        *self.capture.borrow_mut() = Some(capture.clone());
        *self.language.borrow_mut() = Some(language.clone());
        *self.settings.borrow_mut() = Some(settings.clone());
        *self.lock.borrow_mut() = Some(lock.clone());
        *self.close.borrow_mut() = Some(close.clone());
    }

    pub(crate) fn set_native_origin(&self, origin: (f64, f64)) {
        *self.origin.borrow_mut() = Some(origin);
    }

    pub(crate) fn clear(&self) {
        *self.origin.borrow_mut() = None;
        *self.capture.borrow_mut() = None;
        *self.language.borrow_mut() = None;
        *self.settings.borrow_mut() = None;
        *self.lock.borrow_mut() = None;
        *self.close.borrow_mut() = None;
    }

    fn update_controls(&self, children: Vec<Bounds<Pixels>>, window: &Window) {
        let origin = self.native_origin(window);
        for (slot, bounds) in [
            (&self.capture, children.first()),
            (&self.language, children.get(1)),
            (&self.settings, children.get(2)),
            (&self.lock, children.get(3)),
        ] {
            if let Some(bounds) = bounds {
                Self::update_fragment(slot, *bounds, window, origin);
            }
        }
    }

    fn update_close(&self, children: Vec<Bounds<Pixels>>, window: &Window) {
        let origin = self.native_origin(window);
        if let Some(bounds) = children.last() {
            Self::update_fragment(&self.close, *bounds, window, origin);
        }
    }

    fn native_origin(&self, window: &Window) -> (f64, f64) {
        self.origin.borrow().unwrap_or_else(|| {
            let origin = window.bounds().origin;
            (
                f64::from(f32::from(origin.x)),
                f64::from(f32::from(origin.y)),
            )
        })
    }

    fn update_fragment(
        slot: &RefCell<Option<UiaFragment>>,
        bounds: Bounds<Pixels>,
        window: &Window,
        origin: (f64, f64),
    ) {
        let Some(fragment) = slot.borrow().clone() else {
            return;
        };
        let scale = f64::from(window.scale_factor());
        let result = fragment.set_bounds(UiaBounds {
            left: (origin.0 + f64::from(f32::from(bounds.origin.x))) * scale,
            top: (origin.1 + f64::from(f32::from(bounds.origin.y))) * scale,
            width: f64::from(f32::from(bounds.size.width)) * scale,
            height: f64::from(f32::from(bounds.size.height)) * scale,
        });
        if let Err(error) = result {
            eprintln!("AnyCaptions UIA bounds update failed: {error}");
        }
    }
}

pub(crate) struct Toolbar {
    start: Entity<Button>,
    stop: Entity<Button>,
    settings: Entity<Button>,
    lock: Entity<Button>,
    close: Entity<Button>,
    language: Entity<Picker>,
    locale: Locale,
    mode: CaptionMode,
    engine: TranslationEngine,
    translation_available: bool,
    busy: bool,
    locked: bool,
    paint_bounds: Rc<ToolbarPaintBounds>,
}

impl Toolbar {
    pub fn new(
        snapshot: &AppSnapshot,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<OverlayView>,
    ) -> Self {
        let locale = Locale::from_tag(snapshot.ui_language.tag()).unwrap_or_default();
        let weak = cx.weak_entity();
        let start_weak = weak.clone();
        let stop_weak = weak.clone();
        let settings_weak = weak.clone();
        let lock_weak = weak.clone();
        let close_weak = weak.clone();
        let start = cx.new(|button_cx| {
            Button::new(format!("▶ {}", text(locale, MessageKey::Start)), button_cx)
                .disabled(busy || !snapshot.can_start)
                .on_press(move |window, cx| {
                    let _ = start_weak.update(cx, |overlay, cx| {
                        overlay.perform_action(OverlayCommand::StartCapture, window, cx);
                    });
                })
        });
        let stop = cx.new(|button_cx| {
            Button::new(format!("■ {}", text(locale, MessageKey::Stop)), button_cx)
                .disabled(busy)
                .on_press(move |window, cx| {
                    let _ = stop_weak.update(cx, |overlay, cx| {
                        overlay.perform_action(OverlayCommand::StopCapture, window, cx);
                    });
                })
        });
        let settings = cx.new(|button_cx| {
            Button::new(text(locale, MessageKey::Settings), button_cx).on_press(
                move |window, cx| {
                    let _ = settings_weak.update(cx, |overlay, cx| {
                        overlay.set_settings_open(!overlay.settings_open(), window, cx);
                    });
                },
            )
        });
        let lock = cx.new(|button_cx| {
            Button::new(text(locale, MessageKey::LockWindow), button_cx)
                .disabled(busy || !snapshot.can_lock)
                .on_press(move |window, cx| {
                    let _ = lock_weak.update(cx, |overlay, cx| {
                        overlay.perform_action(OverlayCommand::LockWindow, window, cx);
                    });
                })
        });
        let close = cx.new(|button_cx| {
            Button::new(text(locale, MessageKey::CloseToTray), button_cx).on_press(
                move |window, cx| {
                    let _ = close_weak.update(cx, |overlay, cx| {
                        overlay.close_to_tray(window, cx);
                    });
                },
            )
        });
        let language = Self::make_language(snapshot, busy, weak, window, cx);
        let mut toolbar = Self {
            start,
            stop,
            settings,
            lock,
            close,
            language,
            locale,
            mode: snapshot.mode,
            engine: snapshot.translation_engine,
            translation_available: snapshot.resources.translation,
            busy,
            locked: snapshot.window_mode == WindowMode::Locked,
            paint_bounds: Rc::new(ToolbarPaintBounds::default()),
        };
        toolbar.sync_buttons(snapshot, busy, cx);
        toolbar
    }

    pub(crate) fn paint_bounds(&self) -> Rc<ToolbarPaintBounds> {
        Rc::clone(&self.paint_bounds)
    }

    fn make_language(
        snapshot: &AppSnapshot,
        busy: bool,
        weak: WeakEntity<OverlayView>,
        window: &mut Window,
        cx: &mut Context<OverlayView>,
    ) -> Entity<Picker> {
        let locale = Locale::from_tag(snapshot.ui_language.tag()).unwrap_or_default();
        let mut options = vec![
            PickerOption::new("auto", text(locale, MessageKey::TranscribeMode))
                .name(text(locale, MessageKey::AutoModeHint)),
        ];
        options.extend(LanguageCatalog::translation_targets().iter().map(|target| {
            PickerOption::new(
                target.mode,
                format!("{} {}", text(locale, MessageKey::TranslateTo), target.short),
            )
            .name(target.name)
            .country(target.country)
            .disabled(!snapshot.resources.translation)
            .hidden(
                !snapshot.translation_engine.supports(
                    target
                        .caption_mode()
                        .translation_target()
                        .expect("translation target"),
                ),
            )
        }));
        let footer_weak = weak.clone();
        let change_weak = weak.clone();
        let selected = selected_mode(snapshot.mode);
        let disabled = busy || snapshot.window_mode == WindowMode::Locked;
        cx.new(|picker_cx| {
            let mut picker = Picker::new(options, window, picker_cx)
                .with_selected(selected)
                .disabled(disabled)
                .on_change(move |value, window, cx| {
                    let mode = if value == "auto" {
                        Some(CaptionMode::Auto)
                    } else {
                        LanguageCatalog::translation_targets()
                            .iter()
                            .find(|target| target.mode == value.as_str())
                            .map(|target| target.caption_mode())
                    };
                    if let Some(mode) = mode {
                        let _ = change_weak.update(cx, |overlay, cx| {
                            overlay.perform_action(OverlayCommand::SetMode(mode), window, cx);
                        });
                    }
                });
            if snapshot.translation_engine == TranslationEngine::Opus {
                picker = picker.footer(PickerFooter::new(
                    text(locale, MessageKey::MoreLanguagesRiva),
                    move |window, cx| {
                        let _ = footer_weak.update(cx, |overlay, cx| {
                            overlay.set_settings_open(true, window, cx);
                        });
                    },
                ));
            }
            picker
        })
    }

    pub fn sync(
        &mut self,
        snapshot: &AppSnapshot,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<OverlayView>,
    ) {
        let locale = Locale::from_tag(snapshot.ui_language.tag()).unwrap_or_default();
        if locale != self.locale {
            let previous_focus = window.focused(cx);
            *self = Self::new(snapshot, busy, window, cx);
            if let Some(focus) = previous_focus {
                window.focus(&focus);
            }
            return;
        }
        self.sync_buttons(snapshot, busy, cx);
        let locked = snapshot.window_mode == WindowMode::Locked;
        if self.mode != snapshot.mode
            || self.engine != snapshot.translation_engine
            || self.translation_available != snapshot.resources.translation
            || self.busy != busy
            || self.locked != locked
        {
            let previous_focus = window.focused(cx);
            let language_focused = self.language.read(cx).focus_handle(cx).is_focused(window);
            self.language = Self::make_language(snapshot, busy, cx.weak_entity(), window, cx);
            self.mode = snapshot.mode;
            self.engine = snapshot.translation_engine;
            self.translation_available = snapshot.resources.translation;
            self.busy = busy;
            self.locked = locked;
            if language_focused {
                let focus = self.language.read(cx).focus_handle(cx);
                window.focus(&focus);
            } else if let Some(focus) = previous_focus {
                window.focus(&focus);
            }
        }
    }

    fn sync_buttons(&mut self, snapshot: &AppSnapshot, busy: bool, cx: &mut Context<OverlayView>) {
        self.start.update(cx, |button, cx| {
            button.set_disabled(busy || !snapshot.can_start, cx);
        });
        self.stop.update(cx, |button, cx| {
            button.set_disabled(busy, cx);
        });
        self.lock.update(cx, |button, cx| {
            button.set_disabled(busy || !snapshot.can_lock, cx);
        });
    }

    pub fn settings_focus(&self, cx: &App) -> gpui::FocusHandle {
        self.settings.read(cx).focus_handle(cx)
    }

    pub fn render(&self, stopped: bool) -> impl IntoElement {
        let capture = if stopped {
            self.start.clone()
        } else {
            self.stop.clone()
        };
        let control_bounds = self.paint_bounds.clone();
        let close_bounds = self.paint_bounds.clone();
        div()
            .on_children_prepainted(move |children, window, _| {
                close_bounds.update_close(children, window);
            })
            .id("overlay-toolbar")
            .h(px(40.0))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_2()
            .pt_2()
            .child(
                div()
                    .id("overlay-brand")
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        div()
                            .border_1()
                            .border_color(BORDER_STRONG)
                            .rounded_sm()
                            .px_1()
                            .text_xs()
                            .child("cc"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("AnyCaptions"),
                    ),
            )
            .child(
                div()
                    .id("overlay-drag-spacer")
                    .h_full()
                    .flex_1()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .on_children_prepainted(move |children, window, _| {
                        control_bounds.update_controls(children, window);
                    })
                    .id("overlay-controls")
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().id("capture").child(capture))
                    .child(div().id("language").child(self.language.clone()))
                    .child(div().id("settings-toggle").child(self.settings.clone()))
                    .child(div().id("lock").child(self.lock.clone())),
            )
            .child(
                div()
                    .id("close")
                    .text_color(MUTED)
                    .hover(|style| style.bg(CLOSE_HOVER).text_color(BASE_CONTENT))
                    .child(self.close.clone()),
            )
    }
}
