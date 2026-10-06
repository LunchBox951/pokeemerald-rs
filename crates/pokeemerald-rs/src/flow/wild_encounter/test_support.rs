//! Shared fixtures for the wild-encounter tests: the Route 101 phase builders, the walk
//! and input drivers, and the fixed-seed constants the sibling `*_tests` modules draw on.

use crate::flow::overworld_phase::OverworldPhase;
use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::PlayerState;
use platform::{ButtonState, Buttons};

/// Route 101, the map whose real wild table and real object events the phase
/// below resolves against.
pub(super) const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

/// `WALK_FRAMES_PER_TILE` — how many `step` calls one tile takes.
pub(super) const FRAMES_PER_STEP: usize = engine::overworld::WALK_FRAMES_PER_TILE as usize;

/// Hold East through `steps` whole tile crossings and then give the last one
/// its landing call.
///
/// The crossing itself is [`FRAMES_PER_STEP`] calls; the call after it is
/// upstream's `T_TILE_CENTER` CB1, where the completed step's coordinate
/// event, door-shaped warp and encounter roll actually run
/// (`OverworldPhase::step`'s "Frame shape" docs, issue #1039). That last
/// call is deliberately neutral, so it cannot also begin a further crossing
/// the way a still-held direction would.
pub(super) fn walk_east_and_land(phase: &mut OverworldPhase, steps: usize) {
    for _ in 0..(steps * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
}

/// A held (not newly-pressed) direction, the input a walk is driven with --
/// two updates, so the button reads as held rather than freshly pressed,
/// matching a real multi-frame hold.
pub(super) fn held(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state.update(button);
    state
}

/// A max-IV, fixed-personality player mon — deterministic stats, so these
/// scenarios don't depend on the shared stream for the *player's* side too.
pub(super) fn player_mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), SpeciesId(species), level, ivs, 0, moves)
        .expect("player mon: species/moves must be in the dex")
}

/// A phase standing in a synthetic 10x10 open room but named `MAP_ROUTE101`,
/// with a single tall-grass tile at `grass`. Real map header, real event
/// data (Route 101's own object events all sit at `y >= 8`, clear of the
/// `y == 5` walking lane these tests use), real wild table — only the layout
/// grid is synthetic, because no bundled pack is needed to prove the roll.
pub(super) fn route_101_phase(player: PlayerState, grass: (u16, u16)) -> OverworldPhase {
    route_101_phase_with_grass(10, 10, player, &[grass])
}

/// [`route_101_phase`] over a room of the caller's own size with any number
/// of tall-grass tiles, for scenarios that need several *rolling* steps in a
/// row rather than one.
pub(super) fn route_101_phase_with_grass(
    width: u16,
    height: u16,
    player: PlayerState,
    grass: &[(u16, u16)],
) -> OverworldPhase {
    let specials: Vec<((u16, u16), u8)> = grass.iter().map(|&pos| (pos, MB_TALL_GRASS)).collect();
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles(width, height, &specials),
        ROUTE_101,
        player,
        None,
    )
}

/// The seed used below, chosen (by enumeration over the LCG) so the first
/// *rolled* step produces an encounter. Its first four draws are
/// `24107, 54858, 56010, 31`:
///
/// - `24107 % 100 = 7 < 60` — `AllowWildCheckOnNewMetatile` allows the check
///   (the step moved from ordinary ground onto grass, so this draw happens);
/// - `54858 % 2880 = 138 < 320` — `WildEncounterCheck` passes Route 101's
///   `20 * 16` rate;
/// - `56010 % 100 = 10` — `ChooseWildMonIndex_Land` picks slot 0;
/// - `31 % 1 = 0` — `ChooseWildMonLevel` over slot 0's flat 2..=2 band.
///
/// Slot 0 of the extracted Route 101 table is a level-2 Wurmple.
pub(super) const ENCOUNTER_SEED: u32 = 17;

/// `SPECIES_WURMPLE`, slot 0 of Route 101's land table.
pub(super) const WURMPLE: SpeciesId = SpeciesId(290);

/// A placeholder save-owner id for scenarios that exercise something other
/// than opponent OT assignment (`opponent_ot_id_tests` pins that directly).
pub(super) const TEST_PLAYER_TRAINER_ID: u32 = 0x1234_5678;
