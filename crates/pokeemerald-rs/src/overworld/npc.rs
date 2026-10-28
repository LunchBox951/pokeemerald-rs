//! Renders live object events backed by supported object graphics.
//!
//! [`resolve_sprite_source`] is the complete graphics-id binding table: the
//! protagonist sheets, the 16x32 people sheets, and the [`sheets`] rows for
//! the other 16x16, 16x32, 32x32 and 48x48 graphics the checkpoint maps place.
//! Hidden events are filtered through `engine::overworld::object_event_is_visible`,
//! the same visibility used for interaction. A visible unsupported event remains
//! interaction-ready but produces no [`OamEntry`]. `OBJ_EVENT_GFX_VAR_0` binds
//! only when its event variable contains one of the two Route 103 rival ids;
//! decoration slots remain unsupported. Code that clears a `FLAG_DECORATION_n`
//! hide flag must first write that slot's `VAR_OBJ_GFX_ID_0 + n`, or the
//! persistent rival id resolves the decoration to a rival sprite.
//! `OBJ_EVENT_GFX_BERRY_TREE` binds per event, by its berry tree id, to the
//! fresh-save stage ([`sheets::initial_berry_sheet`]).
//!
//! Each object draws its live tile and the standing frame of its live facing.
//! Positions share [`super::viewport::camera_lag_px`] with the background during a
//! step, and each sprite's bottom edge sits on its tile's bottom edge, centred on
//! the tile (`event_object_movement.c:1459-1464`).
//!
//! Upstream resolves equal-priority overlap with a y-derived subpriority
//! (`SetObjectSubpriorityByElevation`, `event_object_movement.c:7773-7779`)
//! taken from the sprite's bottom edge. [`rendering::SpriteLayer`] has no
//! subpriority, so [`order_sprites`] reproduces it by giving the
//! lower-on-screen object the lower OAM index. The 48x48 truck is twelve
//! horizontal OBJs (`sOamTable_48x48`) that sort as one object by the parent's
//! priority and subpriority and all draw at priority 2.

use std::collections::HashMap;

use assets::pack::{AssetPack, PaletteRef};
use assets::ObjectEvent;
use engine::event_data::EventData;
use engine::overworld::initial_facing_direction;
use rendering::{Bgr555, Palette};
#[cfg(test)]
use rendering::{BitDepth, OamEntry};

use super::avatar::PlayerCharacter;
#[cfg(test)]
use super::{avatar, METATILE_PX};
use super::{fill_palette_bank, OverworldSceneError};

