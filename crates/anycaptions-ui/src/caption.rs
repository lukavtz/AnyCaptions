use std::sync::Arc;

use anycaptions_core::{
    caption::{CaptionFrame, CaptionLines, CaptionPart, speaker_label},
    domain::{Appearance, CaptionColor},
    presentation::CaptionFontCatalog,
};
use gpui::{Font, FontFallbacks, FontWeight, Hsla, TextRun, rgb};

pub const SPEAKER_COLORS: [u32; 8] = [
    0x7cc4ff, 0xffb36b, 0x7fe0a0, 0xd6a6ff, 0xff9bb8, 0x6fe6e0, 0xff8a80, 0xb8c2ff,
];

/// Literal caption text plus the GPUI runs needed to paint it.
/// Newlines are explicit speaker-turn breaks; all other wrapping is measured.
#[derive(Debug, Clone, Default)]
pub struct CaptionRuns {
    pub text: String,
    pub runs: Vec<TextRun>,
}

struct CaptionStyle {
    font: Font,
    final_color: Hsla,
    partial_color: Hsla,
}

impl CaptionStyle {
    fn new(appearance: &Appearance) -> Self {
        Self {
            font: caption_font(appearance),
            final_color: caption_color(appearance.color),
            partial_color: caption_color(appearance.partial_color),
        }
    }
}

pub fn caption_color(color: CaptionColor) -> Hsla {
    rgb(color.rgb()).into()
}

pub fn speaker_color(speaker: u32) -> Hsla {
    let index = speaker.clamp(1, 8) as usize - 1;
    rgb(SPEAKER_COLORS[index]).into()
}

/// Converts the shared CSS family stack into GPUI's family plus fallbacks.
pub fn caption_font(appearance: &Appearance) -> Font {
    let stack = CaptionFontCatalog::family(appearance.font.id());
    let mut families = stack.split(',').map(|family| {
        family
            .trim()
            .trim_matches(|character| character == '"' || character == '\'')
            .to_owned()
    });
    let family = families.next().unwrap_or_else(|| "Inter Variable".into());
    let fallbacks: Vec<String> = families
        .filter(|family| !matches!(family.as_str(), "sans-serif" | "serif" | "monospace"))
        .collect();
    Font {
        family: family.into(),
        fallbacks: (!fallbacks.is_empty()).then(|| FontFallbacks(Arc::new(fallbacks))),
        weight: FontWeight::BOLD,
        ..gpui::font(".SystemUIFont")
    }
}

/// Builds display runs without interpreting caption text as markup.
pub fn caption_runs(frame: &CaptionFrame, appearance: &Appearance) -> CaptionRuns {
    let mut output = CaptionRuns::default();
    if frame.text.is_empty() {
        return output;
    }
    let style = CaptionStyle::new(appearance);
    if let Some(parts) = &frame.parts {
        push_parts(&mut output, parts, frame.lines, &style);
    } else if let Some(partial) = nonempty(frame.live_partial_text.as_deref()) {
        push_live(&mut output, frame, partial, &style);
    } else if !frame.sentences.is_empty() || frame.provisional_text.is_some() {
        push_translation(&mut output, frame, &style);
    } else {
        push_run(
            &mut output,
            &frame.text,
            &style.font,
            style.final_color,
            false,
        );
    }
    output
}

fn push_parts(
    output: &mut CaptionRuns,
    parts: &[CaptionPart],
    lines: CaptionLines,
    style: &CaptionStyle,
) {
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            push_separator(
                output,
                lines == CaptionLines::Multi && part.speaker.is_some(),
                style,
            );
        }
        if let Some(speaker) = part.speaker {
            push_label(output, speaker, &style.font);
        }
        push_run(
            output,
            &part.text,
            &style.font,
            if part.partial {
                style.partial_color
            } else {
                style.final_color
            },
            part.partial,
        );
    }
}

fn push_live(output: &mut CaptionRuns, frame: &CaptionFrame, partial: &str, style: &CaptionStyle) {
    if let Some(committed) = nonempty(frame.committed_text.as_deref()) {
        push_run(output, committed, &style.font, style.final_color, false);
        push_separator(output, false, style);
    }
    push_run(output, partial, &style.font, style.partial_color, true);
}

