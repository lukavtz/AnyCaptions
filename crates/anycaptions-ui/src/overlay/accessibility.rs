use std::{cell::RefCell, rc::Rc};

use anycaptions_core::{
    domain::{AppSnapshot, CaptionMode, CaptureState, WindowMode},
    presentation::{Locale, MessageKey, translate},
};
use anycaptions_platform_win32::{
    accessibility::{
        UiaAction, UiaMetadata, UiaProviderFactory, WidgetKind, widget_metadata as metadata,
    },
    accessibility_tree::{
        LiveNode, UiaFragment, UiaTree, clear_live_node as clear_live, sync_live_node,
    },
};
use gpui::{
    AnyWindowHandle, App, AsyncApp, Context, Entity, EntityId, WeakEntity, Window, prelude::*,
};

use crate::{
    create_uia_action_node, dispatch_uia_action,
    settings::{SettingsView, accessibility::SettingsAccessibility},
};

use super::{OverlayCommand, OverlayView, native_effects, toolbar::ToolbarPaintBounds};

struct ToolbarNodes {
    capture: UiaFragment,
    language: UiaFragment,
    settings: UiaFragment,
    lock: UiaFragment,
    close: UiaFragment,
    paint_bounds: Rc<ToolbarPaintBounds>,
}

pub(crate) struct OverlayAccessibility {
    _tree: UiaTree,
    root: UiaFragment,
    observer: std::rc::Weak<RefCell<Self>>,
    toolbar: Option<ToolbarNodes>,
    caption: Option<LiveNode>,
    settings: Option<Rc<RefCell<SettingsAccessibility>>>,
    settings_id: Option<EntityId>,
    notice: Option<LiveNode>,
    update_notice: Option<LiveNode>,
    error: Option<LiveNode>,
}

#[derive(Clone, Copy)]
enum ToolbarFocus {
    Capture,
    Language,
    Settings,
    Lock,
    Close,
}

#[derive(Clone, Copy)]
enum ToolbarAction {
    Capture,
    Settings,
    Lock,
    Close,
}

struct OverlayUiaSnapshot {
    app: AppSnapshot,
    settings_view: Option<gpui::AnyView>,
    settings_open: bool,
    caption: String,
    notice: String,
    update_notice: String,
    error: String,
    busy: bool,
    locked: bool,
}

impl OverlayUiaSnapshot {
    fn capture(view: &Entity<OverlayView>, cx: &App) -> Self {
        let overlay = view.read(cx);
        let app = overlay.model.snapshot().clone();
        let locale = Locale::from_tag(app.ui_language.tag()).unwrap_or_default();
        let notice = if app.speaker_labels && !app.resources.speaker_labels {
            text(locale, MessageKey::SpeakerLabelsUnavailable).to_owned()
        } else {
            String::new()
        };
        let update_notice = overlay
            .update_notice
            .visible_check()
            .map(|check| {
                text(locale, MessageKey::UpdateAvailable).replace("{version}", &check.latest)
            })
            .unwrap_or_default();
        let error = overlay.error_text(locale).unwrap_or_default();
        Self {
            locked: app.window_mode == WindowMode::Locked,
            busy: overlay.model.busy(),
            settings_view: overlay.settings_view.clone(),
            settings_open: overlay.settings_open,
            caption: overlay.captions.read(cx).accessible_text(),
            notice,
            update_notice,
            error,
            app,
        }
    }
}

impl OverlayAccessibility {
    /// Attaches once while the owning GPUI window and entity are being created.
    /// The detached window observer owns this object and synchronizes every notify.
    pub(crate) fn attach(
        view: &Entity<OverlayView>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        let accessibility = Self::initialize(view, window, cx)?;
        Self::observe(view, window, cx, accessibility);
        Ok(())
    }

    fn observe(
        view: &Entity<OverlayView>,
        window: &mut Window,
        cx: &mut App,
        accessibility: Rc<RefCell<Self>>,
    ) {
        let observed = Rc::clone(&accessibility);
        window
            .observe(view, cx, move |view, window, cx| {
                Self::observe_sync(&observed, view, window, cx);
            })
            .detach();
    }

