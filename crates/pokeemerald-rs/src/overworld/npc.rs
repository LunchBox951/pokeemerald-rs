//! Renders stationary object events backed by supported object graphics.
//!
//! [`resolve_sprite_source`] is the complete graphics-id binding table: the
//! protagonist sheets, the 16x32 people sheets, and the [`sheets`] rows for
//! the other 16x16, 16x32, 32x32 and 48x48 graphics the checkpoint maps place.
//! Hidden events are filtered through [`object_event_is_visible`], the same
//! visibility used for interaction. A visible unsupported event remains
//! interaction-ready but produces no [`OamEntry`]. `OBJ_EVENT_GFX_VAR_0` binds
//! only when its event variable contains one of the two Route 103 rival ids;
//! decoration slots remain unsupported. Code that clears a `FLAG_DECORATION_n`
//! hide flag must first write that slot's `VAR_OBJ_GFX_ID_0 + n`, or the
//! persistent rival id resolves the decoration to a rival sprite.
//! `OBJ_EVENT_GFX_BERRY_TREE` binds per event, by its berry tree id, to the
//! fresh-save stage ([`sheets::initial_berry_sheet`]).
//!
//! Object events retain their initial movement-facing frame. Their positions
//! share [`super::viewport::camera_lag_px`] with the background during a step,
//! and each sprite's bottom edge sits on its tile's bottom edge, centred on the
//! tile (`event_object_movement.c:1459-1464`).
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
use engine::overworld::{
    initial_facing_direction, object_event_is_in_view, object_event_is_visible, PlayerState,
};
use rendering::{Bgr555, BitDepth, OamEntry, Palette};

use super::avatar::{self, PlayerCharacter};
use super::{fill_palette_bank, OverworldSceneError, METATILE_PX};

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

const OAM_X_MODULUS: i32 = 512;
const OAM_Y_MODULUS: i32 = 256;

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "rem_euclid bounds the result to the nine-bit OAM x range"
)]
fn wrap_oam_x(x: i32) -> u16 {
    x.rem_euclid(OAM_X_MODULUS) as u16
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "rem_euclid bounds the result to the eight-bit OAM y range"
)]
fn wrap_oam_y(y: i32) -> u8 {
    y.rem_euclid(OAM_Y_MODULUS) as u8
}

/// [`avatar::FRAME_H`], the 16x32 player OBJ's height, in pixels.
const PLAYER_FRAME_H_PX: i32 = 32;

/// An object's unwrapped screen top-left for a frame of `size` pixels.
///
/// The sprite is centred on its tile and its bottom edge sits on the tile's
/// bottom edge: upstream's centre is `(16x + 8, 16y + 16 - height / 2)`
/// (`event_object_movement.c:1459-1464`), so the 16x32 player OBJ at
/// [`avatar::PLAYER_OBJ_X`]/[`avatar::PLAYER_OBJ_Y`] fixes every size's
/// colocated origin.
fn object_origin(
    object_pos: (i32, i32),
    player_pos: (i32, i32),
    camera_lag: (i32, i32),
    (width, height): (u16, u16),
) -> (i32, i32) {
    let dx = (object_pos.0 - player_pos.0) * METATILE_PX + camera_lag.0;
    let dy = (object_pos.1 - player_pos.1) * METATILE_PX + camera_lag.1;
    let x = i32::from(avatar::PLAYER_OBJ_X) + dx + (METATILE_PX - i32::from(width)) / 2;
    let y = i32::from(avatar::PLAYER_OBJ_Y) + dy + (PLAYER_FRAME_H_PX - i32::from(height));
    (x, y)
}

/// [`object_origin`] of a 16x32 OBJ, wrapped to OAM coordinates.
#[cfg(test)]
fn object_screen_position(
    object_pos: (i32, i32),
    player_pos: (i32, i32),
    camera_lag: (i32, i32),
) -> (u16, u8) {
    let (x, y) = object_origin(object_pos, player_pos, camera_lag, (16, 32));
    (wrap_oam_x(x), wrap_oam_y(y))
}

