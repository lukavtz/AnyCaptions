use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use anycaptions_core::{
    audio::devices::{AudioDevice, AudioDeviceError, DeviceSelection},
    dictation::{ActivationMode, DictationError, DictationSettings},
    domain::{
        AppSnapshot, AppState, Appearance, CaptionColor, CaptionFont, DomainError, Resources,
        SourceLanguage, TranslationEngine, UiLanguage,
    },
    ipc::CommandError,
    models::{
        ModelKind,
        download::{DownloadFailure, DownloadPhase, DownloadUpdate},
    },
    presentation::{
        Locale, MessageKey,
        application::{Action, SettingsSave, SnapshotRecovery},
        downloads::{DownloadCommand, ModelDownloadState},
        updates::{UpdateActionState, UpdateStatus},
    },
    update::{UpdateCheck, UpdateError},
    updater::{UpdatePhase, UpdateProgress},
};
use gpui::{
    AppContext, Context, Entity, Focusable, KeyUpEvent, Keystroke, TestAppContext,
    VisualTestContext, Window,
};

use super::*;
use crate::widgets::bind_keys;

const RIVA: &str = "riva-translate-4b-instruct-v2-q4_k_m";
const DIARIZATION: &str = "nemotron-3-diarization-q8_0";

fn snapshot() -> AppSnapshot {
    let mut snapshot = AppState::default().snapshot();
    snapshot.ui_language = UiLanguage::from_tag("pt-br").unwrap();
    snapshot.version = "0.1.1";
    snapshot
}

fn devices(ids: &[&str], selected: Option<&str>, available: bool) -> DeviceSelection {
    DeviceSelection {
        devices: ids
            .iter()
            .map(|id| AudioDevice {
                id: (*id).into(),
                name: "Alto-falantes".into(),
            })
            .collect(),
        selected_id: selected.map(Into::into),
        selected_available: available,
    }
}

fn dictation_load() -> DictationLoad {
    DictationLoad {
        settings: DictationSettings::default(),
        available: true,
        inputs: devices(&["mic-a", "mic-b"], None, false),
    }
}

fn downloadable_model(
    model: &'static str,
    kind: ModelKind,
    engine: Option<TranslationEngine>,
    total_bytes: u64,
) -> ModelDownloadState {
    ModelDownloadState {
        model,
        kind,
        engine,
        installed: false,
        downloadable: true,
        downloading: false,
        total_bytes,
    }
}

fn model_catalog() -> Vec<ModelDownloadState> {
    vec![
        downloadable_model(
            RIVA,
            ModelKind::Translation,
            Some(TranslationEngine::Riva),
            2_759_302_368,
        ),
        downloadable_model(DIARIZATION, ModelKind::Diarization, None, 107_012_128),
    ]
}

fn download_update(
    model: &'static str,
    phase: DownloadPhase,
    received_bytes: u64,
    total_bytes: Option<u64>,
    reason: Option<DownloadFailure>,
) -> DownloadUpdate {
    DownloadUpdate {
        model,
        phase,
        received_bytes,
        total_bytes,
        reason,
    }
}

fn update_check(latest: &str, available: bool, installable: bool) -> UpdateCheck {
    UpdateCheck {
        current: "0.1.1".into(),
        latest: latest.into(),
        available,
        installable,
    }
}

fn update_progress(phase: UpdatePhase, received_bytes: u64, total_bytes: u64) -> UpdateProgress {
    UpdateProgress {
        phase,
        received_bytes,
        total_bytes,
        reason: None,
    }
}

fn read<V: 'static, R>(view: &Entity<V>, cx: &VisualTestContext, f: impl FnOnce(&V) -> R) -> R {
    cx.read_entity(view, |view, _| f(view))
}

fn update(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut SettingsView, &mut Window, &mut Context<SettingsView>),
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| f(view, window, cx));
    });
}

fn take_actions(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
) -> Vec<SettingsHostAction> {
    view.update(cx, |view, _| {
        std::iter::from_fn(|| view.take_action()).collect()
    })
}

fn unloaded_panel(cx: &mut TestAppContext) -> (Entity<SettingsView>, &mut VisualTestContext) {
    unloaded_panel_with(snapshot(), cx)
}

fn unloaded_panel_with(
    snapshot: AppSnapshot,
    cx: &mut TestAppContext,
) -> (Entity<SettingsView>, &mut VisualTestContext) {
    cx.update(bind_keys);
    cx.add_window_view(move |window, cx| SettingsView::new(snapshot, window, cx))
}

fn panel(cx: &mut TestAppContext) -> (Entity<SettingsView>, &mut VisualTestContext) {
    panel_with(snapshot(), cx)
}

fn panel_with(
    snapshot: AppSnapshot,
    cx: &mut TestAppContext,
) -> (Entity<SettingsView>, &mut VisualTestContext) {
    let (view, cx) = unloaded_panel_with(snapshot, cx);
    let _ = take_actions(&view, cx);
    update(&view, cx, |view, window, cx| {
        view.complete_dictation(Ok(dictation_load()), window, cx);
        view.complete_outputs(Ok(devices(&[], None, false)), window, cx);
        view.accept_models(vec![], window, cx);
    });
    (view, cx)
}

fn focus<V: Focusable>(view: &Entity<V>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let handle = view.control_focus(cx);
        window.focus(&handle);
    });
}

fn assert_focused<V: Focusable>(view: &Entity<V>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| assert!(view.control_focused(window, cx)));
}

fn press_key(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
}

struct Recovery(AppSnapshot);

impl SnapshotRecovery for Recovery {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        Ok(self.0.clone())
    }
}

struct FakeSave {
    snapshot: AppSnapshot,
    dictation: DictationSettings,
    calls: VecDeque<&'static str>,
    dictation_calls: Vec<DictationSettings>,
    fail: Option<&'static str>,
}

impl FakeSave {
    fn new(snapshot: AppSnapshot) -> Self {
        Self {
            snapshot,
            dictation: DictationSettings::default(),
            calls: VecDeque::new(),
            dictation_calls: vec![],
            fail: None,
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.iter().copied().collect()
    }
}

impl SnapshotRecovery for FakeSave {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        Ok(self.snapshot.clone())
    }
}

impl SettingsSave for FakeSave {
    fn set_appearance(&mut self, appearance: Appearance) -> Result<AppSnapshot, CommandError> {
        self.calls.push_back("appearance");
        if self.fail == Some("appearance") {
            return Err(CommandError::Domain(DomainError::InvalidFontSize));
        }
        self.snapshot.revision += 1;
        self.snapshot.appearance = appearance;
        Ok(self.snapshot.clone())
    }

    fn set_shortcut(&mut self, shortcut: &str) -> Result<AppSnapshot, CommandError> {
        self.calls.push_back("shortcut");
        if self.fail == Some("shortcut") {
            return Err(CommandError::ShortcutUnavailable);
        }
        self.snapshot.revision += 1;
        self.snapshot.shortcut = shortcut.into();
        self.snapshot.shortcut_available = true;
        Ok(self.snapshot.clone())
    }

