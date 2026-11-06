//! Complete GPUI settings surface. Host services stay behind explicit actions so
//! drafts, immediate commands, focus, and native accessibility share one state.

pub(crate) mod accessibility;

use std::collections::VecDeque;

use anycaptions_core::{
    domain::{AppSnapshot, WindowMode},
    ipc::CommandError,
    presentation::{
        Locale, MessageKey,
        application::{Action, ApplicationModel, SettingsSave, SnapshotRecovery},
        downloads::DownloadsModel,
        error_message_key, translate,
        updates::UpdatesModel,
    },
};
use gpui::{App, Context, FocusHandle, Focusable, IntoElement, Render, Window, div, prelude::*};

use crate::{
    theme,
    widgets::{Button, Escape, Tab, TabPrev},
};

macro_rules! impl_focus_handles {
    ($controls:ty, [$($control:ident.$child:ident),+ $(,)?]) => {
        impl $controls {
            pub(super) fn focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
                focus_list([$(self.$control.$child.control_focus(cx)),+])
            }
        }
    };
    ($controls:ty, [$($control:ident),+ $(,)?]) => {
        impl $controls {
            pub(super) fn focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
                focus_list([$(self.$control.control_focus(cx)),+])
            }
        }
    };
}

macro_rules! completion_method {
    ($name:ident($($arg:ident : $type:ty),+ $(,)?), $apply:expr $(,)?) => {
        pub fn $name(
            &mut self,
            $($arg: $type),+,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let _ = self.complete_with(window, cx, $apply);
        }
    };
}

macro_rules! fallible_completion_method {
    ($name:ident($($arg:ident : $type:ty),+ $(,)?), $apply:expr $(,)?) => {
        pub fn $name(
            &mut self,
            $($arg: $type),+,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> bool {
            self.complete_with(window, cx, $apply).unwrap_or(false)
        }
    };
}

mod appearance;
mod dictation;
mod flag_picker;
mod languages;
mod models;
mod updates;

pub use appearance::{AppearancePreview, ColorTarget};
pub use dictation::DictationLoad;
pub use models::DownloadRow;

use appearance::{AppearanceControls, ColorInputs};
use dictation::{DictationControls, OutputState};
use languages::LanguageControls;
use models::ModelControls;
use updates::UpdateControls;

pub const SETTINGS_AUTOMATION_ID: &str = "settings";
pub const SOURCE_LANGUAGE_AUTOMATION_ID: &str = "source-language";
pub const SOURCE_LANGUAGE_MENU_AUTOMATION_ID: &str = "source-language-menu";
pub const CAPTION_COLOR_AUTOMATION_ID: &str = "caption-color";
pub const SETTINGS_TOGGLE_AUTOMATION_ID: &str = "settings-toggle";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsHostAction {
    Application(Action),
    SaveSettings,
    LoadDictation,
    RefreshOutputs,
    SelectOutput(String),
    RefreshModels,
    Download(anycaptions_core::presentation::downloads::DownloadCommand),
    CheckForUpdates,
    InstallUpdate,
    OpenReleasesPage,
    OpenProjectPage,
    ChooseColor {
        target: ColorTarget,
        initial: u32,
    },
    /// Close the panel, destroy this view, and focus `settings-toggle`.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsAutomationState {
    pub source_language_expanded: bool,
    pub caption_color_valid: bool,
    pub partial_color_valid: bool,
}

pub(crate) trait ControlFocusExt<V: Focusable> {
    fn control_focus(&self, cx: &App) -> FocusHandle;
    fn control_focused(&self, window: &Window, cx: &App) -> bool;
}

impl<V: Focusable> ControlFocusExt<V> for gpui::Entity<V> {
    fn control_focus(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn control_focused(&self, window: &Window, cx: &App) -> bool {
        self.read(cx).focus_handle(cx).is_focused(window)
    }
}

pub(crate) fn focus_list(handles: impl IntoIterator<Item = FocusHandle>) -> Vec<FocusHandle> {
    handles.into_iter().collect()
}

pub(crate) fn select_pair<'a, T>(first: &'a T, second: &'a T, use_second: bool) -> &'a T {
    if use_second { second } else { first }
}

pub(crate) struct ButtonAnchor {
    button: gpui::Entity<Button>,
    id: &'static str,
}

impl ButtonAnchor {
    pub(crate) fn new(id: &'static str, button: gpui::Entity<Button>) -> Self {
        Self { button, id }
    }
}

impl Focusable for ButtonAnchor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.button.read(cx).focus_handle(cx)
    }
}

impl Render for ButtonAnchor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id(self.id).child(self.button.clone())
    }
}

pub(crate) fn settings_field(
    id: &'static str,
    label: &'static str,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().child(label))
        .child(control)
}

pub(crate) fn picker_values(options: &[crate::widgets::PickerOption]) -> Vec<String> {
    options
        .iter()
        .filter(|option| !option.hidden)
        .map(|option| option.value.to_string())
        .collect()
}

