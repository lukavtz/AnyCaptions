use super::{presenter::paginate_caption, *};
use crate::{
    domain::{CaptionMode, PT_BR},
    ipc::{CaptionUpdate, SpeakerSpan},
};

#[derive(Debug)]
struct WordCountFits {
    words_per_line: usize,
}

impl WordCountFits {
    fn new(words_per_line: usize) -> Self {
        Self { words_per_line }
    }
}

impl CaptionFits for WordCountFits {
    fn fits(&mut self, text: &str, max_lines: usize) -> bool {
        text.split_whitespace().count() <= self.words_per_line * max_lines
    }
}

fn make_presenter(mode: CaptionMode, words_per_line: usize) -> CaptionPresenter<WordCountFits> {
    let mut presenter = CaptionPresenter::new(WordCountFits::new(words_per_line));
    presenter.set_context(7, mode);
    presenter
}

fn update(
    mode: CaptionMode,
    segment_id: u32,
    revision: u32,
    text: &str,
    is_final: bool,
    speakers: Vec<SpeakerSpan>,
    provisional: bool,
) -> CaptionUpdate {
    CaptionUpdate {
        session_id: 7,
        segment_id,
        revision,
        mode,
        text: text.into(),
        is_final,
        language: is_final.then(|| "en-US".into()),
        speakers,
        provisional,
    }
}

fn auto(segment_id: u32, revision: u32, text: &str, is_final: bool) -> CaptionUpdate {
    update(
        CaptionMode::Auto,
        segment_id,
        revision,
        text,
        is_final,
        vec![],
        false,
    )
}

fn auto_speakers(
    segment_id: u32,
    revision: u32,
    text: &str,
    is_final: bool,
    speakers: Vec<SpeakerSpan>,
) -> CaptionUpdate {
    update(
        CaptionMode::Auto,
        segment_id,
        revision,
        text,
        is_final,
        speakers,
        false,
    )
}

fn translated(
    segment_id: u32,
    revision: u32,
    text: &str,
    is_final: bool,
    speakers: Vec<SpeakerSpan>,
    provisional: bool,
) -> CaptionUpdate {
    update(
        CaptionMode::Translate(PT_BR),
        segment_id,
        revision,
        text,
        is_final,
        speakers,
        provisional,
    )
}

fn sentence(segment_id: u32, revision: u32, text: &str) -> CaptionUpdate {
    translated(segment_id, revision, text, true, vec![], false)
}

fn provisional(segment_id: u32, revision: u32, text: &str) -> CaptionUpdate {
    translated(segment_id, revision, text, false, vec![], true)
}

fn span(speaker: u32, text: &str) -> SpeakerSpan {
    SpeakerSpan {
        speaker,
        text: text.into(),
    }
}

fn part(text: &str, speaker: Option<u32>, partial: bool) -> CaptionPart {
    CaptionPart {
        text: text.into(),
        speaker,
        partial,
    }
}

fn text(presenter: &CaptionPresenter<WordCountFits>) -> &str {
    &presenter.frame().unwrap().text
}

fn assert_live_split(presenter: &CaptionPresenter<WordCountFits>, committed: &str, partial: &str) {
    let frame = presenter.frame().unwrap();
    assert_eq!(frame.committed_text.as_deref(), Some(committed));
    assert_eq!(frame.live_partial_text.as_deref(), Some(partial));
}

fn assert_parts(presenter: &CaptionPresenter<WordCountFits>, expected: &[CaptionPart]) {
    assert_eq!(presenter.frame().unwrap().parts.as_deref(), Some(expected));
}

fn assert_sentences(presenter: &CaptionPresenter<WordCountFits>, expected: &[&str]) {
    let actual: Vec<&str> = presenter
        .frame()
        .unwrap()
        .sentences
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(actual.as_slice(), expected);
}

fn assert_sentence_speakers(presenter: &CaptionPresenter<WordCountFits>, expected: &[Option<u32>]) {
    assert_eq!(
        presenter.frame().unwrap().sentence_speakers.as_deref(),
        Some(expected)
    );
}

fn accept_speaker_update(
    presenter: &mut CaptionPresenter<WordCountFits>,
    segment_id: u32,
    revision: u32,
    speaker: u32,
    text: &str,
    is_final: bool,
    provisional: bool,
) {
    assert!(presenter.accept(translated(
        segment_id,
        revision,
        text,
        is_final,
        vec![span(speaker, text)],
        provisional,
    )));
}

