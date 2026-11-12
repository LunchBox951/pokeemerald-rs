//! The tall-grass field effect (`FLDEFF_TALL_GRASS`) of the player and of the
//! scene's object events.
//!
//! Stepping onto an `MB_TALL_GRASS` tile starts a rustle sprite there
//! (`GroundEffect_StepOnTallGrass`, `event_object_movement.c:7813-7825`, run at
//! the *start* of the step by `DoGroundEffects_OnBeginStep`, `:8098-8116`);
//! being placed on one -- a new game, a continue, a warp landing -- shows it
//! already at rest (`GroundEffect_SpawnOnTallGrass`, `:7800-7811`).
//!
//! Every object event goes through the same two passes: each is created with
//! `triggerGroundEffectsOnMove` set (`InitObjectEventStateFromTemplate`,
//! `:1301`), and its sprite callback runs both before anything else
//! (`UpdateObjectEventCurrentMovement`, `:4929-4945`). An effect therefore has an
//! [`Owner`], the object whose tiles decide when it has left. An object gets its
//! spawn-time pass on the first frame after the scheduler admits it
//! ([`OverworldScene::seed_object_tall_grass`]) and a begin-step pass when its
//! tile changes ([`OverworldScene::object_steps_began`]); an object the
//! scheduler removes loses its effects at once, as the effect's callback finds no
//! owner (`UpdateTallGrassFieldEffect`, `field_effect_helpers.c:332-337`).
//!
//! An effect is kept per tile and simulated like the sprite it models:
//! [`TallGrassEffects::advance`] is `UpdateTallGrassFieldEffect`
//! (`field_effect_helpers.c:316-355`) followed by the sprite's animation step,
//! in the order `AnimateSprites` runs them (`sprite.c:308-320`). Composition
//! only reads, so drawing a frame twice never ages an effect.
//!
//! The sheet and palette are optional pack content: a pack without them draws
//! no effect ([`load_resources`]).
//!
//! A player-owned effect survives a map-edge connection crossing, translated into
//! the entered map's coordinates with its animation and moved-away state kept
//! (`field_effect_helpers.c:325-330`): [`OverworldScene::carry_tall_grass_from`].
//! The entered scene keeps no tileset attributes for its connected layouts, so
//! the departed tile's behaviour is read from the departed scene as the effect
//! crosses and kept with it; the tile must lie inside the entered map's backup
//! band (`viewport::within_backup_map_band`) or the effect is dropped. An
//! object-owned effect crosses with its owner, which the live collection keeps
//! alive across the crossing (`ObjectEventCollection::carry_across_connection`):
//! owners are named by [`LiveObjectEvent::id`], which a collection index is not,
//! since the entered collection lists the entered map's templates first and
//! removing a carried entry compacts the indices after it. The carried object's
//! ground-effect pass is kept too, so the entered scene does not run a second
//! spawn-time pass over it. An effect whose owner the crossing did not keep
//! retires as in an ordinary owner removal.
//!
//! Not modelled: the long grass effect, and the camera's saved-view cells that
//! upstream also copies into the entered map's backup grid
//! (`fieldmap.c:473,617`).

use std::collections::BTreeMap;

use assets::pack::{AssetPack, PaletteRef};
use assets::{ImageRef, PackError};
use engine::event_data::EventData;
use engine::overworld::metatile_behavior::is_tall_grass;
use engine::overworld::object_event::{
    object_event_is_visible, LiveObjectEvent, ObjectEventCollection,
};
use engine::overworld::{PlayerState, TilePos};
use rendering::{BitDepth, OamEntry, ObjShape};

use super::obj_vram::SheetSprite;
use super::{
    avatar, npc, pack_4bpp_region, viewport, OverworldScene, OverworldSceneError, METATILE_PX,
};

