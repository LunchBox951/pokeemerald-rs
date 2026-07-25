//! Hardware display windows: `WIN0`/`WIN1`/`OBJWIN`/`WINOUT` (S-2 slice 4,
//! issue #99).
//!
//! `pokeemerald` configures these directly via `SetGpuReg`/`REG_WIN0H` etc.
//! from many call sites rather than one owning module — text box borders
//! (`text_window.c`), battle-transition irising (`battle_transition.c`), and
//! field-effect masks (fog/binoculars in `field_effect.c`) are typical
//! users. The register *semantics* themselves are verified against
//! `mgba/src/gba/renderers/video-software.c`'s `_breakWindow`/
//! `_breakWindowInner`/`GBAVideoSoftwareRendererPrepareWindow` and the
//! `TEST_LAYER_ENABLED` macro `(behavioral-fidelity)`.

/// A single-axis hardware window range (one of `WINxH`'s X1/X2 or `WINxV`'s
/// Y1/Y2 byte fields).
///
/// Ports the wrap semantics `mgba`'s `_breakWindow` encodes: `start < end`
/// covers `[start, end)` normally; `start > end` **wraps**, covering
/// `[start, screen_len) ∪ [0, end)` instead of being empty (`_breakWindow`
/// splits exactly this case into two non-wrapping sub-windows). `start ==
/// end` covers nothing — `_breakWindow` only special-cases `end < start`,
/// strictly, so an equal pair is an ordinary (degenerate, zero-width)
/// non-wrapping window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRange {
    start: u8,
    end: u8,
}

impl WindowRange {
    /// Build a range from its raw `start`/`end` byte fields (module docs).
    #[must_use]
    pub const fn new(start: u8, end: u8) -> Self {
        Self { start, end }
    }

    /// Whether `coord` falls within this range using the **horizontal**
    /// (`WINxH`) wrap semantics (module docs). This is the 8-bit spatial wrap
    /// mgba's `_breakWindow` encodes; the vertical axis does *not* share it —
    /// see [`contains_vertical`](Self::contains_vertical).
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

    /// Whether visible scanline `y` (`0..160`) falls within this range using
    /// the **vertical** (`WINxV`) semantics, which — unlike the horizontal
    /// axis — are *not* a spatial wrap but the observable result of mgba's
    /// per-scanline `on` flip-flop (`video-software.c`
    /// `GBAVideoSoftwareRendererStepWindow`, ~881-892) seeded by the vblank
    /// preprocessing in `GBAVideoSoftwareRendererFinishFrame` (~760-772).
    ///
    /// - `start <= end` (including `start == end`, empty): the ordinary
    ///   half-open band `[start, end)`. An `end` past the screen simply keeps
    ///   the window open to the bottom, which `[start, 160)` already yields for
    ///   visible `y`.
    /// - `start > end` (**wrapped**): the window closes at `end` and reopens at
    ///   `start`. The upper band `[start, 160)` is only reachable when
    ///   `start < 160`. The lower band `[0, end)` reappears at line 0 only if
    ///   the flip-flop was armed during vblank, which the preprocessing does
    ///   solely for a `start` in `[160, 228)` (`GBA_VIDEO_VERTICAL_PIXELS ..
    ///   VIDEO_VERTICAL_TOTAL_PIXELS`); a `start` of `228..=255` never arms it,
    ///   so the window never opens and every line falls through to `WINOUT`.
    #[must_use]
    pub const fn contains_vertical(&self, y: u8) -> bool {
        /// mgba's `GBA_VIDEO_VERTICAL_PIXELS` — the first non-visible line.
        const VISIBLE_LINES: u8 = 160;
        /// mgba's `VIDEO_VERTICAL_TOTAL_PIXELS` — one past the last reachable
        /// `VCOUNT`; a wrapped `start` at or beyond this never reopens.
        const TOTAL_LINES: u16 = 228;

        if self.start <= self.end {
            // Non-wrapping (start == end is an empty band): the flip-flop is
            // armed at `start` and cleared at `end`, both within the visible
            // range only when they are, giving exactly [start, end).
            return y >= self.start && y < self.end;
        }
        // Wrapped. Upper band [start, 160): only present when `start` is a
        // visible line (start < 160). Lower band [0, end): only present when
        // the vblank preprocessing armed the flip-flop, i.e. start < 228.
        let upper = self.start < VISIBLE_LINES && y >= self.start;
        let lower = (self.start as u16) < TOTAL_LINES && y < self.end;
        upper || lower
    }
}

/// A rectangular hardware window: a `WINxH`/`WINxV` register pair (`WIN0` or
/// `WIN1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    /// The horizontal (`WINxH`) range.
    pub x: WindowRange,
    /// The vertical (`WINxV`) range.
    pub y: WindowRange,
}