    fn set_dictation(
        &mut self,
        settings: DictationSettings,
    ) -> Result<DictationSettings, CommandError> {
        self.calls.push_back("dictation");
        if self.fail == Some("dictation") {
            return Err(CommandError::Dictation(DictationError::InvalidShortcut));
        }
        self.dictation = settings.clone();
        self.dictation_calls.push(settings.clone());
        Ok(settings)
    }
}

macro_rules! update_font_size {
    ($view:expr, $cx:expr, $size:expr, $edit:expr $(,)?) => {
        update($view, $cx, |view, window, cx| {
            view.set_font_size($size, &mut *window, &mut *cx);
            ($edit)(view, window, cx);
        })
    };
}

fn fake_save(view: &Entity<SettingsView>, cx: &VisualTestContext) -> FakeSave {
    FakeSave::new(read(view, cx, |view| view.application().snapshot().clone()))
}

fn save_with(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    backend: &mut FakeSave,
) -> bool {
    let mut saved = false;
    update(view, cx, |view, window, cx| {
        saved = view.save_settings(backend, window, cx);
    });
    saved
}

fn assert_closed(view: &Entity<SettingsView>, cx: &mut VisualTestContext) {
    assert!(!read(view, cx, SettingsView::is_open));
    assert_eq!(take_actions(view, cx), vec![SettingsHostAction::Close]);
}

#[gpui::test]
fn appearance_preview_is_draft_only_and_save_commits_complete_appearance(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    assert_eq!(
        read(&view, cx, SettingsView::caption_font_values),
        vec![
            "inter",
            "fira-code",
            "segoe-ui",
            "arial",
            "verdana",
            "tahoma",
            "trebuchet-ms",
            "georgia",
            "consolas"
        ]
    );
    update(&view, cx, |view, window, cx| {
        view.set_font_size(72.0, window, cx);
        view.set_color_text(ColorTarget::Confirmed, "#33ccff", window, cx);
        view.set_color_text(ColorTarget::Partial, "#ff77aa", window, cx);
        view.set_background(true, window, cx);
        view.set_caption_font("consolas".into(), window, cx);
    });

    let preview = read(&view, cx, SettingsView::preview);
    assert_eq!(preview.font_size, 72);
    assert_eq!(preview.color, CaptionColor::from_rgb(0x33CCFF));
    assert_eq!(preview.partial_color, CaptionColor::from_rgb(0xFF77AA));
    assert!(preview.background);
    assert_eq!(preview.font, CaptionFont::from_id("consolas").unwrap());
    assert_eq!(read(&view, cx, |view| view.font_value()), "72 px");
    assert_eq!(
        read(&view, cx, |view| view
            .color_text(ColorTarget::Confirmed)
            .to_owned()),
        "#33CCFF"
    );
    assert_eq!(
        read(&view, cx, |view| view
            .color_text(ColorTarget::Partial)
            .to_owned()),
        "#FF77AA"
    );
    assert_eq!(
        read(&view, cx, |view| view.application().snapshot().appearance),
        Appearance::default()
    );
    assert!(take_actions(&view, cx).is_empty());

    update(&view, cx, |view, window, cx| view.request_save(window, cx));
    assert!(!read(&view, cx, SettingsView::save_enabled));
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::SaveSettings]
    );
    let mut backend = fake_save(&view, cx);
    assert!(save_with(&view, cx, &mut backend));
    assert_eq!(backend.calls(), vec!["appearance"]);
    assert_eq!(backend.snapshot.appearance.font_size, 72);
    assert_eq!(
        backend.snapshot.appearance.color,
        CaptionColor::from_rgb(0x33CCFF)
    );
    assert_eq!(
        backend.snapshot.appearance.partial_color,
        CaptionColor::from_rgb(0xFF77AA)
    );
    assert!(backend.snapshot.appearance.background);
    assert_eq!(backend.snapshot.appearance.font.id(), "consolas");
    assert_closed(&view, cx);
}

#[gpui::test]
fn color_dialog_cancel_restores_focus_and_reset_only_restores_appearance(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.focus_color_dialog(ColorTarget::Confirmed, window, cx);
        view.request_color(ColorTarget::Confirmed, window, cx);
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::ChooseColor {
            target: ColorTarget::Confirmed,
            initial: CaptionColor::WHITE.rgb(),
        }]
    );
    update(&view, cx, |view, window, cx| {
        view.complete_color_choice(Ok(None), window, cx);
    });
    let dialog = read(&view, cx, |view| {
        view.controls().appearance.confirmed_dialog.clone()
    });
    assert_focused(&dialog, cx);
    assert_eq!(
        read(&view, cx, |view| view
            .color_text(ColorTarget::Confirmed)
            .to_owned()),
        "#FFFFFF"
    );

    update(&view, cx, |view, window, cx| {
        view.request_color(ColorTarget::Confirmed, window, cx);
        view.complete_color_choice(Ok(Some(0x00FF99)), window, cx);
        view.set_shortcut("Ctrl+Alt+K".into(), window, cx);
        view.set_dictation_enabled(true, window, cx);
        view.set_color_text(ColorTarget::Confirmed, "blue", window, cx);
    });
    assert_eq!(
        read(&view, cx, |view| view
            .color_text(ColorTarget::Confirmed)
            .to_owned()),
        "blue"
    );
    assert!(!read(&view, cx, |view| view.color_valid(ColorTarget::Confirmed)));
    assert!(!read(&view, cx, SettingsView::save_enabled));
    assert!(!cx.read_entity(&view, |view, cx| {
        view.automation_state(cx).caption_color_valid
    }));
    let mut backend = fake_save(&view, cx);
    assert!(!save_with(&view, cx, &mut backend));
    assert!(backend.calls().is_empty());

    update(&view, cx, |view, window, cx| {
        view.reset_appearance(window, cx)
    });
    let preview = read(&view, cx, SettingsView::preview);
    assert_eq!(preview, AppearancePreview::from(&Appearance::default()));
    assert_eq!(
        read(&view, cx, |view| view
            .application()
            .draft()
            .unwrap()
            .shortcut
            .clone()),
        "Ctrl+Alt+K"
    );
    assert!(read(&view, cx, |view| view
        .dictation_draft()
        .unwrap()
        .enabled));
    assert!(read(&view, cx, SettingsView::save_enabled));
}