/// The cells of the sheet: image 0 is the settled grass, 1 to 4 the rustle
/// (`sPicTable_TallGrass`, `field_effect_objects.h:71-77`).
const IMAGE_COUNT: usize = 5;
/// An effect cell is one 16x16 OBJ.
const FRAME_PX: i32 = 16;
/// Tiles per cell.
const FRAME_TILES: u16 = 4;
/// Which cell each animation command shows (`sAnim_TallGrass`, `:79-87`).
const COMMAND_IMAGES: [u16; 5] = [1, 2, 3, 4, 0];
/// Frames each command lasts (`ANIMCMD_FRAME(_, 10)`).
const COMMAND_FRAMES: u8 = 10;
/// The command `SeekSpriteAnim(sprite, 4)` places a settled effect on.
const SETTLED_COMMAND: usize = 4;
/// `gFieldEffectArguments[3]`, the OBJ priority both ground effects pass.
const OBJ_PRIORITY: u8 = 2;
/// The extra subpriority of an effect still on its first command
/// (`UpdateTallGrassFieldEffect`, `:343-347`).
const FIRST_COMMAND_SUBPRIORITY: u16 = 4;
const OBJ_SHAPE: ObjShape = ObjShape::Square;
/// The OBJ size code of a 16x16 sprite.
const OBJ_SIZE: u8 = 1;
const SCREEN_W: i32 = 240;
const SCREEN_H: i32 = 160;

/// The scene palette bank `gSpritePalette_GeneralFieldEffect1` fills; banks 0
/// to 5 hold the player, the four generic NPC palettes, and the other
/// protagonist.
pub(super) const GENERAL_1_BANK: u8 = 6;

const SHEET_LABEL: &str = "field-effect/image/tall_grass";

/// The effect's packed cells and palette.
pub(super) struct Resources<'a> {
    pub(super) tile_bytes: Vec<u8>,
    pub(super) palette: PaletteRef<'a>,
}

/// Loads the tall-grass sheet and palette, or `None` when the pack carries no
/// field-effect sheet (a ROM-imported pack does not yet).
///
/// # Errors
///
/// Returns an error when the sheet is malformed, or its palette is missing.
pub(super) fn load_resources(
    pack: &AssetPack,
) -> Result<Option<Resources<'_>>, OverworldSceneError> {
    let image = match pack.field_effect_image("tall_grass") {
        Ok(image) => image,
        Err(PackError::UnknownAsset(_)) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    Ok(Some(Resources {
        tile_bytes: pack_cells(image)?,
        palette: pack.field_effect_palette("general_1")?,
    }))
}

fn pack_cells(image: ImageRef<'_>) -> Result<Vec<u8>, OverworldSceneError> {
    let cell = usize::try_from(FRAME_PX).unwrap_or_default();
    let expected = (
        u32::try_from(IMAGE_COUNT * cell).unwrap_or(u32::MAX),
        u32::try_from(cell).unwrap_or(u32::MAX),
    );
    if (image.width, image.height) != expected {
        return Err(OverworldSceneError::SpriteSheetWrongDimensions {
            id: SHEET_LABEL,
            expected,
            actual: (image.width, image.height),
        });
    }
    let mut bytes = Vec::new();
    for index in 0..IMAGE_COUNT {
        bytes.extend(pack_4bpp_region(
            SHEET_LABEL,
            image,
            index * cell,
            0,
            cell,
            cell,
        )?);
    }
    Ok(bytes)
}

/// One sprite's animation state (`animCmdIndex`, `animDelayCounter`,
/// `animEnded`), stepped as `AnimateSprite` does (`sprite.c:909-997`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Animation {
    begun: bool,
    command: usize,
    delay: u8,
    ended: bool,
}

impl Animation {
    /// A freshly created sprite, whose first `AnimateSprite` runs `BeginAnim`.
    const fn fresh() -> Self {
        Self {
            begun: false,
            command: 0,
            delay: 0,
            ended: false,
        }
    }

