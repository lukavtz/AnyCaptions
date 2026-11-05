use std::sync::Arc;

use anycaptions_core::{
    caption::{CLEAR_FADE, CaptionFrame, CaptionLines, CaptionPresenter},
    domain::{AppSnapshot, Appearance, CaptureState, WindowMode},
    ipc::CaptionUpdate,
    presentation::{Locale, MessageKey, translate},
};
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, Pixels, Render, SharedString, StyledText,
    Subscription, TextRun, TextSystem, Window, div, prelude::*, px,
};

use crate::{
    caption::caption_runs,
    text_measurement::GpuiCaptionFits,
    theme::{CAPTION_BACKGROUND, CAPTION_LINE_HEIGHT, MUTED},
};

#[derive(Debug, Clone)]
pub(crate) struct CaptionLayer {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub animate: bool,
    pub new_turn: bool,
}

/// Splits only translated sentences. Live captions and speaker turns retain
/// their existing run layout; no previous-caption transition layer is kept.
pub(crate) fn caption_layers(frame: &CaptionFrame, appearance: &Appearance) -> Vec<CaptionLayer> {
    if frame.text.is_empty() {
        return Vec::new();
    }
    if frame.sentences.is_empty() {
        let runs = caption_runs(frame, appearance);
        return vec![CaptionLayer {
            text: runs.text,
            runs: runs.runs,
            animate: false,
            new_turn: false,
        }];
    }

    let last = frame.sentences.len() - 1;
    frame
        .sentences
        .iter()
        .enumerate()
        .map(|(index, sentence)| {
            let mut single = frame.clone();
            single.sentences = vec![sentence.clone()];
            single.sentence_speakers = frame
                .sentence_speakers
                .as_ref()
                .and_then(|speakers| speakers.get(index))
                .map(|speaker| vec![*speaker]);
            single.provisional_text = if index == last {
                frame.provisional_text.clone()
            } else {
                None
            };
            single.provisional_speaker = if index == last {
                frame.provisional_speaker
            } else {
                None
            };
            single.animate_newest = false;
            let mut runs = caption_runs(&single, appearance);
            if index < last {
                if let Some(run) = runs.runs.last_mut() {
                    run.len += 1;
                }
                runs.text.push(' ');
            }
            CaptionLayer {
                text: runs.text,
                runs: runs.runs,
                animate: frame.animate_newest && index == last,
                new_turn: index > 0
                    && frame
                        .sentence_speakers
                        .as_ref()
                        .and_then(|speakers| speakers.get(index))
                        .is_some_and(Option::is_some),
            }
        })
        .collect()
}

pub struct CaptionView {
    presenter: CaptionPresenter<GpuiCaptionFits>,
    snapshot: AppSnapshot,
    settings_hidden: bool,
    wrap_width: Pixels,
    content_revision: u64,
    fade_generation: u64,
    fade_task: Option<gpui::Task<()>>,
    subscriptions: Vec<Subscription>,
    #[cfg(test)]
    timer_count: std::rc::Rc<std::cell::Cell<usize>>,
}