mod sheets;
use sheets::{Facing, ObjectSheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcPaletteTag {
    Npc1,
    Npc2,
    Npc3,
    Npc4,
}

const PLAYER_PALETTE_BANK: u8 = 0;

/// Separate because upstream gives rivals `PALSLOT_NPC_SPECIAL`, not the
/// player's `PALSLOT_PLAYER` (`object_event_graphics_info.h:1-18,1920-2032`).
const OTHER_PROTAGONIST_BANK: u8 = 5;

const TILE_BYTES: usize = 32;

impl NpcPaletteTag {
    const fn bank(self) -> u8 {
        match self {
            Self::Npc1 => 1,
            Self::Npc2 => 2,
            Self::Npc3 => 3,
            Self::Npc4 => 4,
        }
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Npc1 => "npc_1",
            Self::Npc2 => "npc_2",
            Self::Npc3 => "npc_3",
            Self::Npc4 => "npc_4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcSpriteSource {
    /// Reuses the player sheet and palette already loaded by the scene.
    PlayerCharacter,
    People16x32 {
        sprite_path: &'static str,
        palette_bank: u8,
    },
    /// Every other fixed graphic: its own frame geometry and sheet layout.
    Object(ObjectSheet),
}

/// `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`).
const VAR_OBJ_GFX_ID_0: u16 = 0x4010;

/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL` (`event_objects.h:107`).
const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL` (`event_objects.h:112`).
const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

/// The 16x32 nine-frame people sheets: graphics id, pack sprite path, and the
/// generic NPC palette tag (`object_event_graphics_info.h`).
const PEOPLE_SHEETS: [(&str, &str, NpcPaletteTag); 26] = [
    ("OBJ_EVENT_GFX_MOM", "mom", NpcPaletteTag::Npc4),
    ("OBJ_EVENT_GFX_TWIN", "twin", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_FAT_MAN", "fat_man", NpcPaletteTag::Npc1),
    ("OBJ_EVENT_GFX_BOY_2", "boy_2", NpcPaletteTag::Npc1),
    (
        "OBJ_EVENT_GFX_PROF_BIRCH",
        "prof_birch",
        NpcPaletteTag::Npc3,
    ),
    ("OBJ_EVENT_GFX_WOMAN_4", "woman_4", NpcPaletteTag::Npc1),
    (
        "OBJ_EVENT_GFX_NORMAN",
        "gym_leaders/norman",
        NpcPaletteTag::Npc4,
    ),
    (
        "OBJ_EVENT_GFX_SCIENTIST_1",
        "scientist_1",
        NpcPaletteTag::Npc3,
    ),
    (
        "OBJ_EVENT_GFX_MART_EMPLOYEE",
        "mart_employee",
        NpcPaletteTag::Npc1,
    ),
    ("OBJ_EVENT_GFX_GIRL_3", "girl_3", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_MANIAC", "maniac", NpcPaletteTag::Npc4),
    ("OBJ_EVENT_GFX_MAN_3", "man_3", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_WOMAN_2", "woman_2", NpcPaletteTag::Npc3),
    ("OBJ_EVENT_GFX_BOY_1", "boy_1", NpcPaletteTag::Npc3),
    ("OBJ_EVENT_GFX_POKEFAN_M", "pokefan_m", NpcPaletteTag::Npc2),
    (
        "OBJ_EVENT_GFX_BLACK_BELT",
        "black_belt",
        NpcPaletteTag::Npc3,
    ),
    ("OBJ_EVENT_GFX_MAN_5", "man_5", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_SWIMMER_F", "swimmer_f", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_SWIMMER_M", "swimmer_m", NpcPaletteTag::Npc1),
    ("OBJ_EVENT_GFX_FISHERMAN", "fisherman", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_GENTLEMAN", "gentleman", NpcPaletteTag::Npc3),
    ("OBJ_EVENT_GFX_POKEFAN_F", "pokefan_f", NpcPaletteTag::Npc2),
    (
        "OBJ_EVENT_GFX_SCHOOL_KID_M",
        "school_kid_m",
        NpcPaletteTag::Npc1,
    ),
    ("OBJ_EVENT_GFX_TEALA", "teala", NpcPaletteTag::Npc1),
    ("OBJ_EVENT_GFX_WOMAN_5", "woman_5", NpcPaletteTag::Npc2),
    ("OBJ_EVENT_GFX_YOUNGSTER", "youngster", NpcPaletteTag::Npc1),
];

/// Returns how to draw `graphics_id`, or `None` for one that is not drawn.
///
/// The protagonist sheets, the people with a `NpcSpriteSource::People16x32`
/// binding and the [`sheets::object_sheet`] rows are drawn. Berry trees bind
/// per event ([`berry_sheet_for`]), so `OBJ_EVENT_GFX_BERRY_TREE` is undrawn
/// here, as are the `OBJ_EVENT_GFX_VAR_*` slots whose variable is unset and
/// every graphic outside the checkpoint maps.
fn resolve_sprite_source(
    graphics_id: &str,
    player: PlayerCharacter,
    event_data: &EventData,
) -> Option<NpcSpriteSource> {
    use NpcSpriteSource::People16x32;

    match graphics_id {
        "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL" => {
            Some(protagonist_source(PlayerCharacter::Brendan, player))
        }
        "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL" => Some(protagonist_source(PlayerCharacter::May, player)),
        "OBJ_EVENT_GFX_VAR_0" => match event_data.var_get(VAR_OBJ_GFX_ID_0).unwrap_or(0) {
            id if id == RIVAL_BRENDAN_NORMAL_GFX_ID => {
                Some(protagonist_source(PlayerCharacter::Brendan, player))
            }
            id if id == RIVAL_MAY_NORMAL_GFX_ID => {
                Some(protagonist_source(PlayerCharacter::May, player))
            }
            _ => None,
        },
        other => PEOPLE_SHEETS
            .iter()
            .find(|(id, _, _)| *id == other)
            .map(|&(_, sprite_path, tag)| People16x32 {
                sprite_path,
                palette_bank: tag.bank(),
            })
            .or_else(|| sheets::object_sheet(other).map(NpcSpriteSource::Object)),
    }
}

/// Reuses the loaded player assets only when `who` is the current player.
/// Rival graphics use the corresponding protagonist's walking assets upstream
/// (`object_event_graphics_info.h:1920-2032`).
fn protagonist_source(who: PlayerCharacter, player: PlayerCharacter) -> NpcSpriteSource {
    if who == player {
        NpcSpriteSource::PlayerCharacter
    } else {
        NpcSpriteSource::People16x32 {
            sprite_path: who.sprite_path(),
            palette_bank: OTHER_PROTAGONIST_BANK,
        }
    }
}

/// Tile, palette and frame layout of a resolved graphic.
///
/// Only the sheet cells some event of the scene shows are packed, in
/// ascending cell order: a frame's tile offset is the number of packed cells
/// before it ([`Self::frame_tile`]).
#[derive(Debug, Clone, Copy)]
pub(super) struct SpriteBinding {
    base_tile: u16,
    palette_bank: u8,
    /// Frame size in pixels (`.width`, `.height`).
    width: u16,
    height: u16,
    facing: Facing,
    /// Bit `n` is set when sheet cell `n` is packed.
    packed_cells: u16,
}

impl SpriteBinding {
    /// A 16x32 nine-frame people sheet whose block starts at tile 0, every
    /// frame packed.
    const PEOPLE_16X32: Self = Self {
        base_tile: 0,
        palette_bank: PLAYER_PALETTE_BANK,
        width: 16,
        height: 32,
        facing: Facing::PEOPLE,
        packed_cells: 0x1FF,
    };

    const fn people(base_tile: u16, palette_bank: u8) -> Self {
        Self {
            base_tile,
            palette_bank,
            ..Self::PEOPLE_16X32
        }
    }

    const fn frame_tiles(self) -> u16 {
        (self.width / 8) * (self.height / 8)
    }

    /// The first tile of sheet `cell`, which must be packed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a cell index below 16 has fewer than 16 packed cells before it"
    )]
    const fn frame_tile(self, cell: u16) -> u16 {
        let before = (self.packed_cells & ((1 << cell) - 1)).count_ones() as u16;
        self.base_tile + before * self.frame_tiles()
    }
}

#[cfg(test)]
impl SpriteBinding {
    /// Returns the first tile in this sprite's frame block.
    pub(super) const fn base_tile(self) -> u16 {
        self.base_tile
    }

    /// Returns this sprite's palette bank.
    pub(super) const fn palette_bank(self) -> u8 {
        self.palette_bank
    }

    /// Returns how many tiles this binding's packed frames span from
    /// [`Self::base_tile`].
    pub(super) const fn packed_tiles(self) -> u16 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a sheet has at most sixteen cells"
        )]
        let cells = self.packed_cells.count_ones() as u16;
        cells * self.frame_tiles()
    }
}

/// The OBJ tile budget one-dimensional mapping addresses (ten tile-index
/// bits, `rendering::OamEntry`).
pub(super) const MAX_OBJ_TILES: usize = 1024;

/// The key a binding for `event` is stored and found under: its graphics id,
/// except a berry tree, whose stage and species belong to the tree.
fn binding_key(event: &ObjectEvent) -> &'static str {
    if event.graphics_id == BERRY_TREE_GRAPHICS_ID {
        event.trainer_sight_or_berry_tree_id
    } else {
        event.graphics_id
    }
}