    /// `SeekSpriteAnim(sprite, 4)`: the settled cell with a full delay, which
    /// `SeekSpriteAnim`'s own `animDelayCounter++` makes one frame longer.
    const fn settled() -> Self {
        Self {
            begun: true,
            command: SETTLED_COMMAND,
            delay: COMMAND_FRAMES,
            ended: false,
        }
    }

    fn update(&mut self) {
        if !self.begun {
            self.begun = true;
            self.delay = COMMAND_FRAMES - 1;
        } else if self.delay > 0 {
            self.delay -= 1;
        } else if self.command + 1 < COMMAND_IMAGES.len() {
            self.command += 1;
            self.delay = COMMAND_FRAMES - 1;
        } else {
            self.ended = true;
        }
    }

    const fn image(self) -> u16 {
        COMMAND_IMAGES[self.command]
    }
}

/// The object event whose ground-effect pass started an effect
/// (`sLocalId`/`sMapNum`/`sMapGroup`, `field_effect_helpers.c:305-307`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Player,
    /// The scene's object event with this [`LiveObjectEvent::id`]. The id, not the entry's index
    /// in the live collection, names the owner: removing a carried entry compacts the indices
    /// after it, and a crossing seeds the entered map's templates ahead of the carried entries.
    Object(u32),
}

/// An owner's current and previous tiles (`currentCoords`, `previousCoords`).
type OwnerTiles = (TilePos, TilePos);

#[derive(Debug, Clone, Copy)]
struct TallGrassEffect {
    owner: Owner,
    tile: TilePos,
    /// `sElevation`: the owner's `previousElevation` when the effect began.
    elevation: u8,
    animation: Animation,
    /// `sObjectMoved`: the owner has left the tile.
    owner_moved: bool,
    /// Whether this frame's callback saw the first command, which adds
    /// [`FIRST_COMMAND_SUBPRIORITY`].
    on_first_command: bool,
    /// The behaviour of a tile outside the active grid, which only a carried
    /// effect can stand on: the connection strip's cell, read when it crossed.
    strip_behavior: Option<u8>,
}

/// An object sprite an effect's depth is compared against
/// (`UpdateGrassFieldEffectSubpriority`, `field_effect_helpers.c:1662-1690`),
/// with 16x32 bounds from its parent's top-left whatever its graphic's size.
#[derive(Debug, Clone, Copy)]
pub(super) struct DepthObject {
    /// Screen x of the object's left edge.
    pub(super) left: i32,
    /// Screen y of the object's top edge.
    pub(super) top: i32,
    pub(super) subpriority: u16,
}

/// The tall-grass effects of one scene.
#[derive(Debug, Default)]
pub(super) struct TallGrassEffects {
    effects: Vec<TallGrassEffect>,
    /// Whether the player's object event has not yet had its spawn-time
    /// ground effects (`triggerGroundEffectsOnMove`, `:1301`).
    placement_pending: bool,
    /// Per [`LiveObjectEvent::id`]: the tile its object stood on when its last
    /// ground-effect pass ran. An object with no sprite, or whose
    /// `triggerGroundEffectsOnMove` is still set, has no record.
    object_passes: BTreeMap<u32, TilePos>,
    /// Whether the first frame's object admission has not yet run (map load's
    /// `TrySpawnObjectEvents`, `overworld.c:2163`, ahead of the first callback).
    admission_pending: bool,
}

impl TallGrassEffects {
    /// The effects of a scene whose player has just been placed.
    pub(super) fn new() -> Self {
        Self {
            effects: Vec::new(),
            placement_pending: true,
            object_passes: BTreeMap::new(),
            admission_pending: true,
        }
    }

    /// Takes the pending first-frame admission, which runs once per scene.
    pub(super) fn take_admission_pending(&mut self) -> bool {
        std::mem::take(&mut self.admission_pending)
    }

    /// Takes the pending spawn-time pass, which runs once per placement.
    pub(super) fn take_placement_pending(&mut self) -> bool {
        std::mem::take(&mut self.placement_pending)
    }

