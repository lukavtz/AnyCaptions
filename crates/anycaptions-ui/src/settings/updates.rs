use anycaptions_core::{
    ipc::CommandError,
    presentation::{
        MessageKey,
        application::Action,
        updates::{UpdateActionState, UpdateActionView, UpdateCommand, UpdateStatus},
    },
    update::{UpdateCheck, UpdateError},
    updater::UpdateProgress,
};
use gpui::{App, Context, Entity, FocusHandle, IntoElement, WeakEntity, Window, div, prelude::*};

use super::{ButtonAnchor, ControlFocusExt, SettingsHostAction, SettingsView, focus_list};
use crate::{
    theme,
    widgets::{Button, Progress, Toggle},
};

pub(super) struct UpdateControls {
    pub(super) check_now: Entity<Button>,
    startup_check: Entity<Toggle>,
    pub(super) download: Entity<Button>,
    progress: Entity<Progress>,
    download_visible: bool,
    progress_visible: bool,
    pub(super) credit: Entity<ButtonAnchor>,
}

impl UpdateControls {
    pub(super) fn new(
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        _window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let check_weak = weak.clone();
        let check_now = cx.new(|cx| {
            Button::new(view.text(MessageKey::UpdateCheckNow), cx)
                .disabled(matches!(view.updates.status(), &UpdateStatus::Checking))
                .on_press(move |window, cx| {
                    let _ = check_weak.update(cx, |view, cx| view.check_updates(window, cx));
                })
        });

        let startup_weak = weak.clone();
        let startup_check = cx.new(|cx| {
            Toggle::new(
                view.text(MessageKey::UpdateCheckOnStart),
                view.application.snapshot().update_check,
                cx,
            )
            .on_change(move |enabled, window, cx| {
                let _ =
                    startup_weak.update(cx, |view, cx| view.set_update_check(enabled, window, cx));
            })
        });

        let action = view.update_action_view();
        let progress_visible = action
            .as_ref()
            .is_some_and(|action| matches!(action.state, UpdateActionState::Progress(_)));
        let download_visible = action.is_some() && !progress_visible;
        let download_label = action.as_ref().map_or_else(String::new, |action| {
            match action.state {
                UpdateActionState::Install => view.text(MessageKey::UpdateInstall),
                UpdateActionState::OpenReleasesPage => view.text(MessageKey::UpdateDownload),
                UpdateActionState::Progress(_) => "",
            }
            .to_owned()
        });
        let progress_label = action
            .as_ref()
            .filter(|_| progress_visible)
            .and_then(|action| action_status_message(view, action))
            .unwrap_or_default();
        let fraction = action
            .as_ref()
            .and_then(|action| action.percent)
            .unwrap_or(0) as f32
            / 100.0;
        let action_weak = weak.clone();
        let download = cx.new(|cx| {
            Button::new(download_label, cx)
                .disabled(!download_visible)
                .on_press(move |window, cx| {
                    let _ = action_weak.update(cx, |view, cx| view.activate_update(window, cx));
                })
        });
        let progress = cx.new(|_| Progress::new(fraction).label(progress_label));

        let credit_weak = weak;
        let credit_button = cx.new(|cx| {
            Button::new(
                format!("{} EduardoViniciusJ", view.text(MessageKey::DevelopedBy)),
                cx,
            )
            .on_press(move |window, cx| {
                let _ = credit_weak.update(cx, |view, cx| view.open_project(window, cx));
            })
        });
        let credit = cx.new(|_| ButtonAnchor::new("developer-credit", credit_button));

        Self {
            check_now,
            startup_check,
            download,
            progress,
            download_visible,
            progress_visible,
            credit,
        }
    }
}

impl_focus_handles!(
    UpdateControls,
    [check_now, startup_check, download, credit,]
);

impl SettingsView {
    pub fn version_text(&self) -> String {
        format!("v{}", self.application.snapshot().version)
    }

    pub fn update_status_message(&self) -> String {
        if let Some(action) = self.update_action_view()
            && let Some(message) = action_status_message(self, &action)
        {
            return message;
        }
        check_status_message(self)
    }

