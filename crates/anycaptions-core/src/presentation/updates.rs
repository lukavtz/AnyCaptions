use crate::{
    update::{UpdateCheck, UpdateError},
    updater::{UpdatePhase, UpdateProgress},
};

use super::i18n::MessageKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCommand {
    Install,
    OpenReleasesPage,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateActionState {
    Install,
    OpenReleasesPage,
    Progress(UpdatePhase),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateActionView {
    pub state: UpdateActionState,
    pub failed: bool,
    pub percent: Option<u64>,
    pub message_key: Option<MessageKey>,
}

/// State local to one UpdateAction. Settings and the startup notice each own
/// an instance, just as the two React components do.
#[derive(Default)]
pub struct UpdateActionModel {
    progress: Option<UpdateProgress>,
    failed: bool,
}

impl UpdateActionModel {
    pub fn progress(&self) -> Option<&UpdateProgress> {
        self.progress.as_ref()
    }

    pub fn view(&self, check: &UpdateCheck) -> UpdateActionView {
        if let Some(progress) = &self.progress {
            let percent = (u128::from(progress.received_bytes) * 100
                / u128::from(progress.total_bytes.max(1)))
            .min(u128::from(u64::MAX)) as u64;
            return UpdateActionView {
                state: UpdateActionState::Progress(progress.phase),
                failed: false,
                percent: Some(percent),
                message_key: Some(if progress.phase == UpdatePhase::Downloading {
                    MessageKey::UpdateDownloading
                } else {
                    MessageKey::UpdateInstalling
                }),
            };
        }
        if self.failed {
            return UpdateActionView {
                state: UpdateActionState::OpenReleasesPage,
                failed: true,
                percent: None,
                message_key: Some(MessageKey::UpdateInstallFailed),
            };
        }
        UpdateActionView {
            state: if check.installable {
                UpdateActionState::Install
            } else {
                UpdateActionState::OpenReleasesPage
            },
            failed: false,
            percent: None,
            message_key: None,
        }
    }

    pub fn activate(&mut self, check: &UpdateCheck) -> UpdateCommand {
        if self.progress.is_some() {
            return UpdateCommand::None;
        }
        if !check.installable || self.failed {
            return UpdateCommand::OpenReleasesPage;
        }
        self.failed = false;
        self.progress = Some(UpdateProgress {
            phase: UpdatePhase::Downloading,
            received_bytes: 0,
            total_bytes: 0,
            reason: None,
        });
        UpdateCommand::Install
    }

    pub fn accept(&mut self, progress: UpdateProgress) {
        if progress.phase == UpdatePhase::Failed {
            self.progress = None;
            self.failed = true;
        } else {
            self.failed = false;
            self.progress = Some(progress);
        }
    }

    /// The host reports the original command error through ApplicationModel.
    pub fn reject_install(&mut self) {
        self.progress = None;
        self.failed = true;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Result(UpdateCheck),
    Failed,
}

/// On-demand Settings → Updates check state.
#[derive(Default)]
pub struct UpdatesModel {
    status: UpdateStatus,
    action: UpdateActionModel,
}

impl UpdatesModel {
    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn begin_check(&mut self) -> bool {
        if self.status == UpdateStatus::Checking {
            return false;
        }
        self.status = UpdateStatus::Checking;
        self.action = UpdateActionModel::default();
        true
    }

    pub fn complete_check(&mut self, result: Result<UpdateCheck, UpdateError>) -> bool {
        match result {
            Ok(check) => {
                self.status = UpdateStatus::Result(check);
                true
            }
            Err(_) => {
                self.status = UpdateStatus::Failed;
                false
            }
        }
    }

    pub fn status_message(&self) -> (MessageKey, Option<&str>) {
        match &self.status {
            UpdateStatus::Idle => (MessageKey::UpdateHint, None),
            UpdateStatus::Checking => (MessageKey::UpdateChecking, None),
            UpdateStatus::Failed => (MessageKey::UpdateFailed, None),
            UpdateStatus::Result(check) if check.available => {
                (MessageKey::UpdateAvailable, Some(&check.latest))
            }
            UpdateStatus::Result(check) => (MessageKey::UpdateLatest, Some(&check.current)),
        }
    }

    pub fn available_check(&self) -> Option<&UpdateCheck> {
        match &self.status {
            UpdateStatus::Result(check) if check.available => Some(check),
            _ => None,
        }
    }

    pub fn action(&self) -> &UpdateActionModel {
        &self.action
    }

    pub fn action_mut(&mut self) -> &mut UpdateActionModel {
        &mut self.action
    }
}

/// Startup update notice. The first enabled snapshot starts the only request;
/// failures are silent and dismissal lasts for this model's lifetime.
#[derive(Default)]
pub struct UpdateNoticeModel {
    check: Option<UpdateCheck>,
    dismissed: bool,
    started: bool,
    pending: bool,
    action: UpdateActionModel,
}

impl UpdateNoticeModel {
    pub fn begin_check(&mut self, enabled: bool) -> bool {
        if !enabled {
            self.pending = false;
            return false;
        }
        if self.started {
            return false;
        }
        self.started = true;
        self.pending = true;
        true
    }

    pub fn complete_check(&mut self, result: Result<UpdateCheck, UpdateError>) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;
        match result {
            Ok(check) => {
                self.check = Some(check);
                true
            }
            Err(_) => false,
        }
    }

    pub fn visible_check(&self) -> Option<&UpdateCheck> {
        self.check
            .as_ref()
            .filter(|check| check.available && !self.dismissed)
    }

    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    pub fn action(&self) -> &UpdateActionModel {
        &self.action
    }

    pub fn action_mut(&mut self) -> &mut UpdateActionModel {
        &mut self.action
    }
}