/// `OBJ_EVENT_GFX_BERRY_TREE`.
const BERRY_TREE_GRAPHICS_ID: &str = "OBJ_EVENT_GFX_BERRY_TREE";

/// How to draw a berry tree event, or `None` for a tree this port draws no
/// fresh-save stage for.
fn berry_sheet_for(event: &ObjectEvent) -> Option<ObjectSheet> {
    (event.graphics_id == BERRY_TREE_GRAPHICS_ID)
        .then(|| sheets::initial_berry_sheet(event.trainer_sight_or_berry_tree_id))
        .flatten()
}

/// A graphic to pack and the sheet cells the scene's events show.
struct WantedSheet {
    key: &'static str,
    sheet: ObjectSheet,
    cells: u16,
}

/// Resolves distinct object graphics and appends the packed frames some event
/// of the scene shows after the player frames already in `sprite_bytes`.
///
/// # Errors
///
/// Returns an asset, sprite-dimension, pixel-count, or tile-alignment error
/// when a resolved sheet cannot be loaded and packed, and
/// [`OverworldSceneError::SpriteTilesExceedObjSpace`] when the scene's sheets
/// would not fit the OBJ tile space.
pub(super) fn resolve_bindings(
    pack: &AssetPack,
    player: PlayerCharacter,
    object_events: &'static [ObjectEvent],
    sprite_bytes: &mut Vec<u8>,
    event_data: &EventData,
) -> Result<HashMap<&'static str, SpriteBinding>, OverworldSceneError> {
    let mut bindings = HashMap::new();
    let mut wanted: Vec<WantedSheet> = Vec::new();
    for event in object_events {
        let key = binding_key(event);
        let source = berry_sheet_for(event)
            .map(NpcSpriteSource::Object)
            .or_else(|| resolve_sprite_source(event.graphics_id, player, event_data));
        let sheet = match source {
            Some(NpcSpriteSource::PlayerCharacter) => {
                bindings.insert(key, SpriteBinding::people(0, PLAYER_PALETTE_BANK));
                continue;
            }
            Some(NpcSpriteSource::People16x32 {
                sprite_path,
                palette_bank,
            }) => ObjectSheet::people(sprite_path, palette_bank),
            Some(NpcSpriteSource::Object(sheet)) => sheet,
            None => continue,
        };
        let (cell, _) = sheet
            .facing
            .cell_for(initial_facing_direction(event.movement_type));
        match wanted.iter_mut().find(|candidate| candidate.key == key) {
            Some(known) => known.cells |= 1 << cell,
            None => wanted.push(WantedSheet {
                key,
                sheet,
                cells: 1 << cell,
            }),
        }
    }
    for WantedSheet { key, sheet, cells } in wanted {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the tile-space guard below rejects any scene past 1024 tiles"
        )]
        let base_tile = (sprite_bytes.len() / TILE_BYTES) as u16;
        sprite_bytes.extend(sheet.pack(pack, cells)?);
        let tiles = sprite_bytes.len() / TILE_BYTES;
        if tiles > MAX_OBJ_TILES {
            return Err(OverworldSceneError::SpriteTilesExceedObjSpace { tiles });
        }
        bindings.insert(
            key,
            SpriteBinding {
                base_tile,
                palette_bank: sheet.palette_bank,
                width: sheet.width,
                height: sheet.height,
                facing: sheet.facing,
                packed_cells: cells,
            },
        );
    }
    Ok(bindings)
}

