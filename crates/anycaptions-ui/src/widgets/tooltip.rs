use gpui::{SharedString, div, prelude::*, px, rgb};

pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }

    fn render_element(&self) -> gpui::Div {
        div()
            .max_w(px(320.))
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x4b5563))
            .bg(rgb(0x111827))
            .text_color(rgb(0xf9fafb))
            .text_sm()
            .shadow_md()
            .child(self.text.clone())
    }
}

impl_static_render!(Tooltip);
