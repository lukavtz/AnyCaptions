use anycaptions_core::{
    domain::{Appearance, CaptionColor, CaptionFont},
    ipc::CommandError,
    presentation::{
        CaptionFontCatalog, MAX_CAPTION_SIZE, MIN_CAPTION_SIZE, MessageKey, normalize_caption_color,
    },
};
use gpui::{
    App, Context, Entity, FocusHandle, IntoElement, WeakEntity, Window, div, prelude::*, px,
};

use super::{
    ButtonAnchor, CAPTION_COLOR_AUTOMATION_ID, ControlFocusExt, SettingsHostAction, SettingsView,
    focus_list, select_pair,
};
use crate::{
    caption::{caption_color, caption_font},
    theme,
    widgets::{Button, Picker, PickerOption, Slider, TextInput, Toggle},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTarget {
    Confirmed,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearancePreview {
    pub font_size: u16,
    pub color: CaptionColor,
    pub partial_color: CaptionColor,
    pub background: bool,
    pub font: CaptionFont,
}

impl From<&Appearance> for AppearancePreview {
    fn from(appearance: &Appearance) -> Self {
        Self {
            font_size: appearance.font_size,
            color: appearance.color,
            partial_color: appearance.partial_color,
            background: appearance.background,
            font: appearance.font,
        }
    }
}

impl From<AppearancePreview> for Appearance {
    fn from(preview: AppearancePreview) -> Self {
        Self {
            font_size: preview.font_size,
            color: preview.color,
            partial_color: preview.partial_color,
            background: preview.background,
            font: preview.font,
        }
    }
}

pub(crate) struct ColorInputs {
    confirmed: String,
    partial: String,
    confirmed_valid: bool,
    partial_valid: bool,
}

impl ColorInputs {
    pub(crate) fn new(appearance: &Appearance) -> Self {
        let confirmed = color_hex(appearance.color);
        let partial = color_hex(appearance.partial_color);
        Self {
            confirmed_valid: parse_color(&confirmed).is_some(),
            partial_valid: parse_color(&partial).is_some(),
            confirmed,
            partial,
        }
    }

    fn text(&self, target: ColorTarget) -> &str {
        select_pair(
            &self.confirmed,
            &self.partial,
            matches!(target, ColorTarget::Partial),
        )
        .as_str()
    }

    fn set(&mut self, target: ColorTarget, value: &str) -> Option<CaptionColor> {
        let normalized = parse_color(value);
        let (text, valid) = match target {
            ColorTarget::Confirmed => (&mut self.confirmed, &mut self.confirmed_valid),
            ColorTarget::Partial => (&mut self.partial, &mut self.partial_valid),
        };
        match normalized {
            Some((normalized, color)) => {
                *text = normalized;
                *valid = true;
                Some(color)
            }
            None => {
                *text = value.to_owned();
                *valid = false;
                None
            }
        }
    }

    pub(crate) fn valid(&self, target: ColorTarget) -> bool {
        *select_pair(
            &self.confirmed_valid,
            &self.partial_valid,
            matches!(target, ColorTarget::Partial),
        )
    }
}

fn color_hex(color: CaptionColor) -> String {
    format!("#{:06X}", color.rgb())
}

fn parse_color(value: &str) -> Option<(String, CaptionColor)> {
    let normalized = normalize_caption_color(value).ok()?;
    let rgb = u32::from_str_radix(&normalized[1..], 16).ok()?;
    Some((normalized, CaptionColor::from_rgb(rgb)))
}

pub(super) struct AppearanceControls {
    font_size: Entity<Slider>,
    pub(crate) confirmed_dialog: Entity<Button>,
    pub(crate) confirmed_input: Entity<TextInput>,
    pub(crate) partial_dialog: Entity<Button>,
    pub(crate) partial_input: Entity<TextInput>,
    caption_font: Entity<Picker>,
    background: Entity<Toggle>,
    pub(super) reset: Entity<ButtonAnchor>,
    _confirmed_blur_subscription: gpui::Subscription,
    _partial_blur_subscription: gpui::Subscription,
}

macro_rules! color_control {
    ($name:ident, $widget:ty, $builder:expr) => {
        fn $name(
            view: &SettingsView,
            target: ColorTarget,
            label: MessageKey,
            disabled: bool,
            weak: WeakEntity<SettingsView>,
            cx: &mut Context<SettingsView>,
        ) -> Entity<$widget> {
            ($builder)(view, target, label, disabled, weak, cx)
        }
    };
}

color_control!(
    color_dialog,
    Button,
    |view: &SettingsView,
     target,
     label,
     disabled,
     weak: WeakEntity<SettingsView>,
     cx: &mut Context<SettingsView>| {
        cx.new(|cx| {
            Button::new(view.text(label), cx)
                .disabled(disabled)
                .on_press(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| view.request_color(target, window, cx));
                })
        })
    }
);