impl WindowRect {
    /// Build a rectangle from its horizontal and vertical ranges.
    #[must_use]
    pub const fn new(x: WindowRange, y: WindowRange) -> Self {
        Self { x, y }
    }

    /// Whether `(x, y)` falls within this rectangle. The horizontal axis uses
    /// [`WindowRange::contains`] (spatial 8-bit wrap); the vertical axis uses
    /// [`WindowRange::contains_vertical`] (the `WINxV` scanline flip-flop),
    /// since the two axes diverge for a wrapped range whose `start` lands in
    /// vblank (module docs on `contains_vertical`).
    #[must_use]
    pub const fn contains(&self, x: u8, y: u8) -> bool {
        self.x.contains(x) && self.y.contains_vertical(y)
    }
}

/// One window region's per-layer enable bits — `WININ`/`WINOUT`'s six used
/// bits per half: BG0..BG3, OBJ, and the color special-effect enable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowLayerEnable {
    /// Per-BG (BG0..BG3) enable bits.
    pub bg: [bool; 4],
    /// Whether the OBJ (sprite) layer is enabled in this region.
    pub obj: bool,
    /// Whether the color special effect (`BLDCNT`) may apply to pixels in
    /// this region at all.
    pub effects: bool,
}

impl WindowLayerEnable {
    /// Every layer and the color effect enabled — the state used when no
    /// window is active at all (module docs on [`WindowConfig::classify`]).
    pub const ALL: Self = Self {
        bg: [true; 4],
        obj: true,
        effects: true,
    };

    /// Nothing enabled — `WININ`/`WINOUT`'s power-on-reset value.
    pub const NONE: Self = Self {
        bg: [false; 4],
        obj: false,
        effects: false,
    };

    /// Whether BG `index` (`0..=3`, masked) is enabled in this region.
    #[must_use]
    pub const fn bg_enabled(&self, index: u8) -> bool {
        self.bg[(index & 0x03) as usize]
    }
}

impl Default for WindowLayerEnable {
    fn default() -> Self {
        Self::NONE
    }
}

/// Full per-frame window configuration: `WIN0`/`WIN1` (each `None` when its
/// `DISPCNT` enable bit is off), `OBJWIN`'s enable-bit set (`None` when
/// `DISPCNT`'s OBJWIN bit is off — its per-pixel *mask* comes from OAM
/// mode-2 sprites, supplied by the caller, not stored here), and the
/// `WINOUT` fallback.
///
/// [`classify`](Self::classify)'s priority order — WIN0 > WIN1 > OBJWIN >
/// WINOUT — mirrors `mgba`'s `_breakWindow` insertion order (WIN1 is broken
/// in first, then WIN0 painted over it, so WIN0 wins any overlap). When
/// **no** window is enabled at all, every layer and the color effect are
/// enabled everywhere (`TEST_LAYER_ENABLED`'s window-disabled fallback) —
/// the all-`None` [`Default`] therefore reproduces "no windowing" exactly,
/// which is what makes [`compose_frame`](crate::compositor::compose_frame)
/// byte-for-byte unaffected by this slice `(behavioral-fidelity)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowConfig {
    /// `WIN0`'s rectangle and per-region enables, or `None` if disabled.
    pub win0: Option<(WindowRect, WindowLayerEnable)>,
    /// `WIN1`'s rectangle and per-region enables, or `None` if disabled.
    pub win1: Option<(WindowRect, WindowLayerEnable)>,
    /// `OBJWIN`'s per-region enables, or `None` if disabled. Its rectangle
    /// is implicit: wherever an OAM mode-2 sprite draws an opaque texel (see
    /// [`crate::sprite::SpriteLayer::objwin_mask`]).
    pub obj_window: Option<WindowLayerEnable>,
    /// The fallback region outside every enabled window.
    pub winout: WindowLayerEnable,
}

impl WindowConfig {
    /// Whether any window is enabled at all (`WIN0`, `WIN1`, or `OBJWIN`'s
    /// `DISPCNT` bits).
    #[must_use]
    pub const fn any_enabled(&self) -> bool {
        self.win0.is_some() || self.win1.is_some() || self.obj_window.is_some()
    }