    /// `GroundEffect_StepOnTallGrass`: a rustling effect on `tile`. A step
    /// settles the placement pass, as the object's first ground-effect pass.
    pub(super) fn spawn_step(&mut self, owner: Owner, tile: TilePos, elevation: u8) {
        if owner == Owner::Player {
            self.placement_pending = false;
        }
        self.push(owner, tile, elevation, Animation::fresh());
    }

    /// `GroundEffect_SpawnOnTallGrass`: a settled effect on `tile`.
    pub(super) fn spawn_placement(&mut self, owner: Owner, tile: TilePos, elevation: u8) {
        self.push(owner, tile, elevation, Animation::settled());
    }

    /// Records that the player's first ground-effect pass found no grass.
    pub(super) fn settle_placement(&mut self) {
        self.placement_pending = false;
    }

    /// Whether the object `id` has had its spawn-time pass since it was
    /// last admitted.
    fn object_pass_done(&self, id: u32) -> bool {
        self.object_pass_tile(id).is_some()
    }

    /// The tile the object `id` stood on at its last pass.
    fn object_pass_tile(&self, id: u32) -> Option<TilePos> {
        self.object_passes.get(&id).copied()
    }

    fn record_object_pass(&mut self, id: u32, tile: TilePos) {
        self.object_passes.insert(id, tile);
    }

    /// The object `id` has no sprite: its next admission runs a spawn pass.
    fn forget_object(&mut self, id: u32) {
        self.object_passes.remove(&id);
    }

    /// Forgets the passes of objects no longer in the live collection, whose ids are never
    /// reused.
    fn retain_object_passes(&mut self, mut is_live: impl FnMut(u32) -> bool) {
        self.object_passes.retain(|&id, _| is_live(id));
    }

    fn push(&mut self, owner: Owner, tile: TilePos, elevation: u8, animation: Animation) {
        self.effects.push(TallGrassEffect {
            owner,
            tile,
            elevation,
            animation,
            owner_moved: false,
            on_first_command: false,
            strip_behavior: None,
        });
    }

    /// One frame of every effect's callback and animation.
    ///
    /// `behavior_at` answers `MapGridGetMetatileBehaviorAt`; a tile it cannot
    /// resolve is not tall grass. `owner_tiles` answers an effect's owner's
    /// current and previous tiles, which decide when it has left; an owner it
    /// cannot find has been removed, and its effects go with it
    /// (`TryGetObjectEventIdByLocalIdAndMap`, `field_effect_helpers.c:332-337`).
    pub(super) fn advance(
        &mut self,
        owner_tiles: impl Fn(Owner) -> Option<OwnerTiles>,
        behavior_at: impl Fn(TilePos) -> Option<u8>,
    ) {
        self.effects.retain_mut(|effect| {
            let Some((current, previous)) = owner_tiles(effect.owner) else {
                return false;
            };
            let tall_grass = behavior_at(effect.tile)
                .or(effect.strip_behavior)
                .is_some_and(is_tall_grass);
            if !tall_grass || (effect.owner_moved && effect.animation.ended) {
                return false;
            }
            if current != effect.tile && previous != effect.tile {
                effect.owner_moved = true;
            }
            effect.on_first_command = effect.animation.command == 0;
            effect.animation.update();
            true
        });
    }