#[gpui::test]
fn hex_inputs_normalize_on_blur_and_partial_dialog_results(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    cx.update(|window, _| window.activate_window());
    let confirmed = read(&view, cx, |view| {
        view.controls().appearance.confirmed_input.clone()
    });
    focus(&confirmed, cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("#33ccff");
    assert_eq!(
        read(&confirmed, cx, |input| input.content().to_owned()),
        "#33ccff"
    );
    focus(&view, cx);
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| assert!(!confirmed.control_focused(window, cx)));
    assert_eq!(
        read(&confirmed, cx, |input| input.content().to_owned()),
        "#33CCFF"
    );

    let partial = read(&view, cx, |view| {
        view.controls().appearance.partial_input.clone()
    });
    focus(&partial, cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("#ff77aa");
    focus(&view, cx);
    cx.run_until_parked();
    assert_eq!(
        read(&partial, cx, |input| input.content().to_owned()),
        "#FF77AA"
    );

    update(&view, cx, |view, window, cx| {
        view.request_color(ColorTarget::Partial, window, cx)
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::ChooseColor {
            target: ColorTarget::Partial,
            initial: 0xFF77AA,
        }]
    );
    update(&view, cx, |view, window, cx| {
        view.complete_color_choice(Ok(Some(0x00FF99)), window, cx);
    });
    let partial = read(&view, cx, |view| {
        view.controls().appearance.partial_input.clone()
    });
    assert_eq!(
        read(&partial, cx, |input| input.content().to_owned()),
        "#00FF99"
    );
    let dialog = read(&view, cx, |view| {
        view.controls().appearance.partial_dialog.clone()
    });
    assert_focused(&dialog, cx);

    focus(&partial, cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("#123abc");
    focus(&view, cx);
    cx.run_until_parked();
    assert_eq!(
        read(&partial, cx, |input| input.content().to_owned()),
        "#123ABC"
    );
    update(&view, cx, |view, window, cx| {
        view.request_color(ColorTarget::Partial, window, cx)
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::ChooseColor {
            target: ColorTarget::Partial,
            initial: 0x123ABC,
        }]
    );
    update(&view, cx, |view, window, cx| {
        view.complete_color_choice(Ok(None), window, cx);
    });
    let partial = read(&view, cx, |view| {
        view.controls().appearance.partial_input.clone()
    });
    assert_eq!(
        read(&partial, cx, |input| input.content().to_owned()),
        "#123ABC"
    );
    let dialog = read(&view, cx, |view| {
        view.controls().appearance.partial_dialog.clone()
    });
    assert_focused(&dialog, cx);

    update(&view, cx, |view, window, cx| {
        view.request_color(ColorTarget::Partial, window, cx)
    });
    let _ = take_actions(&view, cx);
    update(&view, cx, |view, window, cx| {
        view.complete_color_choice(Err(CommandError::StateUnavailable), window, cx);
    });
    let partial = read(&view, cx, |view| {
        view.controls().appearance.partial_input.clone()
    });
    assert_eq!(
        read(&partial, cx, |input| input.content().to_owned()),
        "#123ABC"
    );
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::StateUnavailable)
    );
    let dialog = read(&view, cx, |view| {
        view.controls().appearance.partial_dialog.clone()
    });
    assert_focused(&dialog, cx);
}

#[gpui::test]
fn immediate_language_engine_and_speaker_actions_wait_for_confirmation(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.resources = Resources {
        speaker_labels: true,
        ..initial.resources
    };
    let (view, cx) = panel_with(initial.clone(), cx);
    let source_values = read(&view, cx, SettingsView::source_language_values);
    assert_eq!(source_values.len(), 84);
    assert_eq!(source_values.first().map(String::as_str), Some(""));
    assert!(source_values.contains(&"pt-BR".to_string()));
    update(&view, cx, |view, window, cx| {
        view.select_ui_language("ru".into(), window, cx);
        view.select_source_language("pt-BR".into(), window, cx);
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::Application(Action::SetUiLanguage(
            UiLanguage::from_tag("ru").unwrap(),
        ))]
    );
    let recovery = Recovery(initial.clone());
    update(&view, cx, |view, window, cx| {
        assert!(!view.complete_application(
            Err(CommandError::StateUnavailable),
            &recovery,
            window,
            cx
        ));
    });
    assert_eq!(
        read(&view, cx, |view| view
            .application()
            .snapshot()
            .ui_language
            .tag()),
        "pt-br"
    );
    assert_eq!(
        cx.read_entity(&view, |view, cx| view
            .controls()
            .languages
            .ui_language
            .read(cx)
            .value()
            .to_owned()),
        "pt-br"
    );

    update(&view, cx, |view, window, cx| {
        view.select_ui_language("ru".into(), window, cx);
        let mut changed = initial.clone();
        changed.revision += 1;
        changed.ui_language = UiLanguage::from_tag("ru").unwrap();
        assert!(view.complete_application(Ok(changed), &recovery, window, cx));
        view.select_source_language("pt-BR".into(), window, cx);
        let mut changed = view.application().snapshot().clone();
        changed.revision += 1;
        changed.source_language = Some(SourceLanguage::from_tag("pt-BR").unwrap());
        assert!(view.complete_application(Ok(changed), &recovery, window, cx));
        view.select_source_language("pt-BR".into(), window, cx);
        view.select_translation_engine("riva".into(), window, cx);
        let mut changed = view.application().snapshot().clone();
        changed.revision += 1;
        changed.translation_engine = TranslationEngine::Riva;
        assert!(view.complete_application(Ok(changed), &recovery, window, cx));
        assert_eq!(
            view.controls().languages.source_language.read(cx).value(),
            "pt-BR"
        );
        view.set_speaker_labels(true, window, cx);
        let mut changed = view.application().snapshot().clone();
        changed.revision += 1;
        changed.speaker_labels = true;
        assert!(view.complete_application(Ok(changed), &recovery, window, cx));
        view.set_speaker_labels(true, window, cx);
    });
    let actions = take_actions(&view, cx);
    assert_eq!(actions.len(), 4);
    assert_eq!(
        actions[0],
        SettingsHostAction::Application(Action::SetUiLanguage(UiLanguage::from_tag("ru").unwrap()))
    );
    assert_eq!(
        actions[1],
        SettingsHostAction::Application(Action::SetSourceLanguage(Some(
            SourceLanguage::from_tag("pt-BR").unwrap()
        )))
    );
    assert_eq!(
        actions[2],
        SettingsHostAction::Application(Action::SetTranslationEngine(TranslationEngine::Riva))
    );
    assert_eq!(
        actions[3],
        SettingsHostAction::Application(Action::SetSpeakerLabels(true))
    );
    assert_eq!(read(&view, cx, |view| view.locale()), Locale::Ru);
    assert_eq!(
        read(&view, cx, |view| view.text(MessageKey::Settings)),
        "Настройки"
    );
    assert!(cx.read_entity(&view, |view, cx| {
        view.controls().languages.speaker_labels.read(cx).checked()
    }));
}

#[gpui::test]
fn confirmed_engine_reselection_emits_nothing_and_preserves_capture(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.translation_engine = TranslationEngine::Riva;
    initial.capture = anycaptions_core::domain::CaptureState::Listening;
    let (view, cx) = panel_with(initial, cx);

    update(&view, cx, |view, window, cx| {
        view.select_translation_engine("riva".into(), window, cx)
    });

    assert!(take_actions(&view, cx).is_empty());
    assert!(!read(&view, cx, |view| view.application().busy()));
    assert_eq!(
        read(&view, cx, |view| view.application().snapshot().capture),
        anycaptions_core::domain::CaptureState::Listening
    );
}

