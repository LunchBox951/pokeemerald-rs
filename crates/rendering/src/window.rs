//! Display-window membership and per-region layer masks.
//!
//! Pixels are classified in `WIN0`, `WIN1`, `OBJWIN`, `WINOUT` priority order.
//! With no enabled window, every layer and color effect remains enabled.

/// A window register's inclusive-start, exclusive-end axis range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRange {
    start: u8,
    end: u8,
}

impl WindowRange {
    /// Creates a range from its register byte fields.
    #[must_use]
    pub const fn new(start: u8, end: u8) -> Self {
        Self { start, end }
    }

    /// Returns whether `coord` is in the horizontal range.
    ///
    /// A reversed range wraps across the 8-bit boundary. An equal pair is empty.
    #[must_use]
    pub const fn contains(&self, coord: u8) -> bool {
        if self.start < self.end {
            coord >= self.start && coord < self.end
        } else if self.start > self.end {
            coord >= self.start || coord < self.end
        } else {
            false
        }
    }

    /// Returns whether visible scanline `y` is in the vertical range.
    ///
    /// A reversed range reopens at line zero only when its start is reachable
    /// during the `160..228` vertical-blanking interval. Starts at `228..=255`
    /// are never reached, so those ranges stay closed. This follows mGBA's
    /// `GBAVideoSoftwareRendererFinishFrame` and
    /// `GBAVideoSoftwareRendererStepWindow` flip-flop behavior
    /// `(behavioral-fidelity)`.
    #[must_use]
    pub const fn contains_vertical(&self, y: u8) -> bool {
        const VISIBLE_SCANLINE_COUNT: u8 = 160;
        const TOTAL_SCANLINE_COUNT: u8 = 228;

        if self.start <= self.end {
            return y >= self.start && y < self.end;
        }
        let in_upper_visible_band = self.start < VISIBLE_SCANLINE_COUNT && y >= self.start;
        let in_reopened_lower_band = self.start < TOTAL_SCANLINE_COUNT && y < self.end;
        in_upper_visible_band || in_reopened_lower_band
    }
}

/// A rectangular hardware window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    /// Horizontal `WINxH` range.
    pub x: WindowRange,
    /// Vertical `WINxV` range.
    pub y: WindowRange,
}

impl WindowRect {
    /// Creates a rectangle from its horizontal and vertical ranges.
    #[must_use]
    pub const fn new(x: WindowRange, y: WindowRange) -> Self {
        Self { x, y }
    }

    /// Returns whether `(x, y)` is in the rectangle.
    #[must_use]
    pub const fn contains(&self, x: u8, y: u8) -> bool {
        self.x.contains(x) && self.y.contains_vertical(y)
    }
}

/// One window region's `WININ` or `WINOUT` layer enables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowLayerEnable {
    /// Background-layer enables, ordered BG0 through BG3.
    pub bg: [bool; 4],
    /// Whether sprites are enabled.
    pub obj: bool,
    /// Whether `BLDCNT` color effects may apply.
    pub effects: bool,
}

impl WindowLayerEnable {
    /// Every layer and color effect enabled.
    pub const ALL: Self = Self {
        bg: [true; 4],
        obj: true,
        effects: true,
    };

    /// Every layer and color effect disabled.
    pub const NONE: Self = Self {
        bg: [false; 4],
        obj: false,
        effects: false,
    };

    /// Returns whether background `index` is enabled, wrapping indices modulo four.
    #[must_use]
    pub const fn bg_enabled(&self, index: u8) -> bool {
        self.bg[index as usize % self.bg.len()]
    }
}

impl Default for WindowLayerEnable {
    fn default() -> Self {
        Self::NONE
    }
}

