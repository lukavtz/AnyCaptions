use anycaptions_core::{
    domain::{SourceLanguage, TranslationEngine, UiLanguage},
    presentation::{LanguageCatalog, MessageKey, UI_LANGUAGES, application::Action},
};
use gpui::{App, Context, Entity, FocusHandle, IntoElement, WeakEntity, Window, div, prelude::*};

use crate::{
    theme,
    widgets::{Picker, PickerOption, Toggle},
};

use super::{
    ControlFocusExt, SOURCE_LANGUAGE_AUTOMATION_ID, SOURCE_LANGUAGE_MENU_AUTOMATION_ID,
    SettingsHostAction, SettingsView,
    flag_picker::{FlagPicker, flag_value, menu_anchor},
    focus_list, picker_values, settings_field,
};

pub(super) fn asr_options(view: &SettingsView) -> Vec<PickerOption> {
    let mut options = vec![PickerOption::new("", view.text(MessageKey::AutoMode))];
    options.extend(LanguageCatalog::asr_languages().iter().map(|language| {
        PickerOption::new(language.tag, language.name)
            .name(language.name)
            .country(language.country)
    }));
    options
}

pub(super) struct LanguageControls {
    pub(super) ui_language: Entity<FlagPicker>,
    pub(super) source_language: Entity<FlagPicker>,
    pub(super) translation_engine: Entity<Picker>,
    pub(super) speaker_labels: Entity<Toggle>,
    source_values: Vec<String>,
    _source_subscription: gpui::Subscription,
}

impl LanguageControls {
    pub(super) fn new(
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let snapshot = view.application.snapshot();
        let disabled = view.primary_disabled();
        let source_disabled = !view.open || view.application().busy() || view.save_requested;

        let ui_options = UI_LANGUAGES
            .iter()
            .map(|language| {
                PickerOption::new(language.tag, language.name)
                    .name(language.name)
                    .country(language.country)
            })
            .collect();
        let ui_selected = snapshot.ui_language.tag();
        let ui_weak = weak.clone();
        let ui_language = cx.new(|cx| {
            FlagPicker::new(ui_options, window, cx)
                .with_selected(ui_selected)
                .disabled(disabled)
                .on_change(move |value, window, cx| {
                    let _ = ui_weak.update(cx, |view, cx| {
                        view.select_ui_language(value.to_string(), window, cx)
                    });
                })
        });

        let source_options = asr_options(view);
        let source_values = picker_values(&source_options);
        let source_selected = snapshot.source_language.map_or("", SourceLanguage::tag);
        let source_weak = weak.clone();
        let source_language = cx.new(|cx| {
            FlagPicker::new(source_options, window, cx)
                .with_selected(source_selected)
                .disabled(source_disabled)
                .on_change(move |value, window, cx| {
                    let _ = source_weak.update(cx, |view, cx| {
                        view.select_source_language(value.to_string(), window, cx)
                    });
                })
        });

        let source_subscription = cx.observe(&source_language, |_, _, cx| cx.notify());

        let engine_options = LanguageCatalog::translation_engines()
            .iter()
            .map(|engine| PickerOption::new(engine_tag(engine.id), view.text(engine.label_key)))
            .collect();
        let engine_selected = engine_tag(snapshot.translation_engine);
        let engine_weak = weak.clone();
        let translation_engine = cx.new(|cx| {
            Picker::new(engine_options, window, cx)
                .with_selected(engine_selected)
                .disabled(disabled)
                .on_change(move |value, window, cx| {
                    let _ = engine_weak.update(cx, |view, cx| {
                        view.select_translation_engine(value.to_string(), window, cx)
                    });
                })
        });

        let speaker_weak = weak;
        let speaker_labels = cx.new(|cx| {
            Toggle::new(
                view.text(MessageKey::SpeakerLabelsEnable),
                snapshot.speaker_labels,
                cx,
            )
            .disabled(disabled)
            .on_change(move |enabled, window, cx| {
                let _ = speaker_weak
                    .update(cx, |view, cx| view.set_speaker_labels(enabled, window, cx));
            })
        });

        Self {
            ui_language,
            source_language,
            translation_engine,
            speaker_labels,
            source_values,
            _source_subscription: source_subscription,
        }
    }
}

impl_focus_handles!(
    LanguageControls,
    [
        ui_language,
        source_language,
        translation_engine,
        speaker_labels,
    ]
);

