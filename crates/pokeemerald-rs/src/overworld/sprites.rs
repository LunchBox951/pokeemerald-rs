//! Combined overworld sprite resources and per-frame OAM entries.
//!
//! [`rendering::SpriteLayer`] accepts one tileset and palette. Player walking
//! frames occupy the first tile block; each distinct object graphic's frames
//! follow ([`npc::resolve_bindings`]), then the player's running frames, with
//! palette banks assigned by [`npc::build_combined_palette`].

use std::collections::HashMap;

use assets::pack::AssetPack;
use assets::{LayoutGrid, ObjectEvent};
use engine::event_data::EventData;
use engine::overworld::PlayerState;
use rendering::{BitDepth, OamEntry, Palette, Tileset};

use super::avatar::{self, PlayerCharacter};
use super::npc::{self, SpriteBinding};
use super::OverworldSceneError;

#[derive(Debug)]
pub(super) struct SceneSprites {
    tiles: Tileset,
    palette: Palette,
    /// First tile of the player's running sheet, packed after every NPC sheet
    /// so NPC blocks keep their offsets from the walking block.
    run_base_tile: u16,
    object_events: &'static [ObjectEvent],
    /// Each object's effective elevation, aligned with `object_events`.
    elevations: Vec<u8>,
    bindings: HashMap<&'static str, SpriteBinding>,
}

/// `ELEVATION_TRANSITION`: a cell any elevation may enter, which carries no
/// elevation of its own.
const ELEVATION_TRANSITION: u8 = 0;

/// `ELEVATION_MULTI_LEVEL`: a cell that leaves an object's elevation alone.
const ELEVATION_MULTI_LEVEL: u8 = 15;

/// The elevation a spawned object settles at: its template's, replaced by its
/// own cell's elevation unless that cell is a transition or multi-level one
/// (`ObjectEventUpdateElevation`, `event_object_movement.c:7759-7771`, which
/// the spawn-time ground-effect update reaches).
fn effective_elevation(event: &ObjectEvent, grid: &LayoutGrid<'_>) -> u8 {
    let cell = u16::try_from(event.x)
        .ok()
        .zip(u16::try_from(event.y).ok())
        .and_then(|(x, y)| grid.cell_at(x, y));
    match cell {
        Some(cell) if !matches!(cell.elevation, ELEVATION_TRANSITION | ELEVATION_MULTI_LEVEL) => {
            cell.elevation
        }
        _ => event.elevation,
    }
}

impl SceneSprites {
    pub(super) fn from_pack(
        pack: &AssetPack,
        player: PlayerCharacter,
        events: &assets::MapEvents,
        grid: &LayoutGrid<'_>,
        event_data: &EventData,
    ) -> Result<Self, OverworldSceneError> {
        let sprite_image = pack.sprite(player.sprite_path())?;
        let palette_ref = pack.sprite_palette(player.palette_name())?;
        let mut bytes = avatar::pack_people_sheet_frames("sprite/*/walking", sprite_image)?;
        let bindings =
            npc::resolve_bindings(pack, player, events.object_events, &mut bytes, event_data)?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a scene contains only a small number of NPC sheets"
        )]
        let run_base_tile = (bytes.len() / BitDepth::Bpp4.tile_byte_len()) as u16;
        bytes.extend(avatar::pack_people_sheet_frames(
            "sprite/*/running",
            pack.sprite(player.run_sprite_path())?,
        )?);
        let tiles = bytes.len() / BitDepth::Bpp4.tile_byte_len();
        if tiles > npc::MAX_OBJ_TILES {
            return Err(OverworldSceneError::SpriteTilesExceedObjSpace { tiles });
        }
        Ok(Self {
            tiles: Tileset::decode(BitDepth::Bpp4, &bytes)?,
            run_base_tile,
            palette: npc::build_combined_palette(pack, player, palette_ref, &bindings)?,
            object_events: events.object_events,
            elevations: events
                .object_events
                .iter()
                .map(|event| effective_elevation(event, grid))
                .collect(),
            bindings,
        })
    }

    /// Lists the player first, then NPCs in template order, then orders by OBJ
    /// priority and y-derived subpriority
    /// ([`npc::order_sprites`]); the player leads a tie in every sort key.
    #[must_use]
    pub(super) fn entries(&self, player: &PlayerState, event_data: &EventData) -> Vec<OamEntry> {
        let mut sprites = vec![npc::DepthSprite::single(
            avatar::player_entry(player, self.run_base_tile),
            player.previous_elevation(),
        )];
        sprites.extend(npc::elevated_oam_entries(
            self.object_events,
            &self.elevations,
            &self.bindings,
            player,
            event_data,
        ));
        npc::order_sprites(&sprites)
    }

    pub(super) const fn tiles(&self) -> &Tileset {
        &self.tiles
    }

    pub(super) const fn palette(&self) -> &Palette {
        &self.palette
    }

    #[cfg(test)]
    pub(super) const fn bindings(&self) -> &HashMap<&'static str, SpriteBinding> {
        &self.bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{MapLayout, MetatileCell, MovementType};

    fn grid_with_elevation_row(elevations: [u8; 3]) -> (MapLayout, Vec<u8>) {
        let layout = MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 3,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = elevations
            .into_iter()
            .flat_map(|elevation| {
                MetatileCell {
                    metatile_id: 0,
                    collision: 0,
                    elevation,
                }
                .pack()
                .to_le_bytes()
            })
            .collect();
        (layout, bytes)
    }

    fn event_at(x: i16, elevation: u8) -> ObjectEvent {
        ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_PICHU_DOLL",
            x,
            y: 0,
            elevation,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        }
    }

    /// `ObjectEventUpdateElevation`: a cell's elevation replaces the
    /// template's, except the transition (0) and multi-level (15) cells.
    #[test]
    fn a_spawned_object_takes_its_cells_elevation_unless_transition_or_multi_level() {
        let (layout, bytes) =
            grid_with_elevation_row([3, ELEVATION_TRANSITION, ELEVATION_MULTI_LEVEL]);
        let grid = layout.grid(&bytes).unwrap();
        assert_eq!(effective_elevation(&event_at(0, 4), &grid), 3);
        assert_eq!(effective_elevation(&event_at(1, 3), &grid), 3);
        assert_eq!(effective_elevation(&event_at(1, 4), &grid), 4);
        assert_eq!(effective_elevation(&event_at(2, 4), &grid), 4);
        assert_eq!(
            effective_elevation(&event_at(9, 4), &grid),
            4,
            "off the grid"
        );
    }
}