    fn initialize(
        view: &Entity<OverlayView>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<Rc<RefCell<Self>>> {
        let effects = native_effects(window)?;
        let root_provider = UiaProviderFactory::create(UiaMetadata {
            automation_id: "overlay".into(),
            name: "AnyCaptions".into(),
            ..UiaMetadata::new(WidgetKind::ScrollArea)
        })?;
        let tree = UiaTree::attach(&effects, root_provider)?;
        let root = tree.root();
        let accessibility = Rc::new(RefCell::new(Self {
            _tree: tree,
            root,
            observer: std::rc::Weak::new(),
            toolbar: None,
            caption: None,
            settings: None,
            settings_id: None,
            notice: None,
            update_notice: None,
            error: None,
        }));
        accessibility.borrow_mut().observer = Rc::downgrade(&accessibility);
        accessibility.borrow_mut().sync(view, window, cx)?;
        Ok(accessibility)
    }

    fn observe_sync(
        observed: &Rc<RefCell<Self>>,
        view: Entity<OverlayView>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let result = observed
            .try_borrow_mut()
            .map_err(|_| "UIA observer re-entered".to_owned())
            .and_then(|mut accessibility| {
                accessibility
                    .sync(&view, window, cx)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            eprintln!("AnyCaptions UIA synchronization failed: {error}");
        }
    }

    fn sync(
        &mut self,
        view: &Entity<OverlayView>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        let OverlayUiaSnapshot {
            app,
            settings_view,
            settings_open,
            caption,
            notice,
            update_notice,
            error,
            busy,
            locked,
        } = OverlayUiaSnapshot::capture(view, cx);
        self.sync_toolbar(view, &app, busy, locked, window, cx)?;
        self.sync_caption(caption, settings_open)?;
        self.sync_settings(view, settings_view, settings_open, window, cx)?;
        self.sync_live_regions(locked, notice, update_notice, error)?;
        if let Some(toolbar) = &self.toolbar {
            toolbar
                .settings
                .set_focused(view.read(cx).toolbar.settings_focus(cx).is_focused(window))?;
        }
        Ok(())
    }

    fn sync_live_regions(
        &mut self,
        locked: bool,
        notice: String,
        update_notice: String,
        error: String,
    ) -> gpui::Result<()> {
        if locked {
            clear_live(&mut self.notice)?;
            clear_live(&mut self.update_notice)?;
            clear_live(&mut self.error)?;
            return Ok(());
        }
        sync_live_node(
            &self.root,
            &mut self.notice,
            WidgetKind::Status,
            "notice",
            notice,
        )?;
        sync_live_node(
            &self.root,
            &mut self.update_notice,
            WidgetKind::Status,
            "update-notice",
            update_notice,
        )?;
        Ok(sync_live_node(
            &self.root,
            &mut self.error,
            WidgetKind::Alert,
            "error",
            error,
        )?)
    }

    fn sync_toolbar(
        &mut self,
        view: &Entity<OverlayView>,
        snapshot: &AppSnapshot,
        busy: bool,
        locked: bool,
        window: &Window,
        cx: &App,
    ) -> gpui::Result<()> {
        if locked {
            self.toolbar = None;
            return Ok(());
        }
        let language_expanded = view.read(cx).toolbar.language_expanded(cx);
        if self.toolbar.is_none() {
            self.toolbar = Some(ToolbarNodes::attach(
                &self.root,
                ToolbarState {
                    snapshot,
                    busy,
                    language_expanded,
                },
                view.downgrade(),
                cx.to_async(),
                window.window_handle(),
                view.read(cx).toolbar.paint_bounds(),
            )?);
        }
        self.toolbar
            .as_ref()
            .expect("toolbar created")
            .sync(snapshot, busy, language_expanded)
    }

    fn sync_caption(&mut self, text: String, hidden: bool) -> gpui::Result<()> {
        if hidden {
            return Ok(clear_live(&mut self.caption)?);
        }
        if let Some(caption) = self.caption.as_mut() {
            return Self::update_caption(caption, text);
        }
        self.create_caption(text)
    }

    fn update_caption(caption: &mut LiveNode, text: String) -> gpui::Result<()> {
        if caption.text == text {
            return Ok(());
        }
        caption.fragment.set_metadata(UiaMetadata {
            name: text.clone(),
            ..UiaMetadata::new(WidgetKind::Caption)
        })?;
        Self::announce_caption(&caption.fragment, &text)?;
        caption.text = text;
        Ok(())
    }

    fn create_caption(&mut self, text: String) -> gpui::Result<()> {
        let fragment = self
            .root
            .add_child(UiaProviderFactory::create(UiaMetadata {
                automation_id: "caption-live".into(),
                name: text.clone(),
                ..UiaMetadata::new(WidgetKind::Caption)
            })?)?;
        Self::announce_caption(&fragment, &text)?;
        self.caption = Some(LiveNode { fragment, text });
        Ok(())
    }

    fn announce_caption(fragment: &UiaFragment, text: &str) -> gpui::Result<()> {
        if text.is_empty() {
            Ok(())
        } else {
            Ok(fragment.announce()?)
        }
    }

    fn sync_settings(
        &mut self,
        _view: &Entity<OverlayView>,
        settings_view: Option<gpui::AnyView>,
        open: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        let entity = open
            .then_some(settings_view)
            .flatten()
            .and_then(|view| view.downcast::<SettingsView>().ok());
        let Some(entity) = entity else {
            self.settings = None;
            self.settings_id = None;
            return Ok(());
        };
        if self.settings_id != Some(entity.entity_id()) {
            self.settings = None;
            self.settings_id = Some(entity.entity_id());
        }
        if let Some(accessibility) = self.settings.as_ref() {
            let accessibility = Rc::clone(accessibility);
            entity.update(cx, |view, cx| {
                let mut accessibility = accessibility
                    .try_borrow_mut()
                    .map_err(|_| std::io::Error::other("UIA settings sync re-entered"))?;
                accessibility.sync(view, window, cx)
            })?;
            return Ok(());
        }
        let weak = entity.downgrade();
        let settings = entity.update(cx, |view, cx| {
            SettingsAccessibility::attach(&self.root, view, weak, window, cx)
        })?;
        let settings = Rc::new(RefCell::new(settings));
        self.settings = Some(Rc::clone(&settings));
        self.observe_settings(&entity, window, cx, settings);
        Ok(())
    }

    fn observe_settings(
        &mut self,
        entity: &Entity<SettingsView>,
        window: &mut Window,
        cx: &mut App,
        accessibility: Rc<RefCell<SettingsAccessibility>>,
    ) {
        let accessibility = Rc::downgrade(&accessibility);
        window
            .observe(entity, cx, move |view, window, cx| {
                let Some(accessibility) = accessibility.upgrade() else {
                    return;
                };
                let result = accessibility
                    .try_borrow_mut()
                    .map_err(|_| "UIA settings observer re-entered".to_owned())
                    .and_then(|mut accessibility| {
                        accessibility
                            .sync(view.read(cx), window, cx)
                            .map_err(|error| error.to_string())
                    });
                if let Err(error) = result {
                    eprintln!("AnyCaptions settings UIA synchronization failed: {error}");
                }
            })
            .detach();
    }
}

#[derive(Clone, Copy)]
struct ToolbarState<'a> {
    snapshot: &'a AppSnapshot,
    busy: bool,
    language_expanded: bool,
}

impl ToolbarNodes {
    fn attach(
        root: &UiaFragment,
        state: ToolbarState<'_>,
        weak: WeakEntity<OverlayView>,
        app: AsyncApp,
        window: AnyWindowHandle,
        paint_bounds: Rc<ToolbarPaintBounds>,
    ) -> gpui::Result<Self> {
        let capture = add_toolbar_action_node(
            root,
            capture_metadata(state.snapshot, state.busy),
            ToolbarAction::Capture,
            weak.clone(),
            app.clone(),
            window,
        )?;
        let language = create_uia_action_node(
            root,
            language_metadata(state.snapshot, state.busy, state.language_expanded),
            app.clone(),
            {
                let weak = weak.clone();
                move |action, app| {
                    let UiaAction::Expanded(expanded) = action else {
                        return Ok(());
                    };
                    dispatch_uia_action(app, window, &weak, move |view, window, cx| {
                        view.toolbar.set_language_expanded(expanded, window, cx);
                    })
                }
            },
        )?;
        let settings = add_toolbar_action_node(
            root,
            static_button_metadata("settings-toggle", MessageKey::Settings),
            ToolbarAction::Settings,
            weak.clone(),
            app.clone(),
            window,
        )?;
        let lock = add_toolbar_action_node(
            root,
            lock_metadata(state.snapshot, state.busy),
            ToolbarAction::Lock,
            weak.clone(),
            app.clone(),
            window,
        )?;
        let close = add_toolbar_action_node(
            root,
            static_button_metadata("close", MessageKey::CloseToTray),
            ToolbarAction::Close,
            weak.clone(),
            app.clone(),
            window,
        )?;
        for (fragment, target) in [
            (&capture, ToolbarFocus::Capture),
            (&language, ToolbarFocus::Language),
            (&settings, ToolbarFocus::Settings),
            (&lock, ToolbarFocus::Lock),
            (&close, ToolbarFocus::Close),
        ] {
            install_toolbar_focus(fragment, weak.clone(), app.clone(), window, target)?;
        }
        paint_bounds.attach_nodes(&capture, &language, &settings, &lock, &close);
        Ok(Self {
            capture,
            language,
            settings,
            lock,
            close,
            paint_bounds,
        })
    }

