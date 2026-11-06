use anycaptions_core::{
    audio::devices::{AudioDevice, DeviceSelection},
    dictation::{ActivationMode, DictationSettings, MAX_MAX_DURATION_SECS, MIN_MAX_DURATION_SECS},
    domain::{SourceLanguage, TargetLanguage},
    ipc::CommandError,
    presentation::{LanguageCatalog, MessageKey},
};
use gpui::{App, Context, Entity, FocusHandle, IntoElement, WeakEntity, Window, div, prelude::*};

use super::{
    ControlFocusExt, SettingsHostAction, SettingsView,
    flag_picker::{FlagPicker, flag_value, menu_anchor},
    focus_list,
    languages::asr_options,
    picker_values, settings_field,
};
use crate::{
    theme,
    widgets::{Picker, PickerOption, Slider, TextInput, Toggle},
};

pub struct DictationLoad {
    pub settings: DictationSettings,
    pub available: bool,
    pub inputs: DeviceSelection,
}

pub(crate) struct OutputState {
    selection: DeviceSelection,
    pub(crate) busy: bool,
    loaded: bool,
    refresh_after_selection_error: bool,
}

impl OutputState {
    pub(crate) fn loading() -> Self {
        Self {
            selection: DeviceSelection {
                devices: vec![],
                selected_id: None,
                selected_available: false,
            },
            busy: true,
            loaded: false,
            refresh_after_selection_error: false,
        }
    }
}

pub(crate) struct DictationControls {
    output: Entity<Picker>,
    shortcut: Entity<TextInput>,
    enabled: Entity<Toggle>,
    sound: Entity<Toggle>,
    show_text: Entity<Toggle>,
    limit: Entity<Toggle>,
    input: Entity<Picker>,
    dictation_shortcut: Entity<TextInput>,
    language: Entity<FlagPicker>,
    translate: Entity<FlagPicker>,
    activation: Entity<Picker>,
    duration: Entity<Slider>,
    output_values: Vec<String>,
    input_values: Vec<String>,
    language_values: Vec<String>,
    translation_values: Vec<String>,
    _language_subscription: gpui::Subscription,
    _translation_subscription: gpui::Subscription,
}

fn dictation_toggle(
    label: &'static str,
    value: bool,
    disabled: bool,
    weak: WeakEntity<SettingsView>,
    setter: fn(&mut SettingsView, bool, &mut Window, &mut Context<SettingsView>),
    cx: &mut Context<SettingsView>,
) -> Entity<Toggle> {
    cx.new(|cx| {
        Toggle::new(label, value, cx)
            .disabled(disabled)
            .on_change(move |value, window, cx| {
                let _ = weak.update(cx, |view, cx| setter(view, value, window, cx));
            })
    })
}

