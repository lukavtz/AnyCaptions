use std::{cell::RefCell, rc::Rc};

use anycaptions_platform_win32::{
    accessibility::{
        UiaMetadata, UiaProviderFactory, UiaRange, WidgetKind, widget_metadata as metadata,
    },
    accessibility_tree::{LiveNode, UiaFragment, sync_live_node},
};
use gpui::{AnyWindowHandle, App, AsyncApp, Context, Focusable, WeakEntity, Window, prelude::*};

use super::{
    CAPTION_COLOR_AUTOMATION_ID, ColorTarget, ControlFocusExt, DownloadRow, SETTINGS_AUTOMATION_ID,
    SOURCE_LANGUAGE_AUTOMATION_ID, SOURCE_LANGUAGE_MENU_AUTOMATION_ID, SettingsView,
    languages::asr_options,
};
use crate::widgets::{Down, Home};

struct MenuNodes {
    root: UiaFragment,
    options: Vec<UiaFragment>,
    values: Vec<String>,
}

pub(crate) struct SettingsAccessibility {
    root: UiaFragment,
    picker: UiaFragment,
    weak: WeakEntity<SettingsView>,
    app: AsyncApp,
    window: AnyWindowHandle,
    color: UiaFragment,
    menu: Option<MenuNodes>,
    translation_progress: Option<UiaFragment>,
    diarization_progress: Option<UiaFragment>,
    update_progress: Option<UiaFragment>,
    error: Option<LiveNode>,
    speaker_notice: Option<LiveNode>,
    update_notice: Option<LiveNode>,
}

#[derive(Clone, Copy)]
enum FocusTarget {
    Root,
    Picker,
    Option(usize),
    Color,
}

impl SettingsAccessibility {
    pub(crate) fn attach(
        parent: &UiaFragment,
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        window: &Window,
        cx: &mut Context<SettingsView>,
    ) -> gpui::Result<Self> {
        let root = parent.add_child(UiaProviderFactory::create(metadata(
            WidgetKind::ScrollArea,
            SETTINGS_AUTOMATION_ID,
            view.text(anycaptions_core::presentation::MessageKey::Settings),
        ))?)?;
        let color = root.add_child(UiaProviderFactory::create(color_metadata(view))?)?;
        let picker = root.add_child(UiaProviderFactory::create(picker_metadata(view, cx))?)?;
        let app = cx.to_async();
        let window_handle = window.window_handle();
        install_focus_handlers(
            &root,
            &color,
            &picker,
            weak.clone(),
            app.clone(),
            window_handle,
        )?;
        let mut accessibility = Self {
            root,
            picker,
            weak,
            app,
            window: window_handle,
            color,
            menu: None,
            translation_progress: None,
            diarization_progress: None,
            update_progress: None,
            error: None,
            speaker_notice: None,
            update_notice: None,
        };
        accessibility.sync(view, window, cx)?;
        Ok(accessibility)
    }

    pub(crate) fn sync(
        &mut self,
        view: &SettingsView,
        window: &Window,
        cx: &App,
    ) -> gpui::Result<()> {
        self.picker.set_metadata(picker_metadata(view, cx))?;
        self.color.set_metadata(color_metadata(view))?;
        self.sync_menu(view, cx)?;
        self.sync_progress(view)?;
        self.sync_live(view)?;
        self.sync_focus(view, window, cx)?;
        Ok(())
    }

    fn sync_menu(&mut self, view: &SettingsView, cx: &App) -> gpui::Result<()> {
        let options = asr_options(view)
            .into_iter()
            .filter(|option| !option.hidden)
            .collect::<Vec<_>>();
        if !view.automation_state(cx).source_language_expanded {
            return self.close_menu();
        }

        let values = options
            .iter()
            .map(|option| option.value.to_string())
            .collect::<Vec<_>>();
        self.reconcile_menu_values(&values)?;
        self.create_menu_if_missing(&options, values, view, cx)?;
        self.sync_menu_options(&options, view, cx)
    }

    fn close_menu(&mut self) -> gpui::Result<()> {
        if let Some(menu) = self.menu.take() {
            menu.root.detach()?;
        }
        Ok(())
    }

    fn reconcile_menu_values(&mut self, values: &[String]) -> gpui::Result<()> {
        if self.menu.as_ref().is_some_and(|menu| menu.values != values)
            && let Some(menu) = self.menu.take()
        {
            menu.root.detach()?;
        }
        Ok(())
    }

