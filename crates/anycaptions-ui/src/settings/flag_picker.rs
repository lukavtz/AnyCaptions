use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, ScrollHandle,
    SharedString, Window, anchored, deferred, div, img, prelude::*, px, rgb,
};

use crate::{
    assets, theme,
    widgets::{Down, End, Escape, Home, PickerOption, Tooltip, Up},
};

type SelectionHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// The shared Picker contract plus the frontend's embedded option flags.
/// Kept settings-local so the generic widget remains unchanged.
pub(super) struct FlagPicker {
    options: Vec<PickerOption>,
    value: SharedString,
    open: bool,
    disabled: bool,
    highlighted: Option<usize>,
    trigger_focus: FocusHandle,
    option_focus: Vec<FocusHandle>,
    scroll_handle: ScrollHandle,
    on_change: Option<SelectionHandler>,
}

#[derive(Clone, Copy)]
enum Navigation {
    Previous,
    Next,
    First,
    Last,
}

impl FlagPicker {
    pub(super) fn new(
        options: Vec<PickerOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = options
            .iter()
            .find(|option| !option.hidden && !option.disabled)
            .or_else(|| options.first())
            .map(|option| option.value.clone())
            .unwrap_or_default();
        let highlighted = options
            .iter()
            .position(|option| !option.hidden && !option.disabled && option.value == value);
        let trigger_focus = cx.focus_handle().tab_index(0).tab_stop(true);
        let option_focus = options
            .iter()
            .map(|_| cx.focus_handle().tab_stop(false))
            .collect();
        window.focus(&trigger_focus);
        Self {
            options,
            value,
            open: false,
            disabled: false,
            highlighted,
            trigger_focus,
            option_focus,
            scroll_handle: ScrollHandle::new(),
            on_change: None,
        }
    }

    pub(super) fn with_selected(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self.highlighted = self
            .options
            .iter()
            .position(|option| option.value == self.value && Self::selectable(option))
            .or_else(|| self.enabled_at_edge(false));
        self
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.trigger_focus = self.trigger_focus.clone().tab_stop(!disabled);
        self.open = false;
        self
    }

