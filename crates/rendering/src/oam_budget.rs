//! Selects the OAM entries that fit within one scanline's OBJ cycle budget.
//!
//! The GBA scans OAM in index order. Disabled entries and entries past the
//! strict horizontal cleanup boundaries (`x + width < 0` or `x >= SCREEN_WIDTH`)
//! are absent from the sprite list. An entry ending exactly at `x = 0` remains
//! on the list, and vertically off-scanline entries remain in the traversal.
//! Entries admitted before exhaustion apply to both visible sprites and the
//! object-window mask. [`crate::sprite`] caches this admission once per
//! scanline for both consumers.
//!
//! The timing oracle is mGBA:
//! `mgba/include/mgba/internal/gba/video.h:36-37` defines the 1,210-cycle normal
//! budget and 954-cycle HBlank-free budget, while
//! `mgba/src/gba/renderers/common.c:13-45` defines the 128-entry limit and
//! regular versus affine entry costs. The ordering in
//! `mgba/src/gba/renderers/video-software.c:1029-1064` is significant: OAM
//! index zero has no traversal charge; each later index costs two cycles;
//! exhaustion is checked after traversal but before scanline clipping; and an
//! admitted entry's processing cost is charged only after admission.

use crate::oam::{AffineMode, OamEntry};

const NORMAL_SCANLINE_CYCLES: i32 = 1210;
const HBLANK_FREE_SCANLINE_CYCLES: i32 = 954;
const MAX_OAM_ENTRIES: usize = 128;
const OAM_ENTRY_TRAVERSAL_CYCLES: i32 = 2;
const SCREEN_WIDTH: i32 = 240;
const REGULAR_ENTRY_BASE_REDUCTION: i32 = 2;
const AFFINE_ENTRY_BASE_CYCLES: i32 = 8;
const AFFINE_CYCLES_PER_COLUMN: i32 = 2;