    fn create_menu_if_missing(
        &mut self,
        options: &[crate::widgets::PickerOption],
        values: Vec<String>,
        view: &SettingsView,
        cx: &App,
    ) -> gpui::Result<()> {
        if self.menu.is_some() {
            return Ok(());
        }
        let root = self.picker.add_child(UiaProviderFactory::create(metadata(
            WidgetKind::List,
            SOURCE_LANGUAGE_MENU_AUTOMATION_ID,
            view.text(anycaptions_core::presentation::MessageKey::SourceLanguage),
        ))?)?;
        let mut nodes = Vec::with_capacity(options.len());
        for (index, option) in options.iter().enumerate() {
            let id = option_id(&option.value);
            let node = root.add_child(UiaProviderFactory::create(option_metadata(
                &id, option, view, cx,
            ))?)?;
            install_focus(
                &node,
                self.weak.clone(),
                self.app.clone(),
                self.window,
                FocusTarget::Option(index),
            )?;
            nodes.push(node);
        }
        self.menu = Some(MenuNodes {
            root,
            options: nodes,
            values,
        });
        Ok(())
    }

    fn sync_menu_options(
        &self,
        options: &[crate::widgets::PickerOption],
        view: &SettingsView,
        cx: &App,
    ) -> gpui::Result<()> {
        let menu = self.menu.as_ref().expect("menu created");
        for (node, option) in menu.options.iter().zip(options) {
            node.set_metadata(option_metadata(&option_id(&option.value), option, view, cx))?;
        }
        Ok(())
    }

    fn sync_progress(&mut self, view: &SettingsView) -> gpui::Result<()> {
        for (row, slot, id) in [
            (
                DownloadRow::Translation,
                &mut self.translation_progress,
                "translation-progress",
            ),
            (
                DownloadRow::Diarization,
                &mut self.diarization_progress,
                "diarization-progress",
            ),
        ] {
            let download = view.download_view(row);
            sync_progress_node(
                &self.root,
                slot,
                id,
                download
                    .as_ref()
                    .is_some_and(|download| download.visible && download.progress_visible),
                download.as_ref().map_or(0, |download| download.percent),
                download.map_or_else(String::new, |download| {
                    view.download_status_label(&download)
                }),
            )?;
        }
        let update = view.update_action_view();
        sync_progress_node(
            &self.root,
            &mut self.update_progress,
            "update-progress",
            update.as_ref().and_then(|action| action.percent).is_some(),
            update
                .as_ref()
                .and_then(|action| action.percent)
                .unwrap_or(0) as u8,
            view.update_status_message(),
        )?;
        Ok(())
    }

    fn sync_live(&mut self, view: &SettingsView) -> gpui::Result<()> {
        let error = view.application.error().map(|error| {
            view.text(anycaptions_core::presentation::error_message_key(&error))
                .to_owned()
        });
        sync_live_node(
            &self.root,
            &mut self.error,
            WidgetKind::Alert,
            "error",
            error.unwrap_or_default(),
        )?;
        sync_live_node(
            &self.root,
            &mut self.speaker_notice,
            WidgetKind::Status,
            "speaker-notice",
            view.speaker_notice()
                .map(|key| view.text(key).to_owned())
                .unwrap_or_default(),
        )?;
        let update_failed = view
            .update_action_view()
            .is_some_and(|action| action.failed);
        sync_live_node(
            &self.root,
            &mut self.update_notice,
            WidgetKind::Status,
            "update-download-failed",
            if update_failed {
                view.update_status_message()
            } else {
                String::new()
            },
        )?;
        Ok(())
    }

    fn sync_focus(&self, view: &SettingsView, window: &Window, cx: &App) -> gpui::Result<()> {
        self.root
            .set_focused(view.focus_handle(cx).is_focused(window))?;
        self.picker.set_focused(
            view.controls()
                .languages
                .source_language
                .control_focus(cx)
                .is_focused(window),
        )?;
        self.color.set_focused(
            view.controls()
                .appearance
                .confirmed_input
                .control_focus(cx)
                .is_focused(window),
        )?;
        Ok(())
    }
}

impl Drop for SettingsAccessibility {
    fn drop(&mut self) {
        let _ = self.root.detach();
    }
}