/// `sElevationToSubpriority` (`event_object_movement.c:7691-7693`). Elevations
/// sharing an OBJ priority still differ here, so it is part of the draw order.
const ELEVATION_TO_SUBPRIORITY: [u16; 16] = [
    115, 115, 83, 115, 83, 115, 83, 115, 83, 115, 83, 115, 83, 0, 0, 115,
];

/// Upstream's subpriority for an OBJ at `elevation` whose bottom edge is at
/// screen row `bottom`:
/// `sElevationToSubpriority[elevation] + (16 - (((bottom + 8) & 0xFF) >> 4)) * 2 + 1`
/// (`SetObjectSubpriorityByElevation`, `:7739-7745`, called with subpriority 1).
fn subpriority_from_bottom(bottom: i32, elevation: u8) -> u16 {
    let bottom_plus_bias = u16::try_from((bottom + 8).rem_euclid(OAM_Y_MODULUS)).unwrap_or(0);
    let base = ELEVATION_TO_SUBPRIORITY
        .get(usize::from(elevation))
        .copied()
        .unwrap_or(ELEVATION_TO_SUBPRIORITY[0]);
    base + (16 - (bottom_plus_bias >> 4)) * 2 + 1
}

/// [`subpriority_from_bottom`] for a 16x32 `entry`.
#[cfg(test)]
fn subpriority(entry: OamEntry, elevation: u8) -> u16 {
    subpriority_from_bottom(i32::from(entry.y()) + 32, elevation)
}

/// One object event's OBJs and the parent keys they sort by. A multi-OBJ
/// object such as the truck sorts as one: upstream sorts the parent sprite
/// and emits its subsprites in place (`sprite.c:361-368`, `:1742-1750`).
#[derive(Debug, Clone)]
pub(super) struct DepthSprite {
    entries: Vec<OamEntry>,
    /// The parent's OBJ priority, from its elevation, not the subsprites'.
    parent_priority: u8,
    /// The parent's wrapped top `y`, the final sort key.
    parent_y: u8,
    /// The parent's subpriority.
    subpriority: u16,
}

impl DepthSprite {
    /// A 16x32 sprite whose single OBJ is `entry` at `elevation`.
    pub(super) fn single(entry: OamEntry, elevation: u8) -> Self {
        Self {
            parent_priority: entry.priority(),
            parent_y: entry.y(),
            subpriority: subpriority_from_bottom(i32::from(entry.y()) + 32, elevation),
            entries: vec![entry],
        }
    }
}

/// Orders sprites as upstream emits OAM: ascending parent OBJ priority, then
/// subpriority, then lower on-screen `y` (`SortSprites`, `sprite.c:413-419`);
/// ties keep input order and each sprite's OBJs stay together.
pub(super) fn order_sprites(sprites: &[DepthSprite]) -> Vec<OamEntry> {
    let mut ordered: Vec<&DepthSprite> = sprites.iter().collect();
    ordered.sort_by_key(|sprite| {
        (
            sprite.parent_priority,
            sprite.subpriority,
            std::cmp::Reverse(sprite.parent_y),
        )
    });
    ordered
        .into_iter()
        .flat_map(|sprite| sprite.entries.iter().copied())
        .collect()
}

/// [`order_sprites`] for 16x32 OBJs given as `(entry, elevation)`.
#[cfg(test)]
pub(super) fn order_by_depth(entries: &[(OamEntry, u8)]) -> Vec<OamEntry> {
    let sprites: Vec<DepthSprite> = entries
        .iter()
        .map(|&(entry, elevation)| DepthSprite::single(entry, elevation))
        .collect();
    order_sprites(&sprites)
}

