//! Typed `ObjectEventGraphicsInfo` rows for the stationary object graphics the
//! checkpoint maps place, other than the 144x32 nine-frame people sheets that
//! [`super::NpcSpriteSource::People16x32`] binds.
//!
//! Each row records the frame geometry (`.width`/`.height`), the sheet cell its
//! initial animation shows for a facing, and the palette, as upstream's
//! `object_event_graphics_info.h`, `object_event_pic_tables.h` and
//! `object_event_anims.h` declare them. Only the stationary first frame is
//! modelled: walking, bowing, cutting and berry-growth animation are not.

use assets::pack::AssetPack;
use engine::overworld::Direction;

use super::super::avatar::stand_frame_for;
use super::super::{pack_4bpp_region, OverworldSceneError};

/// Which sheet cell an object shows while it stands still.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Facing {
    /// `sAnimTable_Standard`'s south, north and west stand frames, resolved
    /// through the graphic's pic table to these sheet cells; east reuses west
    /// flipped (`GetFaceDirectionAnimNum`, `event_object_movement.c`).
    Standard { south: u16, north: u16, west: u16 },
    /// An `.inanimate` object, or one whose animation ignores facing, always
    /// shows this cell unflipped: `sAnimTable_Inanimate` has one frame and
    /// the facing animation is never started
    /// (`event_object_movement.c:1469-1471`).
    Fixed(u16),
}

impl Facing {
    /// The nine-cell people layout: south, north, west at cells 0 to 2.
    pub(super) const PEOPLE: Self = Self::Standard {
        south: 0,
        north: 1,
        west: 2,
    };

    /// The cell and horizontal flip that show `facing`.
    pub(super) const fn cell_for(self, facing: Direction) -> (u16, bool) {
        match self {
            Self::Standard { south, north, west } => {
                let (stand, h_flip) = stand_frame_for(facing);
                let cell = match stand {
                    0 => south,
                    1 => north,
                    _ => west,
                };
                (cell, h_flip)
            }
            Self::Fixed(cell) => (cell, false),
        }
    }
}

/// One non-people-sheet graphic: where its frames live and how one is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObjectSheet {
    /// The pack sprite path (`sprite/<path>`).
    pub(super) sprite_path: &'static str,
    /// One frame's width in pixels (`.width`).
    pub(super) width: u16,
    /// One frame's height in pixels (`.height`).
    pub(super) height: u16,
    /// How many frames the source sheet holds side by side.
    pub(super) cells: u16,
    /// The combined-palette bank the graphic draws with.
    pub(super) palette_bank: u8,
    pub(super) facing: Facing,
}

/// Combined-palette banks for upstream's `PALSLOT_NPC_SPECIAL` object
/// palettes. Upstream keeps one special palette resident and the last
/// loaded wins; the native palette gives each its own bank so every object on
/// a map keeps its own colours. Bank 6 is left free for field effects.
pub(super) const MOVING_BOX_BANK: u8 = 7;
pub(super) const TRUCK_BANK: u8 = 8;
pub(super) const VIGOROTH_BANK: u8 = 9;
pub(super) const ZIGZAGOON_BANK: u8 = 10;

/// The pack palette name and bank of each object palette beside the generic
/// `npc_*` banks (`sObjectEventSpritePalettes`).
pub(super) const OBJECT_PALETTES: [(&str, u8); 4] = [
    ("moving_box", MOVING_BOX_BANK),
    ("truck", TRUCK_BANK),
    ("vigoroth", VIGOROTH_BANK),
    ("enemy_zigzagoon", ZIGZAGOON_BANK),
];

const fn sheet(
    sprite_path: &'static str,
    (width, height, cells): (u16, u16, u16),
    palette_bank: u8,
    facing: Facing,
) -> ObjectSheet {
    ObjectSheet {
        sprite_path,
        width,
        height,
        cells,
        palette_bank,
        facing,
    }
}

const STILL: Facing = Facing::Fixed(0);