color_control!(
    color_input,
    TextInput,
    |view: &SettingsView,
     target,
     label,
     disabled,
     weak: WeakEntity<SettingsView>,
     cx: &mut Context<SettingsView>| {
        cx.new(|cx| {
            TextInput::new(view.text(label), cx)
                .with_max_length(7usize)
                .with_content(view.color_text(target).to_owned())
                .disabled(disabled)
                .on_change(move |text, window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.set_color_text(target, &text, window, cx);
                    });
                })
        })
    }
);

impl AppearanceControls {
    pub(super) fn new(
        view: &SettingsView,
        weak: WeakEntity<SettingsView>,
        window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let preview = view.preview();
        let disabled = view.primary_disabled();

        let font_weak = weak.clone();
        let font_size = cx.new(|cx| {
            Slider::new(
                MIN_CAPTION_SIZE as f32,
                MAX_CAPTION_SIZE as f32,
                1.0,
                preview.font_size as f32,
                cx,
            )
            .disabled(disabled)
            .on_change(move |value, window, cx| {
                let _ = font_weak.update(cx, |view, cx| {
                    view.set_font_size(value, window, cx);
                });
            })
        });

        let confirmed_dialog = color_dialog(
            view,
            ColorTarget::Confirmed,
            MessageKey::PickConfirmedColor,
            disabled,
            weak.clone(),
            cx,
        );
        let confirmed_input = color_input(
            view,
            ColorTarget::Confirmed,
            MessageKey::ConfirmedColorHex,
            disabled,
            weak.clone(),
            cx,
        );
        let partial_dialog = color_dialog(
            view,
            ColorTarget::Partial,
            MessageKey::PickRecentColor,
            disabled,
            weak.clone(),
            cx,
        );
        let partial_input = color_input(
            view,
            ColorTarget::Partial,
            MessageKey::RecentColorHex,
            disabled,
            weak.clone(),
            cx,
        );

        let confirmed_focus = confirmed_input.control_focus(cx);
        let confirmed_blur_subscription =
            cx.on_focus_out(&confirmed_focus, &mut *window, |view, _, _, cx| {
                view.normalize_color_input(ColorTarget::Confirmed, cx)
            });
        let partial_focus = partial_input.control_focus(cx);
        let partial_blur_subscription =
            cx.on_focus_out(&partial_focus, &mut *window, |view, _, _, cx| {
                view.normalize_color_input(ColorTarget::Partial, cx)
            });

        let options = CaptionFontCatalog::fonts()
            .iter()
            .map(|font| PickerOption::new(font.id, font.name))
            .collect();
        let caption_font_weak = weak.clone();
        let caption_font = cx.new(|cx| {
            Picker::new(options, window, cx)
                .with_selected(preview.font.id())
                .disabled(disabled)
                .on_change(move |font, window, cx| {
                    let _ = caption_font_weak.update(cx, |view, cx| {
                        view.set_caption_font(font.to_string(), window, cx);
                    });
                })
        });

        let background_weak = weak.clone();
        let background = cx.new(|cx| {
            Toggle::new(
                view.text(MessageKey::CaptionBackground),
                preview.background,
                cx,
            )
            .disabled(disabled)
            .on_change(move |enabled, window, cx| {
                let _ = background_weak.update(cx, |view, cx| {
                    view.set_background(enabled, window, cx);
                });
            })
        });

        let reset_inner = cx.new(|cx| {
            Button::new(view.text(MessageKey::RestoreDefault), cx)
                .disabled(disabled)
                .on_press(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| view.reset_appearance(window, cx));
                })
        });
        let reset = cx.new(|_| ButtonAnchor::new("reset-appearance", reset_inner));

        Self {
            font_size,
            confirmed_dialog,
            confirmed_input,
            partial_dialog,
            partial_input,
            caption_font,
            background,
            reset,
            _confirmed_blur_subscription: confirmed_blur_subscription,
            _partial_blur_subscription: partial_blur_subscription,
        }
    }

    fn dialog(&self, target: ColorTarget) -> &Entity<Button> {
        select_pair(
            &self.confirmed_dialog,
            &self.partial_dialog,
            matches!(target, ColorTarget::Partial),
        )
    }
}

