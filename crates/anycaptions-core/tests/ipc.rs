use anycaptions_core::{
    domain::{AppSnapshot, Appearance, CaptionMode},
    ipc::{CommandError, Controller, ShortcutRegistry, StateEvents},
};
use serde_json::{Value, json};

struct Events;

impl StateEvents for Events {
    fn state_changed(&self, _: &AppSnapshot) -> Result<(), CommandError> {
        Ok(())
    }
}

struct Shortcuts;

impl ShortcutRegistry for Shortcuts {
    fn register(&self, _: &str) -> Result<(), CommandError> {
        Err(CommandError::ShortcutUnavailable)
    }

    fn unregister(&self, _: &str) -> Result<(), CommandError> {
        Err(CommandError::ShortcutUnavailable)
    }
}

/// Runtime-neutral equivalent of the legacy Tauri mock IPC harness. The same
/// window, origin and command gates feed the core controller without WebView2.
struct App {
    controller: Controller,
    events: Events,
    shortcuts: Shortcuts,
}

fn app() -> App {
    App {
        controller: Controller::default(),
        events: Events,
        shortcuts: Shortcuts,
    }
}

fn command_error(error: CommandError) -> Value {
    serde_json::to_value(error).unwrap()
}

fn invalid_request(error: serde_json::Error) -> Value {
    json!({"kind":"invalid-request","code":error.to_string()})
}

fn command(result: Result<AppSnapshot, CommandError>) -> Result<AppSnapshot, Value> {
    result.map_err(command_error)
}

impl App {
    fn invoke(
        &self,
        window: &str,
        command_name: &str,
        body: Value,
        url: &str,
    ) -> Result<Value, Value> {
        if window != "overlay" || url != "http://tauri.localhost" {
            return Err(command_error(CommandError::UnauthorizedWindow));
        }
        let result = match command_name {
            "get_state" => command(self.controller.snapshot()),
            "set_caption_mode" => {
                serde_json::from_value::<CaptionMode>(body["request"]["mode"].clone())
                    .map_err(invalid_request)
                    .and_then(|mode| {
                        command(
                            self.controller
                                .update(&self.events, |state| state.set_mode(mode)),
                        )
                    })
            }
            "start_capture" => command(self.controller.start_session(&self.events)),
            "set_appearance" => serde_json::from_value::<Appearance>(body["request"].clone())
                .map_err(invalid_request)
                .and_then(|appearance| {
                    command(
                        self.controller
                            .update(&self.events, |state| state.set_appearance(appearance)),
                    )
                }),
            "set_shortcut" => serde_json::from_value::<String>(body["request"]["shortcut"].clone())
                .map_err(invalid_request)
                .and_then(|shortcut| {
                    command(self.controller.change_shortcut(
                        &self.shortcuts,
                        &self.events,
                        &shortcut,
                    ))
                }),
            _ => Err(command_error(CommandError::UnauthorizedWindow)),
        }?;
        serde_json::to_value(result).map_err(invalid_request)
    }
}

#[test]
fn local_overlay_can_query_and_change_mode_but_cannot_start_without_resources() {
    let app = app();
    let url = "http://tauri.localhost";
    let state = app.invoke("overlay", "get_state", json!({}), url).unwrap();
    assert_eq!(state["capture"], "stopped");
    assert_eq!(state["windowMode"], "edit");
    let changed = app
        .invoke(
            "overlay",
            "set_caption_mode",
            json!({"request":{"mode":"translate-pt-br"}}),
            url,
        )
        .unwrap();
    assert_eq!(changed["mode"], "translate-pt-br");
    assert_eq!(changed["sessionId"], 1);
    assert!(
        app.invoke("overlay", "start_capture", json!({}), url)
            .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "set_appearance",
            json!({"request":{"fontSize":100,"color":"#FFFFFF","partialColor":"#B8B8B8"}}),
            url
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "set_caption_mode",
            json!({"request":{"mode":"en-pt"}}),
            url
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "set_shortcut",
            json!({"request":{"shortcut":"Win+L"}}),
            url
        )
        .is_err()
    );
}

#[test]
fn acl_denies_other_windows_remote_origins_and_ungranted_commands() {
    let app = app();
    assert!(
        app.invoke("other", "get_state", json!({}), "http://tauri.localhost")
            .is_err()
    );
    assert!(
        app.invoke("overlay", "get_state", json!({}), "https://example.com")
            .is_err()
    );
    assert!(
        app.invoke(
            "other",
            "fit_settings_window",
            json!({}),
            "http://tauri.localhost"
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "other",
            "fit_caption_window",
            json!({}),
            "http://tauri.localhost"
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "fit_settings_window",
            json!({}),
            "https://example.com"
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "plugin:window|set_always_on_top",
            json!({"value":false}),
            "http://tauri.localhost"
        )
        .is_err()
    );
    assert!(
        app.invoke(
            "overlay",
            "plugin:event|emit",
            json!({"event":"app-state-changed","payload":{}}),
            "http://tauri.localhost"
        )
        .is_err()
    );
}