/// Returns the sheet row of a fixed non-people-sheet graphic, or `None`.
pub(super) fn object_sheet(graphics_id: &str) -> Option<ObjectSheet> {
    // Generic NPC palette tags NPC_1..NPC_4 sit at banks 1..4.
    Some(match graphics_id {
        "OBJ_EVENT_GFX_NINJA_BOY" => sheet("ninja_boy", (16, 16, 9), 1, Facing::PEOPLE),
        "OBJ_EVENT_GFX_NURSE" => sheet("nurse", (16, 32, 4), 1, Facing::PEOPLE),
        "OBJ_EVENT_GFX_MYSTERY_GIFT_MAN" => {
            sheet("mystery_event_deliveryman", (16, 32, 3), 1, Facing::PEOPLE)
        }
        "OBJ_EVENT_GFX_ITEM_BALL" => sheet("misc/item_ball", (16, 16, 1), 3, STILL),
        "OBJ_EVENT_GFX_BIRCHS_BAG" => sheet("misc/birchs_bag", (16, 16, 1), 2, STILL),
        "OBJ_EVENT_GFX_CUTTABLE_TREE" => sheet("misc/cuttable_tree", (16, 16, 4), 3, STILL),
        "OBJ_EVENT_GFX_MOVING_BOX" => sheet("misc/moving_box", (16, 16, 1), MOVING_BOX_BANK, STILL),
        "OBJ_EVENT_GFX_PICHU_DOLL" => sheet("dolls/pichu_doll", (16, 16, 1), 2, STILL),
        "OBJ_EVENT_GFX_SWABLU_DOLL" => sheet("dolls/swablu_doll", (16, 16, 1), 1, STILL),
        "OBJ_EVENT_GFX_TRUCK" => sheet("misc/truck", (48, 48, 1), TRUCK_BANK, STILL),
        // `sPicTable_VigorothCarryingBox` shows cell 0 for the stand frames and
        // `sPicTable_VigorothFacingAway` cell 3; both flip for east.
        "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX" => sheet(
            "pokemon/vigoroth",
            (32, 32, 5),
            VIGOROTH_BANK,
            Facing::Standard {
                south: 0,
                north: 0,
                west: 0,
            },
        ),
        "OBJ_EVENT_GFX_VIGOROTH_FACING_AWAY" => sheet(
            "pokemon/vigoroth",
            (32, 32, 5),
            VIGOROTH_BANK,
            Facing::Standard {
                south: 3,
                north: 3,
                west: 3,
            },
        ),
        "OBJ_EVENT_GFX_ZIGZAGOON_1" => sheet(
            "pokemon/enemy_zigzagoon",
            (32, 32, 9),
            ZIGZAGOON_BANK,
            Facing::PEOPLE,
        ),
        _ => return None,
    })
}

/// The sheet of a berry tree at `BERRY_STAGE_BERRIES`, the stage
/// `data/scripts/new_game.inc:27-30` plants Route 103's three trees at.
///
/// Stage 4's animation (`sAnim_BerryTreeStage4`) starts at pic 7, the fifth
/// 16x32 cell of the berry sheet (`sPicTable_CheriBerryTree`,
/// `sPicTable_LeppaBerryTree`: pics 3 to 8 are cells 0 to 5), drawn with the
/// stage's palette slot from `gBerryTreePaletteSlotTable_Cheri`/`_Leppa`:
/// `NPC_3` for Cheri and `NPC_2` for Leppa.
///
/// This is a static initial state: berry growth, planting and watering are
/// not modelled, so a tree is drawn as its fresh-save stage whatever the save
/// holds.
pub(super) fn initial_berry_sheet(tree_id: &str) -> Option<ObjectSheet> {
    match tree_id {
        "BERRY_TREE_ROUTE_103_CHERI_1" | "BERRY_TREE_ROUTE_103_CHERI_2" => {
            Some(sheet("berry_trees/cheri", (16, 32, 6), 3, Facing::Fixed(4)))
        }
        "BERRY_TREE_ROUTE_103_LEPPA" => {
            Some(sheet("berry_trees/leppa", (16, 32, 6), 2, Facing::Fixed(4)))
        }
        _ => None,
    }
}

impl ObjectSheet {
    /// A nine-frame 16x32 people sheet.
    pub(super) const fn people(sprite_path: &'static str, palette_bank: u8) -> Self {
        sheet(sprite_path, (16, 32, 9), palette_bank, Facing::PEOPLE)
    }

    /// Packs the cells whose bit is set in `cells` as consecutive
    /// raster-order tile blocks, in ascending cell order.
    ///
    /// # Errors
    ///
    /// Returns a dimension or packing error when the pack's sheet is not
    /// `self.cells` frames of `width` by `height` pixels.
    pub(super) fn pack(self, pack: &AssetPack, cells: u16) -> Result<Vec<u8>, OverworldSceneError> {
        let image = pack.sprite(self.sprite_path)?;
        let expected = (
            u32::from(self.width) * u32::from(self.cells),
            u32::from(self.height),
        );
        if (image.width, image.height) != expected {
            return Err(OverworldSceneError::SpriteSheetWrongDimensions {
                id: self.sprite_path,
                expected,
                actual: (image.width, image.height),
            });
        }
        let mut bytes = Vec::new();
        for cell in (0..self.cells).filter(|cell| cells & (1 << cell) != 0) {
            bytes.extend(pack_4bpp_region(
                self.sprite_path,
                image,
                usize::from(cell) * usize::from(self.width),
                0,
                usize::from(self.width),
                usize::from(self.height),
            )?);
        }
        Ok(bytes)
    }
}