/// Window geometry and per-region layer masks for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowConfig {
    /// `WIN0` geometry and enables, or `None` when disabled in `DISPCNT`.
    pub win0: Option<(WindowRect, WindowLayerEnable)>,
    /// `WIN1` geometry and enables, or `None` when disabled in `DISPCNT`.
    pub win1: Option<(WindowRect, WindowLayerEnable)>,
    /// `OBJWIN` enables, or `None` when disabled in `DISPCNT`.
    pub obj_window: Option<WindowLayerEnable>,
    /// Enables outside every active window.
    pub winout: WindowLayerEnable,
}

impl WindowConfig {
    /// Returns whether `WIN0`, `WIN1`, or `OBJWIN` is enabled.
    #[must_use]
    pub const fn any_enabled(&self) -> bool {
        self.win0.is_some() || self.win1.is_some() || self.obj_window.is_some()
    }

    /// Returns the layer enables for a pixel and its caller-supplied OBJ-window mask.
    #[must_use]
    pub fn classify(&self, x: u8, y: u8, objwin_mask: bool) -> WindowLayerEnable {
        self.classify_with_region(x, y, objwin_mask).0
    }

    /// [`classify`](Self::classify), also returning which region matched
    /// (`software-obj.c:161`, `video-software.c:131-134`).
    #[must_use]
    pub(crate) fn classify_with_region(
        &self,
        x: u8,
        y: u8,
        objwin_mask: bool,
    ) -> (WindowLayerEnable, WindowRegion) {
        if !self.any_enabled() {
            return (WindowLayerEnable::ALL, WindowRegion::WinOut);
        }
        if let Some((rect, enable)) = self.win0 {
            if rect.contains(x, y) {
                return (enable, WindowRegion::Win0);
            }
        }
        if let Some((rect, enable)) = self.win1 {
            if rect.contains(x, y) {
                return (enable, WindowRegion::Win1);
            }
        }
        if objwin_mask {
            if let Some(enable) = self.obj_window {
                return (enable, WindowRegion::ObjWindow);
            }
        }
        (self.winout, WindowRegion::WinOut)
    }

    /// Returns the ascending columns that begin a span of mGBA's surviving
    /// per-scanline window list, always starting at zero.
    ///
    /// mGBA partitions a scanline geometrically from the `WIN0`/`WIN1` edges
    /// active on it and redraws every layer once per span, so a span start is
    /// where per-span draw state restarts. Three properties of that partition
    /// are not derivable from per-pixel classification: adjacent spans are
    /// never coalesced when their control bits agree, a window with equal
    /// horizontal endpoints matches no pixel yet still splits the span it
    /// falls in, and an edge an outranking window covers completely is
    /// trimmed back out
    /// (`mgba/src/gba/renderers/video-software.c:446-500,628-632,903-912`)
    /// `(behavioral-fidelity)`.
    pub(crate) fn scanline_span_starts(&self, y: u8) -> Vec<usize> {
        let mut spans = SpanEnds::WHOLE_SCANLINE;
        if self.any_enabled() {
            // `WIN0` outranks `WIN1` by being broken in second, over it.
            for (rect, _) in [self.win1, self.win0].into_iter().flatten() {
                if rect.y.contains_vertical(y) {
                    break_window(&mut spans, i32::from(rect.x.start), i32::from(rect.x.end));
                }
            }
        }
        let mut starts = vec![0];
        // The last span's end is the scanline's end, not a start; ends at or
        // past the right edge (which a range running past it can produce)
        // begin no visible span either.
        for &end in &spans.ends[..spans.len.saturating_sub(1)] {
            if !(1..HORIZONTAL_PIXELS).contains(&end) {
                continue;
            }
            let Ok(column) = usize::try_from(end) else {
                continue;
            };
            if !starts.contains(&column) {
                starts.push(column);
            }
        }
        starts
    }
}

/// mGBA's `GBA_VIDEO_HORIZONTAL_PIXELS`: the column every scanline's span
/// list ends at.
const HORIZONTAL_PIXELS: i32 = 240;