#[gpui::test]
fn speaker_notice_requires_both_enabled_preference_and_missing_resource(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.speaker_labels = true;
    initial.resources.speaker_labels = false;
    let (view, cx) = panel_with(initial, cx);
    assert_eq!(
        read(&view, cx, SettingsView::speaker_notice),
        Some(MessageKey::SpeakerLabelsUnavailable)
    );
    let hint = read(&view, cx, |view| view.text(MessageKey::SpeakerLabelsHint));
    assert!(hint.contains("Pessoa 1"));
    assert!(hint.contains("não reconhece identidades"));
    update(&view, cx, |view, window, cx| {
        let mut available = view.application().snapshot().clone();
        available.revision += 1;
        available.resources.speaker_labels = true;
        view.accept_snapshot(available, window, cx);
    });
    assert_eq!(read(&view, cx, SettingsView::speaker_notice), None);
    update(&view, cx, |view, window, cx| {
        let mut disabled = view.application().snapshot().clone();
        disabled.revision += 1;
        disabled.resources.speaker_labels = false;
        disabled.speaker_labels = false;
        view.accept_snapshot(disabled, window, cx);
    });
    assert_eq!(read(&view, cx, SettingsView::speaker_notice), None);
}

#[gpui::test]
fn source_picker_keyboard_selection_uses_tag_and_returns_focus(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    let picker = read(&view, cx, |view| {
        view.controls().languages.source_language.clone()
    });
    focus(&picker, cx);
    cx.simulate_keystrokes("down");
    press_key(cx, "enter");
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::Application(Action::SetSourceLanguage(
            Some(SourceLanguage::from_tag("pt-BR").unwrap()),
        ))]
    );
    let pending = read(&view, cx, |view| {
        view.controls().languages.source_language.clone()
    });
    assert_focused(&pending, cx);
    assert_eq!(read(&pending, cx, |picker| picker.value().to_owned()), "");
    let mut confirmed = read(&view, cx, |view| view.application().snapshot().clone());
    confirmed.revision += 1;
    confirmed.source_language = Some(SourceLanguage::from_tag("pt-BR").unwrap());
    let recovery = Recovery(confirmed.clone());
    update(&view, cx, |view, window, cx| {
        assert!(view.complete_application(Ok(confirmed), &recovery, window, cx));
    });
    let selected = read(&view, cx, |view| {
        view.controls().languages.source_language.clone()
    });
    assert_eq!(
        read(&selected, cx, |picker| picker.value().to_owned()),
        "pt-BR"
    );
    assert_eq!(
        cx.read_entity(&view, |view, cx| view.source_language_flag("pt-BR", cx)),
        crate::assets::flag("br")
    );
    assert_focused(&selected, cx);
}

#[gpui::test]
fn picker_escape_precedes_settings_escape_and_restores_picker_focus(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    let picker = read(&view, cx, |view| {
        view.controls().languages.source_language.clone()
    });
    focus(&picker, cx);
    cx.simulate_keystrokes("down");
    assert!(read(&picker, cx, |picker| picker.is_open()));
    assert!(cx.read_entity(&view, |view, cx| {
        view.automation_state(cx).source_language_expanded
    }));

    cx.simulate_keystrokes("escape");
    assert!(!read(&picker, cx, |picker| picker.is_open()));
    assert!(read(&view, cx, SettingsView::is_open));
    assert_focused(&picker, cx);

    cx.simulate_keystrokes("escape");
    assert_closed(&view, cx);
}

#[gpui::test]
fn model_rows_progress_cancel_retry_and_refresh_independently(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.translation_engine = TranslationEngine::Riva;
    let (view, cx) = panel_with(initial, cx);
    update(&view, cx, |view, window, cx| {
        view.accept_models(model_catalog(), window, cx)
    });
    let download = read(&view, cx, |view| {
        view.download_view(DownloadRow::Translation).unwrap()
    });
    assert!(download.visible);
    assert_eq!(
        read(&view, cx, |view| view.download_action_label(&download)),
        "Baixar (2.8 GB)"
    );

    update(&view, cx, |view, window, cx| {
        view.activate_download(DownloadRow::Translation, window, cx)
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::Download(DownloadCommand::Start(RIVA))]
    );
    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                RIVA,
                DownloadPhase::Downloading,
                1_379_651_184,
                Some(2_759_302_368),
                None,
            ),
            window,
            cx,
        );
    });
    let download = read(&view, cx, |view| {
        view.download_view(DownloadRow::Translation).unwrap()
    });
    assert_eq!(download.percent, 50);
    assert!(download.progress_visible);
    assert_eq!(
        read(&view, cx, |view| view.download_status_label(&download)),
        "Baixando… 50%"
    );
    assert_eq!(
        read(&view, cx, |view| {
            view.download_progress_label(DownloadRow::Translation)
        }),
        Some("Baixando… 50%".into())
    );
    update(&view, cx, |view, window, cx| {
        view.activate_download(DownloadRow::Translation, window, cx)
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::Download(DownloadCommand::Cancel)]
    );

    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                RIVA,
                DownloadPhase::Failed,
                0,
                None,
                Some(DownloadFailure::Corrupted),
            ),
            window,
            cx,
        );
    });
    let download = read(&view, cx, |view| {
        view.download_view(DownloadRow::Translation).unwrap()
    });
    assert!(download.failed);
    assert!(!download.progress_visible);
    assert_eq!(
        read(&view, cx, |view| view.download_action_label(&download)),
        "Tentar de novo"
    );
    update(&view, cx, |view, window, cx| {
        view.activate_download(DownloadRow::Translation, window, cx);
        view.reject_download_start(RIVA, window, cx);
    });
    assert!(read(&view, cx, |view| view
        .download_view(DownloadRow::Translation)
        .unwrap()
        .failed));

    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                DIARIZATION,
                DownloadPhase::Downloading,
                26_753_032,
                Some(107_012_128),
                None,
            ),
            window,
            cx,
        );
    });
    assert_eq!(
        read(&view, cx, |view| view
            .download_view(DownloadRow::Diarization)
            .unwrap()
            .percent),
        25
    );
    let translation_action = read(&view, cx, |view| {
        view.controls().models.translation.action.clone()
    });
    focus(&translation_action, cx);
    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                DIARIZATION,
                DownloadPhase::Ready,
                107_012_128,
                Some(107_012_128),
                None,
            ),
            window,
            cx,
        );
    });
    let translation_action = read(&view, cx, |view| {
        view.controls().models.translation.action.clone()
    });
    assert_focused(&translation_action, cx);
    assert_eq!(
        take_actions(&view, cx),
        vec![
            SettingsHostAction::Download(DownloadCommand::Start(RIVA)),
            SettingsHostAction::RefreshModels
        ]
    );
    assert!(!read(&view, cx, |view| view
        .download_view(DownloadRow::Diarization)
        .unwrap()
        .visible));
}

