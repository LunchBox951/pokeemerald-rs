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
        if !self.any_enabled() {
            return WindowLayerEnable::ALL;
        }
        if let Some((rect, enable)) = self.win0 {
            if rect.contains(x, y) {
                return enable;
            }
        }
        if let Some((rect, enable)) = self.win1 {
            if rect.contains(x, y) {
                return enable;
            }
        }
        if objwin_mask {
            if let Some(enable) = self.obj_window {
                return enable;
            }
        }
        self.winout
    }
}

#[cfg(test)]
mod tests {
    use super::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};

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
}
