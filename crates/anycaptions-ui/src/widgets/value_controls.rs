use std::rc::Rc;

use gpui::{
    AnyView, App, Bounds, Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable, Pixels,
    Render, SharedString, Window, canvas, div, prelude::*, px, relative, rgb,
};

use super::{Down, End, Escape, Home, KEY_CONTEXT, Left, Right, Tab, TabPrev, TextInput, Up};

type NumberHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type EscapeHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct Slider {
    min: f32,
    max: f32,
    step: f32,
    value: f32,
    disabled: bool,
    focus_handle: FocusHandle,
    pub(super) track_bounds: Option<Bounds<Pixels>>,
    on_change: Option<NumberHandler>,
}

impl Slider {
    pub fn new(min: f32, max: f32, step: f32, value: f32, cx: &mut Context<Self>) -> Self {
        assert!(min.is_finite() && max.is_finite() && min <= max);
        assert!(step.is_finite() && step > 0.0);
        assert!(value.is_finite());
        let mut slider = Self {
            min,
            max,
            step,
            value,
            disabled: false,
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            track_bounds: None,
            on_change: None,
        };
        slider.value = slider.normalize(value);
        slider
    }

    widget_builders!(disabled);
    widget_builders!(change on_change, f32);

    widget_builders!(getter value, f32);

    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        let value = self.normalize(value);
        if self.value != value {
            self.value = value;
            cx.notify();
        }
    }

    pub fn value_for_ratio(&self, ratio: f32) -> f32 {
        if ratio.is_nan() {
            return self.min;
        }
        self.normalize(self.min + (self.max - self.min) * ratio.clamp(0.0, 1.0))
    }

    fn normalize(&self, value: f32) -> f32 {
        if value.is_nan() {
            return self.min;
        }
        let stepped = self.min + ((value - self.min) / self.step).round() * self.step;
        stepped.clamp(self.min, self.max)
    }

    fn change_value(&mut self, value: f32, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.normalize(value);
        if self.value == value {
            return;
        }
        self.value = value;
        cx.notify();
        if let Some(handler) = self.on_change.clone() {
            handler(value, window, cx);
        }
    }

    fn value_for_x(&self, x: Pixels, bounds: Bounds<Pixels>) -> f32 {
        let width = f32::from(bounds.size.width);
        if width <= 0.0 {
            return self.min;
        }
        self.value_for_ratio((f32::from(x) - f32::from(bounds.left())) / width)
    }

    fn change_value_at(
        &mut self,
        x: Pixels,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.value_for_x(x, bounds);
        self.change_value(value, window, cx);
    }

    fn step_by(&mut self, steps: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.change_value(self.value + self.step * steps, window, cx);
    }
}

impl_focus_handle!(Slider);

impl Render for Slider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fraction = if self.max == self.min {
            0.0
        } else {
            (self.value - self.min) / (self.max - self.min)
        };
        let bounds_slider = cx.entity();
        let drag_slider = cx.entity();
        let bounds_canvas = canvas(
            |_, _, _| {},
            move |bounds, _, _, cx| {
                bounds_slider.update(cx, |slider, _| slider.track_bounds = Some(bounds));
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let mut element = div()
            .id("slider")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .tab_index(0)
            .tab_stop(!self.disabled)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .h(px(20.))
            .rounded_sm()
            .border_1()
            .border_color(gpui::transparent_black())
            .in_focus(|style| style.border_color(rgb(0x93c5fd)))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(rgb(0x4b5563))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .h_full()
                            .w(relative(fraction))
                            .rounded_full()
                            .bg(if self.disabled {
                                rgb(0x4b5563)
                            } else {
                                rgb(0x2563eb)
                            }),
                    )
                    .child(bounds_canvas),
            );

        if !self.disabled {
            element = element
                .cursor_pointer()
                .on_action(cx.listener(|this, _: &Left, window, cx| this.step_by(-1.0, window, cx)))
                .on_action(cx.listener(|this, _: &Right, window, cx| this.step_by(1.0, window, cx)))
                .on_action(
                    cx.listener(|this, _: &Home, window, cx| {
                        this.change_value(this.min, window, cx)
                    }),
                )
                .on_action(
                    cx.listener(|this, _: &End, window, cx| {
                        this.change_value(this.max, window, cx)
                    }),
                )
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        if let Some(bounds) = this.track_bounds {
                            this.change_value_at(event.position.x, bounds, window, cx);
                        }
                    }),
                )
                .on_drag((), move |_, _, window, cx| {
                    let x = window.mouse_position().x;
                    drag_slider.update(cx, |slider, cx| {
                        if let Some(bounds) = slider.track_bounds {
                            slider.change_value_at(x, bounds, window, cx);
                        }
                    });
                    cx.new(|_| Empty)
                })
                .on_drag_move(cx.listener(|this, event: &DragMoveEvent<()>, window, cx| {
                    this.change_value_at(event.event.position.x, event.bounds, window, cx);
                }));
        }
        element
    }
}