#[gpui::test]
fn diarization_start_uses_id_size_and_optimistic_progress(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.accept_models(model_catalog(), window, cx)
    });
    let download = read(&view, cx, |view| {
        view.download_view(DownloadRow::Diarization).unwrap()
    });
    assert_eq!(
        read(&view, cx, |view| view.download_action_label(&download)),
        "Baixar (107 MB)"
    );

    update(&view, cx, |view, window, cx| {
        view.activate_download(DownloadRow::Diarization, window, cx)
    });

    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::Download(DownloadCommand::Start(
            DIARIZATION
        ))]
    );
    let optimistic = read(&view, cx, |view| {
        view.download_view(DownloadRow::Diarization).unwrap()
    });
    assert!(optimistic.downloading && optimistic.progress_visible);
    assert_eq!(optimistic.percent, 0);
    assert_eq!(
        read(&view, cx, |view| view.download_action_label(&optimistic)),
        "Cancelar download"
    );
    assert_eq!(
        read(&view, cx, |view| {
            view.download_progress_label(DownloadRow::Diarization)
        }),
        Some("Baixando… 0%".into())
    );
}

#[gpui::test]
fn model_rows_hide_for_other_engines_installation_and_cancellation(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.accept_models(model_catalog(), window, cx)
    });
    assert!(read(&view, cx, |view| view
        .download_view(DownloadRow::Translation)
        .is_none()));
    assert!(read(&view, cx, |view| view
        .download_view(DownloadRow::Diarization)
        .unwrap()
        .visible));
    update(&view, cx, |view, window, cx| {
        view.activate_download(DownloadRow::Translation, window, cx)
    });
    assert!(take_actions(&view, cx).is_empty());

    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                DIARIZATION,
                DownloadPhase::Failed,
                10,
                Some(107_012_128),
                Some(DownloadFailure::Cancelled),
            ),
            window,
            cx,
        );
    });
    let cancelled = read(&view, cx, |view| {
        view.download_view(DownloadRow::Diarization).unwrap()
    });
    assert!(cancelled.visible && !cancelled.failed && !cancelled.progress_visible);
    update(&view, cx, |view, window, cx| {
        let installed = model_catalog()
            .into_iter()
            .map(|mut model| {
                model.installed = true;
                model
            })
            .collect();
        view.accept_models(installed, window, cx);
    });
    assert!(!read(&view, cx, |view| view
        .download_view(DownloadRow::Diarization)
        .unwrap()
        .visible));
}

#[gpui::test]
fn ready_download_moves_focus_from_disappearing_action_to_panel(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.translation_engine = TranslationEngine::Riva;
    let (view, cx) = panel_with(initial, cx);
    update(&view, cx, |view, window, cx| {
        view.accept_models(model_catalog(), window, cx)
    });
    let action = read(&view, cx, |view| {
        view.controls().models.translation.action.clone()
    });
    focus(&action, cx);
    update(&view, cx, |view, window, cx| {
        view.accept_download(
            download_update(
                RIVA,
                DownloadPhase::Ready,
                2_759_302_368,
                Some(2_759_302_368),
                None,
            ),
            window,
            cx,
        );
    });
    assert_focused(&view, cx);
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::RefreshModels]
    );
}

#[gpui::test]
fn initial_output_failure_reports_without_inventing_an_output_state(cx: &mut TestAppContext) {
    let (view, cx) = unloaded_panel(cx);
    let _ = take_actions(&view, cx);
    update(&view, cx, |view, window, cx| {
        assert!(!view.complete_outputs(
            Err(CommandError::AudioDevice(AudioDeviceError::Unavailable)),
            window,
            cx,
        ));
    });
    assert!(!read(&view, cx, SettingsView::output_busy));
    assert_eq!(read(&view, cx, SettingsView::output_status), None);
    assert_eq!(
        read(&view, cx, SettingsView::output_option_values),
        vec![""]
    );
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::AudioDevice(AudioDeviceError::Unavailable))
    );
}

#[gpui::test]
fn audio_output_uses_ids_preserves_stale_selection_and_recovers_busy(cx: &mut TestAppContext) {
    let (view, cx) = unloaded_panel(cx);
    assert!(!read(&view, cx, SettingsView::save_enabled));
    assert_eq!(
        take_actions(&view, cx),
        vec![
            SettingsHostAction::RefreshOutputs,
            SettingsHostAction::LoadDictation,
            SettingsHostAction::RefreshModels
        ]
    );
    update(&view, cx, |view, window, cx| {
        view.complete_dictation(Ok(dictation_load()), window, cx);
        view.complete_outputs(
            Ok(devices(
                &["speaker-a", "speaker-b"],
                Some("speaker-b"),
                true,
            )),
            window,
            cx,
        );
        view.select_output("speaker-a".into(), window, cx);
        view.select_output("speaker-b".into(), window, cx);
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::SelectOutput("speaker-a".into())]
    );
    assert_eq!(
        read(&view, cx, SettingsView::output_option_values),
        vec!["speaker-a", "speaker-b"]
    );
    assert!(read(&view, cx, |view| view.output_busy()));
    update(&view, cx, |view, window, cx| {
        view.complete_output_selection(
            Ok(devices(
                &["speaker-a", "speaker-b"],
                Some("speaker-a"),
                true,
            )),
            window,
            cx,
        );
        view.complete_outputs(
            Ok(devices(&["speaker-b"], Some("speaker-a"), false)),
            window,
            cx,
        );
    });
    assert!(!read(&view, cx, |view| view.output_busy()));
    assert_eq!(
        read(&view, cx, |view| view
            .output_selection()
            .selected_id
            .clone()),
        Some("speaker-a".to_string())
    );
    assert_eq!(
        read(&view, cx, |view| view.output_status()),
        Some(MessageKey::SavedOutputDisconnected)
    );
    assert_eq!(
        read(&view, cx, SettingsView::output_option_values),
        vec!["speaker-b", "speaker-a"]
    );
    assert!(take_actions(&view, cx).is_empty());

    update(&view, cx, |view, window, cx| {
        view.select_output("speaker-b".into(), window, cx);
        view.complete_output_selection(
            Err(CommandError::AudioDevice(AudioDeviceError::NotFound)),
            window,
            cx,
        );
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![
            SettingsHostAction::SelectOutput("speaker-b".into()),
            SettingsHostAction::RefreshOutputs
        ]
    );
    assert!(read(&view, cx, |view| view.output_busy()));
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::AudioDevice(AudioDeviceError::NotFound))
    );
    update(&view, cx, |view, window, cx| {
        view.complete_outputs(
            Err(CommandError::AudioDevice(AudioDeviceError::Unavailable)),
            window,
            cx,
        );
    });
    assert!(!read(&view, cx, |view| view.output_busy()));
    assert_eq!(
        read(&view, cx, |view| view
            .output_selection()
            .selected_id
            .clone()),
        Some("speaker-a".to_string())
    );
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::AudioDevice(AudioDeviceError::NotFound))
    );
    update(&view, cx, |view, window, cx| {
        view.complete_outputs(Ok(devices(&[], None, false)), window, cx);
    });
    assert_eq!(
        read(&view, cx, |view| view.output_status()),
        Some(MessageKey::NoActiveOutput)
    );
}