impl DictationControls {
    pub(super) fn new(
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let draft = view.dictation_draft().cloned().unwrap_or_default();
        let primary_disabled = view.primary_disabled();
        let fields_disabled = !view.dictation_fields_enabled();

        let output_options = build_output_options(view);
        let output_values = picker_values(&output_options);
        let output_selected = view
            .output
            .selection
            .selected_id
            .clone()
            .unwrap_or_default();
        let output_disabled =
            primary_disabled || !view.output.loaded || view.output.selection.devices.is_empty();
        let output_weak = weak.clone();
        let output = cx.new(|cx| {
            Picker::new(output_options, window, cx)
                .with_selected(output_selected)
                .disabled(output_disabled)
                .on_change(move |id, window, cx| {
                    let _ = output_weak.update(cx, |view, cx| {
                        view.select_output(id.to_string(), window, cx)
                    });
                })
        });

        let shortcut_weak = weak.clone();
        let shortcut = cx.new(|cx| {
            TextInput::new(view.text(MessageKey::LockShortcut), cx)
                .with_content(
                    view.application
                        .draft()
                        .map(|draft| draft.shortcut.clone())
                        .unwrap_or_default(),
                )
                .with_max_length(16usize)
                .disabled(primary_disabled)
                .on_change(move |value, window, cx| {
                    let _ = shortcut_weak.update(cx, |view, cx| {
                        view.set_shortcut(value.to_string(), window, cx)
                    });
                })
        });

        let enabled = dictation_toggle(
            view.text(MessageKey::DictationEnable),
            draft.enabled,
            fields_disabled,
            weak.clone(),
            SettingsView::set_dictation_enabled,
            cx,
        );
        let sound = dictation_toggle(
            view.text(MessageKey::DictationSound),
            draft.sound_cue,
            fields_disabled,
            weak.clone(),
            SettingsView::set_dictation_sound,
            cx,
        );
        let show_text = dictation_toggle(
            view.text(MessageKey::DictationShowText),
            draft.show_live_text,
            fields_disabled,
            weak.clone(),
            SettingsView::set_dictation_show_text,
            cx,
        );
        let limit = dictation_toggle(
            view.text(MessageKey::DictationLimitDuration),
            draft.limit_duration,
            fields_disabled,
            weak.clone(),
            SettingsView::set_dictation_limit_duration,
            cx,
        );

        let input_options = build_input_options(view, &draft);
        let input_values = picker_values(&input_options);
        let input_selected = draft.input_id.clone().unwrap_or_default();
        let input_weak = weak.clone();
        let input = cx.new(|cx| {
            Picker::new(input_options, window, cx)
                .with_selected(input_selected)
                .disabled(fields_disabled)
                .on_change(move |value, window, cx| {
                    let _ = input_weak.update(cx, |view, cx| {
                        view.set_dictation_input(value.to_string(), window, cx)
                    });
                })
        });

        let dictation_shortcut_weak = weak.clone();
        let dictation_shortcut = cx.new(|cx| {
            TextInput::new(view.text(MessageKey::DictationShortcut), cx)
                .with_content(draft.shortcut.clone())
                .with_max_length(24usize)
                .disabled(fields_disabled)
                .on_change(move |value, window, cx| {
                    let _ = dictation_shortcut_weak.update(cx, |view, cx| {
                        view.set_dictation_shortcut(value.to_string(), window, cx)
                    });
                })
        });

        let language_options = asr_options(view);
        let language_values = picker_values(&language_options);
        let language_selected = draft.language.clone().unwrap_or_default();
        let language_weak = weak.clone();
        let language = cx.new(|cx| {
            FlagPicker::new(language_options, window, cx)
                .with_selected(language_selected)
                .disabled(fields_disabled)
                .on_change(move |value, window, cx| {
                    let _ = language_weak.update(cx, |view, cx| {
                        view.set_dictation_language(value.to_string(), window, cx)
                    });
                })
        });

        let translation_options = build_translation_options(view);
        let translation_values = picker_values(&translation_options);
        let translation_selected = draft
            .translate_to
            .map(|target| target.tag().to_owned())
            .unwrap_or_default();
        let translate_weak = weak.clone();
        let translate = cx.new(|cx| {
            FlagPicker::new(translation_options, window, cx)
                .with_selected(translation_selected)
                .disabled(fields_disabled)
                .on_change(move |value, window, cx| {
                    let _ = translate_weak.update(cx, |view, cx| {
                        view.set_dictation_translate_to(value.to_string(), window, cx)
                    });
                })
        });

        let activation_options = vec![
            PickerOption::new("hold", view.text(MessageKey::DictationHold)),
            PickerOption::new("toggle", view.text(MessageKey::DictationToggle)),
        ];
        let activation_selected = match draft.activation {
            ActivationMode::Hold => "hold",
            ActivationMode::Toggle => "toggle",
        };
        let activation_weak = weak.clone();
        let activation = cx.new(|cx| {
            Picker::new(activation_options, window, cx)
                .with_selected(activation_selected)
                .disabled(fields_disabled)
                .on_change(move |value, window, cx| {
                    let _ = activation_weak.update(cx, |view, cx| {
                        view.set_dictation_activation(value.to_string(), window, cx)
                    });
                })
        });

        let duration = duration_control(
            weak,
            draft.max_duration_secs,
            fields_disabled || !draft.limit_duration,
            cx,
        );
        let language_subscription = cx.observe(&language, |_, _, cx| cx.notify());
        let translation_subscription = cx.observe(&translate, |_, _, cx| cx.notify());

        Self {
            output,
            shortcut,
            enabled,
            sound,
            show_text,
            limit,
            input,
            dictation_shortcut,
            language,
            translate,
            activation,
            duration,
            output_values,
            input_values,
            language_values,
            translation_values,
            _language_subscription: language_subscription,
            _translation_subscription: translation_subscription,
        }
    }
}