fn picker_metadata(view: &SettingsView, cx: &App) -> UiaMetadata {
    UiaMetadata {
        expanded: view.automation_state(cx).source_language_expanded,
        disabled: view.primary_disabled(),
        ..metadata(
            WidgetKind::Picker,
            SOURCE_LANGUAGE_AUTOMATION_ID,
            view.text(anycaptions_core::presentation::MessageKey::SourceLanguage),
        )
    }
}

fn color_metadata(view: &SettingsView) -> UiaMetadata {
    UiaMetadata {
        value: Some(view.color_text(ColorTarget::Confirmed).to_owned()),
        invalid: !view.color_valid(ColorTarget::Confirmed),
        disabled: view.primary_disabled(),
        ..metadata(
            WidgetKind::TextInput,
            CAPTION_COLOR_AUTOMATION_ID,
            view.text(anycaptions_core::presentation::MessageKey::ConfirmedColorHex),
        )
    }
}

fn option_metadata(
    id: &str,
    option: &crate::widgets::PickerOption,
    view: &SettingsView,
    cx: &App,
) -> UiaMetadata {
    let selected =
        view.controls().languages.source_language.read(cx).value() == option.value.as_ref();
    UiaMetadata {
        selected,
        disabled: option.disabled,
        name: option.name.as_ref().unwrap_or(&option.label).to_string(),
        ..metadata(WidgetKind::PickerOption, id, "")
    }
}

fn option_id(value: &str) -> String {
    if value.is_empty() {
        "source-language-option-auto".to_owned()
    } else {
        format!("source-language-option-{value}")
    }
}

fn progress_metadata(id: &str, name: String, percent: u8) -> UiaMetadata {
    UiaMetadata {
        name,
        range: Some(UiaRange {
            value: percent.into(),
            minimum: 0.0,
            maximum: 100.0,
            small_change: 1.0,
            large_change: 10.0,
            read_only: true,
        }),
        ..UiaMetadata {
            automation_id: id.to_owned(),
            ..UiaMetadata::new(WidgetKind::Progress)
        }
    }
}

fn sync_progress_node(
    parent: &UiaFragment,
    slot: &mut Option<UiaFragment>,
    id: &str,
    visible: bool,
    percent: u8,
    name: String,
) -> gpui::Result<()> {
    if !visible {
        if let Some(node) = slot.take() {
            node.detach()?;
        }
        return Ok(());
    }
    let metadata = progress_metadata(id, name, percent);
    if let Some(node) = slot.as_ref() {
        node.set_metadata(metadata)?;
    } else {
        *slot = Some(parent.add_child(UiaProviderFactory::create(metadata)?)?);
    }
    Ok(())
}

fn install_focus_handlers(
    root: &UiaFragment,
    color: &UiaFragment,
    picker: &UiaFragment,
    weak: WeakEntity<SettingsView>,
    app: AsyncApp,
    window: AnyWindowHandle,
) -> gpui::Result<()> {
    install_focus(root, weak.clone(), app.clone(), window, FocusTarget::Root)?;
    install_focus(color, weak.clone(), app.clone(), window, FocusTarget::Color)?;
    install_focus(picker, weak, app, window, FocusTarget::Picker)
}

fn install_focus(
    fragment: &UiaFragment,
    weak: WeakEntity<SettingsView>,
    app: AsyncApp,
    window: AnyWindowHandle,
    target: FocusTarget,
) -> gpui::Result<()> {
    let app = Rc::new(RefCell::new(app));
    fragment.set_focus_handler(move || {
        let _ = app.borrow_mut().update_window(window, |_, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                let focus = match target {
                    FocusTarget::Root => view.focus_handle(cx),
                    FocusTarget::Picker => {
                        view.controls().languages.source_language.control_focus(cx)
                    }
                    FocusTarget::Color => {
                        view.controls().appearance.confirmed_input.control_focus(cx)
                    }
                    FocusTarget::Option(index) => {
                        let focus = view.controls().languages.source_language.control_focus(cx);
                        focus.dispatch_action(&Down, window, cx);
                        focus.dispatch_action(&Home, window, cx);
                        for _ in 0..index {
                            focus.dispatch_action(&Down, window, cx);
                        }
                        return;
                    }
                };
                window.focus(&focus);
            });
        });
    })?;
    Ok(())
}