    /// The effects' OBJs with their subpriorities, for depth ordering against
    /// `objects`. `first_tile` is the combined tileset's first effect tile.
    pub(super) fn oam_entries(
        &self,
        player: &PlayerState,
        first_tile: u16,
        objects: &[DepthObject],
    ) -> Vec<(SheetSprite, u16)> {
        let player_tile = player.position();
        let (lag_x, lag_y) = viewport::camera_lag_px(player);
        self.effects
            .iter()
            .filter_map(|effect| {
                // The effect covers the tile under the player's lower half:
                // one metatile below an object event's 16x32 top.
                let left = i32::from(avatar::PLAYER_OBJ_X)
                    + (effect.tile.0 - player_tile.0) * METATILE_PX
                    + lag_x;
                let top = i32::from(avatar::PLAYER_OBJ_Y)
                    + METATILE_PX
                    + (effect.tile.1 - player_tile.1) * METATILE_PX
                    + lag_y;
                if !on_screen(left, top) {
                    return None;
                }
                let sheet_tile = first_tile + effect.animation.image() * FRAME_TILES;
                let entry = SheetSprite::new(
                    OamEntry::new(
                        npc::wrap_oam_x(left),
                        npc::wrap_oam_y(top),
                        sheet_tile,
                        GENERAL_1_BANK,
                        BitDepth::Bpp4,
                        false,
                        false,
                        OBJ_SHAPE,
                        OBJ_SIZE,
                        OBJ_PRIORITY,
                        true,
                    ),
                    sheet_tile,
                );
                Some((entry, effect.subpriority(left, top, objects)))
            })
            .collect()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.effects.len()
    }
}

/// `UpdateObjectEventSpriteInvisibility`'s on-screen test for a 16x16 sprite
/// (`event_object_movement.c:8578-8584`): the sprite's origin is its top-left
/// corner here, and upstream's second test point sits a quarter-size inside it.
const fn on_screen(left: i32, top: i32) -> bool {
    const INSET: i32 = FRAME_PX / 4;
    const MARGIN: i32 = 16;
    left < SCREEN_W + MARGIN
        && left + INSET >= -MARGIN
        && top < SCREEN_H + MARGIN
        && top + INSET >= -MARGIN
}

impl TallGrassEffect {
    /// `SetObjectSubpriorityByElevation` with the first-command bias, then
    /// `UpdateGrassFieldEffectSubpriority`'s overlap rule: the first object
    /// whose body overlaps this effect and sits at or behind it pulls the
    /// effect two behind itself.
    fn subpriority(&self, left: i32, top: i32, objects: &[DepthObject]) -> u16 {
        let bias = if self.on_first_command {
            FIRST_COMMAND_SUBPRIORITY
        } else {
            0
        };
        let own = npc::subpriority_from_bottom(top + FRAME_PX, self.elevation, bias);
        let half = FRAME_PX / 2;
        let overlapping = objects.iter().find(|object| {
            let object_centre = object.left + half;
            left < object_centre
                && left + FRAME_PX > object_centre
                && object.top < top + FRAME_PX
                && object.top + FRAME_PX > top
                && own <= object.subpriority
        });
        overlapping.map_or(own, |object| object.subpriority + 2)
    }
}

/// Whether the object has a sprite: the scheduler has admitted it and its hide
/// flag is still clear. A script that sets the flag removes the object
/// (`RemoveObjectEventByLocalIdAndMap`, `event_object_movement.c:1389-1397`),
/// and the scheduler does not retire an admitted entry for it, so the flag is
/// read here as drawing reads it.
fn has_sprite(live: &LiveObjectEvent, event_data: &EventData) -> bool {
    live.is_active() && object_event_is_visible(live.template(), event_data)
}

impl OverworldScene {
    /// The player's spawn-time ground-effect pass (`DoGroundEffects_OnSpawn`,
    /// `event_object_movement.c:8080-8096`): a settled effect when the player
    /// was placed on tall grass. Runs once per placement, ahead of the first
    /// frame's movement; a later call does nothing.
    pub(crate) fn seed_tall_grass_placement(&mut self, player: &PlayerState) {
        if !self.grass.take_placement_pending() || !self.sprites.draws_tall_grass() {
            return;
        }
        if self.is_tall_grass_at(player.position()) {
            self.grass.spawn_placement(
                Owner::Player,
                player.position(),
                player.previous_elevation(),
            );
        }
    }