impl_focus_handles!(
    DictationControls,
    [
        output,
        shortcut,
        enabled,
        sound,
        show_text,
        limit,
        input,
        dictation_shortcut,
        language,
        translate,
        activation,
        duration,
    ]
);

macro_rules! dictation_bool_setter {
    ($name:ident, $field:ident) => {
        pub(super) fn $name(&mut self, value: bool, _window: &mut Window, cx: &mut Context<Self>) {
            self.edit_dictation(cx, |draft| draft.$field = value);
        }
    };
}

macro_rules! dictation_picker_accessors {
    ($values:ident, $flag:ident, $values_field:ident, $picker:ident) => {
        pub fn $values(&self) -> Vec<String> {
            self.controls().dictation.$values_field.clone()
        }

        pub fn $flag(&self, value: &str, cx: &App) -> Option<&'static [u8]> {
            flag_value(&self.controls().dictation.$picker, value, cx)
        }
    };
}

fn optional_dictation_value<T>(
    value: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Option<Option<T>> {
    if value.is_empty() {
        Some(None)
    } else {
        parse(value).map(Some)
    }
}

macro_rules! dictation_value_setter {
    ($name:ident, $field:ident, $parse:expr) => {
        pub(super) fn $name(
            &mut self,
            value: String,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let Some(value) = ($parse)(self, value.as_str()) else {
                return;
            };
            self.edit_dictation(cx, |draft| draft.$field = value);
        }
    };
}

impl SettingsView {
    fallible_completion_method!(
        complete_dictation(result: Result<DictationLoad, CommandError>),
        |view| match result {
            Ok(load) => {
                view.application.load_dictation(load.settings);
                view.dictation_available = load.available;
                view.dictation_inputs = load.inputs;
                view.dictation_loaded = true;
                true
            }
            Err(error) => {
                view.application.report(error);
                false
            }
        }
    );

    fallible_completion_method!(
        complete_outputs(result: Result<DeviceSelection, CommandError>),
        |view| {
            view.output.busy = false;
            let selection_failed = view.output.refresh_after_selection_error;
            view.output.refresh_after_selection_error = false;
            match result {
                Ok(selection) => {
                    view.output.selection = selection;
                    view.output.loaded = true;
                    true
                }
                Err(error) => {
                    if !selection_failed {
                        view.application.report(error);
                    }
                    false
                }
            }
        }
    );

    fallible_completion_method!(
        complete_output_selection(result: Result<DeviceSelection, CommandError>),
        |view| match result {
            Ok(selection) => {
                view.output.selection = selection;
                view.output.busy = false;
                view.output.loaded = true;
                view.output.refresh_after_selection_error = false;
                true
            }
            Err(error) => {
                view.application.report(error);
                view.output.busy = true;
                view.output.refresh_after_selection_error = true;
                view.emit(SettingsHostAction::RefreshOutputs);
                false
            }
        }
    );

    pub fn output_busy(&self) -> bool {
        self.output.busy
    }

    pub fn output_selection(&self) -> &DeviceSelection {
        &self.output.selection
    }

    pub fn output_status(&self) -> Option<MessageKey> {
        if !self.output.loaded {
            return None;
        }
        if self.output.selection.selected_id.is_some() && !self.output.selection.selected_available
        {
            Some(MessageKey::SavedOutputDisconnected)
        } else if self.output.selection.devices.is_empty()
            || self.output.selection.selected_id.is_none()
        {
            Some(MessageKey::NoActiveOutput)
        } else {
            None
        }
    }

    pub fn output_option_values(&self) -> Vec<String> {
        self.controls().dictation.output_values.clone()
    }

    pub fn dictation_draft(&self) -> Option<&DictationSettings> {
        self.application.draft()?.dictation.as_ref()
    }

    pub fn dictation_status(&self) -> Option<MessageKey> {
        (self.dictation_loaded && !self.dictation_available)
            .then_some(MessageKey::DictationModelMissing)
    }

    pub fn dictation_fields_enabled(&self) -> bool {
        self.dictation_loaded && self.dictation_draft().is_some() && !self.primary_disabled()
    }

    pub fn dictation_duration_enabled(&self) -> bool {
        self.dictation_fields_enabled()
            && self
                .dictation_draft()
                .is_some_and(|draft| draft.limit_duration)
    }

    pub fn dictation_input_values(&self) -> Vec<String> {
        self.controls().dictation.input_values.clone()
    }

    dictation_picker_accessors!(
        dictation_language_values,
        dictation_language_flag,
        language_values,
        language
    );

    dictation_picker_accessors!(
        dictation_translation_values,
        dictation_translation_flag,
        translation_values,
        translate
    );

    pub(super) fn select_output(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id.is_empty() || self.primary_disabled() || !self.output.loaded {
            return;
        }
        self.output.busy = true;
        self.emit(SettingsHostAction::SelectOutput(id));
        cx.defer_in(window, |view, window, cx| {
            view.rebuild_controls(window, cx);
            let handle = view.controls().dictation.output.control_focus(cx);
            window.focus(&handle);
        });
    }

    pub(super) fn set_shortcut(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() {
            return;
        }
        if let Some(draft) = self.application.draft_mut() {
            draft.shortcut = truncate_utf16(&value, 16);
            cx.notify();
        }
    }

    fn edit_dictation(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut DictationSettings),
    ) {
        if !self.dictation_fields_enabled() {
            return;
        }
        if let Some(draft) = self
            .application
            .draft_mut()
            .and_then(|draft| draft.dictation.as_mut())
        {
            edit(draft);
            cx.notify();
        }
    }

    dictation_bool_setter!(set_dictation_enabled, enabled);

    pub(super) fn set_dictation_shortcut(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = truncate_utf16(&value, 24);
        self.edit_dictation(cx, |draft| draft.shortcut = value);
    }

    pub(super) fn set_dictation_activation(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let activation = match value.as_str() {
            "hold" => ActivationMode::Hold,
            "toggle" => ActivationMode::Toggle,
            _ => return,
        };
        self.edit_dictation(cx, |draft| draft.activation = activation);
    }

    dictation_bool_setter!(set_dictation_sound, sound_cue);

    dictation_bool_setter!(set_dictation_show_text, show_live_text);

    pub(super) fn set_dictation_limit_duration(
        &mut self,
        value: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dictation_fields_enabled() {
            return;
        }
        self.edit_dictation(cx, |draft| draft.limit_duration = value);
        let duration = duration_control(
            cx.weak_entity(),
            self.dictation_draft()
                .map_or(MIN_MAX_DURATION_SECS, |draft| draft.max_duration_secs),
            !self.dictation_duration_enabled(),
            cx,
        );
        if let Some(controls) = self.controls.as_mut() {
            controls.dictation.duration = duration;
        }
        cx.notify();
    }

    pub(super) fn set_dictation_duration(
        &mut self,
        value: f32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dictation_duration_enabled() {
            return;
        }
        let value = normalize_duration(value);
        self.edit_dictation(cx, |draft| draft.max_duration_secs = value);
    }

    pub(super) fn set_dictation_input(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_dictation(cx, |draft| {
            draft.input_id = (!value.is_empty()).then_some(value)
        });
    }

    dictation_value_setter!(
        set_dictation_language,
        language,
        |_: &SettingsView, value: &str| {
            optional_dictation_value(value, |value| {
                SourceLanguage::from_tag(value).map(|language| language.tag().to_owned())
            })
        }
    );

    dictation_value_setter!(
        set_dictation_translate_to,
        translate_to,
        |view: &SettingsView, value: &str| {
            optional_dictation_value(value, |value| {
                TargetLanguage::from_tag(value).filter(|target| {
                    view.application
                        .snapshot()
                        .translation_engine
                        .supports(*target)
                })
            })
        }
    );
}

