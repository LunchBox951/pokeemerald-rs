//! The overworld scene's OBJ layer: one combined sprite
//! [`Tileset`]/[`Palette`] for the player *and* every NPC object event the
//! room draws, plus the per-frame [`OamEntry`] list built over them.
//!
//! Split out of [`super`] (`one module = one concept` `(oop-boundaries)`,
//! mirroring how [`crate::flow::overworld_phase`] was lifted out of
//! `crate::flow`): [`super::OverworldScene`] owns the *room* -- layout grid,
//! border, BG tilesets, camera -- while this module owns everything about
//! its sprites. A mechanical move of the code that already lived in
//! `OverworldScene::from_pack`/`compose`, with no behavioural change.
//!
//! The two halves it composes stay where they were: [`super::avatar`] owns
//! the player's own frame layout/selection and the shared 9-frame "people"
//! sheet packer, and [`super::npc`] owns which `graphics_id`s resolve to a
//! sprite, the four generic NPC palette banks, and NPC OAM placement.
//!
//! # Why one combined tileset and palette
//!
//! [`SpriteLayer`](rendering::SpriteLayer) draws every sprite it is given
//! from a single shared tileset and a single 256-color palette (that
//! module's own docs), so the scene decodes one of each up front rather
//! than per object event: the player's 9 walking frames occupy base tile
//! `0` and palette bank `0`, each distinct NPC sheet the room references
//! is appended after them at its own [`avatar::FRAME_BLOCK_TILES`] stride,
//! the four generic `npc_1..4` banks sit at banks 1..=4, and the *other*
//! protagonist's own palette -- what a rival object event draws from --
//! sits at `npc::OTHER_PROTAGONIST_BANK`.

use std::collections::HashMap;

use assets::pack::AssetPack;
use assets::ObjectEvent;
use engine::event_data::EventData;
use engine::overworld::PlayerState;
use rendering::{BitDepth, OamEntry, Palette, Tileset};

use super::avatar::{self, PlayerCharacter};
use super::npc::{self, SpriteBinding};
use super::OverworldSceneError;

/// A room's decoded OBJ layer (module docs), built once by
/// [`from_pack`](Self::from_pack) and queried once per frame by
/// [`entries`](Self::entries).
#[derive(Debug)]
pub(super) struct SceneSprites {
    /// The player's own 9 walking frames at tile index `0`, followed by one
    /// 9-frame block per distinct [`npc::resolve_bindings`]-recognized NPC
    /// sheet this room's object events reference (module docs).
    tiles: Tileset,
    /// Bank 0: the player's own palette. Banks 1..=4: the four generic
    /// `npc_1..4` palettes. Bank `npc::OTHER_PROTAGONIST_BANK`: the other
    /// protagonist's own, for a rival object event. All of the non-player
    /// banks are always loaded (see [`npc::build_combined_palette`]).
    palette: Palette,
    /// This room's object events, straight from the generated
    /// [`assets::MapEventsTable`] (`'static`) -- the NPC-rendering source;
    /// never mutated after [`from_pack`](Self::from_pack).
    object_events: &'static [ObjectEvent],
    /// Each recognized `graphics_id`'s resolved binding into
    /// [`tiles`](Self::tiles)/[`palette`](Self::palette) (see
    /// [`npc::resolve_bindings`]).
    bindings: HashMap<&'static str, SpriteBinding>,
}

impl SceneSprites {
    /// Decode `player`'s walking sheet plus every NPC sheet
    /// `events.object_events` references out of an already-loaded `pack`.
    ///
    /// `event_data` is this room's own current flags/vars, threaded down to
    /// [`npc::resolve_bindings`]'s own `"OBJ_EVENT_GFX_VAR_0"` exception
    /// (issue #248, that module's own docs) -- needed only for Route 103's
    /// rival object event; every other bundled map's binding is unaffected
    /// by it.
    ///
    /// # Errors
    ///
    /// [`OverworldSceneError::Pack`]/[`OverworldSceneError::Asset`] if a
    /// needed pack entry is missing;
    /// [`OverworldSceneError::Render`],
    /// [`OverworldSceneError::ImagePixelCountMismatch`],
    /// [`OverworldSceneError::ImageNotTileAligned`], or
    /// [`OverworldSceneError::SpriteSheetWrongDimensions`] if a present
    /// entry's bytes don't fit the shape [`avatar::pack_people_sheet_frames`]
    /// expects (unreachable against a real pack).
    pub(super) fn from_pack(
        pack: &AssetPack,
        player: PlayerCharacter,
        events: &'static assets::MapEvents,
        event_data: &EventData,
    ) -> Result<Self, OverworldSceneError> {
        let sprite_image = pack.sprite(player.sprite_path())?;
        let palette_ref = pack.sprite_palette(player.palette_name())?;
        // The player's own 9 frames seed the combined tileset at base tile
        // 0; every recognized NPC sheet `resolve_bindings` finds among
        // `events.object_events` is appended after it (module docs), so
        // this needs the raw, appendable bytes rather than a decoded
        // single-sheet `Tileset`.
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

    /// This frame's OAM entries: the player at index 0, then one entry per
    /// currently-visible object event [`npc`] recognizes a sprite for
    /// (`event_data` gates visibility --
    /// [`engine::overworld::object_event_is_visible`]).
    ///
    /// The player's index-0 position is load-bearing: lower array index
    /// wins a same-priority tie (`rendering::sprite`'s own docs), so the
    /// player always draws over an NPC sharing its pixels -- see [`npc`]'s
    /// module docs for the documented subpriority delta that follows from
    /// it.
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

    /// The combined sprite tileset [`entries`](Self::entries)' tile indices
    /// address.
    pub(super) const fn tiles(&self) -> &Tileset {
        &self.tiles
    }

    /// The combined 256-color sprite palette [`entries`](Self::entries)'
    /// palette banks address.
    pub(super) const fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The resolved `graphics_id` -> [`SpriteBinding`] map -- exposed for
    /// the scene's own real-pack tests, which assert the OAM entries built
    /// over the real [`assets::MapEventsTable`] data against the frame
    /// blocks those bindings address.
    #[cfg(test)]
    pub(super) const fn bindings(&self) -> &HashMap<&'static str, SpriteBinding> {
        &self.bindings
    }
}