    /// The player's begin-step ground-effect pass (`DoGroundEffects_OnBeginStep`,
    /// `:8098-8116`): a rustling effect when the step entered tall grass at
    /// `tile`. `elevation` is the player's `previousElevation` after the step
    /// began, which `ObjectEventUpdateElevation` has already updated.
    pub(crate) fn player_step_began(&mut self, tile: TilePos, elevation: u8) {
        if self.sprites.draws_tall_grass() && self.is_tall_grass_at(tile) {
            self.grass.spawn_step(Owner::Player, tile, elevation);
        } else {
            self.grass.settle_placement();
        }
    }

    /// Takes the pending first-frame admission: true once per scene, on the
    /// frame whose scheduler pass admits the objects already in view.
    pub(crate) fn take_object_admission_pending(&mut self) -> bool {
        self.grass.take_admission_pending()
    }

    /// The object events' spawn-time ground-effect pass
    /// (`DoGroundEffects_OnSpawn`, `event_object_movement.c:8078-8096`): a
    /// settled effect for each object the scheduler has admitted since its last
    /// pass, standing on tall grass. The object's elevation is first settled on
    /// its tile (`UpdateObjectEventElevationAndPriority`, `:8089`), and the
    /// pass runs whether or not object events are frozen. An object without a
    /// sprite (hidden, outside the view, removed) is forgotten, so its next
    /// admission runs the pass again (`triggerGroundEffectsOnMove`, `:1301`).
    pub(crate) fn seed_object_tall_grass(
        &mut self,
        objects: &ObjectEventCollection,
        event_data: &EventData,
    ) {
        let draws = self.sprites.draws_tall_grass();
        self.grass
            .retain_object_passes(|id| objects.by_id(id).is_some());
        for live in objects.iter() {
            let id = live.id();
            if !has_sprite(live, event_data) {
                self.grass.forget_object(id);
                continue;
            }
            if self.grass.object_pass_done(id) {
                continue;
            }
            let state = live.state();
            let tile = state.position();
            self.grass.record_object_pass(id, tile);
            if draws && self.is_tall_grass_at(tile) {
                let elevation = state.previous_elevation();
                self.grass
                    .spawn_placement(Owner::Object(id), tile, elevation);
            }
        }
    }

    /// The object events' begin-step ground-effect pass
    /// (`DoGroundEffects_OnBeginStep`, `:8098-8116`): a rustling effect for each
    /// object whose step has just begun onto tall grass. The scheduler commits
    /// a step's destination when the step begins, so an object whose tile
    /// differs from its last pass's has begun one. Runs after the frame's
    /// callbacks, as upstream's pass follows the movement callback.
    pub(crate) fn object_steps_began(
        &mut self,
        objects: &ObjectEventCollection,
        event_data: &EventData,
    ) {
        let draws = self.sprites.draws_tall_grass();
        for live in objects.iter() {
            let id = live.id();
            if !has_sprite(live, event_data) {
                self.grass.forget_object(id);
                continue;
            }
            let state = live.state();
            let tile = state.position();
            if self
                .grass
                .object_pass_tile(id)
                .is_none_or(|last| last == tile)
            {
                continue;
            }
            self.grass.record_object_pass(id, tile);
            if draws && self.is_tall_grass_at(tile) {
                let elevation = state.previous_elevation();
                self.grass.spawn_step(Owner::Object(id), tile, elevation);
            }
        }
    }

