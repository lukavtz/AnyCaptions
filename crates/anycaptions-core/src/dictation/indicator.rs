use serde::Serialize;

use crate::{dictation::runtime::DictationEventPayload, ipc::Controller, window_geometry::Rect};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndicatorSnapshot {
    pub revision: u64,
    pub ui_language: String,
    pub event: DictationEventPayload,
}

impl IndicatorSnapshot {
    /// Cache the newest event with the upstream one-based revision sequence.
    pub fn publish(
        previous: Option<&IndicatorSnapshot>,
        controller: &Controller,
        event: DictationEventPayload,
    ) -> Self {
        Self {
            revision: previous.map_or(1, |snapshot| snapshot.revision + 1),
            ui_language: controller
                .snapshot()
                .map(|snapshot| snapshot.ui_language.tag().to_owned())
                .unwrap_or_else(|_| "pt-br".into()),
            event,
        }
    }
}

/// Places the logical-pixel indicator in its monitor work area. Prefer above
/// the captions, then below, and finally bottom-align when neither fits.
pub fn place_indicator(anchor: Rect, screen: Rect, compact: bool) -> Rect {
    let gap = 12.0;
    let width = (if compact { 112.0_f64 } else { 640.0 }).min((screen.width - 24.0).max(1.0));
    let height = (if compact { 56.0_f64 } else { 136.0 }).min(screen.height);
    let x =
        (anchor.x + (anchor.width - width) / 2.0).clamp(screen.x, screen.x + screen.width - width);
    let above = anchor.y - gap - height;
    let below = anchor.y + anchor.height + gap;
    let y = if above >= screen.y {
        above
    } else if below + height <= screen.y + screen.height {
        below
    } else {
        screen.y + screen.height - height
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}
