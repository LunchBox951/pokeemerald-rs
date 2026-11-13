//! Combined overworld sprite resources and per-frame OAM entries.
//!
//! [`rendering::SpriteLayer`] accepts one tileset and palette. Player walking
//! frames occupy the first tile block; each distinct object graphic's frames
//! follow ([`npc::resolve_bindings`]), then the player's running frames, then
//! the tall-grass effect's cells, with palette banks assigned by
//! [`npc::build_combined_palette`]. This combined sheet is the scene's
//! ROM-side image store; composing stages only the drawn frames into OBJ VRAM
//! (`super::obj_vram`), so its size is not bound by the ten-bit OBJ tile field.

use std::collections::HashMap;

use assets::pack::AssetPack;
use assets::{LayoutGrid, ObjectEvent};
use engine::event_data::EventData;
use engine::overworld::object_event::{ObjectEventCollection, ObjectEventState};
use engine::overworld::PlayerState;
use rendering::{BitDepth, Palette, Tileset};

use super::avatar::{self, PlayerCharacter};
use super::grass::{self, DepthObject, TallGrassEffects};
use super::npc::{self, SpriteBinding};
use super::obj_vram::SheetSprite;
use super::OverworldSceneError;

#[derive(Debug)]
pub(super) struct SceneSprites {
    tiles: Tileset,
    palette: Palette,
    /// First tile of the player's running sheet, packed after every NPC sheet
    /// so NPC blocks keep their offsets from the walking block.
    run_base_tile: u16,
    /// First tile of the tall-grass effect's cells, packed after the running
    /// sheet; `None` when the pack carries no field-effect sheet.
    grass_base_tile: Option<u16>,
    /// Template-seeded state for callers that own no live collection.
    seed_objects: ObjectEventCollection,
    /// The layout's cell elevations, which settle each object's elevation at
    /// its live tile.
    cell_elevations: CellElevations,
    bindings: HashMap<&'static str, SpriteBinding>,
}

use engine::overworld::collision::ELEVATION_TRANSITION;

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

    /// The elevation of the cell at `tile`; `None` off the grid.
    fn at(&self, tile: (i32, i32)) -> Option<u8> {
        u16::try_from(tile.0)
            .ok()
            .zip(u16::try_from(tile.1).ok())
            .filter(|&(x, y)| x < self.width && y < self.height)
            .and_then(|(x, y)| {
                self.elevations
                    .get(usize::from(y) * usize::from(self.width) + usize::from(x))
            })
            .copied()
    }

    /// `ObjectEventUpdateElevation` (`event_object_movement.c:7759-7771`) for
    /// one object: its stored elevations take its current and previous cells'.
    /// An object off the grid keeps what it has, and an off-grid previous cell
    /// reads as a transition.
    fn update(&self, state: &mut ObjectEventState) {
        if let Some(current) = self.at(state.position()) {
            let previous = self
                .at(state.previous_position())
                .unwrap_or(ELEVATION_TRANSITION);
            state.update_elevation(current, previous);
        }
    }

    /// Updates every object's elevations from the cells they stand on.
    fn update_all(&self, objects: &mut ObjectEventCollection) {
        for live in objects.iter_mut() {
            self.update(live.state_mut());
        }
    }
}

