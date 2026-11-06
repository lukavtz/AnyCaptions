macro_rules! widget_builders {
    (flag $name:ident) => {
        pub fn $name(mut self, value: bool) -> Self {
            self.$name = value;
            self
        }
    };
    (disabled) => {
        pub fn disabled(mut self, disabled: bool) -> Self {
            self.disabled = disabled;
            self.focus_handle = self.focus_handle.clone().tab_stop(!disabled);
            self
        }
    };
    (composed_disabled) => {
        pub fn disabled(mut self, disabled: bool) -> Self {
            self.state.set_disabled(disabled);
            self
        }
    };
    (getter $name:ident, $value:ty) => {
        pub fn $name(&self) -> $value {
            self.$name
        }
    };
    (change $name:ident, $value:ty) => {
        pub fn $name(
            mut self,
            handler: impl Fn($value, &mut gpui::Window, &mut gpui::App) + 'static,
        ) -> Self {
            self.$name = Some(std::rc::Rc::new(handler));
            self
        }
    };
    (option $name:ident) => {
        pub fn $name(mut self, value: impl Into<gpui::SharedString>) -> Self {
            self.$name = Some(value.into());
            self
        }
    };
    (option $method:ident, $field:ident, $value:ty) => {
        pub fn $method(mut self, value: impl Into<$value>) -> Self {
            self.$field = Some(value.into());
            self
        }
    };
    (handler $name:ident) => {
        pub fn $name(
            mut self,
            handler: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
        ) -> Self {
            self.$name = Some(std::rc::Rc::new(handler));
            self
        }
    };
}

macro_rules! impl_focus_handle {
    ($view:ident, state) => {
        impl gpui::Focusable for $view {
            fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
                self.state.focus_handle.clone()
            }
        }
    };
    ($view:ident) => {
        impl gpui::Focusable for $view {
            fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
                self.focus_handle.clone()
            }
        }
    };
}

macro_rules! impl_static_render {
    ($view:ident) => {
        impl gpui::Render for $view {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                self.render_element()
            }
        }
    };
}

mod button;
mod picker;
mod text_input;
mod tooltip;
mod value_controls;

pub use button::{Button, Toggle};
pub use picker::{Picker, PickerFooter, PickerOption};
pub use text_input::TextInput;
pub use tooltip::Tooltip;
pub use value_controls::{FocusScope, NumberInput, Progress, Slider};

use gpui::{App, KeyBinding, actions};

actions!(
    anycaptions_widgets,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Up,
        Down,
        Escape,
        Paste,
        Cut,
        Copy,
        Tab,
        TabPrev,
    ]
);

const KEY_CONTEXT: &str = "AnyCaptionsWidgets";

/// Register the shared keyboard contract once at application startup.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("up", Up, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Down, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Escape, Some(KEY_CONTEXT)),
        KeyBinding::new("tab", Tab, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", TabPrev, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(KEY_CONTEXT)),
    ]);
}

#[cfg(test)]
mod tests;