#[cfg(test)]
thread_local! {
    static ADMISSION_WALK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_walk_count() {
    ADMISSION_WALK_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn walk_count() -> usize {
    ADMISSION_WALK_COUNT.with(std::cell::Cell::get)
}

/// The OAM indices a scanline's cycle budget admits, walked once per scanline.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OamAdmission {
    admitted: [bool; MAX_OAM_ENTRIES],
}

impl OamAdmission {
    /// Walks entries in OAM order until the scanline's cycle budget is spent.
    pub(crate) fn for_scanline(entries: &[OamEntry], y: usize, hblank_free_interval: bool) -> Self {
        Self::walk_with_remaining_cycles(entries, y, hblank_free_interval).0
    }

    fn walk_with_remaining_cycles(
        entries: &[OamEntry],
        y: usize,
        hblank_free_interval: bool,
    ) -> (Self, i32) {
        #[cfg(test)]
        ADMISSION_WALK_COUNT.with(|count| count.set(count.get() + 1));

        let mut admitted = [false; MAX_OAM_ENTRIES];
        let mut remaining_cycles = if hblank_free_interval {
            HBLANK_FREE_SCANLINE_CYCLES
        } else {
            NORMAL_SCANLINE_CYCLES
        };

        for (oam_index, entry) in entries.iter().take(MAX_OAM_ENTRIES).enumerate() {
            if oam_index > 0 {
                remaining_cycles -= OAM_ENTRY_TRAVERSAL_CYCLES;
            }
            if remaining_cycles <= 0 {
                break;
            }
            let Some(processing_cycles) = sprite_list_cost(entry) else {
                continue;
            };
            if !entry.covers_scanline(y) {
                continue;
            }
            admitted[oam_index] = true;
            remaining_cycles -= processing_cycles;
        }

        (Self { admitted }, remaining_cycles)
    }

    /// Whether the entry at `index` was admitted for this scanline.
    pub(crate) fn is_admitted(&self, index: usize) -> bool {
        self.admitted.get(index).copied().unwrap_or(false)
    }
}

#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    reason = "a bounding-box width is at most 128 pixels and fits exactly in i32"
)]
fn sprite_list_cost(entry: &OamEntry) -> Option<i32> {
    if !entry.enabled() {
        return None;
    }

    let (width, _) = entry.bounding_box();
    let width = width as i32;
    let x = i32::from(entry.x());
    if x + width < 0 || x >= SCREEN_WIDTH {
        return None;
    }

    let offscreen_adjustment = if x < 0 { x } else { 0 };
    Some(match entry.affine() {
        AffineMode::Regular => {
            // `mgba/src/gba/renderers/common.c:37` uses an arithmetic right
            // shift, so an odd negative X rounds down (`-33 >> 1 == -17`).
            width - REGULAR_ENTRY_BASE_REDUCTION + (offscreen_adjustment >> 1)
        }
        AffineMode::Affine { .. } | AffineMode::AffineDoubleSize { .. } => {
            AFFINE_ENTRY_BASE_CYCLES + width * AFFINE_CYCLES_PER_COLUMN + offscreen_adjustment
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        sprite_list_cost, OamAdmission, AFFINE_CYCLES_PER_COLUMN, AFFINE_ENTRY_BASE_CYCLES,
        MAX_OAM_ENTRIES, REGULAR_ENTRY_BASE_REDUCTION,
    };
    use crate::{
        oam::{AffineMode, OamEntry, ObjShape},
        tile::BitDepth,
    };

    const OAM_X_COORDINATE_SPACE: i32 = 512;
    const EIGHT_PIXEL_SQUARE_SIZE: u8 = 0;
    const SIXTEEN_PIXEL_SQUARE_SIZE: u8 = 1;
    const THIRTY_TWO_PIXEL_SQUARE_SIZE: u8 = 2;
    const SIXTY_FOUR_PIXEL_SQUARE_SIZE: u8 = 3;
    const ENABLED: bool = true;
    const DISABLED: bool = false;

    fn raw_x(x: i16) -> u16 {
        u16::try_from(i32::from(x).rem_euclid(OAM_X_COORDINATE_SPACE)).unwrap()
    }

    fn entry_at_x(x: i16, square_size: u8) -> OamEntry {
        entry_at(x, 0, square_size, ENABLED)
    }

    fn entry_at(x: i16, y: u8, square_size: u8, enabled: bool) -> OamEntry {
        OamEntry::new(
            raw_x(x),
            y,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            square_size,
            0,
            enabled,
        )
    }

    fn affine(entry: OamEntry) -> OamEntry {
        entry.with_affine(AffineMode::Affine { matrix_num: 0 })
    }

    fn wide_entry() -> OamEntry {
        entry_at_x(0, SIXTY_FOUR_PIXEL_SQUARE_SIZE)
    }

    fn admitted_count(admission: &OamAdmission, entries: usize) -> usize {
        (0..entries)
            .filter(|&index| admission.is_admitted(index))
            .count()
    }

    #[test]
    fn sprite_list_cost_matches_regular_and_affine_formulas() {
        let width = 64;
        let regular_cost = width - REGULAR_ENTRY_BASE_REDUCTION;
        let affine_cost = AFFINE_ENTRY_BASE_CYCLES + width * AFFINE_CYCLES_PER_COLUMN;

        assert_eq!(
            sprite_list_cost(&entry_at_x(0, SIXTY_FOUR_PIXEL_SQUARE_SIZE)),
            Some(regular_cost)
        );
        assert_eq!(
            sprite_list_cost(&entry_at_x(-32, SIXTY_FOUR_PIXEL_SQUARE_SIZE)),
            Some(regular_cost - 16)
        );
        assert_eq!(
            sprite_list_cost(&entry_at_x(-64, SIXTY_FOUR_PIXEL_SQUARE_SIZE)),
            Some(regular_cost - 32)
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at_x(0, SIXTY_FOUR_PIXEL_SQUARE_SIZE))),
            Some(affine_cost)
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at_x(-32, SIXTY_FOUR_PIXEL_SQUARE_SIZE))),
            Some(affine_cost - 32)
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at_x(-64, SIXTY_FOUR_PIXEL_SQUARE_SIZE))),
            Some(affine_cost - 64)
        );
        assert_eq!(
            sprite_list_cost(&entry_at_x(-33, SIXTY_FOUR_PIXEL_SQUARE_SIZE)),
            Some(regular_cost - 17)
        );
        let double_size = entry_at_x(0, THIRTY_TWO_PIXEL_SQUARE_SIZE)
            .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 });
        let plain = affine(entry_at_x(0, THIRTY_TWO_PIXEL_SQUARE_SIZE));
        assert_eq!(sprite_list_cost(&double_size), Some(136));
        assert_eq!(sprite_list_cost(&plain), Some(72));
        assert_eq!(
            sprite_list_cost(&entry_at_x(0, EIGHT_PIXEL_SQUARE_SIZE)),
            Some(6)
        );
    }

    #[test]
    fn entries_past_the_strict_left_cleanup_boundary_are_rejected() {
        assert!(sprite_list_cost(&entry_at_x(-64, SIXTY_FOUR_PIXEL_SQUARE_SIZE)).is_some());
        assert_eq!(
            sprite_list_cost(&entry_at_x(-65, SIXTY_FOUR_PIXEL_SQUARE_SIZE)),
            None
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at_x(-96, SIXTY_FOUR_PIXEL_SQUARE_SIZE))),
            None
        );
        let disabled = entry_at(0, 0, SIXTY_FOUR_PIXEL_SQUARE_SIZE, DISABLED);
        assert_eq!(sprite_list_cost(&disabled), None);
    }

    #[test]
    fn entries_at_or_beyond_the_right_edge_are_rejected() {
        assert!(sprite_list_cost(&entry_at_x(239, EIGHT_PIXEL_SQUARE_SIZE)).is_some());
        assert_eq!(
            sprite_list_cost(&entry_at_x(240, EIGHT_PIXEL_SQUARE_SIZE)),
            None
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at_x(255, SIXTY_FOUR_PIXEL_SQUARE_SIZE))),
            None
        );
    }

    #[test]
    fn right_edge_entries_cannot_displace_a_later_visible_entry() {
        const REJECTED_ENTRIES: usize = 100;

        let mut entries = vec![entry_at_x(240, SIXTY_FOUR_PIXEL_SQUARE_SIZE); REJECTED_ENTRIES];
        entries.push(wide_entry());

        let (admission, remaining_cycles) =
            OamAdmission::walk_with_remaining_cycles(&entries, 0, false);

        assert!((0..REJECTED_ENTRIES).all(|index| !admission.is_admitted(index)));
        assert!(admission.is_admitted(REJECTED_ENTRIES));
        assert_eq!(remaining_cycles, 948);
    }

    #[test]
    fn a_rejected_entry_costs_only_its_traversal() {
        const REJECTED_ENTRIES: usize = 100;
        const VISIBLE_ENTRIES: usize = 20;

        let mut entries = vec![entry_at_x(-65, SIXTY_FOUR_PIXEL_SQUARE_SIZE); REJECTED_ENTRIES];
        entries.extend(vec![wide_entry(); VISIBLE_ENTRIES]);

        let (admission, remaining_cycles) =
            OamAdmission::walk_with_remaining_cycles(&entries, 0, false);

        assert!((0..REJECTED_ENTRIES).all(|index| !admission.is_admitted(index)));
        assert_eq!(
            admitted_count(&admission, REJECTED_ENTRIES + VISIBLE_ENTRIES),
            16
        );
        assert!(admission.is_admitted(115));
        assert!(!admission.is_admitted(116));
        assert_eq!(remaining_cycles, -14);
    }

    #[test]
    fn normal_budget_admits_the_last_entry_charged_before_exhaustion() {
        let entries = vec![wide_entry(); 25];
        let (admission, remaining_cycles) =
            OamAdmission::walk_with_remaining_cycles(&entries, 0, false);

        assert!((0..=18).all(|index| admission.is_admitted(index)));
        assert!((19..entries.len()).all(|index| !admission.is_admitted(index)));
        assert_eq!(remaining_cycles, -6);
    }

    #[test]
    fn hblank_free_budget_exhausts_earlier_than_the_normal_budget() {
        let entries = vec![wide_entry(); 25];
        let normal = OamAdmission::for_scanline(&entries, 0, false);
        let (hblank_free, remaining_cycles) =
            OamAdmission::walk_with_remaining_cycles(&entries, 0, true);

        assert!(normal.is_admitted(18));
        assert!(!normal.is_admitted(19));
        assert!(hblank_free.is_admitted(14));
        assert!(!hblank_free.is_admitted(15));
        assert_eq!(remaining_cycles, -6);
    }

    #[test]
    fn affine_entries_exhaust_the_budget_before_same_width_regular_entries() {
        const INPUT_ENTRIES: usize = 150;

        let regular = vec![entry_at_x(0, EIGHT_PIXEL_SQUARE_SIZE); INPUT_ENTRIES];
        let affine = vec![affine(entry_at_x(0, EIGHT_PIXEL_SQUARE_SIZE)); INPUT_ENTRIES];

        let regular_admission = OamAdmission::for_scanline(&regular, 0, false);
        let affine_admission = OamAdmission::for_scanline(&affine, 0, false);

        assert_eq!(
            admitted_count(&regular_admission, INPUT_ENTRIES),
            MAX_OAM_ENTRIES
        );
        assert_eq!(admitted_count(&affine_admission, INPUT_ENTRIES), 47);
    }

    #[test]
    fn double_size_affine_entries_exhaust_the_budget_before_plain_affine_entries() {
        const INPUT_ENTRIES: usize = 40;

        let plain = vec![affine(entry_at_x(0, SIXTEEN_PIXEL_SQUARE_SIZE)); INPUT_ENTRIES];
        let double_size = vec![
            entry_at_x(0, SIXTEEN_PIXEL_SQUARE_SIZE)
                .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 });
            INPUT_ENTRIES
        ];

        let plain_admission = OamAdmission::for_scanline(&plain, 0, false);
        let double_admission = OamAdmission::for_scanline(&double_size, 0, false);

        assert_eq!(admitted_count(&plain_admission, INPUT_ENTRIES), 29);
        assert_eq!(admitted_count(&double_admission, INPUT_ENTRIES), 17);
    }

    #[test]
    fn off_scanline_entries_still_consume_traversal_cycles() {
        const OFF_SCANLINE_ENTRIES: usize = 32;
        const OFF_SCANLINE_Y: u8 = 100;

        let baseline = vec![wide_entry(); 25];
        let baseline_admission = OamAdmission::for_scanline(&baseline, 0, false);

        let off_scanline = entry_at(0, OFF_SCANLINE_Y, SIXTY_FOUR_PIXEL_SQUARE_SIZE, ENABLED);
        let mut with_fillers = vec![off_scanline; OFF_SCANLINE_ENTRIES];
        with_fillers.extend(vec![wide_entry(); 25]);
        let filler_admission = OamAdmission::for_scanline(&with_fillers, 0, false);

        assert!(baseline_admission.is_admitted(18));
        assert!(!baseline_admission.is_admitted(19));
        assert!((0..OFF_SCANLINE_ENTRIES).all(|index| !filler_admission.is_admitted(index)));
        assert!(filler_admission.is_admitted(OFF_SCANLINE_ENTRIES + 17));
        assert!(!filler_admission.is_admitted(OFF_SCANLINE_ENTRIES + 18));
    }

    #[test]
    fn scanning_stops_at_the_physical_oam_entry_count() {
        let capped = vec![entry_at_x(0, EIGHT_PIXEL_SQUARE_SIZE); 128];
        let mut overlong = capped.clone();
        overlong.push(wide_entry());

        let (capped_admission, capped_remaining) =
            OamAdmission::walk_with_remaining_cycles(&capped, 0, false);
        let (overlong_admission, overlong_remaining) =
            OamAdmission::walk_with_remaining_cycles(&overlong, 0, false);

        assert_eq!(capped_remaining, 188);
        assert_eq!(overlong_remaining, capped_remaining);
        assert!((0..=128).all(|index| {
            capped_admission.is_admitted(index) == overlong_admission.is_admitted(index)
        }));
        assert!(capped_admission.is_admitted(127));
        assert!(!overlong_admission.is_admitted(128));
    }

    #[test]
    fn partially_off_left_regular_entries_cost_half_the_hidden_columns() {
        const INPUT_ENTRIES: usize = 40;

        let clipped = vec![entry_at_x(-32, SIXTY_FOUR_PIXEL_SQUARE_SIZE); INPUT_ENTRIES];
        let baseline = vec![wide_entry(); INPUT_ENTRIES];

        let (clipped_admission, clipped_remaining) =
            OamAdmission::walk_with_remaining_cycles(&clipped, 0, false);
        let baseline_admission = OamAdmission::for_scanline(&baseline, 0, false);

        assert_eq!(admitted_count(&clipped_admission, INPUT_ENTRIES), 26);
        assert_eq!(admitted_count(&baseline_admission, INPUT_ENTRIES), 19);
        assert_eq!(clipped_remaining, -38);
    }
}
