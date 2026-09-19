//! Combined overworld sprite resources and per-frame OAM entries.
//!
//! [`rendering::SpriteLayer`] accepts one tileset and palette. Player frames
//! occupy the first tile block; distinct NPC sheets follow, with palette banks
//! assigned by [`npc::build_combined_palette`].

use std::collections::HashMap;

use assets::pack::AssetPack;
use assets::ObjectEvent;
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
    object_events: &'static [ObjectEvent],
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
        Ok(Self {
            tiles: Tileset::decode(BitDepth::Bpp4, &bytes)?,
            palette: npc::build_combined_palette(pack, player, palette_ref)?,
            object_events: events.object_events,
            bindings,
        })
    }

    /// Returns the player first because lower OAM indices win same-priority
    /// ties, making the player draw over an overlapping NPC.
    #[must_use]
    pub(super) fn entries(&self, player: &PlayerState, event_data: &EventData) -> Vec<OamEntry> {
        let mut entries = vec![avatar::player_entry(player)];
        entries.extend(npc::oam_entries(
            self.object_events,
            &self.bindings,
            player,
            event_data,
        ));
        entries
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