#[test]
fn live_commit_state_preserves_full_blocks_finals_and_unlabeled_text() {
    // Two trailing live words are partial; finalization commits them and stale contexts lose.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    assert!(presenter.accept(auto(1, 1, "what we have you're launching like the", false)));
    assert_live_split(&presenter, "what we have you're launching", "like the");
    assert_eq!(text(&presenter), "what we have you're launching like the");

    assert!(presenter.accept(auto(1, 2, "what we have you're launching like the", true)));
    assert_eq!(
        presenter.frame().unwrap().live_partial_text.as_deref(),
        Some("")
    );
    assert_eq!(presenter.frame().unwrap().state, CaptionState::Final);
    assert!(!presenter.accept(auto(1, 1, "Old words", false)));
    presenter.set_context(8, CaptionMode::Auto);
    assert_eq!(text(&presenter), "");
    assert!(!presenter.accept(auto(1, 3, "Late", true)));

    // A full block is replaced directly without transition or fade state.
    let mut presenter = make_presenter(CaptionMode::Auto, 3);
    presenter.accept(auto(1, 1, "One two three", false));
    presenter.accept(auto(1, 2, "One two three four", false));
    assert_eq!(text(&presenter), "four");
    assert!(!presenter.fade_pending());
    assert_eq!(presenter.frame().unwrap().opacity, 1.0);

    // A regressive partial is ignored, but a later correction is accepted.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto(1, 1, "The current sentence", false));
    presenter.accept(auto(1, 2, "The current", false));
    assert_eq!(text(&presenter), "The current sentence");
    presenter.accept(auto(1, 3, "The corrected sentence continues", false));
    assert_eq!(text(&presenter), "The corrected sentence continues");

    // Updates without speaker spans retain the original unlabeled frame shape.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto(1, 1, "what we have you're launching", false));
    assert!(presenter.frame().unwrap().parts.is_none());
    assert_live_split(&presenter, "what we have", "you're launching");
    assert!(presenter.frame().unwrap().sentence_speakers.is_none());
}

#[test]
fn segment_layout_rolls_words_appends_finals_and_reflows() {
    // Fill the measured width before rolling over, without repeating old words.
    let mut presenter = make_presenter(CaptionMode::Auto, 5);
    presenter.accept(auto(1, 1, "One two three four five", false));
    assert_eq!(text(&presenter), "One two three four five");
    presenter.accept(auto(1, 2, "One two three four five six", false));
    assert_eq!(text(&presenter), "six");
    presenter.accept(auto(1, 3, "One two three four five six seven", true));
    assert_eq!(text(&presenter), "six seven");
    presenter.accept(auto(2, 1, "After a pause", false));
    assert_eq!(text(&presenter), "six seven After a pause");
    assert_live_split(&presenter, "six seven After", "a pause");

    // A final stays through silence and the next segment appends while it fits.
    let mut presenter = make_presenter(CaptionMode::Auto, 8);
    presenter.accept(auto(1, 1, "Vou pra casa", true));
    assert_eq!(text(&presenter), "Vou pra casa");
    assert!(!presenter.fade_pending());
    // A host timer callback or later layout cannot clear an active final caption.
    presenter.complete_clear();
    presenter.refresh_layout();
    assert_eq!(text(&presenter), "Vou pra casa");
    assert!(!presenter.fade_pending());
    presenter.accept(auto(2, 1, "Depois vou pra academia", false));
    assert_eq!(text(&presenter), "Vou pra casa Depois vou pra academia");

    // Resizing immediately reflows the retained words.
    let mut presenter = make_presenter(CaptionMode::Auto, 5);
    presenter.accept(auto(1, 1, "One two three four five", false));
    presenter.fits_mut().words_per_line = 3;
    presenter.refresh_layout();
    assert_eq!(text(&presenter), "four five");

    // Missing segments never block current speech; delayed older finals stay hidden.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto(3, 1, "Current speech", false));
    assert!(!presenter.accept(auto(1, 5, "Old final", true)));
    assert_eq!(text(&presenter), "Current speech");
    for segment in 4..150 {
        assert!(presenter.accept(auto(
            segment,
            1,
            &format!("Phrase {segment} with words"),
            false,
        )));
    }
    assert!(text(&presenter).ends_with("Phrase 149 with words"));
    assert!(text(&presenter).split_whitespace().count() <= 20);
    assert!(!text(&presenter).contains("Old final"));
}