/// mGBA's window list caps at `MAX_WINDOW` (five) and logs when it overflows.
/// Four [`break_window_inner`] calls — `WIN0` and `WIN1`, each split in two
/// when its range wraps — each grow the list by at most two from an initial
/// single span, so nine slots make an overflow unreachable instead.
const MAX_SPANS: usize = 9;

/// The `endX` column of each span in mGBA's `windows` array, ascending.
///
/// Control bits are omitted: they never affect where the partition splits.
struct SpanEnds {
    ends: [i32; MAX_SPANS],
    len: usize,
}

impl SpanEnds {
    const WHOLE_SCANLINE: Self = Self {
        ends: [HORIZONTAL_PIXELS; MAX_SPANS],
        len: 1,
    };

    fn insert(&mut self, index: usize, end: i32) {
        self.ends.copy_within(index..self.len, index + 1);
        self.ends[index] = end;
        self.len += 1;
    }
}

/// mGBA's `_breakWindow`: a range that wraps, or that runs past the right
/// edge, splits into a head starting at column zero and a tail ending at the
/// right edge (`mgba/src/gba/renderers/video-software.c:446-456`).
fn break_window(spans: &mut SpanEnds, start: i32, end: i32) {
    if end > HORIZONTAL_PIXELS || end < start {
        break_window_inner(spans, 0, end);
        break_window_inner(spans, start, HORIZONTAL_PIXELS);
    } else {
        break_window_inner(spans, start, end);
    }
}

/// mGBA's `_breakWindowInner` (`video-software.c:458-500`).
fn break_window_inner(spans: &mut SpanEnds, start: i32, end: i32) {
    if end <= 0 {
        return;
    }
    let mut span_start = 0;
    let mut active = 0;
    while active < spans.len {
        if start < spans.ends[active] {
            let old_end = spans.ends[active];
            if start > span_start {
                spans.insert(active, start);
                active += 1;
            }
            spans.ends[active] = end;
            active += 1;
            if end >= old_end {
                // Trimming shifts one span down per step and drops the last
                // live slot, rather than closing the gap across the tail.
                while spans.len > active + 1 && end >= spans.ends[active] {
                    spans.ends[active] = spans.ends[active + 1];
                    spans.len -= 1;
                    active += 1;
                }
            } else {
                spans.insert(active, old_end);
            }
            return;
        }
        span_start = spans.ends[active];
        active += 1;
    }
}

/// Which region [`WindowConfig::classify_with_region`] selected for a pixel,
/// in mgba's rank order `WIN0 < WIN1 < OBJWIN < WINOUT` (`video-software.c:131-134`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowRegion {
    Win0,
    Win1,
    ObjWindow,
    WinOut,
}

impl WindowRegion {
    /// Whether an `OBJWIN` sprite's hole is barred from upgrading OBJ order here —
    /// true only for `WIN0`/`WIN1` (`software-obj.c:161`, `video-software.c:131-134`).
    #[must_use]
    pub(crate) const fn suppresses_objwin_hole(self) -> bool {
        matches!(self, Self::Win0 | Self::Win1)
    }
}