    /// Classify pixel `(x, y)` into the layer/effect enables that apply
    /// there, given whether an OBJ-window sprite (OAM mode 2) covers this
    /// pixel (`objwin_mask`) — see module docs for the WIN0 > WIN1 > OBJWIN
    /// > WINOUT priority order, and the all-disabled fallback.
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
        let r = WindowRange::new(10, 20);
        assert!(!r.contains(9));
        assert!(r.contains(10));
        assert!(r.contains(19));
        assert!(!r.contains(20));
    }

    #[test]
    fn range_wraps_when_start_exceeds_end() {
        // start=200, end=40: covers 200..=255 and 0..40.
        let r = WindowRange::new(200, 40);
        assert!(r.contains(200));
        assert!(r.contains(255));
        assert!(r.contains(0));
        assert!(r.contains(39));
        assert!(!r.contains(40));
        assert!(!r.contains(199));
    }

    #[test]
    fn range_start_equals_end_contains_nothing() {
        let r = WindowRange::new(50, 50);
        for coord in [0, 49, 50, 51, 255] {
            assert!(!r.contains(coord), "coord {coord}");
        }
    }

    #[test]
    fn vertical_non_wrapping_is_the_ordinary_half_open_band() {
        let r = WindowRange::new(30, 40);
        assert!(!r.contains_vertical(29));
        assert!(r.contains_vertical(30));
        assert!(r.contains_vertical(39));
        assert!(!r.contains_vertical(40));
    }

    #[test]
    fn vertical_start_equals_end_contains_no_scanline() {
        let r = WindowRange::new(50, 50);
        for y in [0, 49, 50, 51, 159] {
            assert!(!r.contains_vertical(y), "y {y}");
        }
    }

    #[test]
    fn vertical_wrapped_with_visible_start_shows_both_bands() {
        // start=100 > end=40, start visible: closes at 40, reopens at 100.
        let r = WindowRange::new(100, 40);
        assert!(r.contains_vertical(0));
        assert!(r.contains_vertical(39));
        assert!(!r.contains_vertical(40));
        assert!(!r.contains_vertical(99));
        assert!(r.contains_vertical(100));
        assert!(r.contains_vertical(159));
    }

    #[test]
    fn vertical_wrapped_start_in_vblank_reopens_at_line_zero() {
        // start=200 is in [160,228): the vblank preprocessing arms the
        // flip-flop, so the window reopens at line 0 and closes at end=40.
        // (The horizontal `contains` for the same bytes would coincidentally
        // agree here; the divergence is the 228..=255 case below.)
        let r = WindowRange::new(200, 40);
        assert!(r.contains_vertical(0));
        assert!(r.contains_vertical(39));
        assert!(!r.contains_vertical(40));
        assert!(!r.contains_vertical(100));
        assert!(!r.contains_vertical(159));
    }

    #[test]
    fn vertical_wrapped_start_past_vblank_never_opens() {
        // start in 228..=255 (unreachable VCOUNT): the flip-flop is never
        // armed, so no scanline is inside — every line falls through to
        // WINOUT. This is where vertical diverges from the horizontal wrap,
        // which would have shown [0, end).
        for start in [228u8, 240, 255] {
            let r = WindowRange::new(start, 40);
            for y in [0u8, 20, 39, 40, 100, 159] {
                assert!(
                    !r.contains_vertical(y),
                    "start={start} y={y} must never open"
                );
            }
            // The horizontal wrap, unchanged, still reports the lower band.
            assert!(r.contains(0), "horizontal wrap semantics stay intact");
            assert!(r.contains(39));
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
        let cfg = WindowConfig::default();
        let enable = cfg.classify(0, 0, false);
        assert_eq!(enable, WindowLayerEnable::ALL);
    }

    #[test]
    fn classify_win0_beats_win1_on_overlap() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let win1_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let cfg = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: Some((win1_rect, enable_only_bg(1))),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        };
        let enable = cfg.classify(10, 10, false);
        assert!(enable.bg_enabled(0), "WIN0 must win the overlap");
        assert!(!enable.bg_enabled(1));
    }

    #[test]
    fn classify_falls_back_to_win1_outside_win0() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let win1_rect = WindowRect::new(WindowRange::new(0, 100), WindowRange::new(0, 100));
        let cfg = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: Some((win1_rect, enable_only_bg(1))),
            obj_window: None,
            winout: WindowLayerEnable::NONE,
        };
        let enable = cfg.classify(50, 50, false);
        assert!(enable.bg_enabled(1));
        assert!(!enable.bg_enabled(0));
    }

    #[test]
    fn classify_uses_objwin_when_masked_and_outside_win0_win1() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let cfg = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: None,
            obj_window: Some(enable_only_bg(2)),
            winout: WindowLayerEnable::NONE,
        };
        assert!(cfg.classify(50, 50, true).bg_enabled(2));
        // Same pixel without the objwin mask falls through to WINOUT.
        assert!(!cfg.classify(50, 50, false).bg_enabled(2));
    }

    #[test]
    fn classify_falls_back_to_winout_outside_every_window() {
        let win0_rect = WindowRect::new(WindowRange::new(0, 10), WindowRange::new(0, 10));
        let cfg = WindowConfig {
            win0: Some((win0_rect, enable_only_bg(0))),
            win1: None,
            obj_window: None,
            winout: enable_only_bg(3),
        };
        assert!(cfg.classify(200, 200, false).bg_enabled(3));
    }
}