fn engine_tag(engine: TranslationEngine) -> &'static str {
    match engine {
        TranslationEngine::Opus => "opus",
        TranslationEngine::Riva => "riva",
    }
}

impl SettingsView {
    fn apply_language_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            return;
        }
        if self.primary_disabled() {
            // Widgets update before their callback. Restore confirmed values if
            // another immediate operation rejected this change.
            self.defer_rebuild(window, cx);
            return;
        }
        if self.application.try_begin(action.clone()) {
            self.emit(SettingsHostAction::Application(action.clone()));
            cx.defer_in(window, move |view, window, cx| {
                view.rebuild_controls(window, cx);
                let handle = match action {
                    Action::SetUiLanguage(_) => {
                        view.controls().languages.ui_language.control_focus(cx)
                    }
                    Action::SetSourceLanguage(_) => {
                        view.controls().languages.source_language.control_focus(cx)
                    }
                    Action::SetTranslationEngine(_) => view
                        .controls()
                        .languages
                        .translation_engine
                        .control_focus(cx),
                    Action::SetSpeakerLabels(_) => {
                        view.controls().languages.speaker_labels.control_focus(cx)
                    }
                    _ => return,
                };
                window.focus(&handle);
            });
        }
    }

    pub(super) fn select_ui_language(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(language) = UiLanguage::from_tag(&value) else {
            return;
        };
        self.apply_language_action(Action::SetUiLanguage(language), window, cx);
    }

    pub(super) fn select_source_language(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = if value.is_empty() {
            None
        } else {
            let Some(language) = SourceLanguage::from_tag(&value) else {
                return;
            };
            Some(language)
        };
        self.apply_language_action(Action::SetSourceLanguage(language), window, cx);
    }

    pub(super) fn select_translation_engine(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(engine) = LanguageCatalog::translation_engines()
            .iter()
            .find(|engine| engine_tag(engine.id) == value)
            .map(|engine| engine.id)
        else {
            return;
        };
        if self.application.snapshot().translation_engine == engine {
            return;
        }
        self.apply_language_action(Action::SetTranslationEngine(engine), window, cx);
    }

    pub(super) fn set_speaker_labels(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_language_action(Action::SetSpeakerLabels(enabled), window, cx);
    }

    pub fn source_language_values(&self) -> Vec<String> {
        self.controls().languages.source_values.clone()
    }

    pub fn source_language_flag(&self, value: &str, cx: &App) -> Option<&'static [u8]> {
        flag_value(&self.controls().languages.source_language, value, cx)
    }

    pub fn speaker_notice(&self) -> Option<MessageKey> {
        let snapshot = self.application.snapshot();
        (snapshot.speaker_labels && !snapshot.resources.speaker_labels)
            .then_some(MessageKey::SpeakerLabelsUnavailable)
    }
}

pub(super) fn render(view: &mut SettingsView, cx: &mut Context<SettingsView>) -> impl IntoElement {
    let languages = &view.controls().languages;
    let ui_language = languages.ui_language.clone();
    let source_language = languages.source_language.clone();
    let translation_engine = languages.translation_engine.clone();
    let source_expanded = source_language.read(cx).is_open();

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(settings_field(
            "ui-language",
            view.text(MessageKey::UiLanguage),
            ui_language,
        ))
        .child(settings_field(
            SOURCE_LANGUAGE_AUTOMATION_ID,
            view.text(MessageKey::SourceLanguage),
            menu_anchor(
                SOURCE_LANGUAGE_MENU_AUTOMATION_ID,
                source_language,
                source_expanded,
            ),
        ))
        .child(settings_field(
            "translation-engine",
            view.text(MessageKey::TranslationEngine),
            translation_engine,
        ))
}

pub(super) fn render_speaker(
    view: &mut SettingsView,
    _cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let speaker_labels = view.controls().languages.speaker_labels.clone();
    let notice = view.speaker_notice();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(view.text(MessageKey::SpeakerLabels)),
        )
        .child(div().id("speaker-labels").child(speaker_labels))
        .child(
            div()
                .id("speaker-labels-hint")
                .text_sm()
                .text_color(theme::MUTED)
                .child(view.text(MessageKey::SpeakerLabelsHint)),
        )
        .when_some(notice, |root, notice| {
            root.child(
                div()
                    .id("notice")
                    .text_sm()
                    .text_color(theme::WARNING)
                    .child(view.text(notice)),
            )
        })
}