#[cfg(test)]
mod tests {
    use super::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect, WindowRegion};

    /// Builds a config whose `WIN0`/`WIN1` rects span scanline 0.
    fn windows_on_line_zero(win0: Option<WindowRange>, win1: Option<WindowRange>) -> WindowConfig {
        let on_line_zero = |x: WindowRange| {
            (
                WindowRect::new(x, WindowRange::new(0, 1)),
                WindowLayerEnable::ALL,
            )
        };
        WindowConfig {
            win0: win0.map(on_line_zero),
            win1: win1.map(on_line_zero),
            obj_window: None,
            winout: WindowLayerEnable::ALL,
        }
    }

    #[test]
    fn a_scanline_with_no_active_window_is_one_span() {
        assert_eq!(WindowConfig::default().scanline_span_starts(0), vec![0]);
        let vertically_inactive = WindowConfig {
            win0: Some((
                WindowRect::new(WindowRange::new(10, 20), WindowRange::new(5, 9)),
                WindowLayerEnable::ALL,
            )),
            ..WindowConfig::default()
        };
        assert_eq!(vertically_inactive.scanline_span_starts(0), vec![0]);
    }

    #[test]
    fn a_window_splits_the_scanline_at_both_of_its_edges() {
        let config = windows_on_line_zero(Some(WindowRange::new(10, 20)), None);
        assert_eq!(config.scanline_span_starts(0), vec![0, 10, 20]);
    }

    #[test]
    fn a_zero_width_window_still_splits_the_span_it_falls_in() {
        // Equal endpoints match no pixel, yet `_breakWindowInner` inserts the
        // empty span and re-inserts the remainder behind it, so the column is
        // a span start with identical control on both sides.
        let config = windows_on_line_zero(Some(WindowRange::new(5, 5)), None);
        assert_eq!(config.scanline_span_starts(0), vec![0, 5]);
    }

    #[test]
    fn an_edge_wholly_covered_by_an_outranking_window_is_trimmed_away() {
        // `WIN0` is broken over `WIN1`, and the trim loop deletes the spans
        // it completely overwrote, so `WIN1`'s edges leave no span start.
        let config = windows_on_line_zero(
            Some(WindowRange::new(0, 100)),
            Some(WindowRange::new(20, 30)),
        );
        assert_eq!(config.scanline_span_starts(0), vec![0, 100]);
    }

    #[test]
    fn a_window_wrapping_past_the_right_edge_splits_at_both_of_its_halves() {
        let config = windows_on_line_zero(Some(WindowRange::new(200, 40)), None);
        assert_eq!(config.scanline_span_starts(0), vec![0, 40, 200]);
    }

    #[test]
    fn range_contains_the_normal_non_wrapping_case() {
        let range = WindowRange::new(10, 20);
        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(19));
        assert!(!range.contains(20));
    }

    #[test]
    fn range_wraps_when_start_exceeds_end() {
        let range = WindowRange::new(200, 40);
        assert!(range.contains(200));
        assert!(range.contains(255));
        assert!(range.contains(0));
        assert!(range.contains(39));
        assert!(!range.contains(40));
        assert!(!range.contains(199));
    }

    #[test]
    fn range_start_equals_end_contains_nothing() {
        let range = WindowRange::new(50, 50);
        for coord in [0, 49, 50, 51, 255] {
            assert!(!range.contains(coord), "coord {coord}");
        }
    }

    #[test]
    fn vertical_non_wrapping_is_the_ordinary_half_open_band() {
        let range = WindowRange::new(30, 40);
        assert!(!range.contains_vertical(29));
        assert!(range.contains_vertical(30));
        assert!(range.contains_vertical(39));
        assert!(!range.contains_vertical(40));
    }

    #[test]
    fn vertical_start_equals_end_contains_no_scanline() {
        let range = WindowRange::new(50, 50);
        for y in [0, 49, 50, 51, 159] {
            assert!(!range.contains_vertical(y), "y {y}");
        }
    }

    #[test]
    fn vertical_wrapped_with_visible_start_shows_both_bands() {
        let range = WindowRange::new(100, 40);
        assert!(range.contains_vertical(0));
        assert!(range.contains_vertical(39));
        assert!(!range.contains_vertical(40));
        assert!(!range.contains_vertical(99));
        assert!(range.contains_vertical(100));
        assert!(range.contains_vertical(159));
    }

    #[test]
    fn vertical_wrapped_start_in_vblank_reopens_at_line_zero() {
        let range = WindowRange::new(200, 40);
        assert!(range.contains_vertical(0));
        assert!(range.contains_vertical(39));
        assert!(!range.contains_vertical(40));
        assert!(!range.contains_vertical(100));
        assert!(!range.contains_vertical(159));
    }

    #[test]
    fn vertical_wrapped_start_past_vblank_never_opens() {
        for start in [228u8, 240, 255] {
            let range = WindowRange::new(start, 40);
            for y in [0u8, 20, 39, 40, 100, 159] {
                assert!(
                    !range.contains_vertical(y),
                    "start={start} y={y} must never open"
                );
            }
            assert!(range.contains(0), "horizontal wrap semantics stay intact");
            assert!(range.contains(39));
        }
    }

    #[test]
    fn rect_requires_both_axes() {
        let rect = WindowRect::new(WindowRange::new(10, 20), WindowRange::new(30, 40));
        assert!(rect.contains(15, 35));
        assert!(!rect.contains(5, 35), "x outside");
        assert!(!rect.contains(15, 5), "y outside");
    }

    fn enable_only_bg(index: u8) -> WindowLayerEnable {
        let mut bg = [false; 4];
        bg[index as usize] = true;
        WindowLayerEnable {
            bg,
            obj: false,
            effects: false,
        }
    }

    #[test]
    fn classify_returns_all_enabled_when_no_window_is_active() {
        let config = WindowConfig::default();
        let enabled_layers = config.classify(0, 0, false);
        assert_eq!(enabled_layers, WindowLayerEnable::ALL);
    }

    #[test]
    fn classify_win0_beats_win1_on_overlap() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let win1_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: Some((win1_rect, enable_only_bg(1))),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        };
        let enabled_layers = config.classify(10, 10, false);
        assert!(enabled_layers.bg_enabled(0), "WIN0 must win the overlap");
        assert!(!enabled_layers.bg_enabled(1));
    }

    #[test]
    fn classify_falls_back_to_win1_outside_win0() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let win1_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: Some((win1_rect, enable_only_bg(1))),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        };
        let enabled_layers = config.classify(50, 50, false);
        assert!(enabled_layers.bg_enabled(1));
        assert!(!enabled_layers.bg_enabled(0));
    }

    #[test]
    fn classify_uses_objwin_when_masked_and_outside_win0_win1() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: None,
            obj_window: Some(enable_only_bg(2)),
            winout: WindowLayerEnable::NONE,
        };
        assert!(config.classify(50, 50, true).bg_enabled(2));
        let without_obj_window_pixel = config.classify(50, 50, false);
        assert!(!without_obj_window_pixel.bg_enabled(2));
    }

    #[test]
    fn classify_falls_back_to_winout_outside_every_window() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: None,
            obj_window: None,
            winout: enable_only_bg(3),
        };
        assert!(config.classify(200, 200, false).bg_enabled(3));
    }

    #[test]
    fn classify_with_region_reports_win0_and_win1_and_suppresses_objwin_hole() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let win1_rect = WindowRect::new(WindowRange::new(10, 20), WindowRange::new(0, 20));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: Some((win1_rect, enable_only_bg(1))),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        };
        let (_, win0_region) = config.classify_with_region(5, 5, false);
        assert_eq!(win0_region, WindowRegion::Win0);
        assert!(win0_region.suppresses_objwin_hole());

        let (_, win1_region) = config.classify_with_region(15, 5, false);
        assert_eq!(win1_region, WindowRegion::Win1);
        assert!(win1_region.suppresses_objwin_hole());
    }

    #[test]
    fn classify_with_region_does_not_suppress_objwin_or_winout_or_no_window() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let config = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: None,
            obj_window: Some(enable_only_bg(2)),
            winout: enable_only_bg(3),
        };

        let (_, objwin_region) = config.classify_with_region(50, 50, true);
        assert_eq!(objwin_region, WindowRegion::ObjWindow);
        assert!(!objwin_region.suppresses_objwin_hole());

        let (_, winout_region) = config.classify_with_region(50, 50, false);
        assert_eq!(winout_region, WindowRegion::WinOut);
        assert!(!winout_region.suppresses_objwin_hole());

        let (_, disabled_region) = WindowConfig::default().classify_with_region(0, 0, false);
        assert_eq!(disabled_region, WindowRegion::WinOut);
        assert!(!disabled_region.suppresses_objwin_hole());
    }
}