    pub fn update_action_view(&self) -> Option<UpdateActionView> {
        self.updates
            .available_check()
            .map(|check| self.updates.action().view(check))
    }

    fallible_completion_method!(
        complete_update_check(result: Result<UpdateCheck, UpdateError>),
        |view| view.updates.complete_check(result)
    );

    pub fn restore_pending_update_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.complete_with(window, cx, |view| {
            view.updates.begin_check();
        });
    }

    completion_method!(accept_update_progress(progress: UpdateProgress), |view| {
        view.updates.action_mut().accept(progress);
    });

    completion_method!(reject_install(error: CommandError), |view| {
        view.updates.action_mut().reject_install();
        view.application.report(error);
    });

    fn rebuild_after_update_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let download_focused = self.controls().updates.download.control_focused(window, cx);
        self.rebuild_controls(window, cx);
        if download_focused {
            let handle = self.controls().updates.check_now.control_focus(cx);
            window.focus(&handle);
        }
    }

    fn defer_update_rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.defer_in(window, |view, window, cx| {
            view.rebuild_after_update_action(window, cx)
        });
    }

    pub(super) fn check_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open && self.updates.begin_check() {
            self.emit(SettingsHostAction::CheckForUpdates);
            self.defer_update_rebuild(window, cx);
        }
    }

    pub(super) fn activate_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let Some(check) = self.updates.available_check().cloned() else {
            return;
        };
        match self.updates.action_mut().activate(&check) {
            UpdateCommand::Install => {
                self.emit(SettingsHostAction::InstallUpdate);
                self.defer_update_rebuild(window, cx);
            }
            UpdateCommand::OpenReleasesPage => {
                self.emit(SettingsHostAction::OpenReleasesPage);
                cx.notify();
            }
            UpdateCommand::None => {}
        }
    }

    pub(super) fn set_update_check(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            return;
        }
        let action = Action::SetUpdateCheck(enabled);
        if self.application.try_begin(action.clone()) {
            self.emit(SettingsHostAction::Application(action));
        }
        // Toggle changes optimistically; rebuild confirmed state even when the
        // pending-action or same-value guard rejects the command.
        self.defer_rebuild(window, cx);
    }

    pub(super) fn open_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.emit(SettingsHostAction::OpenProjectPage);
            cx.notify();
        }
    }
}

fn check_status_message(view: &SettingsView) -> String {
    let (key, version) = view.updates.status_message();
    view.replacement(key, "{version}", version.unwrap_or(""))
}

fn action_status_message(view: &SettingsView, action: &UpdateActionView) -> Option<String> {
    let key = action.message_key?;
    Some(view.replacement(key, "{percent}", &action.percent.unwrap_or(0).to_string()))
}

pub(super) fn render(view: &SettingsView, _: &mut Context<SettingsView>) -> impl IntoElement {
    let controls = view.controls();
    let mut action_row = div().flex().flex_col().gap_2().child(
        div()
            .id("update-status")
            .text_color(theme::MUTED)
            .child(check_status_message(view)),
    );
    if controls.updates.progress_visible {
        action_row = action_row.child(
            div()
                .id("update-progress")
                .child(controls.updates.progress.clone()),
        );
    } else if controls.updates.download_visible {
        if view
            .update_action_view()
            .is_some_and(|action| action.failed)
        {
            action_row = action_row.child(
                div()
                    .id("update-download-failed")
                    .text_color(theme::MUTED)
                    .child(view.update_status_message()),
            );
        }
        action_row = action_row.child(
            div()
                .id("update-download")
                .child(controls.updates.download.clone()),
        );
    }

    div()
        .id("updates")
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .font_weight(gpui::FontWeight::BOLD)
                .child(view.text(MessageKey::Updates)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(view.text(MessageKey::AppVersion))
                        .child(
                            div()
                                .id("app-version")
                                .text_color(theme::SUBTLE)
                                .child(view.version_text()),
                        ),
                )
                .child(
                    div()
                        .id("update-check-now")
                        .child(controls.updates.check_now.clone()),
                ),
        )
        .child(
            div()
                .id("update-check")
                .child(controls.updates.startup_check.clone()),
        )
        .child(action_row)
}