#[gpui::test]
fn output_selection_recovers_successfully_after_rejection(cx: &mut TestAppContext) {
    let (view, cx) = unloaded_panel(cx);
    let _ = take_actions(&view, cx);
    update(&view, cx, |view, window, cx| {
        view.complete_outputs(
            Ok(devices(
                &["speaker-a", "speaker-b"],
                Some("speaker-a"),
                true,
            )),
            window,
            cx,
        );
        view.select_output("speaker-b".into(), window, cx);
        view.complete_output_selection(
            Err(CommandError::AudioDevice(AudioDeviceError::NotFound)),
            window,
            cx,
        );
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![
            SettingsHostAction::SelectOutput("speaker-b".into()),
            SettingsHostAction::RefreshOutputs,
        ]
    );
    update(&view, cx, |view, window, cx| {
        view.complete_outputs(
            Ok(devices(
                &["speaker-a", "speaker-b"],
                Some("speaker-a"),
                true,
            )),
            window,
            cx,
        );
        view.select_output("speaker-b".into(), window, cx);
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::SelectOutput("speaker-b".into())]
    );

    update(&view, cx, |view, window, cx| {
        view.complete_output_selection(
            Ok(devices(
                &["speaker-a", "speaker-b"],
                Some("speaker-b"),
                true,
            )),
            window,
            cx,
        );
    });

    assert!(!read(&view, cx, SettingsView::output_busy));
    assert_eq!(
        read(&view, cx, |view| view
            .output_selection()
            .selected_id
            .clone()),
        Some("speaker-b".into())
    );
    assert_eq!(read(&view, cx, SettingsView::output_status), None);
    assert_eq!(
        read(&view, cx, SettingsView::output_option_values),
        vec!["speaker-a", "speaker-b"]
    );
    assert!(take_actions(&view, cx).is_empty());
}

#[gpui::test]
fn every_dictation_field_stays_draft_and_save_trims_only_the_shortcut(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    let languages = read(&view, cx, SettingsView::dictation_language_values);
    assert_eq!(languages.len(), 84);
    assert_eq!(languages.first().map(String::as_str), Some(""));
    assert!(languages.contains(&"ja-JP".to_string()));
    assert_eq!(
        cx.read_entity(&view, |view, cx| view.dictation_language_flag("ja-JP", cx)),
        crate::assets::flag("jp")
    );
    assert!(!read(&view, cx, |view| view
        .dictation_translation_values()
        .contains(&"ja".to_string())));
    update(&view, cx, |view, window, cx| {
        view.set_dictation_enabled(true, window, cx);
        view.set_dictation_shortcut("  Ctrl+Space  ".into(), window, cx);
        view.set_dictation_activation("toggle".into(), window, cx);
        view.set_dictation_sound(false, window, cx);
        view.set_dictation_show_text(false, window, cx);
        view.set_dictation_duration(65.0, window, cx);
        view.set_dictation_limit_duration(false, window, cx);
        view.set_dictation_input("mic-b".into(), window, cx);
        view.set_dictation_language("ja-JP".into(), window, cx);
        view.set_dictation_translate_to("en".into(), window, cx);
    });
    assert!(!read(&view, cx, SettingsView::dictation_duration_enabled));
    assert!(take_actions(&view, cx).is_empty());
    let draft = read(&view, cx, |view| view.dictation_draft().unwrap().clone());
    assert!(draft.enabled);
    assert_eq!(draft.shortcut, "  Ctrl+Space  ");
    assert_eq!(draft.activation, ActivationMode::Toggle);
    assert!(!draft.sound_cue);
    assert!(!draft.show_live_text);
    assert!(!draft.limit_duration);
    assert_eq!(draft.max_duration_secs, 65);
    assert_eq!(draft.input_id.as_deref(), Some("mic-b"));
    assert_eq!(draft.language.as_deref(), Some("ja-JP"));
    assert_eq!(draft.translate_to.unwrap().tag(), "en");

    update(&view, cx, |view, window, cx| {
        let mut changed = view.application().snapshot().clone();
        changed.revision += 1;
        changed.translation_engine = TranslationEngine::Riva;
        view.accept_snapshot(changed, window, cx);
        view.set_dictation_translate_to("ja".into(), window, cx);
    });
    assert!(read(&view, cx, |view| view
        .dictation_translation_values()
        .contains(&"ja".to_string())));
    assert_eq!(
        cx.read_entity(&view, |view, cx| view.dictation_translation_flag("ja", cx)),
        crate::assets::flag("jp")
    );
    let mut backend = fake_save(&view, cx);
    assert!(save_with(&view, cx, &mut backend));
    assert_eq!(backend.calls(), vec!["dictation"]);
    let saved = backend.dictation_calls.last().unwrap();
    assert_eq!(saved.shortcut, "Ctrl+Space");
    assert_eq!(saved.translate_to.unwrap().tag(), "ja");
    assert!(!saved.sound_cue && !saved.show_live_text && !saved.limit_duration);
}

#[gpui::test]
fn dictation_rejection_retries_only_dictation_with_every_field(cx: &mut TestAppContext) {
    let mut initial = snapshot();
    initial.translation_engine = TranslationEngine::Riva;
    let (view, cx) = panel_with(initial, cx);
    update(&view, cx, |view, window, cx| {
        view.set_font_size(55.0, window, cx);
        view.set_shortcut("Ctrl+Alt+K".into(), window, cx);
        view.set_dictation_enabled(true, window, cx);
        view.set_dictation_shortcut("  Ctrl+Space  ".into(), window, cx);
        view.set_dictation_activation("toggle".into(), window, cx);
        view.set_dictation_sound(false, window, cx);
        view.set_dictation_show_text(false, window, cx);
        view.set_dictation_duration(65.0, window, cx);
        view.set_dictation_limit_duration(false, window, cx);
        view.set_dictation_input("mic-b".into(), window, cx);
        view.set_dictation_language("ja-JP".into(), window, cx);
        view.set_dictation_translate_to("ja".into(), window, cx);
    });
    let mut backend = fake_save(&view, cx);
    backend.fail = Some("dictation");
    assert!(!save_with(&view, cx, &mut backend));
    assert_eq!(backend.calls(), vec!["appearance", "shortcut", "dictation"]);

    backend.fail = None;
    assert!(save_with(&view, cx, &mut backend));

    assert_eq!(
        backend.calls(),
        vec!["appearance", "shortcut", "dictation", "dictation"]
    );
    assert_eq!(backend.snapshot.appearance.font_size, 55);
    assert_eq!(backend.snapshot.shortcut, "Ctrl+Alt+K");
    assert_eq!(backend.dictation_calls.len(), 1);
    let saved = backend.dictation_calls.last().unwrap();
    assert!(saved.enabled);
    assert_eq!(saved.shortcut, "Ctrl+Space");
    assert_eq!(saved.activation, ActivationMode::Toggle);
    assert!(!saved.sound_cue && !saved.show_live_text && !saved.limit_duration);
    assert_eq!(saved.max_duration_secs, 65);
    assert_eq!(saved.input_id.as_deref(), Some("mic-b"));
    assert_eq!(saved.language.as_deref(), Some("ja-JP"));
    assert_eq!(saved.translate_to.unwrap().tag(), "ja");
    assert_closed(&view, cx);
}

#[gpui::test]
fn dictation_load_preserves_missing_mic_and_availability_does_not_disable_fields(
    cx: &mut TestAppContext,
) {
    let (view, cx) = unloaded_panel(cx);
    let _ = take_actions(&view, cx);
    let mut load = dictation_load();
    load.available = false;
    load.settings.enabled = true;
    load.settings.input_id = Some("missing-mic".into());
    load.inputs = devices(&["mic-a"], Some("missing-mic"), false);
    update(&view, cx, |view, window, cx| {
        view.complete_dictation(Ok(load), window, cx);
        view.complete_outputs(Ok(devices(&[], None, false)), window, cx);
    });
    assert_eq!(
        read(&view, cx, |view| view.dictation_status()),
        Some(MessageKey::DictationModelMissing)
    );
    assert_eq!(
        read(&view, cx, |view| view
            .dictation_draft()
            .unwrap()
            .input_id
            .clone()),
        Some("missing-mic".to_string())
    );
    assert!(read(&view, cx, |view| view
        .dictation_input_values()
        .contains(&"missing-mic".to_string())));
    assert!(read(&view, cx, |view| view.dictation_fields_enabled()));
}

#[gpui::test]
fn rejected_save_retains_draft_and_retry_skips_confirmed_writes(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update_font_size!(
        &view,
        cx,
        55.0,
        |view: &mut SettingsView, window: &mut Window, cx: &mut Context<SettingsView>| {
            view.set_shortcut("Ctrl+Alt+K".into(), &mut *window, &mut *cx);
            view.set_dictation_enabled(true, &mut *window, &mut *cx);
            let mut incoming = view.application().snapshot().clone();
            incoming.revision += 1;
            incoming.capture = anycaptions_core::domain::CaptureState::Listening;
            view.accept_snapshot(incoming, window, cx);
        }
    );
    assert_eq!(
        read(&view, cx, |view| view
            .application()
            .draft()
            .unwrap()
            .appearance
            .font_size),
        55
    );
    assert_eq!(
        read(&view, cx, |view| view
            .application()
            .snapshot()
            .appearance
            .font_size),
        32
    );

    let mut backend = fake_save(&view, cx);
    backend.fail = Some("shortcut");
    assert!(!save_with(&view, cx, &mut backend));
    assert_eq!(backend.calls(), vec!["appearance", "shortcut"]);
    assert!(read(&view, cx, SettingsView::is_open));
    assert!(!read(&view, cx, |view| view.application().busy()));
    assert_eq!(
        read(&view, cx, |view| view
            .application()
            .draft()
            .unwrap()
            .appearance
            .font_size),
        55
    );
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::ShortcutUnavailable)
    );
    assert!(take_actions(&view, cx).is_empty());

    backend.fail = None;
    update(&view, cx, |view, window, cx| {
        view.set_font_size(56.0, window, cx);
        view.set_font_size(55.0, window, cx);
        assert!(view.save_settings(&mut backend, window, cx));
    });
    assert_eq!(
        backend.calls(),
        vec!["appearance", "shortcut", "shortcut", "dictation"]
    );
    assert_closed(&view, cx);
}