    pub(super) fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub(super) fn value(&self) -> &str {
        &self.value
    }

    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn flag_for_value(&self, value: &str) -> Option<&'static [u8]> {
        let option = self
            .options
            .iter()
            .find(|option| option.value.as_ref() == value)?;
        assets::flag(option.country.as_deref()?)
    }

    fn selectable(option: &PickerOption) -> bool {
        !option.hidden && !option.disabled
    }

    fn enabled_at_edge(&self, from_end: bool) -> Option<usize> {
        (0..self.options.len())
            .map(|offset| {
                if from_end {
                    self.options.len() - 1 - offset
                } else {
                    offset
                }
            })
            .find(|index| Self::selectable(&self.options[*index]))
    }

    fn adjacent_enabled(&self, current: Option<usize>, next: bool) -> Option<usize> {
        let len = self.options.len();
        if len == 0 {
            return None;
        }
        let current = current.unwrap_or(if next { len - 1 } else { 0 });
        (1..=len)
            .map(|distance| {
                if next {
                    (current + distance) % len
                } else {
                    (current + len - distance) % len
                }
            })
            .find(|index| Self::selectable(&self.options[*index]))
    }

    fn scroll_to_highlighted(&self) {
        let Some(highlighted) = self.highlighted else {
            return;
        };
        let row = self.options[..highlighted]
            .iter()
            .filter(|option| !option.hidden)
            .count();
        self.scroll_handle.scroll_to_item(row);
    }

    fn focus_highlighted(&self, window: &mut Window) {
        if let Some(index) = self.highlighted {
            window.focus(&self.option_focus[index]);
        }
    }

    fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.open = true;
        if self
            .highlighted
            .is_none_or(|index| !Self::selectable(&self.options[index]))
        {
            self.highlighted = self.enabled_at_edge(false);
        }
        self.scroll_to_highlighted();
        self.focus_highlighted(window);
        cx.notify();
    }

    fn move_highlight(
        &mut self,
        navigation: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        self.open = true;
        self.highlighted = match navigation {
            Navigation::Previous => self.adjacent_enabled(self.highlighted, false),
            Navigation::Next => self.adjacent_enabled(self.highlighted, true),
            Navigation::First => self.enabled_at_edge(false),
            Navigation::Last => self.enabled_at_edge(true),
        };
        self.scroll_to_highlighted();
        self.focus_highlighted(window);
        cx.notify();
    }

    fn close(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if restore_focus {
            window.focus(&self.trigger_focus);
        }
        cx.notify();
    }

    fn select_option(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || !self.options.get(index).is_some_and(Self::selectable) {
            return;
        }
        self.value = self.options[index].value.clone();
        self.highlighted = Some(index);
        if let Some(handler) = self.on_change.clone() {
            handler(self.value.clone(), window, cx);
        }
        self.close(true, window, cx);
    }

    fn escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(true, window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn set_expanded(
        &mut self,
        expanded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if expanded {
            self.open_menu(window, cx);
        } else {
            self.close(true, window, cx);
        }
    }

    fn toggle_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(true, window, cx);
        } else {
            self.open_menu(window, cx);
        }
    }

    fn render_trigger(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let selected_value = self.value();
        let selected = self
            .options
            .iter()
            .find(|option| option.value.as_ref() == selected_value)
            .or_else(|| self.options.first());
        let selected_label = selected
            .map(|option| option.label.clone())
            .unwrap_or_default();
        let selected_name = selected.and_then(|option| option.name.clone());
        let selected_flag = selected.and_then(flag_image);
        let mut trigger = div()
            .id("picker-trigger")
            .track_focus(&self.trigger_focus)
            .tab_index(0)
            .tab_stop(!self.disabled)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x4b5563))
            .bg(rgb(0x111827))
            .text_color(rgb(0xf9fafb))
            .in_focus(|style| style.border_color(rgb(0x93c5fd)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(selected_flag, |row, flag| row.child(flag))
                    .child(selected_label),
            )
            .child(if self.open { "▲" } else { "▼" });
        if let Some(name) = selected_name {
            trigger = trigger.tooltip(move |_, cx| cx.new(|_| Tooltip::new(name.clone())).into());
        }
        if !self.disabled {
            trigger = trigger
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.toggle_open(window, cx)));
        }
        trigger
    }

    fn render_option(
        &self,
        index: usize,
        option: &PickerOption,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        if option.hidden {
            return None;
        }
        let highlighted = self.highlighted == Some(index);
        let selected = self.value == option.value;
        let flag = flag_image(option);
        let mut row = div()
            .id(index)
            .track_focus(&self.option_focus[index])
            .tab_stop(false)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(if highlighted {
                rgb(0x1d4ed8)
            } else {
                gpui::rgba(0x00000000)
            })
            .text_color(if option.disabled {
                rgb(0x6b7280)
            } else {
                rgb(0xf9fafb)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(flag, |row, flag| row.child(flag))
                    .child(option.label.clone()),
            )
            .when(selected, |row| row.child("✓"));
        if Self::selectable(option) {
            row = row.cursor_pointer().on_click(
                cx.listener(move |this, _, window, cx| this.select_option(index, window, cx)),
            );
        }
        if let Some(name) = option.name.clone() {
            row = row.tooltip(move |_, cx| cx.new(|_| Tooltip::new(name.clone())).into());
        }
        Some(row)
    }

    fn render_menu(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("picker-menu")
            .track_scroll(&self.scroll_handle)
            .overflow_y_scroll()
            .max_h(px(240.))
            .w_full()
            .mt_1()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x4b5563))
            .bg(rgb(0x111827))
            .children(
                self.options
                    .iter()
                    .enumerate()
                    .filter_map(|(index, option)| self.render_option(index, option, cx)),
            )
    }
}

impl Focusable for FlagPicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.trigger_focus.clone()
    }
}

impl Render for FlagPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("picker")
            .key_context("AnyCaptionsWidgets")
            .relative()
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.open && !this.scroll_handle.bounds().contains(&event.position) {
                        this.close(true, window, cx);
                    }
                }),
            );
        if !self.disabled {
            root = root
                .on_action(cx.listener(|this, _: &Up, window, cx| {
                    this.move_highlight(Navigation::Previous, window, cx)
                }))
                .on_action(cx.listener(|this, _: &Down, window, cx| {
                    this.move_highlight(Navigation::Next, window, cx)
                }))
                .on_action(cx.listener(|this, _: &Home, window, cx| {
                    this.move_highlight(Navigation::First, window, cx)
                }))
                .on_action(cx.listener(|this, _: &End, window, cx| {
                    this.move_highlight(Navigation::Last, window, cx)
                }))
                .on_action(cx.listener(Self::escape));
        }
        root = root.child(self.render_trigger(cx));
        if self.open && !self.disabled {
            root = root.child(deferred(anchored().child(self.render_menu(cx))));
        }
        root
    }
}

pub(super) fn menu_anchor(
    id: &'static str,
    picker: Entity<FlagPicker>,
    expanded: bool,
) -> AnyElement {
    let picker = div().child(picker);
    if expanded {
        div().id(id).child(picker).into_any_element()
    } else {
        picker.into_any_element()
    }
}

pub(super) fn flag_value(
    picker: &Entity<FlagPicker>,
    value: &str,
    cx: &App,
) -> Option<&'static [u8]> {
    picker.read(cx).flag_for_value(value)
}

fn flag_image(option: &PickerOption) -> Option<gpui::Img> {
    let country = option.country.as_deref()?;
    Some(
        img(SharedString::from(format!(
            "flags/{}.png",
            country.as_ref()
        )))
        .w(px(theme::FLAG_WIDTH))
        .h(px(theme::FLAG_HEIGHT))
        .flex_none(),
    )
}