pub struct NumberInput {
    input: Entity<TextInput>,
    text: SharedString,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    on_change: Option<NumberHandler>,
}

impl NumberInput {
    pub fn new(min: f32, max: f32, step: f32, value: f32, cx: &mut Context<Self>) -> Self {
        assert!(min.is_finite() && max.is_finite() && min <= max);
        assert!(step.is_finite() && step > 0.0);
        assert!(value.is_finite());
        let value = value.clamp(min, max);
        let text = SharedString::from(format_number(value));
        let this = cx.entity().downgrade();
        let input = cx.new(|cx| {
            TextInput::new("0", cx)
                .with_content(text.clone())
                .on_change(move |text, window, cx| {
                    let _ = this.update(cx, |number, cx| {
                        number.read_text(text, window, cx);
                    });
                })
        });
        Self {
            input,
            text,
            value,
            min,
            max,
            step,
            on_change: None,
        }
    }

    widget_builders!(change on_change, f32);

    widget_builders!(getter value, f32);

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        assert!(value.is_finite());
        self.value = value.clamp(self.min, self.max);
        self.text = SharedString::from(format_number(self.value));
        let text = self.text.clone();
        self.input
            .update(cx, |input, cx| input.set_content(&text, cx));
        cx.notify();
    }

    fn read_text(&mut self, text: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.text = text;
        if let Ok(value) = self.text.parse::<f32>()
            && value.is_finite()
        {
            let value = value.clamp(self.min, self.max);
            if self.value != value {
                self.value = value;
                if let Some(handler) = self.on_change.clone() {
                    handler(value, window, cx);
                }
            }
        }
        cx.notify();
    }

    fn step_by(&mut self, steps: f32, window: &mut Window, cx: &mut Context<Self>) {
        let value = (self.value + self.step * steps).clamp(self.min, self.max);
        if self.value == value {
            return;
        }
        self.set_value(value, cx);
        if let Some(handler) = self.on_change.clone() {
            handler(self.value, window, cx);
        }
    }
}

impl Focusable for NumberInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for NumberInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("number-input")
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &Up, window, cx| this.step_by(1.0, window, cx)))
            .on_action(cx.listener(|this, _: &Down, window, cx| this.step_by(-1.0, window, cx)))
            .child(self.input.clone())
    }
}

pub struct Progress {
    fraction: f32,
    label: Option<SharedString>,
}

impl Progress {
    pub fn new(fraction: f32) -> Self {
        assert!(fraction.is_finite());
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            label: None,
        }
    }

    widget_builders!(option label);

    widget_builders!(getter fraction, f32);

    pub fn set_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        assert!(fraction.is_finite());
        self.fraction = fraction.clamp(0.0, 1.0);
        cx.notify();
    }

    fn render_element(&self) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .children(self.label.clone())
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .bg(rgb(0x374151))
                    .child(
                        div()
                            .h_full()
                            .w(relative(self.fraction))
                            .rounded_full()
                            .bg(rgb(0x2563eb)),
                    ),
            )
    }
}

impl_static_render!(Progress);

pub struct FocusScope {
    focus_handle: FocusHandle,
    children: Vec<AnyView>,
    on_escape: Option<EscapeHandler>,
}

impl FocusScope {
    pub fn new(content: impl Into<AnyView>, cx: &mut Context<Self>) -> Self {
        Self::with_children(vec![content.into()], cx)
    }

    pub fn with_children(children: Vec<AnyView>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            children,
            on_escape: None,
        }
    }

    widget_builders!(handler on_escape);

    fn escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handler) = self.on_escape.clone() {
            handler(window, cx);
        } else {
            cx.propagate();
        }
    }
}

impl_focus_handle!(FocusScope);

impl Render for FocusScope {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("focus-scope")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &Tab, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &TabPrev, window, _| window.focus_prev()))
            .on_action(cx.listener(Self::escape))
            .children(self.children.clone())
    }
}

fn format_number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}
