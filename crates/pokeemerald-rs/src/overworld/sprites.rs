//! Combined overworld sprite resources and per-frame OAM entries.
//!
//! [`rendering::SpriteLayer`] accepts one tileset and palette. Player walking
//! frames occupy the first tile block; each distinct object graphic's frames
//! follow ([`npc::resolve_bindings`]), then the player's running frames, with
//! palette banks assigned by [`npc::build_combined_palette`].

use std::collections::HashMap;

use assets::pack::AssetPack;
use assets::LayoutGrid;
use engine::event_data::EventData;
use engine::overworld::object_event::ObjectEventCollection;
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
    /// Template-seeded state for callers that own no live collection.
    seed_objects: ObjectEventCollection,
    /// The layout's cell elevations, which settle each object's elevation at
    /// its live tile.
    cell_elevations: CellElevations,
    bindings: HashMap<&'static str, SpriteBinding>,
}

/// `ELEVATION_TRANSITION`: a cell any elevation may enter, which carries no
/// elevation of its own.
const ELEVATION_TRANSITION: u8 = 0;

/// `ELEVATION_MULTI_LEVEL`: a cell that leaves an object's elevation alone.
const ELEVATION_MULTI_LEVEL: u8 = 15;

/// Each cell's elevation, row-major, copied from the layout grid so the
/// sprites need not borrow it.
#[derive(Debug)]
struct CellElevations {
    width: u16,
    height: u16,
    elevations: Vec<u8>,
}

impl CellElevations {
    fn from_grid(grid: &LayoutGrid<'_>) -> Self {
        let (width, height) = (grid.width(), grid.height());
        let elevations = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                grid.cell_at(x, y)
                    .map_or(ELEVATION_TRANSITION, |c| c.elevation)
            })
            .collect();
        Self {
            width,
            height,
            elevations,
        }
    }

    /// The elevation an object on `tile` settles at: its own, replaced by the
    /// cell's unless that cell is a transition or multi-level one, or off the
    /// grid (`ObjectEventUpdateElevation`,
    /// `event_object_movement.c:7759-7771`, which the spawn-time ground-effect
    /// update reaches).
    fn settled(&self, tile: (i32, i32), elevation: u8) -> u8 {
        let cell = u16::try_from(tile.0)
            .ok()
            .zip(u16::try_from(tile.1).ok())
            .filter(|&(x, y)| x < self.width && y < self.height)
            .and_then(|(x, y)| {
                self.elevations
                    .get(usize::from(y) * usize::from(self.width) + usize::from(x))
            })
            .copied();
        match cell {
            Some(cell) if !matches!(cell, ELEVATION_TRANSITION | ELEVATION_MULTI_LEVEL) => cell,
            _ => elevation,
        }
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
            seed_objects: ObjectEventCollection::from_templates(events.object_events),
            cell_elevations: CellElevations::from_grid(grid),
            bindings,
        })
    }

    /// [`Self::live_entries`] over the template-seeded state.
    #[must_use]
    pub(super) fn entries(&self, player: &PlayerState, event_data: &EventData) -> Vec<OamEntry> {
        self.live_entries(&self.seed_objects, player, event_data)
    }

    /// Lists the player first, then NPCs in `objects` order, then orders by OBJ
    /// priority and y-derived subpriority
    /// ([`npc::order_sprites`]); the player leads a tie in every sort key.
    /// `objects` supplies tile, facing, and elevation, the last settled by the
    /// layout cell it stands on; the scene's templates keep the graphics
    /// bindings.
    #[must_use]
    pub(super) fn live_entries(
        &self,
        objects: &ObjectEventCollection,
        player: &PlayerState,
        event_data: &EventData,
    ) -> Vec<OamEntry> {
        let elevations: Vec<u8> = objects
            .iter()
            .map(|live| {
                let state = live.state();
                self.cell_elevations
                    .settled(state.position(), state.elevation())
            })
            .collect();
        let mut sprites = vec![npc::DepthSprite::single(
            avatar::player_entry(player, self.run_base_tile),
            player.previous_elevation(),
        )];
        sprites.extend(npc::elevated_oam_entries(
            objects,
            &elevations,
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
    use assets::{MapLayout, MetatileCell};

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

    /// `ObjectEventUpdateElevation`: a cell's elevation replaces the
    /// template's, except the transition (0) and multi-level (15) cells.
    #[test]
    fn a_spawned_object_takes_its_cells_elevation_unless_transition_or_multi_level() {
        let (layout, bytes) =
            grid_with_elevation_row([3, ELEVATION_TRANSITION, ELEVATION_MULTI_LEVEL]);
        let cells = CellElevations::from_grid(&layout.grid(&bytes).unwrap());
        assert_eq!(cells.settled((0, 0), 4), 3);
        assert_eq!(cells.settled((1, 0), 3), 3);
        assert_eq!(cells.settled((1, 0), 4), 4);
        assert_eq!(cells.settled((2, 0), 4), 4);
        assert_eq!(cells.settled((9, 0), 4), 4, "off the grid");
        assert_eq!(cells.settled((-1, 0), 4), 4, "left of the grid");
    }
}
