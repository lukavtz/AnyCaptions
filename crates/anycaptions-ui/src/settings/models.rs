use anycaptions_core::{
    models::{ModelKind, catalog::NEMOTRON_STREAMING_EN_Q8, download::DownloadUpdate},
    presentation::{
        MessageKey,
        downloads::{DownloadView, ModelDownloadState},
    },
};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, WeakEntity, Window, div, prelude::*,
};

use crate::{
    theme,
    widgets::{Button, Progress},
};

use super::{ControlFocusExt, SettingsHostAction, SettingsView, focus_list};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadRow {
    EnglishAsr,
    Translation,
    Diarization,
}

pub(crate) struct DownloadControls {
    pub(super) action: Entity<Button>,
    progress: Entity<Progress>,
}

impl DownloadControls {
    fn new(
        view: &SettingsView,
        row: DownloadRow,
        download: Option<&DownloadView>,
        weak: WeakEntity<SettingsView>,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let visible = download.is_some_and(|download| download.visible);
        let label =
            download.map_or_else(String::new, |download| view.download_action_label(download));
        let fraction = download.map_or(0.0, |download| f32::from(download.percent) / 100.0);
        let progress_label = view.download_progress_label(row).unwrap_or_default();
        let action = cx.new(|cx| {
            Button::new(label, cx)
                .disabled(!visible)
                .on_press(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.activate_download(row, window, cx);
                    });
                })
        });
        let progress = cx.new(|_| Progress::new(fraction).label(progress_label));
        Self { action, progress }
    }
}

pub(super) struct ModelControls {
    pub(super) english_asr: DownloadControls,
    pub(super) translation: DownloadControls,
    pub(super) diarization: DownloadControls,
}

impl ModelControls {
    pub(super) fn new(
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        _window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let english_asr_view = view.download_view(DownloadRow::EnglishAsr);
        let translation_view = view.download_view(DownloadRow::Translation);
        let diarization_view = view.download_view(DownloadRow::Diarization);
        Self {
            english_asr: DownloadControls::new(
                view,
                DownloadRow::EnglishAsr,
                english_asr_view.as_ref(),
                weak.clone(),
                cx,
            ),
            translation: DownloadControls::new(
                view,
                DownloadRow::Translation,
                translation_view.as_ref(),
                weak.clone(),
                cx,
            ),
            diarization: DownloadControls::new(
                view,
                DownloadRow::Diarization,
                diarization_view.as_ref(),
                weak,
                cx,
            ),
        }
    }

    fn row(&self, row: DownloadRow) -> &DownloadControls {
        match row {
            DownloadRow::EnglishAsr => &self.english_asr,
            DownloadRow::Translation => &self.translation,
            DownloadRow::Diarization => &self.diarization,
        }
    }
}

impl_focus_handles!(
    ModelControls,
    [english_asr.action, translation.action, diarization.action]
);

impl SettingsView {
    completion_method!(accept_models(models: Vec<ModelDownloadState>), |view| {
        view.downloads.replace_models(models);
    });

    pub fn accept_download(
        &mut self,
        update: DownloadUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            return;
        }
        let focused_row = [
            DownloadRow::EnglishAsr,
            DownloadRow::Translation,
            DownloadRow::Diarization,
        ]
        .into_iter()
        .find(|row| {
            self.controls()
                .models
                .row(*row)
                .action
                .control_focused(window, cx)
        });
        let ready = self.downloads.accept(update);
        if ready {
            self.emit(SettingsHostAction::RefreshModels);
        }
        self.rebuild_controls(window, cx);
        if ready
            && focused_row.is_some_and(|row| {
                self.download_view(row)
                    .is_none_or(|download| !download.visible)
            })
        {
            let handle = self.focus_handle(cx);
            window.focus(&handle);
        }
    }

    completion_method!(reject_download_start(model: &'static str), |view| {
        view.downloads.reject_start(model);
    });

    pub(super) fn activate_download(
        &mut self,
        row: DownloadRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            return;
        }
        let Some(download) = self.download_view(row).filter(|download| download.visible) else {
            return;
        };
        if let Some(command) = self.downloads.activate(download.model) {
            self.emit(SettingsHostAction::Download(command));
            self.defer_rebuild(window, cx);
        }
    }

    pub fn download_view(&self, row: DownloadRow) -> Option<DownloadView> {
        match row {
            DownloadRow::EnglishAsr => self.downloads.view_for_model(
                NEMOTRON_STREAMING_EN_Q8.id,
                MessageKey::EnglishAsrModelMissing,
            ),
            DownloadRow::Translation => self.downloads.view_for_engine(
                self.application.snapshot().translation_engine,
                MessageKey::ModelDownloadMissing,
            ),
            DownloadRow::Diarization => self.downloads.view_for_kind(
                ModelKind::Diarization,
                MessageKey::SpeakerLabelsModelMissing,
            ),
        }
    }

    pub fn download_status_label(&self, download: &DownloadView) -> String {
        self.replacement(
            download.status_key,
            "{percent}",
            &download.percent.to_string(),
        )
    }

    pub fn download_progress_label(&self, row: DownloadRow) -> Option<String> {
        self.download_view(row)
            .filter(|download| download.progress_visible)
            .map(|download| self.download_status_label(&download))
    }

    pub fn download_action_label(&self, download: &DownloadView) -> String {
        download.action_size.as_ref().map_or_else(
            || self.text(download.action_key).to_owned(),
            |size| self.replacement(download.action_key, "{size}", size),
        )
    }
}

pub(super) fn render_download(view: &mut SettingsView, row: DownloadRow) -> impl IntoElement {
    let controls = view.controls().models.row(row);
    let action = controls.action.clone();
    let progress = controls.progress.clone();
    let download = view.download_view(row);
    let visible = download.as_ref().is_some_and(|download| download.visible);
    let progress_visible = download
        .as_ref()
        .is_some_and(|download| download.progress_visible);
    let status = download
        .as_ref()
        .map_or_else(String::new, |download| view.download_status_label(download));
    let (row_id, status_id, action_id, progress_id) = match row {
        DownloadRow::EnglishAsr => (
            "english-asr-download",
            "english-asr-download-status",
            "english-asr-download-action",
            "english-asr-download-progress",
        ),
        DownloadRow::Translation => (
            "model-download",
            "model-download-status",
            "model-download-action",
            "model-download-progress",
        ),
        DownloadRow::Diarization => (
            "diarization-download",
            "diarization-download-status",
            "diarization-download-action",
            "diarization-download-progress",
        ),
    };

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .id(row_id)
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .py_2()
                .when(!visible, |row| row.hidden())
                .child(
                    div()
                        .id(status_id)
                        .text_sm()
                        .text_color(theme::MUTED)
                        .child(status),
                )
                .child(div().id(action_id).child(action)),
        )
        .child(
            div()
                .id(progress_id)
                .when(!progress_visible, |progress| progress.hidden())
                .child(progress),
        )
}