impl SceneSprites {
    pub(super) fn from_pack(
        pack: &AssetPack,
        player: PlayerCharacter,
        events: &assets::MapEvents,
        carried: &[&'static ObjectEvent],
        grid: &LayoutGrid<'_>,
        event_data: &EventData,
    ) -> Result<Self, OverworldSceneError> {
        let sprite_image = pack.sprite(player.sprite_path())?;
        let palette_ref = pack.sprite_palette(player.palette_name())?;
        let mut bytes = avatar::pack_people_sheet_frames("sprite/*/walking", sprite_image)?;
        let bindings = npc::resolve_bindings(
            pack,
            player,
            events.object_events.iter().chain(carried.iter().copied()),
            &mut bytes,
            event_data,
        )?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a scene contains only a small number of NPC sheets"
        )]
        let run_base_tile = (bytes.len() / BitDepth::Bpp4.tile_byte_len()) as u16;
        bytes.extend(avatar::pack_people_sheet_frames(
            "sprite/*/running",
            pack.sprite(player.run_sprite_path())?,
        )?);
        let grass = grass::load_resources(pack)?;
        let cell_elevations = CellElevations::from_grid(grid);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a scene contains only a small number of sprite sheets"
        )]
        let grass_base_tile = grass
            .as_ref()
            .map(|_| (bytes.len() / BitDepth::Bpp4.tile_byte_len()) as u16);
        if let Some(grass) = &grass {
            bytes.extend_from_slice(&grass.tile_bytes);
        }
        Ok(Self {
            tiles: Tileset::decode(BitDepth::Bpp4, &bytes)?,
            run_base_tile,
            grass_base_tile,
            palette: npc::build_combined_palette(
                pack,
                player,
                palette_ref,
                &bindings,
                grass.as_ref().map(|grass| grass.palette),
            )?,
            seed_objects: {
                let mut seeds = ObjectEventCollection::from_templates(events.object_events);
                cell_elevations.update_all(&mut seeds);
                seeds
            },
            cell_elevations,
            bindings,
        })
    }

    #[cfg(test)]
    pub(super) const fn grass_base_tile(&self) -> Option<u16> {
        self.grass_base_tile
    }

    /// Whether the pack supplied the tall-grass effect's sheet and palette.
    pub(super) const fn draws_tall_grass(&self) -> bool {
        self.grass_base_tile.is_some()
    }

    /// Settles every live object's elevations on the cells it stands on
    /// (`ObjectEventUpdateElevation`). Rendering only reads the result, so a
    /// caller runs this after creating or moving objects and before composing.
    pub(super) fn update_object_elevations(&self, objects: &mut ObjectEventCollection) {
        self.cell_elevations.update_all(objects);
    }

    /// Settles one object's elevations on the cells it stands on, for a caller that
    /// runs a finish-step update between frames ([`Self::update_object_elevations`]).
    pub(super) fn update_object_elevation(&self, state: &mut ObjectEventState) {
        self.cell_elevations.update(state);
    }

    /// [`Self::live_entries`] over the template-seeded state, admitted as map load would admit
    /// it for `player` under `event_data`.
    #[must_use]
    pub(super) fn entries(
        &self,
        player: &PlayerState,
        event_data: &EventData,
        grass: &TallGrassEffects,
    ) -> Vec<SheetSprite> {
        let mut objects = self.seed_objects.clone();
        let _ = objects.admit_on_map_load(player.position(), event_data);
        self.live_entries(&objects, player, event_data, grass)
    }

    /// Lists the player first, then NPCs in `objects` order, then the tall-grass
    /// effects, and orders by OBJ priority and y-derived subpriority
    /// ([`npc::order_sprites`]); the player leads a tie in every sort key.
    /// `objects` supplies tile, facing, and the retained elevation that sets
    /// each sprite's priority, settled by
    /// [`Self::update_object_elevations`]; the scene's templates keep the graphics
    /// bindings. Each effect carries its own subpriority, set against the
    /// object sprites it overlaps.
    #[must_use]
    pub(super) fn live_entries(
        &self,
        objects: &ObjectEventCollection,
        player: &PlayerState,
        event_data: &EventData,
        grass: &TallGrassEffects,
    ) -> Vec<SheetSprite> {
        let elevations: Vec<u8> = objects
            .iter()
            .map(|live| live.state().previous_elevation())
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
        if let Some(first_tile) = self.grass_base_tile {
            let depth_objects: Vec<DepthObject> = sprites
                .iter()
                .map(|sprite| DepthObject {
                    left: i32::from(sprite.left()),
                    top: signed_screen_y(sprite.top()),
                    subpriority: sprite.parent_subpriority(),
                })
                .collect();
            sprites.extend(
                grass
                    .oam_entries(player, first_tile, &depth_objects)
                    .into_iter()
                    .map(|(entry, subpriority)| {
                        npc::DepthSprite::with_subpriority(entry, subpriority)
                    }),
            );
        }
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

/// An OAM `y` as a screen row, reading the wrapped rows just above the screen
/// as negative.
fn signed_screen_y(y: u8) -> i32 {
    const SCREEN_BOTTOM: i32 = 192;
    let y = i32::from(y);
    if y > SCREEN_BOTTOM {
        y - 256
    } else {
        y
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

    fn npc_state(x: i16, elevation: u8) -> ObjectEventState {
        ObjectEventState::from_template(&assets::ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y: 0,
            elevation,
            movement_type: assets::MovementType::FaceRight,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: None,
            flag: "0",
        })
    }

    fn settle(cells: &CellElevations, state: &mut ObjectEventState) -> (u8, u8) {
        cells.update(state);
        (state.elevation(), state.previous_elevation())
    }

    /// `ObjectEventUpdateElevation`: a cell's elevation replaces the
    /// template's, except the transition (0) and multi-level (15) cells.
    #[test]
    fn a_spawned_object_takes_its_cells_elevation_unless_transition_or_multi_level() {
        let (layout, bytes) = grid_with_elevation_row([3, ELEVATION_TRANSITION, 15]);
        let cells = CellElevations::from_grid(&layout.grid(&bytes).unwrap());
        assert_eq!(settle(&cells, &mut npc_state(0, 4)), (3, 3));
        assert_eq!(settle(&cells, &mut npc_state(1, 3)), (0, 3));
        assert_eq!(settle(&cells, &mut npc_state(1, 4)), (0, 4));
        assert_eq!(settle(&cells, &mut npc_state(2, 4)), (4, 4));
        assert_eq!(settle(&cells, &mut npc_state(9, 4)), (4, 4), "off the grid");
        assert_eq!(
            settle(&cells, &mut npc_state(-1, 4)),
            (4, 4),
            "left of the grid"
        );
    }

    /// `ObjectEventUpdateElevation` keeps `previousElevation` from the last
    /// ordinary cell when the object steps onto a transition cell.
    #[test]
    fn walking_onto_a_transition_keeps_the_settled_elevation() {
        let (layout, bytes) = grid_with_elevation_row([4, ELEVATION_TRANSITION, 4]);
        let cells = CellElevations::from_grid(&layout.grid(&bytes).unwrap());
        let mut state = npc_state(0, 3);
        assert_eq!(settle(&cells, &mut state), (4, 4));
        state.walk(engine::overworld::Direction::East);
        assert_eq!(settle(&cells, &mut state), (0, 4));
    }

    /// `ObjectEventUpdateElevation` returns early when the previous cell is
    /// multi-level, leaving the elevation unchanged.
    #[test]
    fn leaving_a_multi_level_cell_keeps_the_elevation() {
        let (layout, bytes) = grid_with_elevation_row([4, 15, 6]);
        let cells = CellElevations::from_grid(&layout.grid(&bytes).unwrap());
        let mut state = npc_state(1, 4);
        assert_eq!(settle(&cells, &mut state), (4, 4));
        state.walk(engine::overworld::Direction::East);
        assert_eq!(settle(&cells, &mut state), (4, 4));
    }
}
