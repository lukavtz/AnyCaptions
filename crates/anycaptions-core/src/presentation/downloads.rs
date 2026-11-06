use crate::{
    domain::TranslationEngine,
    models::{
        ModelKind,
        download::{DownloadFailure, DownloadPhase, DownloadUpdate},
    },
};

use super::i18n::MessageKey;

/// Authoritative catalog state for one downloadable model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDownloadState {
    pub model: &'static str,
    pub kind: ModelKind,
    pub engine: Option<TranslationEngine>,
    pub installed: bool,
    pub downloadable: bool,
    pub downloading: bool,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadCommand {
    Start(&'static str),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadView {
    pub model: &'static str,
    pub visible: bool,
    pub downloading: bool,
    pub failed: bool,
    pub progress_visible: bool,
    pub percent: u8,
    pub status_key: MessageKey,
    pub action_key: MessageKey,
    /// Replacement for `{size}` in the ordinary download action.
    pub action_size: Option<String>,
}

/// Shared catalog/event state used by both translation and diarization rows.
#[derive(Default)]
pub struct DownloadsModel {
    models: Vec<ModelDownloadState>,
    update: Option<DownloadUpdate>,
}

impl DownloadsModel {
    pub fn models(&self) -> &[ModelDownloadState] {
        &self.models
    }

    pub fn replace_models(&mut self, models: Vec<ModelDownloadState>) {
        self.models = models;
    }

    pub fn update(&self) -> Option<&DownloadUpdate> {
        self.update.as_ref()
    }

    pub fn set_update(&mut self, update: Option<DownloadUpdate>) {
        self.update = update;
    }

    /// Every event replaces the shared event. `true` requests a catalog
    /// refresh because an installation became ready.
    pub fn accept(&mut self, update: DownloadUpdate) -> bool {
        let ready = update.phase == DownloadPhase::Ready;
        self.update = Some(update);
        ready
    }

    /// Applies the same optimistic transition as the download button. Cancels
    /// are left to the backend event; starting at zero is immediate.
    pub fn activate(&mut self, model: &str) -> Option<DownloadCommand> {
        let entry = self.models.iter().find(|entry| entry.model == model)?;
        let event = self.event_for(entry.model);
        let downloading = is_downloading(entry, event);
        if downloading {
            return Some(DownloadCommand::Cancel);
        }
        let model = entry.model;
        let total_bytes = entry.total_bytes;
        self.update = Some(DownloadUpdate {
            model,
            phase: DownloadPhase::Downloading,
            received_bytes: 0,
            total_bytes: Some(total_bytes),
            reason: None,
        });
        Some(DownloadCommand::Start(model))
    }

    /// A rejected start command has the frontend's network-failure fallback.
    pub fn reject_start(&mut self, model: &'static str) {
        self.update = Some(DownloadUpdate {
            model,
            phase: DownloadPhase::Failed,
            received_bytes: 0,
            total_bytes: None,
            reason: Some(DownloadFailure::Network),
        });
    }

    pub fn view_for_engine(
        &self,
        engine: TranslationEngine,
        missing_key: MessageKey,
    ) -> Option<DownloadView> {
        self.view_for(|entry| entry.engine == Some(engine), missing_key)
    }

    pub fn view_for_kind(&self, kind: ModelKind, missing_key: MessageKey) -> Option<DownloadView> {
        self.view_for(|entry| entry.kind == kind, missing_key)
    }

    fn view_for(
        &self,
        matches: impl Fn(&ModelDownloadState) -> bool,
        missing_key: MessageKey,
    ) -> Option<DownloadView> {
        self.models
            .iter()
            .find(|entry| matches(entry))
            .map(|entry| self.view(entry, missing_key))
    }

    fn event_for(&self, model: &str) -> Option<&DownloadUpdate> {
        self.update.as_ref().filter(|update| update.model == model)
    }

    fn view(&self, entry: &ModelDownloadState, missing_key: MessageKey) -> DownloadView {
        let event = self.event_for(entry.model);
        let downloading = is_downloading(entry, event);
        let failed = is_failed(event);
        let visible = is_visible(entry, event);
        let (action_key, action_size) = download_action(entry, downloading, failed);
        DownloadView {
            model: entry.model,
            visible,
            downloading,
            failed,
            progress_visible: visible && downloading,
            percent: download_percent(entry, event),
            status_key: download_status(event, downloading, failed, missing_key),
            action_key,
            action_size,
        }
    }
}

fn is_downloading(entry: &ModelDownloadState, event: Option<&DownloadUpdate>) -> bool {
    event.map_or(entry.downloading, |event| {
        event.phase == DownloadPhase::Downloading
    })
}

fn download_percent(entry: &ModelDownloadState, event: Option<&DownloadUpdate>) -> u8 {
    let total = event.map_or(entry.total_bytes, |event| {
        event.total_bytes.unwrap_or(entry.total_bytes)
    });
    if total == 0 {
        return 0;
    }
    let received = event.map_or(0, |event| event.received_bytes);
    ((u128::from(received) * 100) / u128::from(total)).min(100) as u8
}

fn is_failed(event: Option<&DownloadUpdate>) -> bool {
    event.is_some_and(|event| {
        event.phase == DownloadPhase::Failed && event.reason != Some(DownloadFailure::Cancelled)
    })
}

fn is_visible(entry: &ModelDownloadState, event: Option<&DownloadUpdate>) -> bool {
    entry.downloadable
        && !entry.installed
        && event.is_none_or(|event| event.phase != DownloadPhase::Ready)
}

fn download_status(
    event: Option<&DownloadUpdate>,
    downloading: bool,
    failed: bool,
    missing_key: MessageKey,
) -> MessageKey {
    if downloading {
        return MessageKey::ModelDownloadProgress;
    }
    if !failed {
        return missing_key;
    }
    match event.and_then(|event| event.reason) {
        Some(DownloadFailure::Disk) => MessageKey::ModelDownloadFailedDisk,
        Some(DownloadFailure::Corrupted) => MessageKey::ModelDownloadFailedCorrupted,
        _ => MessageKey::ModelDownloadFailedNetwork,
    }
}

fn download_action(
    entry: &ModelDownloadState,
    downloading: bool,
    failed: bool,
) -> (MessageKey, Option<String>) {
    if downloading {
        return (MessageKey::ModelDownloadCancel, None);
    }
    if failed {
        return (MessageKey::ModelDownloadRetry, None);
    }
    size_action(entry.total_bytes)
}

fn size_action(bytes: u64) -> (MessageKey, Option<String>) {
    if bytes >= 1_000_000_000 {
        (
            MessageKey::ModelDownloadAction,
            Some(format!("{:.1}", bytes as f64 / 1e9)),
        )
    } else {
        (
            MessageKey::ModelDownloadActionMb,
            Some(((bytes as f64 / 1e6).round() as u64).to_string()),
        )
    }
}
