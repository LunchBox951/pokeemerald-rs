//! Per-scanline OAM admission: the fixed hardware cycle budget that caps how
//! many of a scanline's OAM entries the GBA's OBJ engine actually processes
//! (S-2, issue #329).
//!
//! Real OBJ hardware does not composite every enabled sprite on every
//! scanline it covers: each scanline the PPU walks OAM in index order,
//! spending a fixed "dot cycle" budget as it goes, and simply stops once
//! that budget runs out. A sprite (or an OBJWIN mask contribution) past the
//! cutoff is dropped for that scanline even though nothing else about it
//! would keep it off-screen. Before this module, [`crate::sprite`] had no
//! such budget — visible resolution walked every enabled entry and OBJWIN
//! masking independently rescanned every entry, so both could show a late
//! sprite/mask that hardware (and the pinned mgba renderer) would drop, and
//! since they scanned independently they had no way to agree with each
//! other about where a scanline's cutoff fell.
//!
//! [`OamAdmission::for_scanline`] is the shared admission stage: it is the
//! only place this budget is computed, and both
//! [`SpriteLayer::resolve_pixel`](crate::sprite::SpriteLayer::resolve_pixel)
//! and
//! [`SpriteLayer::objwin_mask`](crate::sprite::SpriteLayer::objwin_mask)
//! consult it through one shared per-scanline cache (see
//! [`crate::sprite::SpriteLayer`]'s docs), so the two can never disagree
//! about which entries a given scanline exhausted.
//!
//! # Budget selection
//!
//! `mgba/include/mgba/internal/gba/video.h:36-37` defines the two
//! per-scanline budgets: [`OBJ_LENGTH`] (1210 cycles) normally, or
//! [`OBJ_HBLANK_FREE_LENGTH`] (954 cycles, less available time) when
//! `DISPCNT`'s HBlank-interval-free bit is set. `video-software.c:623`
//! selects between them per scanline from that bit; [`OamAdmission::for_scanline`]'s
//! `hblank_free_interval` parameter is that same selector, threaded in by
//! [`SpriteLayer::with_hblank_free_interval`](crate::sprite::SpriteLayer::with_hblank_free_interval)
//! (pokeemerald sets the bit at `pokeemerald/src/overworld.c:2122-2123`).
//!
//! # The sprite list: which entries the walk can spend cycles on
//!
//! mgba splits the work in two. `common.c:13-45`'s OAM clean pass builds a
//! *sprite list* out of the 128 physical OAM slots, recording each surviving
//! entry's own cycle cost and its original OAM index; `video-software.c:1029-1064`
//! then walks that list for one scanline. An OAM slot that never reaches the
//! list still exists as an index gap, and the walk's traversal charge is
//! computed from index *deltas* — so a rejected slot costs
//! [`TRAVERSAL_COST`] and nothing more. This port keeps the flat
//! `&[OamEntry]` slice (index == OAM index) and folds the clean pass into
//! [`sprite_list_cost`], which returns `None` for exactly the slots mgba
//! drops:
//!
//! - **Disabled** entries (`common.c:18`'s `IsTransformed || !IsDisable`;
//!   this port models the hardware "hidden" attr0 encoding as
//!   [`OamEntry::enabled`] — see `oam.rs`'s module docs).
//! - Entries lying **entirely off the left edge**, `x + width < 0`. For a
//!   regular entry that is `common.c:34-36`'s explicit `continue`. An affine
//!   entry has no such `continue` in its own cost branch, but
//!   `common.c:43-45` — which tests the *raw* 9-bit X field, where a
//!   negative `x` reads back as `x + 512 >= 240` — rejects it a few lines
//!   later all the same: `rawX + width < 512` is exactly `x + width < 0`.
//!   So the rule is common to both sprite kinds, and it is what keeps every
//!   cost below non-negative (see [`sprite_list_cost`]); admitting a
//!   far-off-left affine entry would otherwise *refund* budget.
//! - Entries entirely off the **right edge**, `x >= 240`
//!   (`common.c:43-45`). Like an off-left entry, such a slot retains only
//!   the walk's [`TRAVERSAL_COST`] and cannot drain the per-scanline budget
//!   or displace a later visible sprite.
//!
//! `common.c:40-42` also rejects entries whose box can never reach a visible
//! scanline. That vertical rejection needs no separate modelling: the walk
//! below already skips an entry that does not cover the scanline *after*
//! charging traversal and *without* charging its own cost, which is the same
//! arithmetic the clean pass' index gap produces.
//!
//! # Per-entry cost
//!
//! [`sprite_list_cost`] ports `common.c:24-39` exactly. `width` is the
//! entry's [`OamEntry::bounding_box`] width, which is already the
//! `width <<= doubleSize` of `common.c:26` for
//! [`AffineMode::AffineDoubleSize`]:
//!
//! - **Affine** (`common.c:27-30`): `8 + width * 2`, then `+= x` when
//!   `x < 0` — one full column of cost per off-screen column, never clipped
//!   or clamped.
//! - **Regular** (`common.c:32-38`): `width - 2`, then `+= x >> 1` when
//!   `x < 0` — half an off-screen column each, with C's arithmetic right
//!   shift rounding *away* from zero for odd negative `x` (Rust's `>>` on
//!   `i32` matches bit for bit: `-33 >> 1 == -17`).
//!
//! mgba never clips `width` itself, and the two adjustments are not the same
//! function of `x`; a 64-wide entry costs 62 (regular) / 136 (affine) at
//! `x = 0`, 46 / 104 at `x = -32`, and 30 / 72 at `x = -64`.
//!
//! # Traversal and the exhaustion boundary
//!
//! `common.c:13` caps the clean pass at [`MAX_OAM_ENTRIES`] (128) physical
//! OAM slots — the 129th (and beyond) is never even stepped, regardless of
//! budget. Per `video-software.c:1035-1039`, stepping onto a slot costs
//! `2 * (index - lastIndex)` with `lastIndex` starting at `0`: walking every
//! slot in turn, that is [`TRAVERSAL_COST`] per slot *except OAM index 0,
//! whose traversal is never charged*. The charge lands *before* the
//! exhaustion test, and that test is `remaining <= 0` (not `< 0`), so a
//! budget that lands exactly on zero is already exhausted.
//!
//! An entry that does not cover the scanline is skipped only *after* the
//! traversal charge and the exhaustion test (`video-software.c:1040-1042`),
//! so it still spends those 2 cycles and can itself trip exhaustion even
//! though it draws nothing. An admitted entry's own cost is deducted only
//! *after* it is processed (`video-software.c:1064`), so a sprite whose own
//! cost drives the remaining budget to or below zero is still fully drawn —
//! the *next* slot's traversal charge is what detects that and stops every
//! entry after it (never the admitted entry itself).
//!
//! Reimplemented from the cited mgba behavior (verified, not transliterated
//! — `no-verbatim`): the module lives at a different granularity (a
//! standalone per-scanline admission pass over a borrowed `&[OamEntry]`
//! slice, rather than mgba's split clean-pass/row-buffer walk) and uses this
//! port's own naming and control flow throughout.