#[test]
fn clear_lifecycle_fades_cancels_and_honors_reduced_motion() {
    // HTML-shaped input remains literal, and ordinary clear fades before emptying.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    let literal = "<img src=x onerror=\"alert(1)\">";
    presenter.accept(auto(1, 1, literal, true));
    assert_eq!(text(&presenter), literal);
    presenter.clear_with_reduced_motion(false);
    assert_eq!(presenter.frame().unwrap().opacity, 0.0);
    assert_eq!(text(&presenter), literal);
    assert!(presenter.fade_pending());
    assert_eq!(CLEAR_FADE.as_millis(), 120);
    presenter.complete_clear();
    assert_eq!(text(&presenter), "");
    assert!(!presenter.fade_pending());

    // New input cancels an outgoing fade; a stale timer cannot clear the new caption.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto(1, 1, "First phrase", true));
    presenter.clear();
    assert_eq!(presenter.frame().unwrap().opacity, 0.0);
    presenter.accept(auto(2, 1, "New speech", false));
    presenter.complete_clear();
    assert_eq!(text(&presenter), "New speech");
    assert_eq!(presenter.frame().unwrap().opacity, 1.0);
    assert!(!presenter.fade_pending());

    // Reduced motion takes the immediate clear path with no pending fade.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto(1, 1, "Some speech", true));
    presenter.clear_with_reduced_motion(true);
    assert_eq!(text(&presenter), "");
    assert_eq!(presenter.frame().unwrap().opacity, 1.0);
    assert!(!presenter.fade_pending());
}

#[test]
fn translated_history_rolls_sentences_and_preserves_long_text() {
    // Sentences flow across three lines; only the newest animates and overflow drops oldest.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 2);
    presenter.accept(sentence(1, 1, "Primeira frase."));
    assert_eq!(text(&presenter), "Primeira frase.");
    assert_eq!(presenter.frame().unwrap().lines, CaptionLines::Multi);
    assert_eq!(presenter.frame().unwrap().state, CaptionState::Final);
    presenter.accept(sentence(2, 1, "Segunda frase."));
    assert_eq!(text(&presenter), "Primeira frase. Segunda frase.");
    presenter.accept(sentence(3, 1, "Terceira frase."));
    assert_sentences(
        &presenter,
        &["Primeira frase.", "Segunda frase.", "Terceira frase."],
    );
    assert!(presenter.frame().unwrap().animate_newest);

    presenter.refresh_layout();
    assert!(!presenter.frame().unwrap().animate_newest);
    presenter.accept(sentence(4, 1, "Quarta frase."));
    assert!(presenter.frame().unwrap().animate_newest);
    assert_eq!(
        text(&presenter),
        "Segunda frase. Terceira frase. Quarta frase."
    );
    presenter.accept(sentence(5, 1, "Quinta frase."));
    let rolled = "Terceira frase. Quarta frase. Quinta frase.";
    assert_eq!(text(&presenter), rolled);
    // No pending fade exists: a stale host callback and later relayout keep the final text.
    assert!(!presenter.fade_pending());
    presenter.complete_clear();
    presenter.refresh_layout();
    assert_eq!(text(&presenter), rolled);
    assert!(!presenter.fade_pending());

    // Pagination keeps every word once and never splits a word across pages.
    let input = "Uma tradução extensa deve continuar em blocos ordenados com todas as palavras preservadas sem criar histórico infinito";
    let mut fits = WordCountFits::new(6);
    let blocks = paginate_caption(input, &mut fits, 1);
    assert!(blocks.len() > 1);
    assert_eq!(blocks.join(" "), input);
    assert!(
        blocks
            .iter()
            .all(|block| block.split_whitespace().count() <= 6)
    );
}

