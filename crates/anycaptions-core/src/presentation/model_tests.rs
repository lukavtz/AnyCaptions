use super::{
    application::{Action, ApplicationModel, SettingsSave, SnapshotRecovery},
    dictation::{DictationModel, dictation_error_key},
    downloads::{DownloadCommand, DownloadView, DownloadsModel, ModelDownloadState},
    i18n::{Locale, MessageKey},
    updates::{UpdateActionModel, UpdateCommand, UpdateNoticeModel, UpdatesModel},
};
use crate::{
    dictation::{
        ActivationMode, DictationSettings,
        indicator::{IndicatorSnapshot, place_indicator},
        runtime::DictationEventPayload,
    },
    domain::{
        AppSnapshot, Appearance, CaptionMode, CaptureState, DomainError, PT_BR, Resources,
        TranslationEngine, UiLanguage, WindowMode,
    },
    ipc::{CommandError, StateEvents},
    models::{
        ModelKind,
        download::{DownloadFailure, DownloadPhase, DownloadUpdate},
    },
    update::{UpdateCheck, UpdateError},
    updater::{UpdatePhase, UpdateProgress},
    window_geometry::Rect,
};

fn snapshot(revision: u32) -> AppSnapshot {
    AppSnapshot {
        revision,
        ui_language: UiLanguage::from_tag("pt-br").unwrap(),
        session_id: 0,
        mode: CaptionMode::Auto,
        translation_engine: TranslationEngine::Opus,
        source_language: None,
        speaker_labels: false,
        capture: CaptureState::Stopped,
        window_mode: WindowMode::Edit,
        appearance: Appearance::default(),
        resources: Resources {
            audio_capture: true,
            transcription: true,
            translation: true,
            speaker_labels: false,
        },
        can_start: true,
        can_lock: false,
        shortcut: "Ctrl+Alt+L".into(),
        shortcut_available: true,
        error: None,
        version: "0.2.0",
        update_check: false,
    }
}

struct Recovery {
    result: Result<AppSnapshot, CommandError>,
}

impl Recovery {
    fn success(snapshot: AppSnapshot) -> Self {
        Self {
            result: Ok(snapshot),
        }
    }

    fn unavailable() -> Self {
        Self {
            result: Err(CommandError::StateUnavailable),
        }
    }
}

impl SnapshotRecovery for Recovery {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        self.result.clone()
    }
}

struct IgnoreEvents;

impl StateEvents for IgnoreEvents {
    fn state_changed(&self, _state: &AppSnapshot) -> Result<(), CommandError> {
        Ok(())
    }
}

#[test]
fn application_accepts_only_non_regressing_snapshots() {
    let mut app = ApplicationModel::new(snapshot(7));
    assert!(!app.accept(snapshot(6)));
    assert_eq!(app.snapshot().revision, 7);

    let mut equal = snapshot(7);
    equal.can_lock = true;
    assert!(app.accept(equal));
    assert!(app.snapshot().can_lock);
    assert!(app.accept(snapshot(8)));
    assert_eq!(app.snapshot().revision, 8);
}

#[test]
fn application_actions_are_single_flight_until_completion() {
    let mut app = ApplicationModel::new(snapshot(7));
    app.report(CommandError::EventUnavailable);
    assert!(app.try_begin(Action::StartCapture));
    assert!(app.busy());
    assert_eq!(app.error(), None);
    assert!(!app.try_begin(Action::StopCapture));

    let mut stale = snapshot(6);
    stale.capture = CaptureState::Preparing;
    let recovery = Recovery::unavailable();
    assert!(app.complete(Ok(stale), &recovery));
    assert!(!app.busy());
    assert_eq!(app.snapshot().revision, 7);

    let mut preparing = snapshot(8);
    preparing.capture = CaptureState::Preparing;
    assert!(app.accept(preparing));
    assert!(app.try_begin(Action::StopCapture));
    assert!(app.complete(Ok(snapshot(9)), &recovery));
}