#[gpui::test]
fn cancel_and_locked_snapshot_discard_drafts_and_unchanged_save_closes(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update_font_size!(
        &view,
        cx,
        55.0,
        |view: &mut SettingsView, window: &mut Window, cx: &mut Context<SettingsView>| {
            view.cancel_settings(&mut *window, &mut *cx);
            view.complete_outputs(
                Ok(devices(&["late"], Some("late"), true)),
                &mut *window,
                &mut *cx,
            );
            view.complete_dictation(Ok(dictation_load()), window, cx);
        }
    );
    assert!(read(&view, cx, |view| view.application().draft().is_none()));
    assert!(read(&view, cx, |view| view
        .output_selection()
        .devices
        .is_empty()));
    assert_closed(&view, cx);

    let (unchanged, cx) = panel(cx);
    assert_eq!(
        read(&unchanged, cx, |view| view
            .application()
            .draft()
            .unwrap()
            .appearance
            .font_size),
        32
    );
    let mut backend = fake_save(&unchanged, cx);
    assert!(save_with(&unchanged, cx, &mut backend));
    assert!(backend.calls().is_empty());

    let (locked, cx) = panel(cx);
    update_font_size!(&locked, cx, 55.0, |view: &mut SettingsView, window, cx| {
        let mut incoming = view.application().snapshot().clone();
        incoming.revision += 1;
        incoming.window_mode = anycaptions_core::domain::WindowMode::Locked;
        view.accept_snapshot(incoming, window, cx);
    });
    assert_closed(&locked, cx);
}

#[gpui::test]
fn locked_application_completion_and_rejection_close_panel(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.select_ui_language("ru".into(), window, cx)
    });
    let _ = take_actions(&view, cx);
    let mut locked = read(&view, cx, |view| view.application().snapshot().clone());
    locked.revision += 1;
    locked.window_mode = anycaptions_core::domain::WindowMode::Locked;
    let recovery = Recovery(locked.clone());
    update(&view, cx, |view, window, cx| {
        assert!(view.complete_application(Ok(locked), &recovery, window, cx));
    });
    assert!(read(&view, cx, |view| view.application().draft().is_none()));
    assert_closed(&view, cx);

    let (rejected, cx) = panel(cx);
    update(&rejected, cx, |view, window, cx| {
        view.select_ui_language("ru".into(), window, cx)
    });
    let _ = take_actions(&rejected, cx);
    let mut locked = read(&rejected, cx, |view| view.application().snapshot().clone());
    locked.revision += 1;
    locked.window_mode = anycaptions_core::domain::WindowMode::Locked;
    let recovery = Recovery(locked);
    update(&rejected, cx, |view, window, cx| {
        assert!(!view.complete_application(
            Err(CommandError::StateUnavailable),
            &recovery,
            window,
            cx,
        ));
    });
    assert_eq!(
        read(&rejected, cx, |view| view.application().error()),
        Some(CommandError::StateUnavailable)
    );
    assert!(read(&rejected, cx, |view| {
        view.application().draft().is_none()
    }));
    assert_closed(&rejected, cx);
}

#[gpui::test]
fn locked_rejected_save_recovery_closes_panel(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.set_font_size(55.0, window, cx);
        view.set_shortcut("Ctrl+Alt+K".into(), window, cx);
    });
    let mut backend = fake_save(&view, cx);
    backend.fail = Some("shortcut");
    backend.snapshot.window_mode = anycaptions_core::domain::WindowMode::Locked;

    assert!(!save_with(&view, cx, &mut backend));

    assert_eq!(backend.calls(), vec!["appearance", "shortcut"]);
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::ShortcutUnavailable)
    );
    assert!(read(&view, cx, |view| view.application().draft().is_none()));
    assert_closed(&view, cx);
}