use crate::oam::{AffineMode, OamEntry};

/// `mgba/include/mgba/internal/gba/video.h:37`: the normal per-scanline OBJ
/// cycle budget, selected when `DISPCNT`'s HBlank-interval-free bit is
/// clear.
pub const OBJ_LENGTH: i32 = 1210;

/// `mgba/include/mgba/internal/gba/video.h:36`: the reduced per-scanline OBJ
/// cycle budget selected when `DISPCNT`'s HBlank-interval-free bit is set —
/// less time is available per scanline because OBJ preparation for the
/// *next* line starts earlier.
pub const OBJ_HBLANK_FREE_LENGTH: i32 = 954;

/// `common.c:13`: OAM holds 128 physical entries: the hardware OAM scan
/// never steps past the 128th regardless of how many entries a caller's
/// model contains.
pub const MAX_OAM_ENTRIES: usize = 128;

/// Per-slot traversal charge (module docs): `video-software.c:1035`'s
/// `2 * (index - lastIndex)`, which over a walk that steps every slot is
/// this much per slot after OAM index 0.
const TRAVERSAL_COST: i32 = 2;

// Counts `OamAdmission::for_scanline` walks on the current thread, so the
// per-scanline caching in `crate::sprite` can be asserted on rather than
// merely argued (`sprite.rs`'s cache tests). Thread-local because the test
// harness runs each test on its own thread.
#[cfg(test)]
thread_local! {
    pub(crate) static WALK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Reset this thread's [`WALK_COUNT`] to `0`.
#[cfg(test)]
pub(crate) fn reset_walk_count() {
    WALK_COUNT.with(|count| count.set(0));
}

/// How many [`OamAdmission::for_scanline`] walks ran on this thread since
/// the last [`reset_walk_count`].
#[cfg(test)]
pub(crate) fn walk_count() -> usize {
    WALK_COUNT.with(std::cell::Cell::get)
}

/// Which of a scanline's OAM entries the hardware cycle budget admits,
/// computed once per `(entries, y, hblank_free_interval)` by
/// [`for_scanline`](Self::for_scanline) (module docs).
#[derive(Debug, Clone, Copy)]
pub(crate) struct OamAdmission {
    admitted: [bool; MAX_OAM_ENTRIES],
}

impl OamAdmission {
    /// Walk `entries` in OAM order for scanline `y` (`0..160`), charging
    /// traversal and per-sprite cost exactly as the module docs describe,
    /// and record which indices the budget admitted.
    ///
    /// `entries` may hold more than [`MAX_OAM_ENTRIES`] — only the first
    /// [`MAX_OAM_ENTRIES`] are ever stepped, matching real OAM's fixed
    /// physical size (`common.c:13`); every later index is neither charged
    /// for nor admitted.
    pub(crate) fn for_scanline(entries: &[OamEntry], y: usize, hblank_free_interval: bool) -> Self {
        Self::walk(entries, y, hblank_free_interval).0
    }

    /// [`for_scanline`](Self::for_scanline)'s walk, also returning the
    /// budget left when it ended — which the unit tests below assert on to
    /// pin down cycles that no admission flag can show (most importantly
    /// that a 129th entry is charged for neither traversal nor cost).
    fn walk(entries: &[OamEntry], y: usize, hblank_free_interval: bool) -> (Self, i32) {
        #[cfg(test)]
        WALK_COUNT.with(|count| count.set(count.get() + 1));

        let mut admitted = [false; MAX_OAM_ENTRIES];
        let mut remaining = if hblank_free_interval {
            OBJ_HBLANK_FREE_LENGTH
        } else {
            OBJ_LENGTH
        };

        let scanned = entries.len().min(MAX_OAM_ENTRIES);
        for (index, entry) in entries[..scanned].iter().enumerate() {
            if index > 0 {
                // `2 * (sprite->index - lastIndex)` with `lastIndex`
                // advancing one slot at a time, so OAM index 0 is free and
                // every later slot costs `TRAVERSAL_COST` -- module docs.
                remaining -= TRAVERSAL_COST;
            }
            // Charged first, tested second, and `<= 0` rather than `< 0`.
            if remaining <= 0 {
                break;
            }
            // Slots mgba's clean pass never puts in the sprite list cost
            // their traversal above and nothing else (module docs).
            let Some(cost) = sprite_list_cost(entry) else {
                continue;
            };
            // On the list but not on this scanline: skipped only now, having
            // already paid the traversal charge and passed the test above.
            if !entry.covers_scanline(y) {
                continue;
            }
            admitted[index] = true;
            // Deducted after admission: this may drive `remaining` to or
            // below zero, but that only stops the *next* slot's traversal
            // charge, not this entry (module docs' boundary rule).
            remaining -= cost;
        }

        (Self { admitted }, remaining)
    }

    /// Whether OAM index `index` was admitted for the scanline this
    /// [`OamAdmission`] was computed for.
    pub(crate) fn is_admitted(&self, index: usize) -> bool {
        self.admitted.get(index).copied().unwrap_or(false)
    }
}

/// The cycle cost mgba's OAM clean pass records for `entry`, or `None` if
/// that pass rejects it outright so it never enters the sprite list — see
/// the module docs for both the rejection rules (`common.c:18`,
/// `common.c:34-36`, `common.c:43-45`) and the two cost formulas
/// (`common.c:24-39`).
///
/// The returned cost is always positive: the off-left rejection bounds `x`
/// at `-width`, leaving a regular entry at least `width / 2 - 2` and an
/// affine one at least `width + 8`.
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    reason = "a bounding-box width is at most 128 (64 doubled), so the i32 cast is exact"
)]
fn sprite_list_cost(entry: &OamEntry) -> Option<i32> {
    if !entry.enabled() {
        return None;
    }
    let (width, _height) = entry.bounding_box();
    let width = width as i32;
    let x = i32::from(entry.x());
    if x + width < 0 || x >= 240 {
        return None;
    }
    Some(match entry.affine() {
        AffineMode::Regular => {
            // `x >> 1` is C's arithmetic shift on a negative value, which
            // Rust's `>>` on `i32` reproduces exactly (`-33 >> 1 == -17`).
            width - 2 + if x < 0 { x >> 1 } else { 0 }
        }
        AffineMode::Affine { .. } | AffineMode::AffineDoubleSize { .. } => {
            8 + width * 2 + if x < 0 { x } else { 0 }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{sprite_list_cost, OamAdmission, MAX_OAM_ENTRIES};
    use crate::oam::{AffineMode, OamEntry, ObjShape};

    /// A regular (non-affine) opaque square sprite at `y=0`, enabled, whose
    /// width comes from `width_size` (an [`ObjShape::Square`] size index)
    /// and whose raw 9-bit X field is `x_raw` (`0x1E0` is -32, and so on).
    fn entry_at(x_raw: u16, width_size: u8) -> OamEntry {
        OamEntry::new(
            x_raw,
            0,
            0,
            0,
            crate::tile::BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            width_size,
            0,
            true,
        )
    }

    /// A regular sprite at `x=0`; `width_size` 3 ([`ObjShape::Square`]) is
    /// 64x64.
    fn regular_entry(width_size: u8) -> OamEntry {
        entry_at(0, width_size)
    }

    /// The 64-px-wide regular sprite at `x=0` that the issue's reachability
    /// example uses: cost exactly 62.
    fn wide_64_entry() -> OamEntry {
        regular_entry(3)
    }

    #[test]
    fn sprite_list_cost_matches_mgbas_two_formulas_including_the_negative_x_adjustments() {
        // `common.c:24-39`, pinned as absolute cycle counts rather than
        // relative comparisons. A 64-wide entry: regular `width - 2` and
        // affine `8 + width * 2` at x=0, then `x >> 1` / `x` added as x goes
        // negative -- mgba never clips `width` itself, so the two kinds do
        // *not* fall off at the same rate.
        let affine = |entry: OamEntry| entry.with_affine(AffineMode::Affine { matrix_num: 0 });

        assert_eq!(sprite_list_cost(&entry_at(0, 3)), Some(62), "regular, x=0");
        assert_eq!(
            sprite_list_cost(&entry_at(0x1E0, 3)),
            Some(46),
            "regular, x=-32: 62 + (-32 >> 1)"
        );
        assert_eq!(
            sprite_list_cost(&entry_at(0x1C0, 3)),
            Some(30),
            "regular, x=-64: 62 + (-64 >> 1)"
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at(0, 3))),
            Some(136),
            "affine, x=0: 8 + 64 * 2"
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at(0x1E0, 3))),
            Some(104),
            "affine, x=-32: 136 - 32"
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at(0x1C0, 3))),
            Some(72),
            "affine, x=-64: 136 - 64"
        );

        // C's arithmetic right shift rounds an odd negative away from zero:
        // -33 >> 1 == -17, not -16. A 64-wide regular entry at x=-33 is
        // therefore 62 - 17 = 45.
        assert_eq!(
            sprite_list_cost(&entry_at(0x1DF, 3)),
            Some(45),
            "regular, x=-33: 62 + (-33 >> 1) == 62 - 17"
        );

        // Double-size doubles the bounding box before the formula runs
        // (`common.c:26`), so a 32x32 double-size entry costs exactly what a
        // 64-wide plain affine entry costs, and twice-ish what a 32-wide
        // plain affine one costs.
        assert_eq!(
            sprite_list_cost(
                &entry_at(0, 2).with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 })
            ),
            Some(136),
            "double-size 32x32 == plain affine 64-wide"
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at(0, 2))),
            Some(72),
            "plain affine, 32-wide"
        );

        // The smallest entries, for the runs the budget tests below use.
        assert_eq!(sprite_list_cost(&entry_at(0, 0)), Some(6), "regular 8x8");
    }

    #[test]
    fn an_entry_entirely_off_the_left_edge_never_enters_the_sprite_list() {
        // `x + width < 0` (`common.c:34-36`, and `common.c:43-45` for the
        // affine kind): rejected outright, for both sprite kinds. x=-64 with
        // width 64 is the last accepted position (its rightmost column is
        // screen column -1... exactly `x + width == 0`), x=-65 the first
        // rejected one.
        assert!(
            sprite_list_cost(&entry_at(0x1C0, 3)).is_some(),
            "x=-64 kept"
        );
        assert_eq!(
            sprite_list_cost(&entry_at(0x1BF, 3)),
            None,
            "x=-65 rejected"
        );
        assert_eq!(
            sprite_list_cost(&entry_at(0x1A0, 3).with_affine(AffineMode::Affine { matrix_num: 0 })),
            None,
            "affine x=-96 rejected too -- otherwise its cost would be 136 - 96 = 40 here \
             and negative (a budget *refund*) further left"
        );
        // A disabled entry is likewise absent from the list (`common.c:18`).
        let disabled = OamEntry::new(
            0,
            0,
            0,
            0,
            crate::tile::BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3,
            0,
            false,
        );
        assert_eq!(sprite_list_cost(&disabled), None, "disabled entry rejected");
    }

    #[test]
    fn an_entry_at_or_beyond_the_right_edge_never_enters_the_sprite_list() {
        let affine = |entry: OamEntry| entry.with_affine(AffineMode::Affine { matrix_num: 0 });

        assert!(
            sprite_list_cost(&entry_at(239, 0)).is_some(),
            "x=239 is the last decoded X position retained by the clean pass"
        );
        assert_eq!(
            sprite_list_cost(&entry_at(240, 0)),
            None,
            "regular x=240 is the first right-edge rejection"
        );
        assert_eq!(
            sprite_list_cost(&affine(entry_at(255, 3))),
            None,
            "affine entries at the far end of the positive X range are rejected too"
        );
    }

    #[test]
    fn right_edge_entries_cannot_exhaust_budget_or_displace_a_later_visible_sprite() {
        // If these 100 64-wide entries at x=240 were charged their regular
        // 62-cycle cost, the walk would exhaust its budget before reaching
        // the visible entry at index 100. Rejection leaves only traversal:
        // 200 cycles through index 100, then 62 for the visible entry.
        let mut entries = vec![entry_at(240, 3); 100];
        entries.push(wide_64_entry());

        let (admission, remaining) = OamAdmission::walk(&entries, 0, false);

        for index in 0..100 {
            assert!(
                !admission.is_admitted(index),
                "right-edge entry {index} is absent from the sprite list"
            );
        }
        assert!(
            admission.is_admitted(100),
            "right-edge entries must not displace the later visible sprite"
        );
        assert_eq!(remaining, 948, "1210 - 100 * 2 traversal - 62 cost");
    }

    #[test]
    fn a_rejected_entry_costs_its_traversal_and_nothing_else() {
        // 100 fully-off-left 64-wide regular entries (x=-65) ahead of the
        // usual x=0 run. Each rejected slot costs only its 2-cycle traversal
        // -- and, critically, must not *refund* budget the way a clipped
        // width of 0 (cost -2) would.
        //
        // Traversal for slots 1..=99 is 198 (slot 0 is free), so the run
        // starting at index 100 sees 1210 - 200 = 1010 on stepping onto it
        // (its own traversal included). Each admitted 64-wide entry then
        // costs 62 + 2 = 64: 1010 - 64 * 15 = 50 > 0 admits index 115, and
        // 1010 - 64 * 16 = -14 <= 0 stops at index 116.
        let mut entries = vec![entry_at(0x1BF, 3); 100];
        entries.extend(vec![wide_64_entry(); 20]);
        let (admission, remaining) = OamAdmission::walk(&entries, 0, false);

        for index in 0..100 {
            assert!(
                !admission.is_admitted(index),
                "off-left entry {index} is not in the sprite list at all"
            );
        }
        let admitted = (0..120).filter(|&i| admission.is_admitted(i)).count();
        assert_eq!(admitted, 16, "indices 100..=115");
        assert!(admission.is_admitted(115));
        assert!(!admission.is_admitted(116));
        assert_eq!(remaining, -14, "1210 - 198 - 2 - 16 * 64");
    }

    #[test]
    fn normal_budget_drops_the_documented_entry_under_the_1210_cycle_budget() {
        // Issue #329's reachability example, rederived under mgba's
        // `lastIndex` traversal rule (OAM index 0's traversal is free): the
        // budget on stepping onto index i is 1210 - 64i (62 own cost + 2
        // traversal per earlier entry, minus the free first traversal).
        // 1210 - 64 * 18 = 58 > 0 admits index 18 (leaving 58 - 62 = -4),
        // and index 19's traversal makes that -6 <= 0, so the walk stops
        // there.
        let entries = vec![wide_64_entry(); 25];
        let (admission, remaining) = OamAdmission::walk(&entries, 0, false);
        for index in 0..=18 {
            assert!(
                admission.is_admitted(index),
                "entry {index} must be admitted"
            );
        }
        assert!(
            !admission.is_admitted(19),
            "entry 19 must be dropped once the 1210 budget is exhausted"
        );
        // Nothing after the cutoff is admitted either.
        for index in 20..25 {
            assert!(!admission.is_admitted(index));
        }
        assert_eq!(remaining, -6, "1210 - 19 * 64 + 2 (index 0 traverses free)");
    }

    #[test]
    fn hblank_free_budget_exhausts_strictly_earlier_than_the_normal_budget() {
        // Same 64-px-wide sprites, but under the reduced 954-cycle budget
        // (DISPCNT's HBlank-interval-free bit set). By the same arithmetic:
        // 954 - 64 * 14 = 58 > 0 admits index 14 (leaving -4), and index
        // 15's traversal makes that -6 <= 0.
        let entries = vec![wide_64_entry(); 25];
        let normal = OamAdmission::for_scanline(&entries, 0, false);
        let (hblank_free, remaining) = OamAdmission::walk(&entries, 0, true);

        assert!(normal.is_admitted(18));
        assert!(!normal.is_admitted(19));

        assert!(hblank_free.is_admitted(14));
        assert!(!hblank_free.is_admitted(15));
        assert_eq!(remaining, -6, "954 - 15 * 64 + 2");
    }

    #[test]
    fn affine_sprites_cost_more_than_a_same_width_regular_sprite() {
        // An 8x8 affine entry costs `8 + 8 * 2` = 24; the same-size regular
        // entry costs `8 - 2` = 6. 150 entries (more than MAX_OAM_ENTRIES)
        // so the regular run is limited only by the 128-entry cap
        // (1210 - 128 * 8 + 2 = 188 left over), while the pricier affine run
        // genuinely exhausts the cycle budget: 1210 - 26 * (24 + 2) = 534
        // ... admits index 46 at 1210 - 46 * 26 = 14 > 0 and stops at index
        // 47 (14 - 24 - 2 = -12 <= 0), i.e. exactly 47 admitted.
        let regular: Vec<OamEntry> = (0..150).map(|_| regular_entry(0)).collect(); // 8x8
        let affine: Vec<OamEntry> = (0..150)
            .map(|_| regular_entry(0).with_affine(AffineMode::Affine { matrix_num: 0 }))
            .collect();

        let regular_admission = OamAdmission::for_scanline(&regular, 0, false);
        let affine_admission = OamAdmission::for_scanline(&affine, 0, false);

        let regular_count = (0..150)
            .filter(|&i| regular_admission.is_admitted(i))
            .count();
        let affine_count = (0..150)
            .filter(|&i| affine_admission.is_admitted(i))
            .count();

        assert_eq!(
            regular_count, MAX_OAM_ENTRIES,
            "the cheap regular run is limited only by the 128-entry cap"
        );
        assert_eq!(
            affine_count, 47,
            "24 cycles each plus traversal exhausts 1210 after index 46"
        );
    }

    #[test]
    fn double_size_affine_charges_the_doubled_bounding_box_width() {
        // AffineDoubleSize widens the bounding box (oam.rs), which this
        // module's cost formula reads directly (`common.c:26`). 16x16 plain
        // affine costs 8 + 32 = 40 (42 with traversal): 1210 - 28 * 42 = 34
        // > 0 admits index 28 and index 29 stops it (34 - 40 - 2 = -8), so
        // 29 admitted. Double-size makes that a 32-wide box costing 72 (74
        // with traversal): 1210 - 16 * 74 = 26 > 0 admits index 16, index 17
        // stops it (26 - 72 - 2 = -48), so 17 admitted.
        let plain_affine: Vec<OamEntry> = (0..40)
            .map(|_| regular_entry(1).with_affine(AffineMode::Affine { matrix_num: 0 })) // 16x16
            .collect();
        let double_size: Vec<OamEntry> = (0..40)
            .map(|_| regular_entry(1).with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 }))
            .collect();

        let plain_admission = OamAdmission::for_scanline(&plain_affine, 0, false);
        let double_admission = OamAdmission::for_scanline(&double_size, 0, false);

        let plain_count = (0..40).filter(|&i| plain_admission.is_admitted(i)).count();
        let double_count = (0..40).filter(|&i| double_admission.is_admitted(i)).count();

        assert_eq!(plain_count, 29, "16x16 plain affine costs 40 each");
        assert_eq!(double_count, 17, "the doubled 32-wide box costs 72 each");
    }

    #[test]
    fn off_scanline_entries_still_consume_traversal_cycles() {
        // Prepend 32 off-scanline filler entries (y=100, never covering
        // scanline 0) ahead of the same 64-px-wide sprite run used above.
        // Each filler costs nothing of its own, but still charges the flat
        // 2-cycle traversal cost (module docs) -- 31 of them for slots
        // 1..=31 plus the run's own first traversal -- so the run starting
        // at index 32 sees 1210 - 64 = 1146, shifting its cutoff one entry
        // earlier (from 19 to 18) compared to the no-filler baseline.
        let baseline: Vec<OamEntry> = vec![wide_64_entry(); 25];
        let baseline_admission = OamAdmission::for_scanline(&baseline, 0, false);
        assert!(baseline_admission.is_admitted(18));
        assert!(!baseline_admission.is_admitted(19));

        let off_scanline = OamEntry::new(
            0,
            // y=100 with a 64-tall box: 100 + 64 = 164 <= 256, so no
            // bottom-wrap kicks in (OamEntry's module docs) and the box
            // simply sits at rows 100..163 -- nowhere near scanline 0.
            100,
            0,
            0,
            crate::tile::BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3,
            0,
            true,
        );
        let mut with_fillers = vec![off_scanline; 32];
        with_fillers.extend(vec![wide_64_entry(); 25]);
        let filler_admission = OamAdmission::for_scanline(&with_fillers, 0, false);

        // None of the 32 fillers are themselves admitted (they never cover
        // scanline 0)...
        for index in 0..32 {
            assert!(!filler_admission.is_admitted(index));
        }
        // ...but the on-scanline run right after them is now cut off one
        // entry earlier: 1146 - 64 * 17 = 58 > 0 admits index 32 + 17, and
        // 1146 - 64 * 18 = -6 <= 0 stops at 32 + 18, where the no-filler
        // baseline admitted one entry further.
        assert!(filler_admission.is_admitted(32 + 17));
        assert!(
            !filler_admission.is_admitted(32 + 18),
            "32 off-scanline fillers (64 traversal cycles) must shift the cutoff one entry earlier"
        );
    }

    #[test]
    fn the_scan_stops_dead_at_the_128th_entry() {
        // 128 cheap 8x8 regular entries (6 cycles each, 8 with traversal),
        // which the 1210 budget covers with room to spare: 1210 - 128 * 8 +
        // 2 (index 0's free traversal) = 188 left. Appending a 129th entry
        // that would cost a very visible 62 + 2 = 64 must change *nothing*:
        // not the admitted set, and not the leftover budget -- proving the
        // walk stops at the 128-entry physical OAM cap (`common.c:13`)
        // rather than merely failing to record a 129th admission flag.
        let capped = vec![regular_entry(0); MAX_OAM_ENTRIES];
        let mut overlong = capped.clone();
        overlong.push(wide_64_entry());

        let (capped_admission, capped_remaining) = OamAdmission::walk(&capped, 0, false);
        let (overlong_admission, overlong_remaining) = OamAdmission::walk(&overlong, 0, false);

        assert_eq!(capped_remaining, 188, "1210 - 128 * 8 + 2");
        assert_eq!(
            overlong_remaining, capped_remaining,
            "the 129th entry is charged neither its 2-cycle traversal nor its 62-cycle cost"
        );
        for index in 0..=MAX_OAM_ENTRIES {
            assert_eq!(
                capped_admission.is_admitted(index),
                overlong_admission.is_admitted(index),
                "entry {index}'s admission must not depend on a 129th entry existing"
            );
        }
        assert!(
            capped_admission.is_admitted(MAX_OAM_ENTRIES - 1),
            "the 128th entry (index 127) has ample budget and must be admitted"
        );
        assert!(
            !overlong_admission.is_admitted(MAX_OAM_ENTRIES),
            "the 129th entry (index 128) is never scanned at all"
        );
    }

    #[test]
    fn a_partly_off_left_regular_sprite_costs_half_its_off_screen_columns() {
        // A 64-wide regular sprite at raw x=0x1E0 (-32) costs 62 + (-32 >> 1)
        // = 46, i.e. 48 with traversal: 1210 - 48 * 25 = 10 > 0 admits index
        // 25, and 1210 - 48 * 26 = -38 <= 0 stops at index 26 -- exactly 26
        // admitted, against 19 for the same run at x=0. (mgba does not clip
        // the *width* to the on-screen part, which would have cost 30 here.)
        let half_off_left = entry_at(0x1E0, 3);
        let clipped: Vec<OamEntry> = vec![half_off_left; 40];
        let baseline: Vec<OamEntry> = vec![wide_64_entry(); 40];

        let (clipped_admission, clipped_remaining) = OamAdmission::walk(&clipped, 0, false);
        let baseline_admission = OamAdmission::for_scanline(&baseline, 0, false);

        let clipped_count = (0..40)
            .filter(|&i| clipped_admission.is_admitted(i))
            .count();
        let baseline_count = (0..40)
            .filter(|&i| baseline_admission.is_admitted(i))
            .count();

        assert_eq!(clipped_count, 26, "1210 / 48, boundary included");
        assert_eq!(baseline_count, 19, "the x=0 run for comparison");
        assert_eq!(clipped_remaining, -38, "1210 - 26 * 48 + 2");
    }
}