#[test]
fn live_speaker_trust_turns_and_label_width_match_frontend() {
    // Unknown speakers stay unlabeled; known turns split and mismatched spans are distrusted.
    let mut presenter = make_presenter(CaptionMode::Auto, 20);
    presenter.accept(auto_speakers(
        1,
        1,
        "well so",
        false,
        vec![span(0, "well so")],
    ));
    assert!(presenter.frame().unwrap().parts.is_none());
    assert_eq!(text(&presenter), "well so");

    presenter.accept(auto_speakers(
        1,
        2,
        "well so what now then",
        false,
        vec![span(1, "well so what"), span(2, "now then")],
    ));
    assert_eq!(text(&presenter), "well so what now then");
    assert_live_split(&presenter, "well so what", "now then");
    assert_parts(
        &presenter,
        &[
            part("well so what", Some(1), false),
            part("now then", Some(2), true),
        ],
    );

    presenter.accept(auto_speakers(
        1,
        3,
        "well so what now then.",
        true,
        vec![span(1, "well so what"), span(2, "now then.")],
    ));
    assert_eq!(presenter.frame().unwrap().state, CaptionState::Final);
    assert_parts(
        &presenter,
        &[
            part("well so what", Some(1), false),
            part("now then.", Some(2), false),
        ],
    );

    presenter.accept(auto_speakers(2, 1, "and", false, vec![span(2, "and")]));
    assert_parts(
        &presenter,
        &[
            part("well so what", Some(1), false),
            part("now then.", Some(2), false),
            part("and", None, true),
        ],
    );

    presenter.accept(auto_speakers(
        2,
        2,
        "and then some",
        false,
        vec![span(3, "and then")],
    ));
    let parts = presenter.frame().unwrap().parts.clone().unwrap();
    assert_eq!(
        &parts[parts.len() - 2..],
        [
            part("now then. and", Some(2), false),
            part("then some", None, true),
        ]
    );

    // Label tokens consume width and the first visible word starts with its own label.
    let mut labeled = make_presenter(CaptionMode::Auto, 6);
    labeled.accept(auto_speakers(
        1,
        1,
        "a b c d e f g",
        true,
        vec![span(1, "a b c d"), span(2, "e f g")],
    ));
    assert_eq!(text(&labeled), "e f g");
    assert_parts(&labeled, &[part("e f g", Some(2), false)]);
    let mut plain = make_presenter(CaptionMode::Auto, 6);
    plain.accept(auto(1, 1, "a b c d e f", true));
    assert_eq!(text(&plain), "a b c d e f");
}

#[test]
fn speaker_layout_modes_label_turns_and_roll_whole_turns() {
    // Translation labels come from source spans and continue unlabeled within the same turn.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 20);
    for (segment, speaker, value) in [
        (1, 1, "Você conseguiu terminar?"),
        (2, 2, "Sim, terminei agora."),
        (3, 2, "E funciona bem."),
    ] {
        accept_speaker_update(&mut presenter, segment, 1, speaker, value, true, false);
    }
    assert_sentences(
        &presenter,
        &[
            "Você conseguiu terminar?",
            "Sim, terminei agora.",
            "E funciona bem.",
        ],
    );
    assert_sentence_speakers(&presenter, &[Some(1), Some(2), None]);
    assert_eq!(presenter.frame().unwrap().lines, CaptionLines::Multi);
    presenter.accept(sentence(4, 1, "Ok."));
    assert_sentence_speakers(&presenter, &[Some(1), Some(2), None, None]);
    let mut plain = make_presenter(CaptionMode::Translate(PT_BR), 20);
    plain.accept(sentence(1, 1, "Olá."));
    assert!(plain.frame().unwrap().sentence_speakers.is_none());

    // Live speaker mode gives every turn a line, drops whole old turns, and toggles immediately.
    let mut presenter = make_presenter(CaptionMode::Auto, 8);
    presenter.set_speaker_labels(true);
    presenter.accept(auto_speakers(
        1,
        1,
        "embaixo também tripla tá disparo sim",
        true,
        vec![
            span(1, "embaixo também"),
            span(2, "tripla"),
            span(3, "tá disparo sim"),
        ],
    ));
    assert_eq!(presenter.frame().unwrap().lines, CaptionLines::Multi);
    assert_parts(
        &presenter,
        &[
            part("embaixo também", Some(1), false),
            part("tripla", Some(2), false),
            part("tá disparo sim", Some(3), false),
        ],
    );
    presenter.accept(auto_speakers(
        2,
        1,
        "é por causa do vale",
        false,
        vec![span(1, "é por causa do vale")],
    ));
    let parts = presenter.frame().unwrap().parts.clone().unwrap();
    assert_eq!(
        parts.iter().map(|part| part.speaker).collect::<Vec<_>>(),
        [Some(2), Some(3), Some(1), None]
    );
    assert_eq!(
        text(&presenter),
        "tripla tá disparo sim é por causa do vale"
    );

    let mut unknown = make_presenter(CaptionMode::Auto, 8);
    unknown.set_speaker_labels(true);
    unknown.accept(auto_speakers(
        1,
        1,
        "well so",
        false,
        vec![span(0, "well so")],
    ));
    assert_eq!(unknown.frame().unwrap().lines, CaptionLines::Multi);
    assert_parts(&unknown, &[part("well so", None, true)]);
    unknown.set_speaker_labels(false);
    assert_eq!(unknown.frame().unwrap().lines, CaptionLines::Single);
    assert!(unknown.frame().unwrap().parts.is_none());

    // Translated turns also roll by their own line counts only while speaker mode is active.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 6);
    presenter.set_speaker_labels(true);
    for (segment, speaker, value) in [
        (1, 1, "Você conseguiu terminar?"),
        (2, 2, "Sim, terminei agora."),
        (3, 2, "E funciona bem."),
    ] {
        accept_speaker_update(&mut presenter, segment, 1, speaker, value, true, false);
    }
    assert_sentence_speakers(&presenter, &[Some(1), Some(2), None]);
    assert_eq!(presenter.frame().unwrap().sentences.len(), 3);
    let value = "Provavelmente não, só terminar o trabalho.";
    accept_speaker_update(&mut presenter, 4, 1, 3, value, true, false);
    assert_sentences(
        &presenter,
        &[
            "E funciona bem.",
            "Provavelmente não, só terminar o trabalho.",
        ],
    );
    assert_eq!(
        presenter
            .frame()
            .unwrap()
            .sentence_speakers
            .as_ref()
            .unwrap()
            .last(),
        Some(&Some(3))
    );
}