    /// Carries the live effects of `departed` into this just-loaded scene after
    /// a seamless connection crossing (`UpdateTallGrassFieldEffect`,
    /// `field_effect_helpers.c:325-330`). `shift` translates the departed map's
    /// tile coordinates into this map's. `objects` is the object collection the
    /// crossing leaves: an object-owned effect follows its owner, named by
    /// [`LiveObjectEvent::id`], and is dropped with an owner the crossing did
    /// not keep. An effect whose translated tile falls outside this map's backup
    /// band is dropped. Animation, moved-away state, elevation, and depth state
    /// are kept; so is each kept object's ground-effect pass, translated, so the
    /// entered scene does not give a live object a second spawn-time effect. The
    /// scene's own placement and admission passes are left to run.
    pub(crate) fn carry_tall_grass_from(
        &mut self,
        departed: &Self,
        shift: (i32, i32),
        objects: &ObjectEventCollection,
    ) {
        if !self.sprites.draws_tall_grass() {
            return;
        }
        let runtime = departed.runtime();
        let (width, height) = (self.layout.width, self.layout.height);
        let carried: Vec<TallGrassEffect> = departed
            .grass
            .effects
            .iter()
            .filter(|effect| match effect.owner {
                Owner::Player => true,
                Owner::Object(id) => objects.by_id(id).is_some_and(LiveObjectEvent::is_active),
            })
            .filter_map(|effect| {
                let tile = (effect.tile.0 + shift.0, effect.tile.1 + shift.1);
                if !viewport::within_backup_map_band(
                    width,
                    height,
                    i64::from(tile.0),
                    i64::from(tile.1),
                ) {
                    return None;
                }
                let behavior = runtime
                    .metatile_behavior(effect.tile.0, effect.tile.1)
                    .or(effect.strip_behavior);
                Some(TallGrassEffect {
                    tile,
                    strip_behavior: behavior,
                    ..*effect
                })
            })
            .collect();
        self.grass.effects.extend(carried);
        for live in objects.iter().filter(|live| live.is_active()) {
            if let Some(&(x, y)) = departed.grass.object_passes.get(&live.id()) {
                self.grass
                    .record_object_pass(live.id(), (x + shift.0, y + shift.1));
            }
        }
    }

    /// Destroys every effect and readies every object event's spawn-time pass
    /// again, the player's included, as a return to the field does
    /// (`ResetSpriteData`, `overworld.c:2134`, then `ResetObjectEventFldEffData`,
    /// `event_object_movement.c:1795-1806`).
    pub(crate) fn reset_field_effects(&mut self) {
        self.grass = TallGrassEffects::new();
    }

    /// Runs every effect's callback and animation for one frame, after the
    /// frame's movement as `AnimateSprites` follows the player's own callback.
    /// `objects` are the owners of the object events' effects.
    pub(crate) fn advance_tall_grass(
        &mut self,
        player: &PlayerState,
        objects: &ObjectEventCollection,
        event_data: &EventData,
    ) {
        let mut grass = std::mem::take(&mut self.grass);
        {
            let runtime = self.runtime();
            grass.advance(
                |owner| match owner {
                    Owner::Player => Some((player.position(), player.previous_position())),
                    Owner::Object(id) => objects
                        .by_id(id)
                        .filter(|live| has_sprite(live, event_data))
                        .map(|live| (live.state().position(), live.state().previous_position())),
                },
                |(x, y)| runtime.metatile_behavior(x, y),
            );
        }
        self.grass = grass;
    }

    fn is_tall_grass_at(&self, (x, y): TilePos) -> bool {
        self.runtime()
            .metatile_behavior(x, y)
            .is_some_and(is_tall_grass)
    }

    #[cfg(test)]
    pub(crate) fn tall_grass_effect_count(&self) -> usize {
        self.grass.len()
    }

    /// The first tile of the effect's cells in the combined tileset.
    #[cfg(test)]
    pub(crate) fn tall_grass_first_tile(&self) -> Option<u16> {
        self.sprites.grass_base_tile()
    }

    /// The OBJ VRAM one composed frame stages for these entries: the tile
    /// count it occupies and the entries dropped for exceeding the limit.
    #[cfg(test)]
    pub(crate) fn staged_obj_vram(
        &self,
        player: &PlayerState,
        event_data: &EventData,
    ) -> super::obj_vram::StagedObjVram {
        super::obj_vram::stage(
            &self.sprite_entries(player, event_data),
            self.sprites.tiles(),
        )
    }

    /// The combined sheet every [`SheetSprite`] indexes.
    #[cfg(test)]
    pub(crate) const fn sheet_tiles(&self) -> &rendering::Tileset {
        self.sprites.tiles()
    }
}

#[cfg(test)]
mod tests;