fn duration_control(
    weak: WeakEntity<SettingsView>,
    value: u16,
    disabled: bool,
    cx: &mut Context<SettingsView>,
) -> Entity<Slider> {
    cx.new(|cx| {
        Slider::new(
            f32::from(MIN_MAX_DURATION_SECS),
            f32::from(MAX_MAX_DURATION_SECS),
            5.0,
            f32::from(value),
            cx,
        )
        .disabled(disabled)
        .on_change(move |value, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.set_dictation_duration(value, window, cx)
            });
        })
    })
}

fn extend_device_options(
    options: &mut Vec<PickerOption>,
    devices: &[AudioDevice],
    selected: Option<&String>,
    unavailable_label: &'static str,
) {
    options.extend(devices.iter().map(|device| {
        PickerOption::new(device.id.clone(), device.name.clone()).name(device.name.clone())
    }));
    if let Some(selected) = selected
        && !devices.iter().any(|device| &device.id == selected)
    {
        options.push(PickerOption::new(selected.clone(), unavailable_label));
    }
}

fn build_output_options(view: &SettingsView) -> Vec<PickerOption> {
    let output = &view.output;
    let mut options = Vec::new();
    if !output.loaded {
        options.push(PickerOption::new(
            "",
            view.text(MessageKey::CheckingOutputs),
        ));
        return options;
    }
    if output.selection.devices.is_empty() {
        options.push(PickerOption::new("", view.text(MessageKey::NoOutputs)));
    } else if output.selection.selected_id.is_none() {
        options.push(PickerOption::new("", view.text(MessageKey::SelectOutput)));
    }
    let stale = output
        .selection
        .selected_id
        .as_ref()
        .filter(|_| !output.selection.selected_available);
    extend_device_options(
        &mut options,
        &output.selection.devices,
        stale,
        view.text(MessageKey::SavedOutputUnavailable),
    );
    options
}