    fn sync(
        &self,
        snapshot: &AppSnapshot,
        busy: bool,
        language_expanded: bool,
    ) -> gpui::Result<()> {
        self.capture
            .set_metadata(capture_metadata(snapshot, busy))?;
        self.language
            .set_metadata(language_metadata(snapshot, busy, language_expanded))?;
        self.settings.set_metadata(static_button_metadata(
            "settings-toggle",
            MessageKey::Settings,
        ))?;
        self.lock.set_metadata(lock_metadata(snapshot, busy))?;
        self.close
            .set_metadata(static_button_metadata("close", MessageKey::CloseToTray))?;
        Ok(())
    }
}

impl Drop for ToolbarNodes {
    fn drop(&mut self) {
        self.paint_bounds.clear();
        let _ = self.capture.detach();
        let _ = self.language.detach();
        let _ = self.settings.detach();
        let _ = self.lock.detach();
        let _ = self.close.detach();
    }
}

fn add_toolbar_action_node(
    root: &UiaFragment,
    metadata: UiaMetadata,
    action: ToolbarAction,
    weak: WeakEntity<OverlayView>,
    app: AsyncApp,
    window: AnyWindowHandle,
) -> gpui::Result<UiaFragment> {
    create_uia_action_node(root, metadata, app, move |action_kind, app| {
        if action_kind != UiaAction::Invoke {
            return Ok(());
        }
        dispatch_uia_action(app, window, &weak, move |view, window, cx| {
            invoke_toolbar_action(action, view, window, cx);
        })
    })
}

fn invoke_toolbar_action(
    action: ToolbarAction,
    view: &mut OverlayView,
    window: &mut Window,
    cx: &mut Context<OverlayView>,
) {
    match action {
        ToolbarAction::Capture => {
            let command = if view.model.snapshot().capture == CaptureState::Stopped {
                OverlayCommand::StartCapture
            } else {
                OverlayCommand::StopCapture
            };
            view.perform_action(command, window, cx);
        }
        ToolbarAction::Settings => {
            let open = !view.settings_open();
            view.set_settings_open(open, window, cx);
        }
        ToolbarAction::Lock => view.perform_action(OverlayCommand::LockWindow, window, cx),
        ToolbarAction::Close => view.close_to_tray(window, cx),
    }
}

fn locale(snapshot: &AppSnapshot) -> Locale {
    Locale::from_tag(snapshot.ui_language.tag()).unwrap_or_default()
}

fn text(locale: Locale, key: MessageKey) -> &'static str {
    translate(locale, key).unwrap_or("")
}