#[gpui::test]
fn browser_actions_notify_observer_to_drain(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        view.complete_update_check(Ok(update_check("0.2.0", true, false)), window, cx);
    });
    let drained = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let drained = drained.clone();
        cx.observe(&view, move |view, cx| {
            view.update(cx, |view, _| {
                drained
                    .borrow_mut()
                    .extend(std::iter::from_fn(|| view.take_action()));
            });
        })
    });

    update(&view, cx, |view, window, cx| {
        view.activate_update(window, cx)
    });
    update(&view, cx, |view, window, cx| view.open_project(window, cx));

    assert_eq!(
        drained.borrow().as_slice(),
        [
            SettingsHostAction::OpenReleasesPage,
            SettingsHostAction::OpenProjectPage,
        ]
    );
}

#[gpui::test]
fn update_visibility_changes_preserve_logical_credit_focus(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    let credit = read(&view, cx, |view| view.controls().updates.credit.clone());
    focus(&credit, cx);
    update(&view, cx, |view, window, cx| view.check_updates(window, cx));
    let credit = read(&view, cx, |view| view.controls().updates.credit.clone());
    assert_focused(&credit, cx);
    update(&view, cx, |view, window, cx| {
        view.complete_update_check(Ok(update_check("0.2.0", true, true)), window, cx);
    });
    let credit = read(&view, cx, |view| view.controls().updates.credit.clone());
    assert_focused(&credit, cx);

    let download = read(&view, cx, |view| view.controls().updates.download.clone());
    focus(&download, cx);
    update(&view, cx, |view, window, cx| view.check_updates(window, cx));
    let check_now = read(&view, cx, |view| view.controls().updates.check_now.clone());
    assert_focused(&check_now, cx);
    update(&view, cx, |view, window, cx| {
        view.complete_update_check(Ok(update_check("0.2.0", true, true)), window, cx);
    });
    let download = read(&view, cx, |view| view.controls().updates.download.clone());
    focus(&download, cx);
    update(&view, cx, |view, window, cx| {
        view.activate_update(window, cx)
    });
    let check_now = read(&view, cx, |view| view.controls().updates.check_now.clone());
    assert_focused(&check_now, cx);
}

#[gpui::test]
fn updates_cover_check_install_progress_failure_fallback_and_credit(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    assert_eq!(read(&view, cx, |view| view.version_text()), "v0.1.1");
    assert!(read(&view, cx, |view| view.update_status_message()).contains("GitHub"));
    update(&view, cx, |view, window, cx| {
        view.check_updates(window, cx);
        view.check_updates(window, cx);
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::CheckForUpdates]
    );
    assert_eq!(
        read(&view, cx, |view| view.updates().status().clone()),
        UpdateStatus::Checking
    );

    let available = update_check("0.2.0", true, false);
    update(&view, cx, |view, window, cx| {
        assert!(view.complete_update_check(Ok(available.clone()), window, cx));
        view.activate_update(window, cx);
    });
    assert!(read(&view, cx, |view| view.update_status_message()).contains("0.2.0"));
    assert_eq!(
        read(&view, cx, |view| view.update_action_view().unwrap().state),
        UpdateActionState::OpenReleasesPage
    );
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::OpenReleasesPage]
    );

    let installable = UpdateCheck {
        installable: true,
        ..available.clone()
    };
    update(&view, cx, |view, window, cx| {
        view.check_updates(window, cx);
        view.complete_update_check(Ok(installable.clone()), window, cx);
        view.activate_update(window, cx);
        view.activate_update(window, cx);
        view.accept_update_progress(
            update_progress(UpdatePhase::Downloading, 45, 100),
            window,
            cx,
        );
    });
    assert_eq!(
        take_actions(&view, cx),
        vec![
            SettingsHostAction::CheckForUpdates,
            SettingsHostAction::InstallUpdate
        ]
    );
    assert_eq!(
        read(&view, cx, |view| view.update_action_view().unwrap().percent),
        Some(45)
    );
    update(&view, cx, |view, window, cx| {
        view.accept_update_progress(
            update_progress(UpdatePhase::Installing, 100, 100),
            window,
            cx,
        );
    });
    assert!(read(&view, cx, |view| view.update_status_message()).contains("Instalando"));
    update(&view, cx, |view, window, cx| {
        view.accept_update_progress(update_progress(UpdatePhase::Failed, 0, 0), window, cx);
        view.activate_update(window, cx);
    });
    assert!(read(&view, cx, |view| view.update_status_message()).contains("Nada mudou"));
    assert_eq!(
        take_actions(&view, cx),
        vec![SettingsHostAction::OpenReleasesPage]
    );

    update(&view, cx, |view, window, cx| {
        view.check_updates(window, cx);
        view.complete_update_check(Ok(installable), window, cx);
        view.activate_update(window, cx);
        view.reject_install(CommandError::UpdateUnavailable, window, cx);
    });
    assert!(read(&view, cx, |view| view
        .update_action_view()
        .unwrap()
        .failed));
    assert_eq!(
        read(&view, cx, |view| view.application().error()),
        Some(CommandError::UpdateUnavailable)
    );

    update(&view, cx, |view, window, cx| {
        view.check_updates(window, cx);
        view.complete_update_check(Ok(update_check("0.1.1", false, false)), window, cx);
    });
    assert!(read(&view, cx, |view| view.update_action_view().is_none()));
    assert!(read(&view, cx, |view| view.update_status_message()).contains("0.1.1"));
    update(&view, cx, |view, window, cx| {
        view.check_updates(window, cx);
        assert!(!view.complete_update_check(Err(UpdateError::Network), window, cx));
        view.set_update_check(true, window, cx);
        view.open_project(window, cx);
    });
    assert!(
        read(&view, cx, |view| view.update_status_message()).contains("Não foi possível verificar")
    );
    let actions = take_actions(&view, cx);
    assert!(
        actions.contains(&SettingsHostAction::Application(Action::SetUpdateCheck(
            true
        )))
    );
    assert!(actions.contains(&SettingsHostAction::OpenProjectPage));

    update(&view, cx, |view, window, cx| {
        view.restore_pending_update_check(window, cx)
    });
    assert_eq!(
        read(&view, cx, |view| view.updates().status().clone()),
        UpdateStatus::Checking
    );
    assert!(take_actions(&view, cx).is_empty());
}

#[gpui::test]
fn tab_moves_focus_and_root_escape_closes_after_invalid_color_entry(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx);
    update(&view, cx, |view, window, cx| {
        let handle = view.focus_handle(cx);
        window.focus(&handle);
        view.set_color_text(ColorTarget::Confirmed, "red", window, cx);
    });
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert!(!view.control_focused(window, cx));
    });
    update(&view, cx, |view, window, cx| {
        let input = view.controls().appearance.confirmed_input.clone();
        let handle = input.control_focus(cx);
        window.focus(&handle);
    });
    cx.simulate_keystrokes("escape");
    assert_closed(&view, cx);
}