impl_focus_handles!(
    AppearanceControls,
    [
        font_size,
        confirmed_dialog,
        confirmed_input,
        partial_dialog,
        partial_input,
        caption_font,
        background,
        reset,
    ]
);

impl SettingsView {
    pub fn preview(&self) -> AppearancePreview {
        let appearance = self
            .application
            .draft()
            .map(|draft| &draft.appearance)
            .unwrap_or(&self.application.snapshot().appearance);
        AppearancePreview::from(appearance)
    }

    pub fn caption_font_values(&self) -> Vec<String> {
        CaptionFontCatalog::fonts()
            .iter()
            .map(|font| font.id.to_owned())
            .collect()
    }

    pub(super) fn font_value(&self) -> String {
        format!("{} px", self.preview().font_size)
    }

    pub(super) fn color_text(&self, target: ColorTarget) -> &str {
        self.color_inputs.text(target)
    }

    pub(super) fn color_valid(&self, target: ColorTarget) -> bool {
        self.color_inputs.valid(target)
    }

    fn normalize_color_input(&self, target: ColorTarget, cx: &mut Context<Self>) {
        if !self.color_valid(target) {
            return;
        }
        let content = self.color_text(target).to_owned();
        let Some((normalized, _)) = parse_color(&content) else {
            return;
        };
        let input = match target {
            ColorTarget::Confirmed => &self.controls().appearance.confirmed_input,
            ColorTarget::Partial => &self.controls().appearance.partial_input,
        }
        .clone();
        input.update(cx, |input, cx| input.set_content(&normalized, cx));
    }

    pub(super) fn set_font_size(
        &mut self,
        value: f32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() {
            return;
        }
        let value = if value.is_nan() {
            MIN_CAPTION_SIZE as f32
        } else {
            value
        };
        let font_size = value
            .round()
            .clamp(MIN_CAPTION_SIZE as f32, MAX_CAPTION_SIZE as f32) as u16;
        if let Some(draft) = self.application.draft_mut() {
            draft.appearance.font_size = font_size;
        }
        cx.notify();
    }

    pub(super) fn set_color_text(
        &mut self,
        target: ColorTarget,
        value: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() {
            return;
        }
        let color = self.color_inputs.set(target, value);
        if let (Some(color), Some(draft)) = (color, self.application.draft_mut()) {
            match target {
                ColorTarget::Confirmed => draft.appearance.color = color,
                ColorTarget::Partial => draft.appearance.partial_color = color,
            }
        }
        self.rebuild_disabled(cx);
    }

    pub(super) fn set_caption_font(
        &mut self,
        id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() {
            return;
        }
        let Some(font) = CaptionFont::from_id(&id) else {
            return;
        };
        if let Some(draft) = self.application.draft_mut() {
            draft.appearance.font = font;
        }
        cx.notify();
    }

    pub(super) fn set_background(
        &mut self,
        enabled: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() {
            return;
        }
        if let Some(draft) = self.application.draft_mut() {
            draft.appearance.background = enabled;
        }
        cx.notify();
    }

    pub(super) fn request_color(
        &mut self,
        target: ColorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_disabled() || self.pending_color.is_some() {
            return;
        }
        let preview = self.preview();
        let initial = if self.color_inputs.valid(target) {
            match target {
                ColorTarget::Confirmed => preview.color,
                ColorTarget::Partial => preview.partial_color,
            }
        } else {
            CaptionColor::WHITE
        };
        self.pending_color = Some(target);
        self.emit(SettingsHostAction::ChooseColor {
            target,
            initial: initial.rgb(),
        });
        self.focus_color_dialog(target, window, cx);
        cx.notify();
    }