pub(crate) struct Controls {
    appearance: AppearanceControls,
    languages: LanguageControls,
    models: ModelControls,
    dictation: DictationControls,
    updates: UpdateControls,
    save: gpui::Entity<Button>,
}

impl Controls {
    fn focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
        let mut handles = self.appearance.focus_handles(cx);
        handles.extend(self.languages.focus_handles(cx));
        handles.extend(self.models.focus_handles(cx));
        handles.extend(self.dictation.focus_handles(cx));
        handles.extend(self.updates.focus_handles(cx));
        handles.push(self.save.control_focus(cx));
        handles
    }
}

pub struct SettingsView {
    pub(crate) application: ApplicationModel,
    pub(crate) downloads: DownloadsModel,
    pub(crate) updates: UpdatesModel,
    pub(crate) output: OutputState,
    pub(crate) dictation_inputs: anycaptions_core::audio::devices::DeviceSelection,
    pub(crate) dictation_available: bool,
    pub(crate) dictation_loaded: bool,
    pub(crate) color_inputs: ColorInputs,
    pub(crate) pending_color: Option<ColorTarget>,
    pub(crate) open: bool,
    pub(crate) save_requested: bool,
    actions: VecDeque<SettingsHostAction>,
    focus_handle: FocusHandle,
    controls: Option<Controls>,
}

impl SettingsView {
    /// Creates one open panel. The first three host actions load audio,
    /// dictation, and the shared model catalog; late completions are ignored
    /// after Close.
    pub fn new(snapshot: AppSnapshot, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut application = ApplicationModel::new(snapshot.clone());
        application.open_settings();
        let focus_handle = cx.focus_handle().tab_index(0).tab_stop(true);
        let mut view = Self {
            application,
            downloads: DownloadsModel::default(),
            updates: UpdatesModel::default(),
            output: OutputState::loading(),
            dictation_inputs: anycaptions_core::audio::devices::DeviceSelection {
                devices: vec![],
                selected_id: None,
                selected_available: false,
            },
            dictation_available: true,
            dictation_loaded: false,
            color_inputs: ColorInputs::new(&snapshot.appearance),
            pending_color: None,
            open: true,
            save_requested: false,
            actions: VecDeque::from([
                SettingsHostAction::RefreshOutputs,
                SettingsHostAction::LoadDictation,
                SettingsHostAction::RefreshModels,
            ]),
            focus_handle,
            controls: None,
        };
        view.rebuild_controls(window, cx);
        window.focus(&view.focus_handle);
        view
    }

    pub fn application(&self) -> &ApplicationModel {
        &self.application
    }

    pub fn updates(&self) -> &UpdatesModel {
        &self.updates
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn locale(&self) -> Locale {
        Locale::from_tag(self.application.snapshot().ui_language.tag()).unwrap_or_default()
    }

    pub fn text(&self, key: MessageKey) -> &'static str {
        translate(self.locale(), key).unwrap_or("")
    }

    pub(crate) fn replacement(&self, key: MessageKey, token: &str, value: &str) -> String {
        self.text(key).replace(token, value)
    }

    pub(crate) fn primary_disabled(&self) -> bool {
        !self.open || self.application.busy() || self.output.busy || self.save_requested
    }

    pub fn save_enabled(&self) -> bool {
        self.open
            && !self.primary_disabled()
            && self.color_inputs.valid(ColorTarget::Confirmed)
            && self.color_inputs.valid(ColorTarget::Partial)
    }

    pub fn take_action(&mut self) -> Option<SettingsHostAction> {
        self.actions.pop_front()
    }

    pub(crate) fn emit(&mut self, action: SettingsHostAction) {
        self.actions.push_back(action);
    }

    pub fn automation_state(&self, cx: &App) -> SettingsAutomationState {
        SettingsAutomationState {
            source_language_expanded: self.controls().languages.source_language.read(cx).is_open(),
            caption_color_valid: self.color_inputs.valid(ColorTarget::Confirmed),
            partial_color_valid: self.color_inputs.valid(ColorTarget::Partial),
        }
    }

    pub(crate) fn set_source_language_expanded(
        &mut self,
        expanded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.controls()
            .languages
            .source_language
            .update(cx, |picker, cx| {
                picker.set_expanded(expanded, window, cx);
            });
        cx.notify();
    }

    pub fn report(&mut self, error: CommandError, cx: &mut Context<Self>) {
        self.application.report(error);
        cx.notify();
    }

    fn close_if_locked_without_draft(&mut self, cx: &mut Context<Self>) -> bool {
        if self.application.snapshot().window_mode == WindowMode::Locked
            && self.application.draft().is_none()
        {
            self.close(cx);
            true
        } else {
            false
        }
    }

