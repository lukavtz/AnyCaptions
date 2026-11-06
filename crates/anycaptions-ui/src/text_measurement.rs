use std::sync::Arc;

use anycaptions_core::{caption::CaptionFits, domain::Appearance};
use gpui::{Font, FontWeight, LineFragment, LineWrapper, Pixels, TextRun, TextSystem, px, rgb};

use crate::caption::caption_font;

const FALLBACK_CAPTION_CHARS: usize = 84;

/// Real GPUI glyph measurement at the caption's current width and appearance.
pub struct GpuiCaptionFits {
    text_system: Arc<TextSystem>,
    font: Font,
    font_size: Pixels,
    wrap_width: Pixels,
}

impl GpuiCaptionFits {
    pub fn new(
        text_system: Arc<TextSystem>,
        font: Font,
        font_size: Pixels,
        wrap_width: Pixels,
    ) -> Self {
        let mut font = font;
        font.weight = FontWeight::BOLD;
        Self {
            text_system,
            font,
            font_size,
            wrap_width,
        }
    }

    pub fn from_appearance(
        text_system: Arc<TextSystem>,
        appearance: &Appearance,
        wrap_width: Pixels,
    ) -> Self {
        Self::new(
            text_system,
            caption_font(appearance),
            px(appearance.font_size.into()),
            wrap_width,
        )
    }

    pub fn set_width(&mut self, wrap_width: Pixels) {
        self.wrap_width = wrap_width;
    }

    pub fn set_appearance(&mut self, appearance: &Appearance) {
        self.font = caption_font(appearance);
        self.font_size = px(appearance.font_size.into());
    }

    fn text_run(&self, text: &str) -> TextRun {
        TextRun {
            len: text.len(),
            font: self.font.clone(),
            color: rgb(0xffffff).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

impl CaptionFits for GpuiCaptionFits {
    fn fits(&mut self, text: &str, max_lines: usize) -> bool {
        if f32::from(self.wrap_width) <= 0.0 {
            return text.encode_utf16().count() <= FALLBACK_CAPTION_CHARS;
        }
        let run = self.text_run(text);
        let mut wrapper = self.text_system.line_wrapper(run.font, self.font_size);
        let fragments = [LineFragment::text(text)];
        wrapped_line_count(&mut wrapper, &fragments, self.wrap_width) <= max_lines
    }
}

fn wrapped_line_count(
    wrapper: &mut LineWrapper,
    fragments: &[LineFragment<'_>],
    wrap_width: Pixels,
) -> usize {
    wrapper.wrap_line(fragments, wrap_width).count() + 1
}

#[cfg(test)]
mod tests {
    use anycaptions_core::{caption::CaptionPart, domain::Appearance};
    use gpui::TestAppContext;

    use super::*;
    use crate::caption::caption_runs;

    fn measurement(cx: &TestAppContext, width: f32) -> GpuiCaptionFits {
        GpuiCaptionFits::from_appearance(
            cx.text_system().clone(),
            &Appearance::default(),
            px(width),
        )
    }

    #[gpui::test]
    fn real_gpui_wrapping_rejects_overflow_and_allows_more_lines(cx: &mut TestAppContext) {
        let mut fits = measurement(cx, 192.0);
        let long = "a much longer caption whose glyphs cannot fit on one line";
        assert!(fits.fits("hi", 1));
        assert!(!fits.fits(long, 1));
        assert!(fits.fits(long, 20));

        let run = fits.text_run("hi");
        let mut wrapper = fits.text_system.line_wrapper(run.font, fits.font_size);
        let fragments = [LineFragment::text("hi")];
        assert_eq!(wrapped_line_count(&mut wrapper, &fragments, px(192.0)), 1);
    }

    #[gpui::test]
    fn changing_width_remeasures_the_same_glyphs(cx: &mut TestAppContext) {
        let mut fits = measurement(cx, 32.0);
        assert!(!fits.fits("resize reflow", 1));
        fits.set_width(px(1000.0));
        assert!(fits.fits("resize reflow", 1));
    }

    #[gpui::test]
    fn speaker_label_text_run_contributes_to_measured_width(cx: &mut TestAppContext) {
        let frame = crate::caption::test_frame(vec![CaptionPart {
            text: "a b".into(),
            speaker: Some(1),
            partial: false,
        }]);
        let appearance = Appearance::default();
        let caption = caption_runs(&frame, &appearance);
        assert_eq!(caption.text, "Pessoa 1: a b");
        assert_eq!(
            caption.runs.iter().map(|run| run.len).sum::<usize>(),
            caption.text.len()
        );
        let caption_font = crate::caption::caption_font(&appearance);
        assert_eq!(caption_font.weight, FontWeight::BOLD);
        assert!(caption.runs.iter().all(|run| run.font == caption_font));

        let mut fits = measurement(cx, 192.0);
        assert_eq!(fits.font, caption_font);
        assert!(fits.fits("a b", 1));
        assert!(!fits.fits(&caption.text, 1));
    }
}