    pub fn complete_color_choice(
        &mut self,
        result: Result<Option<u32>, CommandError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.open {
            return false;
        }
        let Some(target) = self.pending_color.take() else {
            return false;
        };
        match result {
            Ok(Some(rgb)) => {
                let text = format!("#{:06X}", rgb);
                self.set_color_text(target, &text, window, cx);
            }
            Ok(None) => {}
            Err(error) => self.report(error, cx),
        }
        self.rebuild_controls(window, cx);
        self.focus_color_dialog(target, window, cx);
        true
    }

    pub(super) fn reset_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.primary_disabled() {
            return;
        }
        let appearance = Appearance::default();
        if let Some(draft) = self.application.draft_mut() {
            draft.appearance = appearance;
        }
        self.color_inputs = ColorInputs::new(&appearance);
        self.pending_color = None;
        self.defer_rebuild(window, cx);
    }

    pub(super) fn focus_color_dialog(&self, target: ColorTarget, window: &mut Window, cx: &App) {
        let dialog = self.controls().appearance.dialog(target).clone();
        let handle = dialog.control_focus(cx);
        window.focus(&handle);
    }
}

fn field(label: &'static str, control: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(div().w(px(140.)).flex_none().text_sm().child(label))
        .child(div().flex_1().child(control))
}

fn color_field(
    view: &SettingsView,
    controls: &AppearanceControls,
    target: ColorTarget,
    label: MessageKey,
    dialog_id: &'static str,
    input_id: &'static str,
) -> gpui::Div {
    let input = match target {
        ColorTarget::Confirmed => &controls.confirmed_input,
        ColorTarget::Partial => &controls.partial_input,
    };
    field(
        view.text(label),
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().id(dialog_id).child(controls.dialog(target).clone()))
            .child(
                div()
                    .id(input_id)
                    .flex_1()
                    .rounded_md()
                    .border_1()
                    .border_color(if view.color_valid(target) {
                        gpui::transparent_black()
                    } else {
                        theme::ERROR.into()
                    })
                    .child(input.clone()),
            ),
    )
}

pub(super) fn render(view: &SettingsView, _: &App) -> impl IntoElement {
    let preview = view.preview();
    let appearance = Appearance::from(preview);
    let controls = &view.controls().appearance;

    let example = div()
        .id("appearance-preview")
        .font(caption_font(&appearance))
        .text_size(px(preview.font_size as f32))
        .text_color(caption_color(preview.color))
        .bg(if preview.background {
            theme::CAPTION_BACKGROUND
        } else {
            theme::PREVIEW_BACKGROUND
        })
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(theme::BORDER_SUBTLE)
        .child(
            div()
                .flex()
                .flex_wrap()
                .child(view.text(MessageKey::PreviewFinal))
                .child(
                    div()
                        .text_color(caption_color(preview.partial_color))
                        .child(view.text(MessageKey::PreviewPartial)),
                ),
        );

    div()
        .id("appearance-settings")
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme::MUTED)
                        .child(view.text(MessageKey::Preview)),
                )
                .child(example),
        )
        .child(field(
            view.text(MessageKey::FontSize),
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().id("font-value").child(view.font_value()))
                .child(div().id("font-size").child(controls.font_size.clone())),
        ))
        .child(color_field(
            view,
            controls,
            ColorTarget::Confirmed,
            MessageKey::ConfirmedText,
            "font-color",
            CAPTION_COLOR_AUTOMATION_ID,
        ))
        .child(color_field(
            view,
            controls,
            ColorTarget::Partial,
            MessageKey::RecentWords,
            "partial-color",
            "partial-color-value",
        ))
        .child(field(
            view.text(MessageKey::CaptionFont),
            div()
                .id("caption-font")
                .child(controls.caption_font.clone()),
        ))
        .child(field(
            view.text(MessageKey::CaptionBackground),
            div()
                .id("caption-background")
                .child(controls.background.clone()),
        ))
}
