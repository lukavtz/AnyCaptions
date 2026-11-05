use std::{collections::HashSet, ops::Range, time::Duration};

use crate::{domain::CaptionMode, ipc::CaptionUpdate};

pub const CLEAR_FADE: Duration = Duration::from_millis(120);
const SENTENCE_LINES: usize = 3;
const MAX_CAPTION_UTF16_LEN: usize = 4000;

/// Measures whether the rendered caption fits in `max_lines` at the current width.
pub trait CaptionFits {
    fn fits(&mut self, text: &str, max_lines: usize) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionLines {
    Single,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionState {
    Final,
    Partial,
}

/// A visible run. `speaker` is present only where a generic label starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionPart {
    pub text: String,
    pub speaker: Option<u32>,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptionFrame {
    pub text: String,
    pub committed_text: Option<String>,
    pub live_partial_text: Option<String>,
    pub sentences: Vec<String>,
    pub parts: Option<Vec<CaptionPart>>,
    pub sentence_speakers: Option<Vec<Option<u32>>>,
    pub provisional_text: Option<String>,
    pub provisional_speaker: Option<u32>,
    pub animate_newest: bool,
    pub lines: CaptionLines,
    pub state: CaptionState,
    pub opacity: f32,
}

/// Generic diarizer slot label used both for display and width measurement.
///
/// Localization replaces this formatter at the presentation boundary.
pub fn speaker_label(speaker: u32) -> String {
    format!("Pessoa {speaker}:")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptionItem {
    text: String,
    speaker: u32,
}

#[derive(Debug, Clone, Default)]
struct Block {
    text: String,
    committed_text: Option<String>,
    live_partial_text: Option<String>,
    parts: Option<Vec<CaptionPart>>,
}

#[derive(Debug, Default)]
struct ActiveLiveSegment {
    words: Vec<CaptionItem>,
    consumed_words: usize,
}

#[derive(Debug)]
struct Turn<T> {
    items: Vec<T>,
}

/// Streaming caption state, independent of a GUI or asynchronous runtime.
pub struct CaptionPresenter<F: CaptionFits> {
    session_id: u32,
    mode: CaptionMode,
    speaker_labels_mode: bool,
    sentences: Vec<CaptionItem>,
    provisional: Option<CaptionItem>,
    active_provisional: bool,
    animate_newest_sentence: bool,
    active_segment_id: u32,
    active_revision: Option<u32>,
    active_final: bool,
    finalized_live_words: Vec<CaptionItem>,
    active_live_segment: Option<ActiveLiveSegment>,
    current: Option<Block>,
    displayed_text: String,
    last_frame: Option<CaptionFrame>,
    fade_pending: bool,
    fits_caption: F,
}

impl<F: CaptionFits> CaptionPresenter<F> {
    pub fn new(fits_caption: F) -> Self {
        Self {
            session_id: 0,
            mode: CaptionMode::Auto,
            speaker_labels_mode: false,
            sentences: Vec::new(),
            provisional: None,
            active_provisional: false,
            animate_newest_sentence: false,
            active_segment_id: 0,
            active_revision: None,
            active_final: false,
            finalized_live_words: Vec::new(),
            active_live_segment: None,
            current: None,
            displayed_text: String::new(),
            last_frame: None,
            fade_pending: false,
            fits_caption,
        }
    }

    pub fn frame(&self) -> Option<&CaptionFrame> {
        self.last_frame.as_ref()
    }

    /// Allows the host to update GPUI width/font settings before `refresh_layout`.
    pub fn fits_mut(&mut self) -> &mut F {
        &mut self.fits_caption
    }

    pub fn fade_pending(&self) -> bool {
        self.fade_pending
    }

    pub fn set_context(&mut self, session_id: u32, mode: CaptionMode) {
        if session_id == self.session_id && mode == self.mode {
            return;
        }
        self.clear_immediate();
        self.session_id = session_id;
        self.mode = mode;
    }

    pub fn set_speaker_labels(&mut self, enabled: bool) {
        if enabled == self.speaker_labels_mode {
            return;
        }
        self.speaker_labels_mode = enabled;
        self.refresh_layout();
    }

    pub fn accept(&mut self, update: CaptionUpdate) -> bool {
        if !self.admits_update(&update) {
            return false;
        }
        let new_segment = self.advance_update(&update);
        if self.mode == CaptionMode::Auto {
            self.accept_live(update, new_segment);
        } else {
            self.accept_translation(update);
        }
        self.render(false);
        true
    }

    fn admits_update(&self, update: &CaptionUpdate) -> bool {
        if update.session_id != self.session_id
            || update.mode != self.mode
            || update.segment_id == 0
            || update.text.encode_utf16().count() > MAX_CAPTION_UTF16_LEN
        {
            return false;
        }
        let stale_revision = self.active_revision.is_some_and(|revision| {
            update.revision <= revision
                // Translation finals have their own lower revision sequence.
                && !(self.active_provisional && !update.provisional)
        });
        update.segment_id >= self.active_segment_id
            && (update.segment_id != self.active_segment_id
                || (!self.active_final && !stale_revision))
    }

    fn advance_update(&mut self, update: &CaptionUpdate) -> bool {
        let new_segment = update.segment_id != self.active_segment_id;
        if new_segment {
            self.commit_active_live_segment();
        }
        self.active_segment_id = update.segment_id;
        self.active_revision = Some(update.revision);
        self.active_final = update.is_final;
        new_segment
    }

    fn accept_translation(&mut self, update: CaptionUpdate) {
        let sentence = update.text.trim();
        let speaker = if update.speakers.len() == 1 {
            update.speakers[0].speaker
        } else {
            0
        };
        self.active_provisional = update.provisional;
        if update.provisional {
            self.provisional = (!sentence.is_empty()).then(|| CaptionItem {
                text: sentence.into(),
                speaker,
            });
        } else {
            self.provisional = None;
            if !sentence.is_empty() {
                self.sentences.push(CaptionItem {
                    text: sentence.into(),
                    speaker,
                });
                self.animate_newest_sentence = true;
            }
        }
        self.layout_sentences();
    }

    fn accept_live(&mut self, update: CaptionUpdate, new_segment: bool) {
        let incoming_words = words_of(&update);
        let previous_words = self
            .active_live_segment
            .as_ref()
            .map(|segment| segment.words.as_slice())
            .unwrap_or_default();
        let regressive_partial = !update.is_final
            && incoming_words.len() < previous_words.len()
            && incoming_words
                .iter()
                .zip(previous_words)
                .all(|(incoming, previous)| incoming.text == previous.text);
        let consumed_words = if new_segment {
            0
        } else {
            self.active_live_segment
                .as_ref()
                .map_or(0, |segment| segment.consumed_words)
        };
        self.active_live_segment = Some(ActiveLiveSegment {
            words: if regressive_partial {
                previous_words.to_vec()
            } else {
                incoming_words
            },
            consumed_words,
        });
        self.layout_live_caption();
        if update.is_final {
            self.commit_active_live_segment();
        }
    }

    pub fn refresh_layout(&mut self) {
        if self.mode != CaptionMode::Auto {
            if self.shown_sentences().is_empty() {
                return;
            }
            self.layout_sentences();
            self.render(false);
            return;
        }
        if self.live_words().is_empty() {
            return;
        }
        self.layout_live_caption();
        self.render(false);
    }

    /// Starts the 120 ms outgoing fade. The GPUI host calls `complete_clear`
    /// from its timer; newer input cancels the pending fade.
    pub fn clear(&mut self) {
        self.reset_caption();
        self.render(false);
    }

    /// Routes the host's reduced-motion preference to the appropriate clear path.
    pub fn clear_with_reduced_motion(&mut self, reduced_motion: bool) {
        if reduced_motion {
            self.clear_immediate();
        } else {
            self.clear();
        }
    }

    /// Immediate path used for reduced motion, context changes, and disposal.
    pub fn clear_immediate(&mut self) {
        self.reset_caption();
        // Reuse the completion path without emitting an intermediate fade.
        self.fade_pending = true;
        self.complete_clear();
    }

    /// Completes an outgoing fade. Does nothing when newer input cancelled it.
    pub fn complete_clear(&mut self) {
        if self.fade_pending {
            self.render(true);
        }
    }

    fn reset_caption(&mut self) {
        self.active_segment_id = 0;
        self.active_revision = None;
        self.active_final = false;
        self.finalized_live_words.clear();
        self.active_live_segment = None;
        self.sentences.clear();
        self.provisional = None;
        self.active_provisional = false;
        self.animate_newest_sentence = false;
        self.current = None;
    }

    fn shown_sentences(&self) -> Vec<CaptionItem> {
        let mut shown = self.sentences.clone();
        shown.extend(self.provisional.iter().cloned());
        shown
    }

    fn sentences_fit(&mut self, sentences: &[CaptionItem]) -> bool {
        if !self.speaker_labels_mode {
            return self
                .fits_caption
                .fits(&render_items(sentences), SENTENCE_LINES);
        }
        let turns = turns_of(sentences, |sentence| sentence.speaker);
        let rendered: Vec<String> = turns.iter().map(|turn| render_items(&turn.items)).collect();
        if rendered.len() == 1 {
            return self.fits_caption.fits(&rendered[0], SENTENCE_LINES);
        }
        keep_recent_turns(&rendered, &mut self.fits_caption, SENTENCE_LINES) == 0
    }

    fn layout_sentences(&mut self) {
        while self.shown_sentences().len() > 1 {
            let shown = self.shown_sentences();
            if self.sentences_fit(&shown) {
                break;
            }
            self.sentences.remove(0);
        }

        let shown = self.shown_sentences();
        let mut text = shown
            .iter()
            .map(|sentence| sentence.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty()
            && !self
                .fits_caption
                .fits(&render_items(&shown), SENTENCE_LINES)
        {
            // A single sentence taller than its area keeps only its tail.
            let only = &shown[0];
            text = paginate_caption(&only.text, &mut self.fits_caption, SENTENCE_LINES)
                .pop()
                .unwrap_or_else(|| only.text.clone());
            let tail = CaptionItem {
                text: text.clone(),
                speaker: only.speaker,
            };
            if self.provisional.is_some() && self.sentences.is_empty() {
                self.provisional = Some(tail);
            } else {
                self.sentences = vec![tail];
            }
        }
        self.current = (!text.is_empty()).then(|| Block {
            text,
            ..Block::default()
        });
    }

    fn live_words(&self) -> Vec<CaptionItem> {
        let active = self
            .active_live_segment
            .as_ref()
            .map(|segment| &segment.words[segment.consumed_words..])
            .unwrap_or_default();
        let mut words = Vec::with_capacity(self.finalized_live_words.len() + active.len());
        words.extend(self.finalized_live_words.iter().cloned());
        words.extend(active.iter().cloned());
        words
    }

    fn commit_active_live_segment(&mut self) {
        let Some(segment) = self.active_live_segment.take() else {
            return;
        };
        self.finalized_live_words
            .extend(segment.words.into_iter().skip(segment.consumed_words));
    }

    fn consume_live_words(&mut self, count: usize) {
        let finalized = count.min(self.finalized_live_words.len());
        self.finalized_live_words.drain(..finalized);
        let active = count - finalized;
        if active > 0
            && let Some(segment) = &mut self.active_live_segment
        {
            segment.consumed_words = segment.words.len().min(segment.consumed_words + active);
        }
    }

    fn layout_live_caption(&mut self) {
        let visible = if self.speaker_labels_mode {
            self.visible_live_turns()
        } else {
            self.visible_live_line()
        };
        let active_visible_words = self
            .active_live_segment
            .as_ref()
            .map_or(0, |segment| segment.words.len() - segment.consumed_words);
        let trailing = if self.active_final {
            0
        } else {
            2.min(active_visible_words)
        };
        let split_at = visible.len().saturating_sub(trailing);
        let labeled = self.speaker_labels_mode || visible.iter().any(|word| word.speaker > 0);
        self.current = (!visible.is_empty()).then(|| Block {
            text: plain_words(&visible),
            committed_text: Some(plain_words(&visible[..split_at])),
            live_partial_text: Some(plain_words(&visible[split_at..])),
            parts: labeled.then(|| parts_of(&visible, split_at)),
        });
    }

    fn visible_live_line(&mut self) -> Vec<CaptionItem> {
        let words = self.live_words();
        let pages = paginate_ranges(&words, render_items, &mut self.fits_caption, 1);
        let discarded = discarded_page_items(&pages);
        if discarded > 0 {
            self.consume_live_words(discarded);
        }
        pages
            .last()
            .map(|page| words[page.clone()].to_vec())
            .unwrap_or_default()
    }

    fn visible_live_turns(&mut self) -> Vec<CaptionItem> {
        let words = self.live_words();
        let turns = turns_of(&words, |word| word.speaker);
        let rendered: Vec<String> = turns.iter().map(|turn| render_items(&turn.items)).collect();
        let dropped = keep_recent_turns(&rendered, &mut self.fits_caption, SENTENCE_LINES);
        let mut discarded: usize = turns[..dropped].iter().map(|turn| turn.items.len()).sum();
        let mut visible = words[discarded..].to_vec();

        if turns.len() - dropped == 1
            && lines_needed(
                rendered.last().unwrap(),
                &mut self.fits_caption,
                SENTENCE_LINES,
            ) > SENTENCE_LINES
        {
            let pages = paginate_ranges(
                &visible,
                render_items,
                &mut self.fits_caption,
                SENTENCE_LINES,
            );
            discarded += discarded_page_items(&pages);
            visible = pages
                .last()
                .map(|page| visible[page.clone()].to_vec())
                .unwrap_or_default();
        }
        if discarded > 0 {
            self.consume_live_words(discarded);
        }
        visible
    }

    fn render(&mut self, immediate: bool) {
        if self.defer_empty_frame(immediate) {
            return;
        }
        self.fade_pending = false;
        self.displayed_text = self
            .current
            .as_ref()
            .map(|current| current.text.clone())
            .unwrap_or_default();
        self.last_frame = Some(self.assemble_frame());
        self.animate_newest_sentence = false;
    }

    fn defer_empty_frame(&mut self, immediate: bool) -> bool {
        if self.current.is_some() || immediate || self.displayed_text.is_empty() {
            return false;
        }
        if self.fade_pending {
            return true;
        }
        if let Some(frame) = &mut self.last_frame {
            frame.opacity = 0.0;
        }
        self.fade_pending = true;
        true
    }

    fn assemble_frame(&self) -> CaptionFrame {
        let sentence_mode = self.mode != CaptionMode::Auto;
        let shown = self.shown_sentences();
        let sentence_starts = label_starts(shown.iter().map(|sentence| sentence.speaker));
        let current = self.current.clone();
        CaptionFrame {
            text: self.displayed_text.clone(),
            committed_text: current
                .as_ref()
                .and_then(|current| current.committed_text.clone()),
            live_partial_text: current
                .as_ref()
                .and_then(|current| current.live_partial_text.clone()),
            parts: current.and_then(|current| current.parts),
            sentences: self.frame_sentences(sentence_mode),
            sentence_speakers: self.frame_sentence_speakers(sentence_mode, &sentence_starts),
            provisional_text: self.frame_provisional_text(sentence_mode),
            provisional_speaker: self.frame_provisional_speaker(
                sentence_mode,
                shown.len(),
                &sentence_starts,
            ),
            animate_newest: self.animate_newest_sentence,
            lines: if sentence_mode || self.speaker_labels_mode {
                CaptionLines::Multi
            } else {
                CaptionLines::Single
            },
            state: if sentence_mode || self.active_final {
                CaptionState::Final
            } else {
                CaptionState::Partial
            },
            opacity: 1.0,
        }
    }

    fn frame_sentences(&self, sentence_mode: bool) -> Vec<String> {
        if !sentence_mode {
            return Vec::new();
        }
        self.sentences
            .iter()
            .map(|sentence| sentence.text.clone())
            .collect()
    }

    fn frame_sentence_speakers(
        &self,
        sentence_mode: bool,
        starts: &HashSet<usize>,
    ) -> Option<Vec<Option<u32>>> {
        if !sentence_mode || starts.is_empty() {
            return None;
        }
        Some(
            self.sentences
                .iter()
                .enumerate()
                .map(|(index, sentence)| starts.contains(&index).then_some(sentence.speaker))
                .collect(),
        )
    }

    fn frame_provisional_text(&self, sentence_mode: bool) -> Option<String> {
        if !sentence_mode {
            return None;
        }
        self.provisional
            .as_ref()
            .map(|sentence| sentence.text.clone())
    }

    fn frame_provisional_speaker(
        &self,
        sentence_mode: bool,
        shown_len: usize,
        starts: &HashSet<usize>,
    ) -> Option<u32> {
        if !sentence_mode {
            return None;
        }
        let provisional = self.provisional.as_ref()?;
        starts
            .contains(&(shown_len - 1))
            .then_some(provisional.speaker)
    }
}

fn caption_words(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// Words are trusted only when the span word count equals the caption word count.
fn words_of(update: &CaptionUpdate) -> Vec<CaptionItem> {
    let plain = caption_words(&update.text);
    if update.speakers.is_empty() {
        return unlabeled_items(plain);
    }
    let tagged: Vec<CaptionItem> = update
        .speakers
        .iter()
        .flat_map(|span| {
            caption_words(&span.text)
                .into_iter()
                .map(|text| CaptionItem {
                    text: text.into(),
                    speaker: span.speaker,
                })
        })
        .collect();
    if tagged.len() != plain.len() {
        return unlabeled_items(plain);
    }
    tagged
}

fn unlabeled_items(words: Vec<&str>) -> Vec<CaptionItem> {
    words
        .into_iter()
        .map(|text| CaptionItem {
            text: text.into(),
            speaker: 0,
        })
        .collect()
}

/// Labels start at each known speaker change; unknown words continue the turn.
fn label_starts(speakers: impl IntoIterator<Item = u32>) -> HashSet<usize> {
    let mut starts = HashSet::new();
    let mut current = 0;
    for (index, speaker) in speakers.into_iter().enumerate() {
        if speaker > 0 && speaker != current {
            starts.insert(index);
            current = speaker;
        }
    }
    starts
}

fn render_items(items: &[CaptionItem]) -> String {
    let starts = label_starts(items.iter().map(|item| item.speaker));
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if starts.contains(&index) {
                format!("{} {}", speaker_label(item.speaker), item.text)
            } else {
                item.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn plain_words(words: &[CaptionItem]) -> String {
    words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn turns_of<T: Clone>(items: &[T], speaker_of: impl Fn(&T) -> u32) -> Vec<Turn<T>> {
    let starts = label_starts(items.iter().map(&speaker_of));
    let mut turns: Vec<Turn<T>> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if index == 0 || starts.contains(&index) {
            turns.push(Turn {
                items: vec![item.clone()],
            });
        } else {
            turns.last_mut().unwrap().items.push(item.clone());
        }
    }
    turns
}

fn lines_needed(text: &str, fits: &mut impl CaptionFits, limit: usize) -> usize {
    for lines in 1..=limit {
        if fits.fits(text, lines) {
            return lines;
        }
    }
    limit + 1
}

/// Returns how many oldest turns must leave; one oversized newest turn remains.
fn keep_recent_turns(rendered: &[String], fits: &mut impl CaptionFits, max_lines: usize) -> usize {
    let mut used = 0;
    for index in (0..rendered.len()).rev() {
        used += lines_needed(&rendered[index], fits, max_lines);
        if used > max_lines {
            return if index == rendered.len() - 1 {
                index
            } else {
                index + 1
            };
        }
    }
    0
}

fn discarded_page_items(pages: &[Range<usize>]) -> usize {
    pages[..pages.len().saturating_sub(1)]
        .iter()
        .map(|page| page.end - page.start)
        .sum()
}

/// Binary-search the longest fitting slice, then repeat from its end.
fn paginate_ranges<T>(
    items: &[T],
    render: impl Fn(&[T]) -> String,
    fits: &mut impl CaptionFits,
    max_lines: usize,
) -> Vec<Range<usize>> {
    let mut pages = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let mut lower = start + 1;
        let mut upper = items.len();
        let mut end = lower;
        while lower <= upper {
            let middle = (lower + upper) / 2;
            if fits.fits(&render(&items[start..middle]), max_lines) {
                end = middle;
                lower = middle + 1;
            } else {
                upper = middle - 1;
            }
        }
        pages.push(start..end);
        start = end;
    }
    pages
}

pub(crate) fn paginate_caption(
    text: &str,
    fits: &mut impl CaptionFits,
    max_lines: usize,
) -> Vec<String> {
    let words = caption_words(text);
    paginate_ranges(&words, |slice| slice.join(" "), fits, max_lines)
        .into_iter()
        .map(|page| words[page].join(" "))
        .collect()
}

fn parts_of(words: &[CaptionItem], split_at: usize) -> Vec<CaptionPart> {
    let starts = label_starts(words.iter().map(|word| word.speaker));
    let mut parts: Vec<CaptionPart> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let partial = index >= split_at;
        if let Some(last) = parts.last_mut()
            && !starts.contains(&index)
            && last.partial == partial
        {
            last.text.push(' ');
            last.text.push_str(&word.text);
            continue;
        }
        parts.push(CaptionPart {
            text: word.text.clone(),
            speaker: starts.contains(&index).then_some(word.speaker),
            partial,
        });
    }
    parts
}