#[test]
fn application_enforces_existing_control_gates() {
    let mut initial = snapshot(7);
    initial.can_start = false;
    let mut app = ApplicationModel::new(initial);
    assert!(!app.try_begin(Action::StartCapture));
    assert!(!app.try_begin(Action::StopCapture));

    let mut running = snapshot(8);
    running.capture = CaptureState::Listening;
    assert!(app.accept(running));
    assert!(app.try_begin(Action::StopCapture));

    let recovery = Recovery::unavailable();
    assert!(app.complete(Ok(snapshot(9)), &recovery));
    assert!(!app.try_begin(Action::LockWindow));

    let mut lockable = snapshot(9);
    lockable.can_lock = true;
    assert!(app.accept(lockable));
    assert!(app.try_begin(Action::LockWindow));
    assert!(app.complete(Ok(snapshot(10)), &recovery));

    assert!(!app.try_begin(Action::SetMode(CaptionMode::Auto)));
    assert!(app.try_begin(Action::SetMode(CaptionMode::Translate(PT_BR))));
}

#[test]
fn rejected_action_recovers_authoritative_snapshot_and_original_error() {
    let mut app = ApplicationModel::new(snapshot(7));
    assert!(app.try_begin(Action::StartCapture));
    let recovery = Recovery::success(snapshot(8));
    assert!(!app.complete(
        Err(CommandError::Domain(DomainError::CaptureUnavailable)),
        &recovery,
    ));
    assert_eq!(app.snapshot().revision, 8);
    assert_eq!(
        app.error(),
        Some(CommandError::Domain(DomainError::CaptureUnavailable))
    );
    assert!(!app.busy());

    assert!(app.try_begin(Action::StartCapture));
    let unavailable = Recovery::unavailable();
    assert!(!app.complete(Err(CommandError::EventUnavailable), &unavailable));
    assert_eq!(app.snapshot().revision, 8);
    assert_eq!(app.error(), Some(CommandError::EventUnavailable));
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SaveStep {
    Appearance,
    Shortcut(String),
    Dictation(String),
}

struct SaveBackend {
    state: AppSnapshot,
    steps: Vec<SaveStep>,
    fail_shortcut: bool,
}

impl SaveBackend {
    fn new(state: AppSnapshot) -> Self {
        Self {
            state,
            steps: Vec::new(),
            fail_shortcut: false,
        }
    }
}

impl SnapshotRecovery for SaveBackend {
    fn recover_snapshot(&self) -> Result<AppSnapshot, CommandError> {
        Ok(self.state.clone())
    }
}

impl SettingsSave for SaveBackend {
    fn set_appearance(&mut self, appearance: Appearance) -> Result<AppSnapshot, CommandError> {
        self.steps.push(SaveStep::Appearance);
        self.state.revision += 1;
        self.state.appearance = appearance;
        Ok(self.state.clone())
    }

    fn set_shortcut(&mut self, shortcut: &str) -> Result<AppSnapshot, CommandError> {
        self.steps.push(SaveStep::Shortcut(shortcut.into()));
        if self.fail_shortcut {
            return Err(CommandError::ShortcutUnavailable);
        }
        self.state.revision += 1;
        self.state.shortcut = shortcut.into();
        Ok(self.state.clone())
    }

    fn set_dictation(
        &mut self,
        settings: DictationSettings,
    ) -> Result<DictationSettings, CommandError> {
        self.steps
            .push(SaveStep::Dictation(settings.shortcut.clone()));
        Ok(settings)
    }
}

fn edit_drafts(app: &mut ApplicationModel) {
    app.open_settings();
    app.load_dictation(DictationSettings::default());
    let draft = app.draft_mut().unwrap();
    draft.appearance.font_size = 48;
    draft.shortcut = "Ctrl+Alt+K".into();
    draft.dictation.as_mut().unwrap().shortcut = "  Ctrl+Space  ".into();
}

#[test]
fn settings_save_uses_appearance_shortcut_dictation_order() {
    let initial = snapshot(7);
    let mut app = ApplicationModel::new(initial.clone());
    let mut backend = SaveBackend::new(initial);
    edit_drafts(&mut app);

    assert!(app.save_settings(&mut backend));
    assert_eq!(
        backend.steps,
        [
            SaveStep::Appearance,
            SaveStep::Shortcut("Ctrl+Alt+K".into()),
            SaveStep::Dictation("Ctrl+Space".into()),
        ]
    );
    assert!(app.draft().is_none());
    assert_eq!(app.snapshot().appearance.font_size, 48);
    assert_eq!(app.snapshot().shortcut, "Ctrl+Alt+K");
}

#[test]
fn rejected_save_retains_drafts_and_retry_skips_confirmed_steps() {
    let initial = snapshot(7);
    let mut app = ApplicationModel::new(initial.clone());
    let mut backend = SaveBackend::new(initial);
    backend.fail_shortcut = true;
    edit_drafts(&mut app);

    assert!(!app.save_settings(&mut backend));
    assert_eq!(
        backend.steps,
        [
            SaveStep::Appearance,
            SaveStep::Shortcut("Ctrl+Alt+K".into())
        ]
    );
    let draft = app.draft().unwrap();
    assert_eq!(draft.appearance.font_size, 48);
    assert_eq!(draft.shortcut, "Ctrl+Alt+K");
    assert_eq!(draft.dictation.as_ref().unwrap().shortcut, "  Ctrl+Space  ");
    assert_eq!(app.error(), Some(CommandError::ShortcutUnavailable));

    backend.fail_shortcut = false;
    backend.steps.clear();
    assert!(app.save_settings(&mut backend));
    assert_eq!(
        backend.steps,
        [
            SaveStep::Shortcut("Ctrl+Alt+K".into()),
            SaveStep::Dictation("Ctrl+Space".into()),
        ]
    );
    assert!(app.draft().is_none());
}

#[test]
fn drafts_survive_snapshots_and_reset_on_cancel() {
    let mut app = ApplicationModel::new(snapshot(7));
    app.open_settings();
    app.draft_mut().unwrap().appearance.font_size = 55;
    assert!(app.accept(snapshot(8)));
    assert_eq!(app.draft().unwrap().appearance.font_size, 55);

    app.cancel_settings();
    app.open_settings();
    assert_eq!(app.draft().unwrap().appearance.font_size, 32);
}

#[test]
fn only_an_accepted_transition_to_locked_discards_settings_draft() {
    let mut app = ApplicationModel::new(snapshot(7));
    app.open_settings();
    app.draft_mut().unwrap().appearance.font_size = 55;

    let mut stale = snapshot(6);
    stale.window_mode = WindowMode::Locked;
    assert!(!app.accept(stale));
    assert_eq!(app.draft().unwrap().appearance.font_size, 55);

    let mut locked = snapshot(8);
    locked.window_mode = WindowMode::Locked;
    assert!(app.accept(locked));
    assert!(app.draft().is_none());
}

fn download_entry() -> ModelDownloadState {
    ModelDownloadState {
        model: "riva",
        kind: ModelKind::Translation,
        engine: Some(TranslationEngine::Riva),
        installed: false,
        downloadable: true,
        downloading: false,
        total_bytes: 100,
    }
}

fn downloads_with_entry() -> DownloadsModel {
    let mut downloads = DownloadsModel::default();
    downloads.replace_models(vec![download_entry()]);
    downloads
}

fn riva_view(downloads: &DownloadsModel) -> DownloadView {
    downloads
        .view_for_engine(TranslationEngine::Riva, MessageKey::ModelDownloadMissing)
        .unwrap()
}

fn download_update(
    phase: DownloadPhase,
    received_bytes: u64,
    total_bytes: Option<u64>,
    reason: Option<DownloadFailure>,
) -> DownloadUpdate {
    DownloadUpdate {
        model: "riva",
        phase,
        received_bytes,
        total_bytes,
        reason,
    }
}

fn fail_download(downloads: &mut DownloadsModel, reason: DownloadFailure) {
    downloads.accept(download_update(
        DownloadPhase::Failed,
        0,
        None,
        Some(reason),
    ));
}

#[test]
fn downloads_start_report_clamped_progress_and_cancel() {
    let mut downloads = downloads_with_entry();
    assert_eq!(
        downloads.activate("riva"),
        Some(DownloadCommand::Start("riva"))
    );
    let view = riva_view(&downloads);
    assert!(view.downloading);
    assert_eq!(view.percent, 0);

    assert!(!downloads.accept(download_update(
        DownloadPhase::Downloading,
        150,
        Some(100),
        None,
    )));
    let view = riva_view(&downloads);
    assert_eq!(view.percent, 100);
    assert_eq!(view.status_key, MessageKey::ModelDownloadProgress);
    assert_eq!(view.action_key, MessageKey::ModelDownloadCancel);
    assert!(view.progress_visible);
    assert_eq!(downloads.activate("riva"), Some(DownloadCommand::Cancel));

    downloads.accept(download_update(DownloadPhase::Downloading, 50, None, None));
    assert_eq!(riva_view(&downloads).percent, 50);
}

#[test]
fn downloads_expose_failures_retries_and_ready_refresh() {
    let mut downloads = downloads_with_entry();
    fail_download(&mut downloads, DownloadFailure::Corrupted);
    let view = riva_view(&downloads);
    assert!(view.failed);
    assert!(!view.progress_visible);
    assert_eq!(view.status_key, MessageKey::ModelDownloadFailedCorrupted);
    assert_eq!(view.action_key, MessageKey::ModelDownloadRetry);

    fail_download(&mut downloads, DownloadFailure::Cancelled);
    let view = riva_view(&downloads);
    assert!(!view.failed);
    assert_eq!(view.status_key, MessageKey::ModelDownloadMissing);

    assert!(downloads.accept(download_update(DownloadPhase::Ready, 100, Some(100), None,)));
    assert!(!riva_view(&downloads).visible);

    downloads.reject_start("riva");
    assert_eq!(
        riva_view(&downloads).status_key,
        MessageKey::ModelDownloadFailedNetwork
    );
}

#[test]
fn download_events_are_isolated_by_model_and_sizes_use_catalog_bytes() {
    let mut downloads = DownloadsModel::default();
    let mut riva = download_entry();
    riva.total_bytes = 2_759_302_368;
    let diarization = ModelDownloadState {
        model: "diarization",
        kind: ModelKind::Diarization,
        engine: None,
        installed: false,
        downloadable: true,
        downloading: false,
        total_bytes: 107_012_128,
    };
    downloads.replace_models(vec![riva, diarization]);
    downloads.accept(DownloadUpdate {
        model: "diarization",
        phase: DownloadPhase::Downloading,
        received_bytes: 25,
        total_bytes: Some(100),
        reason: None,
    });

    let riva_view = downloads
        .view_for_engine(TranslationEngine::Riva, MessageKey::ModelDownloadMissing)
        .unwrap();
    assert_eq!(riva_view.percent, 0);
    assert_eq!(riva_view.action_key, MessageKey::ModelDownloadAction);
    assert_eq!(riva_view.action_size.as_deref(), Some("2.8"));
    let diarization_view = downloads
        .view_for_kind(ModelKind::Diarization, MessageKey::DictationModelMissing)
        .unwrap();
    assert_eq!(diarization_view.percent, 25);
    assert!(diarization_view.downloading);
}

fn check(installable: bool) -> UpdateCheck {
    UpdateCheck {
        current: "0.2.0".into(),
        latest: "0.3.0".into(),
        available: true,
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

#[test]
fn update_action_installs_only_on_click_and_shows_progress() {
    let mut action = UpdateActionModel::default();
    assert_eq!(
        action.activate(&check(false)),
        UpdateCommand::OpenReleasesPage
    );
    assert!(action.progress().is_none());

    assert_eq!(action.activate(&check(true)), UpdateCommand::Install);
    assert_eq!(action.activate(&check(true)), UpdateCommand::None);
    action.accept(update_progress(UpdatePhase::Downloading, 45, 100));
    let view = action.view(&check(true));
    assert_eq!(view.percent, Some(45));
    assert_eq!(view.message_key, Some(MessageKey::UpdateDownloading));

    action.accept(update_progress(UpdatePhase::Installing, 100, 100));
    assert_eq!(
        action.view(&check(true)).message_key,
        Some(MessageKey::UpdateInstalling)
    );
}

#[test]
fn update_action_failure_falls_back_to_release_page() {
    let mut action = UpdateActionModel::default();
    assert_eq!(action.activate(&check(true)), UpdateCommand::Install);
    let mut failed = update_progress(UpdatePhase::Failed, 0, 0);
    failed.reason = Some(crate::updater::UpdateFailure::Signature);
    action.accept(failed);
    let view = action.view(&check(true));
    assert!(view.failed);
    assert_eq!(view.message_key, Some(MessageKey::UpdateInstallFailed));
    assert_eq!(
        action.activate(&check(true)),
        UpdateCommand::OpenReleasesPage
    );

    assert_eq!(
        action.activate(&check(true)),
        UpdateCommand::OpenReleasesPage
    );
    let mut retried = UpdateActionModel::default();
    assert_eq!(retried.activate(&check(true)), UpdateCommand::Install);
    retried.reject_install();
    assert!(retried.view(&check(true)).failed);
}

#[test]
fn update_settings_cover_checking_results_and_failures() {
    let mut updates = UpdatesModel::default();
    assert_eq!(updates.status_message(), (MessageKey::UpdateHint, None));
    assert!(updates.begin_check());
    assert!(!updates.begin_check());
    assert_eq!(updates.status_message(), (MessageKey::UpdateChecking, None));
    assert!(updates.complete_check(Ok(check(false))));
    assert_eq!(
        updates.status_message(),
        (MessageKey::UpdateAvailable, Some("0.3.0"))
    );
    assert_eq!(updates.available_check(), Some(&check(false)));

    assert!(updates.begin_check());
    assert!(!updates.complete_check(Err(UpdateError::Network)));
    assert_eq!(updates.status_message(), (MessageKey::UpdateFailed, None));
}

#[test]
fn update_recheck_resets_failed_and_progress_action_state() {
    let mut updates = UpdatesModel::default();
    updates
        .action_mut()
        .accept(update_progress(UpdatePhase::Failed, 0, 0));
    assert!(updates.action().view(&check(true)).failed);

    assert!(updates.begin_check());
    assert!(updates.action().progress().is_none());
    assert!(!updates.action().view(&check(true)).failed);
    assert_eq!(updates.action().view(&check(true)).message_key, None);

    assert!(updates.complete_check(Ok(check(true))));
    assert_eq!(
        updates.action_mut().activate(&check(true)),
        UpdateCommand::Install
    );
    updates
        .action_mut()
        .accept(update_progress(UpdatePhase::Downloading, 45, 100));
    assert!(updates.action().progress().is_some());

    assert!(updates.begin_check());
    assert!(updates.action().progress().is_none());
    assert_eq!(updates.action().view(&check(true)).message_key, None);
}

#[test]
fn update_notice_disable_cancels_only_pending_completion() {
    let mut pending = UpdateNoticeModel::default();
    assert!(pending.begin_check(true));
    assert!(!pending.begin_check(false));
    assert!(!pending.complete_check(Ok(check(false))));
    assert_eq!(pending.visible_check(), None);
    assert!(!pending.begin_check(true));

    let mut delivered = UpdateNoticeModel::default();
    assert!(delivered.begin_check(true));
    assert!(delivered.complete_check(Ok(check(false))));
    assert!(!delivered.begin_check(false));
    assert_eq!(delivered.visible_check(), Some(&check(false)));

    let mut late = check(false);
    late.latest = "0.4.0".into();
    assert!(!delivered.complete_check(Ok(late)));
    assert_eq!(delivered.visible_check(), Some(&check(false)));
}

#[test]
fn update_notice_checks_once_and_dismisses_permanently() {
    let mut notice = UpdateNoticeModel::default();
    assert!(!notice.begin_check(false));
    assert!(notice.begin_check(true));
    assert!(!notice.begin_check(true));
    notice.complete_check(Ok(check(false)));
    assert_eq!(notice.visible_check(), Some(&check(false)));
    notice.dismiss();
    assert_eq!(notice.visible_check(), None);
    assert!(!notice.begin_check(true));

    let mut silent = UpdateNoticeModel::default();
    assert!(silent.begin_check(true));
    silent.complete_check(Err(UpdateError::Network));
    assert_eq!(silent.visible_check(), None);
    assert!(!silent.begin_check(true));
}

fn dictation_event(phase: &'static str) -> DictationEventPayload {
    DictationEventPayload {
        phase,
        text: String::new(),
        error: None,
        sound_cue: false,
        activation: ActivationMode::Hold,
        show_live_text: true,
        max_duration_secs: 60,
    }
}

#[test]
fn dictation_recording_text_finalizing_and_idle_transitions_match_frontend() {
    let mut model = DictationModel::default();
    model.accept(&dictation_event("recording"));
    assert!(!model.frame().hidden());
    assert_eq!(model.frame().phase.as_str(), "recording");
    assert_eq!(model.frame().hint, Some(MessageKey::DictationListening));
    assert_eq!(
        model.frame().display_text(Locale::En),
        "Listening… release to send"
    );
    assert_eq!(model.frame().elapsed, "0:00");

    let mut text = dictation_event("recording");
    text.text = "hello there".into();
    model.accept(&text);
    assert_eq!(model.frame().text, "hello there");

    let mut finalizing = dictation_event("finalizing");
    finalizing.text = "hello there".into();
    model.accept(&finalizing);
    assert_eq!(model.frame().phase.as_str(), "finalizing");
    assert_eq!(model.frame().hint, Some(MessageKey::DictationFinalizing));
    model.tick(65_000);
    assert_eq!(model.frame().elapsed, "1:05");

    model.accept(&dictation_event("idle"));
    assert!(model.frame().hidden());
    assert_eq!(model.frame().phase.as_str(), "idle");
    assert_eq!(model.frame().elapsed, "");
}

#[test]
fn dictation_elapsed_is_injected_rounded_and_runs_in_both_modes() {
    for activation in [ActivationMode::Hold, ActivationMode::Toggle] {
        let mut model = DictationModel::default();
        let mut event = dictation_event("recording");
        event.activation = activation;
        model.accept(&event);
        model.tick(499);
        assert_eq!(model.frame().elapsed, "0:00");
        model.tick(500);
        assert_eq!(model.frame().elapsed, "0:01");
        model.tick(65_000);
        assert_eq!(model.frame().elapsed, "1:05");
        model.accept(&dictation_event("idle"));
        model.tick(90_000);
        assert_eq!(model.frame().elapsed, "");
    }
}

#[test]
fn dictation_compact_errors_cues_and_literal_text_match_frontend() {
    let mut model = DictationModel::default();
    let mut recording = dictation_event("recording");
    recording.sound_cue = true;
    recording.show_live_text = false;
    recording.text = "<b>bold</b>".into();
    let effects = model.accept(&recording);
    assert_eq!(effects.sound_cue_hz, Some(880));
    assert!(!model.frame().hidden());
    assert!(model.frame().compact);
    assert_eq!(model.frame().text, "");
    assert_eq!(model.frame().display_text(Locale::En), "");

    recording.show_live_text = true;
    model.accept(&recording);
    assert!(!model.frame().compact);
    assert_eq!(model.frame().text, "<b>bold</b>");
    assert_eq!(model.frame().display_text(Locale::En), "<b>bold</b>");

    let mut idle = dictation_event("idle");
    idle.sound_cue = true;
    idle.error = Some("dictation-paste");
    let effects = model.accept(&idle);
    assert_eq!(effects.sound_cue_hz, Some(440));
    assert_eq!(effects.error, Some(MessageKey::ErrDictationPaste));
    assert_eq!(
        dictation_error_key("dictation-capture"),
        MessageKey::ErrDictationMic
    );
    assert_eq!(
        dictation_error_key("dictation-model"),
        MessageKey::ErrDictationModel
    );
    assert_eq!(
        dictation_error_key("dictation-translate"),
        MessageKey::ErrDictationTranslate
    );
    assert_eq!(dictation_error_key("unknown"), MessageKey::ErrDictation);

    let mut ignored = dictation_event("recording");
    ignored.error = Some("dictation-paste");
    assert_eq!(model.accept(&ignored).error, None);
}

#[test]
fn indicator_places_above_below_and_falls_back_to_bottom() {
    let screen = Rect::new(0, 0, 1920, 1040);
    assert_eq!(
        place_indicator(Rect::new(500, 800, 800, 160), screen, false),
        Rect::new(580, 652, 640, 136)
    );
    assert_eq!(
        place_indicator(Rect::new(0, 0, 800, 160), screen, false),
        Rect::new(80, 172, 640, 136)
    );
    assert_eq!(
        place_indicator(Rect::new(0, 20, 400, 160), Rect::new(0, 0, 400, 200), false,),
        Rect::new(12, 64, 376, 136)
    );

    let negative = Rect::new(-400, -100, 400, 500);
    let panel = place_indicator(Rect::new(-380, 150, 320, 160), negative, false);
    assert_eq!(panel, Rect::new(-400, 2, 376, 136));
    assert_eq!(place_indicator(panel, negative, true).width, 112.0);
}

#[test]
fn indicator_snapshots_use_strict_revisions_and_display_only_events() {
    let controller = crate::ipc::Controller::default();
    let mut live = dictation_event("recording");
    live.text = "Live dictation".into();
    live.sound_cue = true;
    live.error = Some("dictation-paste");
    let first = IndicatorSnapshot::publish(None, &controller, live.clone());
    let second = IndicatorSnapshot::publish(Some(&first), &controller, live);
    assert_eq!(first.revision, 1);
    assert_eq!(second.revision, 2);
    assert_eq!(second.ui_language, "en");

    let mut model = DictationModel::default();
    assert!(model.accept_snapshot(&second).is_some());
    assert_eq!(model.frame().text, "Live dictation");
    assert_eq!(model.indicator_language(), Some("en"));
    assert!(model.accept_snapshot(&first).is_none());
    assert_eq!(model.frame().text, "Live dictation");
    assert!(model.accept_snapshot(&second).is_none());

    let mut idle = second.clone();
    idle.revision = 3;
    idle.event.phase = "idle";
    idle.event.sound_cue = true;
    idle.event.error = Some("dictation-paste");
    let effects = model.accept_snapshot(&idle).unwrap();
    assert_eq!(effects.sound_cue_hz, None);
    assert_eq!(effects.error, None);
    assert!(model.frame().hidden());

    let portuguese = crate::ipc::Controller::default();
    portuguese
        .update(&IgnoreEvents, |state| {
            state.set_ui_language(UiLanguage::from_tag("pt-br").unwrap())
        })
        .unwrap();
    let published = IndicatorSnapshot::publish(None, &portuguese, dictation_event("idle"));
    assert_eq!(published.ui_language, "pt-br");
}
