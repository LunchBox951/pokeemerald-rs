//! The door animations' roots: the table row of each door metatile and the
//! tile sheets those rows point at.

use crate::reader::GbaPtr;
use crate::roots::ImageRoot;

/// One row of `sDoorAnimGraphicsTable`: the metatile it animates and where
/// its two pointers must land.
///
/// The sound, the size class, and the palette banks are read from the ROM,
/// not recorded here; the row records what the ROM must agree with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorAnimationRow {
    /// The door metatile the row animates.
    pub metatile: u16,
    /// The normalized name of the row's sheet: the tail of its
    /// `door-animation/image/` id in [`DoorAnimationRoots::images`].
    pub sheet: &'static str,
    /// The eight-byte `sDoorAnimPalettes_*` array the row's palette pointer
    /// must address.
    pub palette_slots: GbaPtr,
}

/// The door animations' roots (`src/field_door.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorAnimationRoots {
    /// `sDoorAnimGraphicsTable`: 12-byte `{metatile, sound, size, tiles,
    /// palettes}` rows followed by an all-zero row.
    pub table: GbaPtr,
    /// The table's rows, in ROM order.
    pub rows: &'static [DoorAnimationRow],
    /// Every `door-animation/image/*` sheet the rows name.
    pub images: &'static [ImageRoot],
}

impl DoorAnimationRoots {
    /// No door roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        table: GbaPtr::AT_BASE,
        rows: &[],
        images: &[],
    };
}