/// Builds OAM entries for visible events with resolved sprite bindings.
#[cfg(test)]
#[must_use]
pub(super) fn oam_entries(
    object_events: &'static [ObjectEvent],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<OamEntry> {
    let elevations: Vec<u8> = object_events.iter().map(|event| event.elevation).collect();
    elevated_oam_entries(object_events, &elevations, bindings, player, event_data)
        .into_iter()
        .flat_map(|sprite| sprite.entries)
        .collect()
}

/// `sOamTable_48x48` (`object_event_subsprites.h:281-386`): per 8-pixel row,
/// a 32x8 OBJ then a 16x8 OBJ whose tiles follow it in the row-major frame,
/// each at priority 2 whatever the elevation.
const TRUCK_PIECE_PRIORITY: u8 = 2;

fn truck_entries(binding: SpriteBinding, (left, top): (i32, i32)) -> Vec<OamEntry> {
    const ROWS: i32 = 6;
    const ROW_TILES: u16 = 6;
    let piece = |x: i32, y: i32, tile: u16, size: u8| {
        OamEntry::new(
            wrap_oam_x(x),
            wrap_oam_y(y),
            tile,
            binding.palette_bank,
            BitDepth::Bpp4,
            false,
            false,
            rendering::ObjShape::Horizontal,
            size,
            TRUCK_PIECE_PRIORITY,
            true,
        )
    };
    (0..ROWS)
        .flat_map(|row| {
            let y = top + row * 8;
            let tile = binding.base_tile + u16::try_from(row).unwrap_or(0) * ROW_TILES;
            [piece(left, y, tile, 1), piece(left + 32, y, tile + 4, 0)]
        })
        .collect()
}

/// The OAM shape and size index of a single-OBJ frame of `size` pixels.
const fn single_obj_shape(width: u16, height: u16) -> (rendering::ObjShape, u8) {
    match (width, height) {
        (16, 16) => (rendering::ObjShape::Square, 1),
        (32, 32) => (rendering::ObjShape::Square, 2),
        _ => (avatar::PLAYER_OBJ_SHAPE, avatar::PLAYER_OBJ_SIZE),
    }
}

/// [`oam_entries`] with each object's parent depth keys. `elevations` holds
/// each event's effective elevation, aligned with `object_events`: it selects
/// the OBJ priority and the subpriority base.
#[must_use]
pub(super) fn elevated_oam_entries(
    object_events: &'static [ObjectEvent],
    elevations: &[u8],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<DepthSprite> {
    let player_pos = player.position();
    let camera_lag = super::viewport::camera_lag_px(player);
    object_events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            object_event_is_visible(event, event_data) && object_event_is_in_view(event, player_pos)
        })
        .filter_map(|(index, event)| {
            let binding = *bindings.get(binding_key(event))?;
            let elevation = elevations.get(index).copied().unwrap_or(event.elevation);
            let facing = initial_facing_direction(event.movement_type);
            let (cell, h_flip) = binding.facing.cell_for(facing);
            let tile_index = binding.frame_tile(cell);
            let origin = object_origin(
                (i32::from(event.x), i32::from(event.y)),
                player_pos,
                camera_lag,
                (binding.width, binding.height),
            );
            let parent_priority = avatar::priority_for_elevation(elevation);
            let entries = if binding.width == 48 {
                truck_entries(binding, origin)
            } else {
                let (shape, size) = single_obj_shape(binding.width, binding.height);
                vec![OamEntry::new(
                    wrap_oam_x(origin.0),
                    wrap_oam_y(origin.1),
                    tile_index,
                    binding.palette_bank,
                    BitDepth::Bpp4,
                    h_flip,
                    false,
                    shape,
                    size,
                    parent_priority,
                    true,
                )]
            };
            Some(DepthSprite {
                entries,
                parent_priority,
                parent_y: wrap_oam_y(origin.1),
                subpriority: subpriority_from_bottom(
                    origin.1 + i32::from(binding.height),
                    elevation,
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod checkpoint_pack_tests;
#[cfg(test)]
mod draw_order_tests;
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod placement_tests;
#[cfg(test)]
mod test_support;