impl CaptionView {
    pub(crate) fn new(
        text_system: Arc<TextSystem>,
        snapshot: AppSnapshot,
        wrap_width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fits = GpuiCaptionFits::from_appearance(text_system, &snapshot.appearance, wrap_width);
        let mut presenter = CaptionPresenter::new(fits);
        presenter.set_context(snapshot.session_id, snapshot.mode);
        presenter.set_speaker_labels(snapshot.speaker_labels && snapshot.resources.speaker_labels);
        let subscription = cx.observe_window_bounds(window, |this, window, cx| {
            this.set_wrap_width(window.viewport_size().width - px(32.0), cx);
        });
        Self {
            presenter,
            snapshot,
            settings_hidden: false,
            wrap_width,
            content_revision: 0,
            fade_generation: 0,
            fade_task: None,
            subscriptions: vec![subscription],
            #[cfg(test)]
            timer_count: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    pub fn frame(&self) -> Option<&CaptionFrame> {
        self.presenter.frame()
    }

    pub fn accessible_text(&self) -> String {
        self.frame()
            .map(|frame| caption_runs(frame, &self.snapshot.appearance).text)
            .unwrap_or_default()
    }

    pub fn fade_pending(&self) -> bool {
        self.fade_task.is_some()
    }

    pub fn listener_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn accept(&mut self, update: CaptionUpdate, cx: &mut Context<Self>) -> bool {
        if self.snapshot.capture == CaptureState::Stopped {
            return false;
        }
        if !self.presenter.accept(update) {
            return false;
        }
        self.cancel_fade();
        self.content_revision = self.content_revision.wrapping_add(1);
        cx.notify();
        true
    }

    pub fn apply_snapshot(
        &mut self,
        snapshot: &AppSnapshot,
        reduced_motion: bool,
        cx: &mut Context<Self>,
    ) {
        let previous = self.snapshot.clone();
        let context_changed =
            previous.session_id != snapshot.session_id || previous.mode != snapshot.mode;
        if context_changed {
            self.cancel_fade();
            self.presenter.clear_immediate();
            self.presenter
                .set_context(snapshot.session_id, snapshot.mode);
        }
        let appearance_changed = previous.appearance != snapshot.appearance;
        // This setter refreshes only when the effective speaker-label mode changes.
        self.presenter
            .set_speaker_labels(snapshot.speaker_labels && snapshot.resources.speaker_labels);
        if appearance_changed {
            self.presenter
                .fits_mut()
                .set_appearance(&snapshot.appearance);
        }
        self.snapshot = snapshot.clone();
        if context_changed || appearance_changed {
            self.presenter.refresh_layout();
        }
        if previous.capture != CaptureState::Stopped && snapshot.capture == CaptureState::Stopped {
            self.clear(reduced_motion, cx);
        } else {
            cx.notify();
        }
    }

    pub fn clear(&mut self, reduced_motion: bool, cx: &mut Context<Self>) {
        self.cancel_fade();
        self.presenter.clear_with_reduced_motion(reduced_motion);
        if self.presenter.fade_pending() {
            self.fade_generation = self.fade_generation.wrapping_add(1);
            let generation = self.fade_generation;
            #[cfg(test)]
            self.timer_count.set(self.timer_count.get() + 1);
            self.fade_task = Some(cx.spawn(async move |view, cx| {
                cx.background_executor().timer(CLEAR_FADE).await;
                let _ = view.update(cx, |view, cx| {
                    if view.fade_generation == generation && view.presenter.fade_pending() {
                        view.presenter.complete_clear();
                        view.fade_task = None;
                        #[cfg(test)]
                        view.timer_count.set(view.timer_count.get() - 1);
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }

    pub fn set_settings_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if self.settings_hidden == hidden {
            return;
        }
        self.settings_hidden = hidden;
        cx.notify();
    }

    fn set_wrap_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = px(f32::from(width).max(1.0));
        if width == self.wrap_width {
            return;
        }
        self.wrap_width = width;
        self.presenter.fits_mut().set_width(width);
        self.presenter.refresh_layout();
        cx.notify();
    }

    fn cancel_fade(&mut self) {
        self.fade_generation = self.fade_generation.wrapping_add(1);
        if self.fade_task.take().is_some() {
            #[cfg(test)]
            self.timer_count.set(self.timer_count.get() - 1);
        }
    }

    #[cfg(test)]
    pub(crate) fn timer_count(&self) -> std::rc::Rc<std::cell::Cell<usize>> {
        self.timer_count.clone()
    }

    fn render_layer(&self, layer: CaptionLayer, index: usize, reduced_motion: bool) -> AnyElement {
        let mut element = div()
            .id(SharedString::from(format!("caption-layer-{index}")))
            .child(StyledText::new(layer.text).with_runs(layer.runs));
        if layer.new_turn {
            element = element.w_full();
        }
        if layer.animate && !reduced_motion {
            let id = SharedString::from(format!(
                "caption-sentence-{}-{index}",
                self.content_revision
            ));
            element
                .with_animation(id, Animation::new(CLEAR_FADE), |element, delta| {
                    element.opacity(delta)
                })
                .into_any_element()
        } else {
            element.into_any_element()
        }
    }
}

impl Render for CaptionView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let locale = Locale::from_tag(self.snapshot.ui_language.tag()).unwrap_or_default();
        let reduced_motion = crate::overlay::reduced_motion();
        let frame = self.presenter.frame();
        let has_caption = frame.is_some_and(|frame| !frame.text.is_empty());
        let show_placeholder = !has_caption
            && self.snapshot.capture == CaptureState::Stopped
            && self.snapshot.window_mode != WindowMode::Locked;
        let multi = frame.is_some_and(|frame| frame.lines == CaptionLines::Multi);
        let opacity = frame.map_or(1.0, |frame| frame.opacity);
        let fading = self.presenter.fade_pending();
        let fade_generation = self.fade_generation;
        let layers = frame
            .map(|frame| caption_layers(frame, &self.snapshot.appearance))
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, layer)| self.render_layer(layer, index, reduced_motion))
            .collect::<Vec<_>>();

        let caption = div()
            .id("caption-live")
            .flex()
            .flex_row()
            .flex_wrap()
            .w_full()
            .overflow_hidden()
            .text_size(px(self.snapshot.appearance.font_size.into()))
            .line_height(px(
                self.snapshot.appearance.font_size as f32 * CAPTION_LINE_HEIGHT
            ))
            .when(!multi, |caption| caption.whitespace_nowrap())
            .when(multi, |caption| caption.whitespace_normal())
            .children(layers);
        let caption = if fading {
            caption
                .with_animation(
                    SharedString::from(format!("caption-clear-{fade_generation}")),
                    Animation::new(CLEAR_FADE),
                    |caption, delta| caption.opacity(1.0 - delta),
                )
                .into_any_element()
        } else {
            caption.opacity(opacity).into_any_element()
        };

        div()
            .id("caption-preview")
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .overflow_hidden()
            .px_4()
            .py_2()
            .rounded_md()
            .when(self.snapshot.appearance.background, |root| {
                root.bg(CAPTION_BACKGROUND)
            })
            .when(
                self.settings_hidden || (!show_placeholder && !has_caption),
                |root| root.hidden(),
            )
            .when(show_placeholder, |root| {
                root.child(
                    div()
                        .id("caption-placeholder")
                        .text_color(MUTED)
                        .child(translate(locale, MessageKey::CaptionPlaceholder).unwrap_or("")),
                )
            })
            .when_some(has_caption.then_some(caption), |root, caption| {
                root.child(caption)
            })
    }
}

impl Drop for CaptionView {
    fn drop(&mut self) {
        self.cancel_fade();
        self.presenter.clear_immediate();
        self.subscriptions.clear();
    }
}