#[test]
fn provisional_translation_replaces_overflows_and_resets_like_frontend() {
    // A lower-revision final supersedes provisional text; stale and post-final interims lose.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 20);
    presenter.accept(sentence(1, 1, "Primeira frase."));
    assert!(presenter.accept(provisional(2, 3, "Você conseguiu")));
    assert_sentences(&presenter, &["Primeira frase."]);
    assert_eq!(
        presenter.frame().unwrap().provisional_text.as_deref(),
        Some("Você conseguiu")
    );
    assert_eq!(text(&presenter), "Primeira frase. Você conseguiu");
    assert!(presenter.accept(provisional(2, 5, "Você conseguiu terminar")));
    assert_eq!(
        presenter.frame().unwrap().provisional_text.as_deref(),
        Some("Você conseguiu terminar")
    );
    assert!(!presenter.accept(provisional(2, 4, "Você")));
    assert!(presenter.accept(translated(
        2,
        1,
        "Você conseguiu terminar?",
        false,
        vec![],
        false,
    )));
    assert!(presenter.accept(sentence(2, 2, "Sim, agora.")));
    assert!(presenter.frame().unwrap().provisional_text.is_none());
    assert_sentences(
        &presenter,
        &["Primeira frase.", "Você conseguiu terminar?", "Sim, agora."],
    );
    assert!(!presenter.accept(provisional(2, 9, "tarde demais")));

    // Older sentences overflow while the provisional turn stays and follows speaker labels.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 4);
    presenter.set_speaker_labels(true);
    for (segment, speaker, value) in [(1, 1, "Um dois."), (2, 2, "Três quatro.")] {
        accept_speaker_update(&mut presenter, segment, 1, speaker, value, true, false);
    }
    accept_speaker_update(&mut presenter, 3, 2, 3, "cinco seis sete", false, true);
    assert_sentences(&presenter, &["Três quatro."]);
    assert_eq!(
        presenter.frame().unwrap().provisional_text.as_deref(),
        Some("cinco seis sete")
    );
    assert_eq!(presenter.frame().unwrap().provisional_speaker, Some(3));
    accept_speaker_update(&mut presenter, 3, 3, 2, "cinco seis", false, true);
    assert_eq!(presenter.frame().unwrap().provisional_speaker, None);

    // A new utterance replaces the old provisional even before its final; reset removes it.
    let mut presenter = make_presenter(CaptionMode::Translate(PT_BR), 20);
    presenter.accept(provisional(1, 2, "Primeira em andamento"));
    presenter.accept(provisional(2, 1, "Segunda em andamento"));
    assert!(presenter.frame().unwrap().sentences.is_empty());
    assert_eq!(
        presenter.frame().unwrap().provisional_text.as_deref(),
        Some("Segunda em andamento")
    );
    presenter.accept(sentence(2, 1, "Segunda fechada."));
    assert!(presenter.frame().unwrap().provisional_text.is_none());
    assert_sentences(&presenter, &["Segunda fechada."]);
    presenter.clear_immediate();
    assert_eq!(text(&presenter), "");
    assert!(presenter.frame().unwrap().provisional_text.is_none());
}