fn build_input_options(view: &SettingsView, draft: &DictationSettings) -> Vec<PickerOption> {
    let mut options = vec![PickerOption::new(
        "",
        view.text(MessageKey::DictationDefaultMic),
    )];
    extend_device_options(
        &mut options,
        &view.dictation_inputs.devices,
        draft.input_id.as_ref(),
        view.text(MessageKey::SavedOutputUnavailable),
    );
    options
}

fn build_translation_options(view: &SettingsView) -> Vec<PickerOption> {
    let engine = view.application.snapshot().translation_engine;
    let mut options = vec![PickerOption::new(
        "",
        view.text(MessageKey::DictationNoTranslate),
    )];
    options.extend(LanguageCatalog::translation_targets().iter().map(|target| {
        let tag = target
            .mode
            .strip_prefix("translate-")
            .expect("translation target uses its wire prefix");
        let language = TargetLanguage::from_tag(tag).expect("presentation target is valid");
        PickerOption::new(tag, target.name)
            .name(target.name)
            .country(target.country)
            .hidden(!engine.supports(language))
    }));
    options
}

fn truncate_utf16(text: &str, max_length: usize) -> String {
    let mut used = 0;
    let mut truncated = String::new();
    for character in text.chars() {
        if used + character.len_utf16() > max_length {
            break;
        }
        used += character.len_utf16();
        truncated.push(character);
    }
    truncated
}

