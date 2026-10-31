//! Combined overworld sprite resources and per-frame OAM entries.
//!
//! [`rendering::SpriteLayer`] accepts one tileset and palette. Player walking
//! frames occupy the first tile block; distinct NPC sheets follow, then the
//! player's running frames, then the tall-grass effect's cells, with palette
//! banks assigned by [`npc::build_combined_palette`].

use std::collections::HashMap;

use assets::pack::AssetPack;
use engine::event_data::EventData;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::PlayerState;
use rendering::{BitDepth, OamEntry, Palette, Tileset};

use super::avatar::{self, PlayerCharacter};
use super::grass::{self, DepthObject, TallGrassEffects};
use super::npc::{self, SpriteBinding};
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
    bindings: HashMap<&'static str, SpriteBinding>,
}

impl SceneSprites {
    pub(super) fn from_pack(
        pack: &AssetPack,
        player: PlayerCharacter,
        events: &assets::MapEvents,
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
        let grass = grass::load_resources(pack)?;
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
                grass.as_ref().map(|grass| grass.palette),
            )?,
            seed_objects: ObjectEventCollection::from_templates(events.object_events),
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

    /// [`Self::live_entries`] over the template-seeded state.
    #[must_use]
    pub(super) fn entries(
        &self,
        player: &PlayerState,
        event_data: &EventData,
        grass: &TallGrassEffects,
    ) -> Vec<OamEntry> {
        self.live_entries(&self.seed_objects, player, event_data, grass)
    }

    /// Lists the player first, then NPCs in `objects` order, then the tall-grass
    /// effects, and orders by OBJ priority and y-derived subpriority
    /// ([`npc::order_by_subpriority`]); the player leads a tie in every sort
    /// key. `objects` supplies tile, facing, and elevation; the scene's
    /// templates keep the graphics bindings.
    #[must_use]
    pub(super) fn live_entries(
        &self,
        objects: &ObjectEventCollection,
        player: &PlayerState,
        event_data: &EventData,
        grass: &TallGrassEffects,
    ) -> Vec<OamEntry> {
        let mut elevated = vec![(
            avatar::player_entry(player, self.run_base_tile),
            player.previous_elevation(),
        )];
        elevated.extend(npc::elevated_oam_entries(
            objects,
            &self.bindings,
            player,
            event_data,
        ));
        let mut entries: Vec<(OamEntry, u16)> = elevated
            .iter()
            .map(|&(entry, elevation)| (entry, npc::subpriority(entry, elevation)))
            .collect();
        if let Some(first_tile) = self.grass_base_tile {
            let depth_objects: Vec<DepthObject> = entries
                .iter()
                .map(|&(entry, subpriority)| DepthObject {
                    left: i32::from(entry.x()),
                    top: signed_screen_y(entry.y()),
                    subpriority,
                })
                .collect();
            entries.extend(grass.oam_entries(player, first_tile, &depth_objects));
        }
        npc::order_by_subpriority(&entries)
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
