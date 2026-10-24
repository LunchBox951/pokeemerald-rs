//! The player's tall-grass field effect (`FLDEFF_TALL_GRASS`).
//!
//! Stepping onto an `MB_TALL_GRASS` tile starts a rustle sprite there
//! (`GroundEffect_StepOnTallGrass`, `event_object_movement.c:7813-7825`, run at
//! the *start* of the step by `DoGroundEffects_OnBeginStep`, `:8098-8116`);
//! being placed on one -- a new game, a continue, a warp landing -- shows it
//! already at rest (`GroundEffect_SpawnOnTallGrass`, `:7800-7811`).
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
//! Not modelled: effects for object events other than the player, the long
//! grass effect, and an effect's carry-over across a map-edge connection
//! (`field_effect_helpers.c:325-330`).

use assets::pack::{AssetPack, PaletteRef};
use assets::{ImageRef, PackError};
use engine::overworld::metatile_behavior::is_tall_grass;
use engine::overworld::{PlayerState, TilePos};
use rendering::{BitDepth, OamEntry, ObjShape};

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

#[derive(Debug, Clone, Copy)]
struct TallGrassEffect {
    tile: TilePos,
    /// `sElevation`: the owner's `previousElevation` when the effect began.
    elevation: u8,
    animation: Animation,
    /// `sObjectMoved`: the owner has left the tile.
    owner_moved: bool,
    /// Whether this frame's callback saw the first command, which adds
    /// [`FIRST_COMMAND_SUBPRIORITY`].
    on_first_command: bool,
}

/// A 16x32 object sprite an effect's depth is compared against
/// (`UpdateGrassFieldEffectSubpriority`, `field_effect_helpers.c:1662-1690`).
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
}

impl TallGrassEffects {
    /// The effects of a scene whose player has just been placed.
    pub(super) fn new() -> Self {
        Self {
            effects: Vec::new(),
            placement_pending: true,
        }
    }

    /// Takes the pending spawn-time pass, which runs once per placement.
    pub(super) fn take_placement_pending(&mut self) -> bool {
        std::mem::take(&mut self.placement_pending)
    }

    /// `GroundEffect_StepOnTallGrass`: a rustling effect on `tile`. A step
    /// settles the placement pass, as the object's first ground-effect pass.
    pub(super) fn spawn_step(&mut self, tile: TilePos, elevation: u8) {
        self.placement_pending = false;
        self.push(tile, elevation, Animation::fresh());
    }

    /// `GroundEffect_SpawnOnTallGrass`: a settled effect on `tile`.
    pub(super) fn spawn_placement(&mut self, tile: TilePos, elevation: u8) {
        self.push(tile, elevation, Animation::settled());
    }

    /// Records that the player's first ground-effect pass found no grass.
    pub(super) fn settle_placement(&mut self) {
        self.placement_pending = false;
    }

    fn push(&mut self, tile: TilePos, elevation: u8, animation: Animation) {
        self.effects.push(TallGrassEffect {
            tile,
            elevation,
            animation,
            owner_moved: false,
            on_first_command: false,
        });
    }

    /// One frame of every effect's callback and animation.
    ///
    /// `behavior_at` answers `MapGridGetMetatileBehaviorAt`; a tile it cannot
    /// resolve is not tall grass. `owner` is the object event that started the
    /// effects: its current and previous tiles decide when it has left.
    pub(super) fn advance(
        &mut self,
        owner: &PlayerState,
        behavior_at: impl Fn(TilePos) -> Option<u8>,
    ) {
        let current = owner.position();
        let previous = owner.previous_position();
        self.effects.retain_mut(|effect| {
            let tall_grass = behavior_at(effect.tile).is_some_and(is_tall_grass);
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
    ) -> Vec<(OamEntry, u16)> {
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
                let entry = OamEntry::new(
                    npc::wrap_oam_x(left),
                    npc::wrap_oam_y(top),
                    first_tile + effect.animation.image() * FRAME_TILES,
                    GENERAL_1_BANK,
                    BitDepth::Bpp4,
                    false,
                    false,
                    OBJ_SHAPE,
                    OBJ_SIZE,
                    OBJ_PRIORITY,
                    true,
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
            self.grass
                .spawn_placement(player.position(), player.previous_elevation());
        }
    }

    /// The player's begin-step ground-effect pass (`DoGroundEffects_OnBeginStep`,
    /// `:8098-8116`): a rustling effect when the step entered tall grass at
    /// `tile`. `elevation` is the player's `previousElevation` after the step
    /// began, which `ObjectEventUpdateElevation` has already updated.
    pub(crate) fn player_step_began(&mut self, tile: TilePos, elevation: u8) {
        if self.sprites.draws_tall_grass() && self.is_tall_grass_at(tile) {
            self.grass.spawn_step(tile, elevation);
        } else {
            self.grass.settle_placement();
        }
    }

    /// Destroys every effect and readies the player's spawn-time pass again,
    /// as a return to the field does (`ResetSpriteData`, `overworld.c:2134`).
    pub(crate) fn reset_field_effects(&mut self) {
        self.grass = TallGrassEffects::new();
    }

    /// Runs every effect's callback and animation for one frame, after the
    /// frame's movement as `AnimateSprites` follows the player's own callback.
    pub(crate) fn advance_tall_grass(&mut self, player: &PlayerState) {
        let mut grass = std::mem::take(&mut self.grass);
        {
            let runtime = self.runtime();
            grass.advance(player, |(x, y)| runtime.metatile_behavior(x, y));
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
}

#[cfg(test)]
mod tests;
