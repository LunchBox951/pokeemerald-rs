//! Object-event OAM composition: size-aware anchoring on the tile, the 48x48
//! truck's twelve pieces, and the parent-based depth order that stands in for
//! upstream's subpriority sort.
//!
//! [`elevated_oam_entries`] turns each visible, bound object event into a
//! [`DepthSprite`], and [`order_sprites`] orders the player and those sprites
//! as `SortSprites` hands them to the PPU (`sprite.c:413-419`). Bindings,
//! sheet packing and palettes stay in the parent module.

use std::collections::HashMap;

use engine::event_data::EventData;
use engine::overworld::object_event::{tile_is_in_view, ObjectEventCollection};
use engine::overworld::{object_event_is_visible, PlayerState};
use rendering::{BitDepth, OamEntry};

use super::super::avatar;
use super::super::METATILE_PX;
use super::{binding_key, SpriteBinding};

const OAM_X_MODULUS: i32 = 512;
pub(super) const OAM_Y_MODULUS: i32 = 256;

#[expect(
    clippy::cast_possible_truncation,
    reason = "rem_euclid bounds the result to the nine-bit OAM x range"
)]
fn wrap_oam_x(x: i32) -> u16 {
    x.rem_euclid(OAM_X_MODULUS) as u16
}

#[expect(
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
pub(super) fn object_origin(
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
pub(super) fn object_screen_position(
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
pub(super) fn subpriority_from_bottom(bottom: i32, elevation: u8) -> u16 {
    let bottom_plus_bias = u16::try_from((bottom + 8).rem_euclid(OAM_Y_MODULUS)).unwrap_or(0);
    let base = ELEVATION_TO_SUBPRIORITY
        .get(usize::from(elevation))
        .copied()
        .unwrap_or(ELEVATION_TO_SUBPRIORITY[0]);
    base + (16 - (bottom_plus_bias >> 4)) * 2 + 1
}

/// [`subpriority_from_bottom`] for a 16x32 `entry`.
#[cfg(test)]
pub(super) fn subpriority(entry: OamEntry, elevation: u8) -> u16 {
    subpriority_from_bottom(i32::from(entry.y()) + 32, elevation)
}

/// One object event's OBJs and the parent keys they sort by. A multi-OBJ
/// object such as the truck sorts as one: upstream sorts the parent sprite
/// and emits its subsprites in place (`sprite.c:361-368`, `:1742-1750`).
#[derive(Debug, Clone)]
pub(in crate::overworld) struct DepthSprite {
    pub(super) entries: Vec<OamEntry>,
    /// The parent's OBJ priority, from its elevation, not the subsprites'.
    pub(super) parent_priority: u8,
    /// The parent's wrapped top `y`, the final sort key.
    parent_y: u8,
    /// The parent's subpriority.
    pub(super) subpriority: u16,
}

impl DepthSprite {
    /// A 16x32 sprite whose single OBJ is `entry` at `elevation`.
    pub(in crate::overworld) fn single(entry: OamEntry, elevation: u8) -> Self {
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
pub(in crate::overworld) fn order_sprites(sprites: &[DepthSprite]) -> Vec<OamEntry> {
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
    objects: &ObjectEventCollection,
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<OamEntry> {
    let elevations: Vec<u8> = objects
        .iter()
        .map(|live| live.state().elevation())
        .collect();
    elevated_oam_entries(objects, &elevations, bindings, player, event_data)
        .into_iter()
        .flat_map(|sprite| sprite.entries)
        .collect()
}

/// `sOamTable_48x48` (`object_event_subsprites.h:281-386`): per 8-pixel row,
/// a 32x8 OBJ then a 16x8 OBJ whose tiles follow it in the row-major frame,
/// each at priority 2 whatever the elevation.
const TRUCK_PIECE_PRIORITY: u8 = 2;

pub(super) fn truck_entries(binding: SpriteBinding, (left, top): (i32, i32)) -> Vec<OamEntry> {
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
/// each object's effective elevation, aligned with `objects`' iteration order:
/// it selects the OBJ priority and the subpriority base.
///
/// Position and facing come from each entry's live state; the template keeps
/// the graphics id and hide flag. The standing frame follows the live facing
/// (`GetFaceDirectionAnimNum`, `event_object_movement.c`), and the spawn window
/// is evaluated at the live tile.
#[must_use]
pub(in crate::overworld) fn elevated_oam_entries(
    objects: &ObjectEventCollection,
    elevations: &[u8],
    bindings: &HashMap<&'static str, SpriteBinding>,
    player: &PlayerState,
    event_data: &EventData,
) -> Vec<DepthSprite> {
    let player_pos = player.position();
    let camera_lag = super::super::viewport::camera_lag_px(player);
    objects
        .iter()
        .enumerate()
        .filter(|(_, live)| {
            object_event_is_visible(live.template(), event_data)
                && tile_is_in_view(live.state().position(), player_pos)
        })
        .filter_map(|(index, live)| {
            let state = live.state();
            let binding = *bindings.get(binding_key(live.template()))?;
            let elevation = elevations.get(index).copied().unwrap_or(state.elevation());
            let (cell, h_flip) = binding.facing.cell_for(state.facing());
            let tile_index = binding.frame_tile(cell);
            let origin = object_origin(
                state.position(),
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