fn push_translation(output: &mut CaptionRuns, frame: &CaptionFrame, style: &CaptionStyle) {
    for (index, sentence) in frame.sentences.iter().enumerate() {
        let speaker = frame
            .sentence_speakers
            .as_ref()
            .and_then(|speakers| speakers.get(index))
            .copied()
            .flatten();
        push_sentence(output, sentence, speaker, index > 0, style);
    }
    push_provisional(output, frame, style);
}

fn push_sentence(
    output: &mut CaptionRuns,
    sentence: &str,
    speaker: Option<u32>,
    separated: bool,
    style: &CaptionStyle,
) {
    if separated {
        push_separator(output, speaker.is_some(), style);
    }
    if let Some(speaker) = speaker {
        push_label(output, speaker, &style.font);
    }
    push_run(output, sentence, &style.font, style.final_color, false);
}

fn push_provisional(output: &mut CaptionRuns, frame: &CaptionFrame, style: &CaptionStyle) {
    let Some(provisional) = &frame.provisional_text else {
        return;
    };
    if !frame.sentences.is_empty() {
        push_separator(output, frame.provisional_speaker.is_some(), style);
    }
    if let Some(speaker) = frame.provisional_speaker {
        push_label(output, speaker, &style.font);
    }
    push_run(output, provisional, &style.font, style.partial_color, true);
}

fn push_separator(output: &mut CaptionRuns, new_turn: bool, style: &CaptionStyle) {
    push_run(
        output,
        if new_turn { "\n" } else { " " },
        &style.font,
        style.final_color,
        false,
    );
}

fn nonempty(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.is_empty())
}

fn push_label(output: &mut CaptionRuns, speaker: u32, font: &Font) {
    push_run(
        output,
        &format!("{} ", speaker_label(speaker)),
        font,
        speaker_color(speaker),
        true,
    );
}

fn push_run(output: &mut CaptionRuns, text: &str, font: &Font, color: Hsla, bold: bool) {
    if text.is_empty() {
        return;
    }
    output.text.push_str(text);
    let mut font = font.clone();
    if bold {
        font.weight = FontWeight::BOLD;
    }
    if let Some(last) = output.runs.last_mut()
        && last.font == font
        && last.color == color
    {
        last.len += text.len();
        return;
    }
    output.runs.push(TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    });
}

#[cfg(test)]
pub(crate) fn test_frame(parts: Vec<CaptionPart>) -> CaptionFrame {
    CaptionFrame {
        text: "a b".into(),
        committed_text: None,
        live_partial_text: None,
        sentences: Vec::new(),
        parts: Some(parts),
        sentence_speakers: None,
        provisional_text: None,
        provisional_speaker: None,
        animate_newest: false,
        lines: CaptionLines::Multi,
        state: anycaptions_core::caption::CaptionState::Final,
        opacity: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_runs_use_bold_literal_text_for_speakers_and_live_words() {
        let frame = test_frame(vec![
            CaptionPart {
                text: "a".into(),
                speaker: Some(1),
                partial: false,
            },
            CaptionPart {
                text: "<b>b</b>".into(),
                speaker: Some(2),
                partial: true,
            },
        ]);
        let runs = caption_runs(&frame, &Appearance::default());
        assert_eq!(runs.text, "Pessoa 1: a\nPessoa 2: <b>b</b>");
        assert_eq!(
            runs.runs.iter().map(|run| run.len).sum::<usize>(),
            runs.text.len()
        );
        assert!(
            runs.runs
                .iter()
                .all(|run| run.font.weight == FontWeight::BOLD)
        );
        assert!(runs.text.contains("<b>b</b>"));

        let mut live = test_frame(Vec::new());
        live.parts = None;
        live.committed_text = Some("a".into());
        live.live_partial_text = Some("<b>b</b>".into());
        let runs = caption_runs(&live, &Appearance::default());
        assert_eq!(runs.text, "a <b>b</b>");
        assert!(
            runs.runs
                .iter()
                .all(|run| run.font.weight == FontWeight::BOLD)
        );
    }

    #[test]
    fn all_eight_speaker_colors_are_stable_and_higher_slots_use_eight() {
        for (index, expected) in SPEAKER_COLORS.iter().enumerate() {
            assert_eq!(speaker_color(index as u32 + 1), Hsla::from(rgb(*expected)));
        }
        assert_eq!(speaker_color(12), speaker_color(8));
    }
}