fn capture_metadata(snapshot: &AppSnapshot, busy: bool) -> UiaMetadata {
    let stopped = snapshot.capture == CaptureState::Stopped;
    let locale = locale(snapshot);
    let name = if stopped {
        format!("▶ {}", text(locale, MessageKey::Start))
    } else {
        format!("■ {}", text(locale, MessageKey::Stop))
    };
    UiaMetadata {
        disabled: busy || (stopped && !snapshot.can_start),
        ..metadata(WidgetKind::Button, "capture", name)
    }
}

fn language_metadata(snapshot: &AppSnapshot, busy: bool, expanded: bool) -> UiaMetadata {
    let locale = locale(snapshot);
    let name = match snapshot.mode {
        CaptionMode::Auto => text(locale, MessageKey::TranscribeMode).to_owned(),
        CaptionMode::Translate(_) => text(locale, MessageKey::TranslateTo).to_owned(),
    };
    UiaMetadata {
        expanded,
        disabled: busy || snapshot.window_mode == WindowMode::Locked,
        ..metadata(WidgetKind::Picker, "language", name)
    }
}

fn static_button_metadata(id: &str, key: MessageKey) -> UiaMetadata {
    metadata(WidgetKind::Button, id, text(Locale::En, key).to_owned())
}

fn lock_metadata(snapshot: &AppSnapshot, busy: bool) -> UiaMetadata {
    UiaMetadata {
        disabled: busy || !snapshot.can_lock,
        ..metadata(
            WidgetKind::Button,
            "lock",
            text(locale(snapshot), MessageKey::LockWindow).to_owned(),
        )
    }
}

fn install_toolbar_focus(
    fragment: &UiaFragment,
    weak: WeakEntity<OverlayView>,
    app: AsyncApp,
    window: AnyWindowHandle,
    target: ToolbarFocus,
) -> gpui::Result<()> {
    let app = Rc::new(RefCell::new(app));
    fragment.set_focus_handler(move || {
        let _ = app.borrow_mut().update_window(window, |_, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                let focus = view.toolbar.settings_focus(cx);
                window.focus(&focus);
                match target {
                    ToolbarFocus::Capture => {
                        window.focus_prev();
                        window.focus_prev();
                    }
                    ToolbarFocus::Language => window.focus_prev(),
                    ToolbarFocus::Settings => {}
                    ToolbarFocus::Lock => window.focus_next(),
                    ToolbarFocus::Close => {
                        window.focus_next();
                        window.focus_next();
                    }
                }
            });
        });
    })?;
    Ok(())
}
