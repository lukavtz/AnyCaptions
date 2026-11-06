use std::rc::Rc;

use gpui::{App, Context, FocusHandle, Render, SharedString, Window, div, prelude::*, px, rgb};

use super::KEY_CONTEXT;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

struct ControlState {
    label: SharedString,
    disabled: bool,
    focus_handle: FocusHandle,
}

impl ControlState {
    fn new<T: 'static>(label: impl Into<SharedString>, cx: &mut Context<T>) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
        }
    }

    fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
        self.focus_handle = self.focus_handle.clone().tab_stop(!disabled);
    }
}

macro_rules! control_constructor {
    ($view:ident; $($field:ident : $value:expr),* $(,)?) => {
        pub fn new(label: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
            Self {
                state: ControlState::new(label, cx),
                $($field: $value),*
            }
        }
    };
    ($view:ident, $param:ident : $type:ty; $($field:ident : $value:expr),* $(,)?) => {
        pub fn new(
            label: impl Into<SharedString>,
            $param: $type,
            cx: &mut Context<Self>,
        ) -> Self {
            Self {
                state: ControlState::new(label, cx),
                $($field: $value),*
            }
        }
    };
}

pub struct Button {
    state: ControlState,
    icon: Option<SharedString>,
    on_press: Option<PressHandler>,
}

impl Button {
    control_constructor!(Button; icon: None, on_press: None);

    widget_builders!(option icon);
    widget_builders!(composed_disabled);
    widget_builders!(handler on_press);

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.state.set_disabled(disabled);
        cx.notify();
    }

    fn press(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.disabled {
            return;
        }
        if let Some(handler) = self.on_press.clone() {
            handler(window, cx);
        }
    }
}

impl_focus_handle!(Button, state);

impl Render for Button {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self
            .state
            .focus_handle
            .clone()
            .tab_stop(!self.state.disabled);
        let mut element = div()
            .id("button")
            .key_context(KEY_CONTEXT)
            .track_focus(&focus_handle)
            .tab_index(0)
            .tab_stop(!self.state.disabled)
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x4b5563))
            .bg(if self.state.disabled {
                rgb(0x374151)
            } else {
                rgb(0x2563eb)
            })
            .text_color(rgb(0xf9fafb))
            .in_focus(|style| style.border_color(rgb(0xf9fafb)))
            .when_some(self.icon.clone(), |element, icon| element.child(icon))
            .child(self.state.label.clone());

        if !self.state.disabled {
            element = element
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.press(window, cx)));
        }
        element
    }
}

pub struct Toggle {
    state: ControlState,
    checked: bool,
    on_change: Option<ToggleHandler>,
}

impl Toggle {
    control_constructor!(Toggle, checked: bool; checked: checked, on_change: None);

    widget_builders!(composed_disabled);
    widget_builders!(change on_change, bool);

    widget_builders!(getter checked, bool);

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.disabled {
            return;
        }
        self.checked = !self.checked;
        cx.notify();
        if let Some(handler) = self.on_change.clone() {
            handler(self.checked, window, cx);
        }
    }
}

impl_focus_handle!(Toggle, state);

impl Render for Toggle {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let track_color = match (self.checked, self.state.disabled) {
            (true, false) => rgb(0x2563eb),
            (false, false) => rgb(0x4b5563),
            (true, true) => rgb(0x1e3a5f),
            (false, true) => rgb(0x30363d),
        };
        let focus_handle = self
            .state
            .focus_handle
            .clone()
            .tab_stop(!self.state.disabled);
        let mut element = div()
            .id("toggle")
            .key_context(KEY_CONTEXT)
            .track_focus(&focus_handle)
            .tab_index(0)
            .tab_stop(!self.state.disabled)
            .flex()
            .items_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(gpui::transparent_black())
            .in_focus(|style| style.border_color(rgb(0xf9fafb)))
            .child(
                div()
                    .w(px(30.))
                    .h(px(16.))
                    .p(px(2.))
                    .rounded_full()
                    .bg(track_color)
                    .flex()
                    .when(self.checked, |track| track.justify_end())
                    .child(div().w(px(12.)).h(px(12.)).rounded_full().bg(rgb(0xf9fafb))),
            )
            .child(self.state.label.clone());

        if !self.state.disabled {
            element = element
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx)));
        }
        element
    }
}
