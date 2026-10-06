//! Shared fixtures (constants, phase builders, lead builders, battle driver) for the `route103_rival_*_tests` modules.

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::{Direction, PlayerState};
use platform::ButtonState;

use super::OverworldPhase;

/// `MAP_ROUTE103`, used throughout this file.
pub(super) const ROUTE_103: MapId = MapId("MAP_ROUTE103");

/// `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`) -- independently
/// transcribed here too, the same "each module cites its own constant"
/// convention `route103_rival_trigger`'s own module docs explain.
pub(super) const VAR_OBJ_GFX_ID_0: u16 = 0x4010;

/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`'s numeric id (female player's
/// rival).
pub(super) const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`'s numeric id (male player's rival).
pub(super) const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

/// `FLAG_HIDE_ROUTE_103_RIVAL` (`include/constants/flags.h:772`).
pub(super) const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

/// `TRAINER_FLAGS_START` (`include/constants/flags.h:1343`), transcribed
/// like [`VAR_OBJ_GFX_ID_0`].
pub(super) const TRAINER_FLAGS_START: u16 = 0x500;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`) -- independently
/// transcribed here too, the same convention [`VAR_OBJ_GFX_ID_0`]'s own doc
/// comment explains. A fresh phase's `EventData` defaults every var to `0`
/// (Treecko), which is why most tests below need no explicit write at all.
pub(super) const VAR_STARTER_MON: u16 = 0x4023;

/// The rival object event's own tile (`data/maps/Route103/map.json`,
/// `local_id` 2) -- `(10, 3)`, elevation 3, facing right.
pub(super) const RIVAL_TILE: (i32, i32) = (10, 3);

/// An [`OverworldPhase`] over a **synthetic** flat, open room but the
/// *real* `MAP_ROUTE103` id (module docs) -- large enough to contain
/// [`RIVAL_TILE`] with room to stand beside it.
pub(super) fn route_103_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(15, 8),
        ROUTE_103,
        player,
        None,
    )
}

/// [`route_103_phase`] with the player already facing [`RIVAL_TILE`] from
/// the west, at rest -- the stance every interaction test below presses A
/// from.
pub(super) fn route_103_phase_facing_the_rival() -> OverworldPhase {
    let (rx, ry) = RIVAL_TILE;
    route_103_phase(PlayerState::new((rx - 1, ry), 3, Direction::East))
}

/// A battle-ready lead of `species`/`level` with a single `move_id`, built
/// directly (not through [`new_game::provisional_starter`]) so the tests
/// below can control level -- needed to force a fast, deterministic win.
/// `species` no longer decides which rival is fought (issue #251:
/// [`begin_route103_rival_battle`] now reads `VAR_STARTER_MON` instead of
/// deriving a starter from the lead's own species -- a fresh phase's var
/// defaults to `0`/Treecko, [`VAR_STARTER_MON`]'s own doc comment), so a
/// test that wants a non-Treecko rival must write the var explicitly rather
/// than lean on this helper's `species` argument.
pub(super) fn lead(species: u16, level: u8, move_id: u16) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(
        &Dex::new(),
        SpeciesId(species),
        level,
        ivs,
        0,
        vec![MoveId(move_id)],
    )
    .expect("species/move must be in the dex")
}

/// `SPECIES_TREECKO`/`SLASH` -- a level-50 lead that any of the six level-5
/// rivals loses to almost immediately, for the tests whose subject is "the
/// battle concludes", not "who wins slowly".
pub(super) fn overwhelming_treecko_lead() -> BattlePokemon {
    lead(277, 50, 163)
}

/// `SPECIES_TREECKO`/`POUND` at level 1 -- heavily overmatched against any
/// level-5, type-advantaged rival, for the [`BattleOutcome::PlayerLost`]
/// tests.
pub(super) fn overmatched_treecko_lead() -> BattlePokemon {
    lead(277, 1, 1)
}

/// Play turns of `phase`'s in-progress rival battle, one per idle
/// [`OverworldPhase::step`] call (mirroring how a real frame drives
/// [`OverworldPhase::advance_route103_rival_battle_frame`]), until it
/// reports a terminal outcome or `budget` turns have passed.
pub(super) fn play_out_rival_battle(
    phase: &mut OverworldPhase,
    budget: usize,
) -> Option<BattleOutcome> {
    for _ in 0..budget {
        phase.step(ButtonState::new());
        if let Some(outcome) = phase.rival_battle_outcome() {
            return Some(outcome);
        }
    }
    None
}