/// Loads the player, four generic NPC, and other-protagonist palette banks,
/// then each object palette ([`sheets::OBJECT_PALETTES`]) that some resolved
/// binding draws with.
///
/// # Errors
///
/// Returns [`OverworldSceneError::Pack`] when a required palette is missing.
pub(super) fn build_combined_palette(
    pack: &AssetPack,
    player: PlayerCharacter,
    player_bank0: PaletteRef<'_>,
    bindings: &HashMap<&'static str, SpriteBinding>,
) -> Result<Palette, OverworldSceneError> {
    let mut colors = [Bgr555::default(); Palette::LEN];
    fill_palette_bank(&mut colors, usize::from(PLAYER_PALETTE_BANK), player_bank0);
    for tag in [
        NpcPaletteTag::Npc1,
        NpcPaletteTag::Npc2,
        NpcPaletteTag::Npc3,
        NpcPaletteTag::Npc4,
    ] {
        let raw = pack.sprite_palette(tag.pack_name())?;
        fill_palette_bank(&mut colors, usize::from(tag.bank()), raw);
    }
    let other = pack.sprite_palette(player.other().palette_name())?;
    fill_palette_bank(&mut colors, usize::from(OTHER_PROTAGONIST_BANK), other);
    for (name, bank) in sheets::OBJECT_PALETTES {
        if bindings
            .values()
            .any(|binding| binding.palette_bank == bank)
        {
            let raw = pack.sprite_palette(name)?;
            fill_palette_bank(&mut colors, usize::from(bank), raw);
        }
    }
    Ok(Palette::new(colors))
}

mod oam;
#[cfg(test)]
use engine::overworld::PlayerState;
#[cfg(test)]
use oam::OAM_Y_MODULUS;
pub(super) use oam::{elevated_oam_entries, order_sprites, DepthSprite};
#[cfg(test)]
use oam::{
    oam_entries, object_origin, object_screen_position, order_by_depth, subpriority,
    subpriority_from_bottom, truck_entries,
};

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod checkpoint_pack_tests;
#[cfg(test)]
mod draw_order_tests;
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod live_state_tests;
#[cfg(test)]
mod placement_tests;
#[cfg(test)]
mod test_support;