fn normalize_duration(value: f32) -> u16 {
    if value.is_nan() {
        return MIN_MAX_DURATION_SECS;
    }
    let min = f32::from(MIN_MAX_DURATION_SECS);
    let max = f32::from(MAX_MAX_DURATION_SECS);
    (min + ((value - min) / 5.0).round() * 5.0).clamp(min, max) as u16
}

fn toggle_field(id: &'static str, control: Entity<Toggle>) -> impl IntoElement {
    div().id(id).child(control)
}

pub(super) fn render(view: &SettingsView, cx: &App) -> impl IntoElement {
    let controls = &view.controls().dictation;
    let mut root = div()
        .id("dictation-output-settings")
        .flex()
        .flex_col()
        .gap_2()
        .child(settings_field(
            "audio-output",
            view.text(MessageKey::AudioOutput),
            controls.output.clone(),
        ));
    if let Some(status) = view.output_status() {
        root = root.child(
            div()
                .id("audio-output-status")
                .text_sm()
                .text_color(theme::MUTED)
                .child(view.text(status)),
        );
    }
    root = root.child(settings_field(
        "shortcut",
        view.text(MessageKey::LockShortcut),
        controls.shortcut.clone(),
    ));
    if !view.application.snapshot().shortcut_available {
        root = root.child(
            div()
                .id("shortcut-status")
                .text_sm()
                .text_color(theme::MUTED)
                .child(view.text(MessageKey::ShortcutUnavailableTray)),
        );
    }
    if view.dictation_draft().is_some() {
        root = root
            .child(
                div()
                    .id("dictation-heading")
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(view.text(MessageKey::Dictation)),
            )
            .child(toggle_field("dictation-enabled", controls.enabled.clone()))
            .child(toggle_field("dictation-sound", controls.sound.clone()))
            .child(toggle_field(
                "dictation-show-text",
                controls.show_text.clone(),
            ))
            .child(toggle_field("dictation-limit", controls.limit.clone()))
            .child(settings_field(
                "dictation-mic",
                view.text(MessageKey::DictationMic),
                controls.input.clone(),
            ))
            .child(settings_field(
                "dictation-shortcut",
                view.text(MessageKey::DictationShortcut),
                controls.dictation_shortcut.clone(),
            ))
            .child(settings_field(
                "dictation-language",
                view.text(MessageKey::DictationLanguage),
                menu_anchor(
                    "dictation-language-menu",
                    controls.language.clone(),
                    controls.language.read(cx).is_open(),
                ),
            ))
            .child(settings_field(
                "dictation-translate",
                view.text(MessageKey::DictationTranslate),
                menu_anchor(
                    "dictation-translate-menu",
                    controls.translate.clone(),
                    controls.translate.read(cx).is_open(),
                ),
            ))
            .child(settings_field(
                "dictation-activation",
                view.text(MessageKey::DictationActivation),
                controls.activation.clone(),
            ))
            .child(settings_field(
                "dictation-duration",
                view.text(MessageKey::DictationMaxDuration),
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(controls.duration.clone())
                    .child(format!(
                        "{} s",
                        view.dictation_draft()
                            .map_or(MIN_MAX_DURATION_SECS, |draft| draft.max_duration_secs)
                    )),
            ));
        if let Some(status) = view.dictation_status() {
            root = root.child(
                div()
                    .id("dictation-status")
                    .text_sm()
                    .text_color(theme::MUTED)
                    .child(view.text(status)),
            );
        }
    }
    root
}