    pub(crate) fn complete_with<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        if !self.open {
            return None;
        }
        let result = apply(self);
        if !self.close_if_locked_without_draft(cx) {
            self.rebuild_controls(window, cx);
        }
        Some(result)
    }

    fallible_completion_method!(
        complete_application(
            result: Result<AppSnapshot, CommandError>,
            recovery: &impl SnapshotRecovery
        ),
        |view| view.application.complete(result, recovery)
    );

    pub fn accept_snapshot(
        &mut self,
        incoming: AppSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.open || !self.application.accept(incoming) {
            return false;
        }
        if self.close_if_locked_without_draft(cx) {
            return true;
        }
        self.rebuild_controls(window, cx);
        true
    }

    pub fn cancel_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.application.cancel_settings();
            self.close(cx);
        }
    }

    pub(crate) fn request_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.save_enabled() {
            return;
        }
        self.save_requested = true;
        self.emit(SettingsHostAction::SaveSettings);
        self.defer_rebuild(window, cx);
    }

    /// Executes the synchronous core transaction after the host drains
    /// `SaveSettings`. Earlier successful writes survive a rejection; retry
    /// skips groups confirmed by returned snapshots.
    pub fn save_settings(
        &mut self,
        backend: &mut impl SettingsSave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.save_enabled() && !self.save_requested {
            return false;
        }
        self.save_requested = false;
        if !self.color_inputs.valid(ColorTarget::Confirmed)
            || !self.color_inputs.valid(ColorTarget::Partial)
            || self.output.busy
        {
            self.rebuild_controls(window, cx);
            return false;
        }
        if self.application.save_settings(backend) {
            self.close(cx);
            true
        } else {
            if !self.close_if_locked_without_draft(cx) {
                self.rebuild_controls(window, cx);
            }
            false
        }
    }

    pub(crate) fn defer_rebuild(&mut self, window: &Window, cx: &mut Context<Self>) {
        cx.defer_in(window, |view, window, cx| view.rebuild_controls(window, cx));
    }

    pub(crate) fn rebuild_disabled(&mut self, cx: &mut Context<Self>) {
        let disabled = !self.save_enabled();
        if let Some(controls) = &self.controls {
            controls
                .save
                .update(cx, |save, cx| save.set_disabled(disabled, cx));
        }
        cx.notify();
    }

    pub(crate) fn rebuild_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous_focus = self.controls.as_ref().and_then(|controls| {
            let mut handles = vec![self.focus_handle.clone()];
            handles.extend(controls.focus_handles(cx));
            handles.iter().position(|handle| handle.is_focused(window))
        });
        let weak = cx.weak_entity();
        let appearance = AppearanceControls::new(self, weak.clone(), window, cx);
        let languages = LanguageControls::new(self, weak.clone(), window, cx);
        let models = ModelControls::new(self, weak.clone(), window, cx);
        let dictation = DictationControls::new(self, weak.clone(), window, cx);
        let updates = UpdateControls::new(self, weak.clone(), window, cx);
        let save_label = self.text(MessageKey::Save);
        let save_disabled = !self.save_enabled();
        let save = cx.new(|cx| {
            Button::new(save_label, cx)
                .disabled(save_disabled)
                .on_press(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| view.request_save(window, cx));
                })
        });
        self.controls = Some(Controls {
            appearance,
            languages,
            models,
            dictation,
            updates,
            save,
        });
        if let Some(index) = previous_focus {
            let mut handles = vec![self.focus_handle.clone()];
            handles.extend(self.controls().focus_handles(cx));
            if let Some(handle) = handles.get(index) {
                window.focus(handle);
            }
        }
        cx.notify();
    }

    pub(crate) fn controls(&self) -> &Controls {
        self.controls.as_ref().expect("controls initialized")
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.save_requested = false;
        self.pending_color = None;
        self.emit(SettingsHostAction::Close);
        cx.notify();
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.application.error();
        let mut root = div()
            .id(SETTINGS_AUTOMATION_ID)
            .key_context("AnyCaptionsWidgets")
            .track_focus(&self.focus_handle)
            .tab_index(0)
            .tab_stop(true)
            .on_action(cx.listener(|_, _: &Tab, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &TabPrev, window, _| window.focus_prev()))
            .on_action(cx.listener(|view, _: &Escape, window, cx| view.cancel_settings(window, cx)))
            .flex()
            .flex_col()
            .gap_3()
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .bg(theme::OVERLAY_SETTINGS_BACKGROUND)
            .text_color(theme::BASE_CONTENT)
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.text(MessageKey::Settings)),
            )
            .child(appearance::render(self, cx))
            .child(languages::render(self, cx))
            .child(models::render_download(self, DownloadRow::EnglishAsr))
            .child(models::render_download(self, DownloadRow::Translation))
            .child(languages::render_speaker(self, cx))
            .child(models::render_download(self, DownloadRow::Diarization))
            .child(dictation::render(self, cx))
            .child(updates::render(self, cx));
        if let Some(error) = error {
            root = root.child(
                div()
                    .id("error")
                    .text_color(theme::ERROR)
                    .child(self.text(error_message_key(&error))),
            );
        }
        root.child(
            div()
                .id("settings-footer")
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(self.controls().updates.credit.clone())
                .child(self.controls().appearance.reset.clone())
                .child(
                    div()
                        .id("save-settings")
                        .child(self.controls().save.clone()),
                ),
        )
    }
}

#[cfg(test)]
mod tests;
