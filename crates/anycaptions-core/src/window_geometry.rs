//! Logical window geometry shared by native window adapters.
//!
//! Positions and sizes remain in logical pixels. `WorkArea::scale` lets an
//! adapter convert them to physical pixels only at the operating-system boundary.

use crate::preferences::SavedGeometry;

const DEFAULT_WIDTH: f64 = 800.0;
const CAPTION_HEIGHT: f64 = 160.0;
const MIN_WINDOW_WIDTH: f64 = 320.0;
const BOTTOM_MARGIN: f64 = 32.0;
const MIN_VISIBLE_WIDTH: f64 = 32.0;
const MIN_VISIBLE_HEIGHT: f64 = 32.0;

/// A rectangle in logical desktop coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(
        x: impl Into<f64>,
        y: impl Into<f64>,
        width: impl Into<f64>,
        height: impl Into<f64>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            width: width.into(),
            height: height.into(),
        }
    }

    /// Whether at least the requested logical width and height intersect `area`.
    pub fn overlaps_at_least(
        &self,
        width: impl Into<f64>,
        height: impl Into<f64>,
        area: Self,
    ) -> bool {
        let left = self.x.max(area.x);
        let top = self.y.max(area.y);
        let right = (self.x + self.width).min(area.x + area.width);
        let bottom = (self.y + self.height).min(area.y + area.height);
        right - left >= width.into() && bottom - top >= height.into()
    }
}

/// A monitor's logical work area and its logical-to-physical scale factor.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkArea {
    pub name: Option<String>,
    pub area: Rect,
    pub scale: f64,
}

impl WorkArea {
    pub fn new(name: impl Into<String>, area: Rect, scale: f64) -> Self {
        Self {
            name: Some(name.into()),
            area,
            scale,
        }
    }
}

/// Centers the default caption window near the bottom of a work area.
pub fn initial_geometry(area: Rect) -> Rect {
    let width = DEFAULT_WIDTH.min(area.width);
    let height = CAPTION_HEIGHT.min(area.height);
    let margin = BOTTOM_MARGIN.min((area.height - height).max(0.0));
    Rect {
        x: area.x + (area.width - width) / 2.0,
        y: area.y + area.height - height - margin,
        width,
        height,
    }
}

/// Restores a saved position on its original monitor, or the first monitor when
/// the original is disconnected. Sizes and positions stay inside the work area.
pub fn restored_geometry<'a>(
    saved: &SavedGeometry,
    areas: &'a [WorkArea],
) -> Option<(Rect, &'a WorkArea)> {
    let area = areas
        .iter()
        .find(|area| area.name == saved.monitor)
        .or_else(|| areas.first())?;
    let width = saved
        .width
        .max(MIN_WINDOW_WIDTH.min(area.area.width))
        .min(area.area.width);
    let height = saved
        .height
        .max(CAPTION_HEIGHT.min(area.area.height))
        .min(area.area.height);
    let x = area.area.x + (area.area.width - width).max(0.0) * saved.relative_x.clamp(0.0, 1.0);
    let y = area.area.y + (area.area.height - height).max(0.0) * saved.relative_y.clamp(0.0, 1.0);
    Some((
        Rect {
            x,
            y,
            width,
            height,
        },
        area,
    ))
}

/// Captures logical window geometry relative to its monitor's work area.
pub fn capture_geometry(geometry: Rect, monitor: &WorkArea) -> SavedGeometry {
    let area = monitor.area;
    let width = geometry.width.min(area.width);
    let height = geometry.height.min(area.height);
    SavedGeometry {
        monitor: monitor.name.clone(),
        relative_x: ((geometry.x - area.x) / (area.width - width).max(1.0)).clamp(0.0, 1.0),
        relative_y: ((geometry.y - area.y) / (area.height - height).max(1.0)).clamp(0.0, 1.0),
        width,
        height,
    }
}

/// Keeps visible geometry unchanged and moves off-screen geometry to the first
/// (primary) work area using the initial placement.
pub fn ensure_visible(geometry: Rect, areas: &[WorkArea]) -> Rect {
    if areas.iter().any(|monitor| {
        geometry.overlaps_at_least(MIN_VISIBLE_WIDTH, MIN_VISIBLE_HEIGHT, monitor.area)
    }) {
        return geometry;
    }
    areas
        .first()
        .map_or(geometry, |primary| initial_geometry(primary.area))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_geometry_matches_work_area_placement_cases() {
        let cases = [
            (
                "centered near the bottom",
                Rect::new(0, 0, 1920, 1040),
                Rect::new(560, 848, 800, 160),
            ),
            (
                "negative monitor coordinates",
                Rect::new(-1280, 0, 1280, 720),
                Rect::new(-1040, 528, 800, 160),
            ),
            (
                "small work area",
                Rect::new(0, 0, 300, 150),
                Rect::new(0, 0, 300, 150),
            ),
        ];

        for (name, area, expected) in cases {
            assert_eq!(initial_geometry(area), expected, "{name}");
        }
    }

    #[test]
    fn dimensions_are_logical_at_high_dpi() {
        let geometry = initial_geometry(Rect::new(0.0, 0.0, 1920.0 / 1.5, 1080.0 / 1.5));
        assert_eq!(geometry.width, 800.0);
        assert!(geometry.y + geometry.height <= 720.0);
    }

    #[test]
    fn restored_geometry_matches_monitor_cases() {
        let cases = [
            (
                "disconnected monitor falls back",
                WorkArea::new("primary", Rect::new(0, 0, 1280, 720), 1.0),
                SavedGeometry {
                    monitor: Some("disconnected".into()),
                    relative_x: 0.5,
                    relative_y: 1.0,
                    width: 800.0,
                    height: 220.0,
                },
                Rect::new(240, 500, 800, 220),
            ),
            (
                "relative position scales with the work area",
                WorkArea::new("primary", Rect::new(0, 0, 1280, 720), 1.5),
                SavedGeometry {
                    monitor: Some("primary".into()),
                    relative_x: 0.5,
                    relative_y: 1.0,
                    width: 1000.0,
                    height: 250.0,
                },
                Rect::new(140, 470, 1000, 250),
            ),
        ];

        for (name, monitor, saved, expected) in cases {
            let areas = [monitor];
            let (restored, _) = restored_geometry(&saved, &areas).unwrap();
            assert_eq!(restored, expected, "{name}");
        }
    }

    #[test]
    fn capture_uses_logical_work_area_coordinates() {
        let monitor = WorkArea::new("primary", Rect::new(0, 0, 1280, 720), 1.5);
        let saved = capture_geometry(Rect::new(140, 470, 1000, 250), &monitor);
        assert_eq!(saved.monitor.as_deref(), Some("primary"));
        assert_eq!(saved.relative_x, 0.5);
        assert_eq!(saved.relative_y, 1.0);
        assert_eq!(saved.width, 1000.0);
        assert_eq!(saved.height, 250.0);
    }

    #[test]
    fn offscreen_geometry_is_restored_to_a_visible_work_area() {
        let areas = [WorkArea::new(
            "left",
            Rect::new(-1920, -1080, 1920, 1080),
            1.0,
        )];
        let restored = ensure_visible(Rect::new(-1900, -1300, 800, 160), &areas);
        assert_eq!(restored, initial_geometry(areas[0].area));
        assert!(restored.overlaps_at_least(32, 32, Rect::new(-1920, -1080, 1920, 1080)));

        let visible = Rect::new(-1000, -500, 800, 160);
        assert_eq!(ensure_visible(visible, &areas), visible);
    }
}
