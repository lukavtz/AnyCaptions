mod presenter;

pub use presenter::{
    CLEAR_FADE, CaptionFits, CaptionFrame, CaptionLines, CaptionPart, CaptionPresenter,
    CaptionState, speaker_label,
};

#[cfg(test)]
mod tests;
