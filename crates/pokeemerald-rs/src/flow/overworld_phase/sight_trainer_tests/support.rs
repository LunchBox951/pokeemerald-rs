//! Fixtures shared by the sight-trainer test modules (module docs on
//! [`super`]): the Route 103 phase, the stand-in battle seeds, and the
//! approach seed. Kept here, not in `test_support`, so the generic helpers
//! stay free of sight-trainer specifics.

use assets::MapId;
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::{ObjectEventState, PlayerState};
use platform::ButtonState;

use super::super::sight_trainer_approach::SightApproach;
use super::super::{ActiveBattle, OverworldPhase};

/// `MAP_ROUTE103`, used throughout the sight-trainer test modules.
pub(super) const ROUTE_103: MapId = MapId("MAP_ROUTE103");

/// `TRAINER_FLAGS_START` (module docs on `sight_trainer_trigger`) --
/// independently transcribed here too, the same "each module cites its own
/// constant" convention this crate's other sibling test files already use.
pub(super) const TRAINER_FLAGS_START: u16 = 0x500;

/// `TRAINER_RHETT` (`include/constants/opponents.h`): a single-battle,
/// no-held-item, level-15 party -- the main subject for the
/// trigger/win/loss/defeated-flag tests. His own object event stands at
/// `(67, 5)`, elevation 3, facing south (`MOVEMENT_TYPE_FACE_DOWN`), sight
/// range 2.
pub(super) const TRAINER_RHETT: u16 = 703;

/// `MOVE_PURSUIT`, Treecko's own level-16 learnset move: `EFFECT_PURSUIT`
/// has no resolver, so `validate_player_move` refuses it ahead of any draw
/// (`crates/battle/src/battle.rs:415`) while a full-PP slot keeps the
/// all-spent Struggle diversion (`crates/battle/src/battle.rs:491`) out of
/// the way.
pub(super) const UNEXECUTABLE_MOVE: u16 = 228;

/// The tile Rhett's object event stands on in `MAP_ROUTE103` (the
/// `TRAINER_RHETT` docs): every fixture places the player relative to it.
pub(super) const RHETT_TILE: (i32, i32) = (67, 5);

/// `TRAINER_ANDREW` (`include/constants/opponents.h`): used only for the
/// "does not trigger" geometry tests, so a false positive there can never be
/// confused with Rhett's own fixtures. His object event stands at
/// `(50, 8)`, elevation 3, facing south (`MOVEMENT_TYPE_WALK_DOWN_AND_UP`'s
/// own initial facing), sight range 3.
pub(super) const ANDREW_TILE: (i32, i32) = (50, 8);

/// `TRAINER_MIGUEL_1` (`include/constants/opponents.h`): a real
/// `TrainerParty::ItemDefaultMoves` party (`begin_sight_trainer_approach_if_seen`'s
/// own "Refusals cost nothing, forever") --
/// [`crate::flow::npc_trainer_battle`] refuses to construct it. His object
/// event stands at `(56, 13)`, elevation 3, facing east, sight range 5.
pub(super) const MIGUEL_TILE: (i32, i32) = (56, 13);

/// `TRAINER_AMY_AND_LIV_1` (`include/constants/opponents.h`): Amy's own
/// object event, `(64, 12)`, elevation 3, facing south, sight range 1 --
/// used for the double-battle refusal test.
pub(super) const AMY_TILE: (i32, i32) = (64, 12);

/// A large-enough-for-every-elevation-3-trainer synthetic open room, paired
/// with the real `MAP_ROUTE103` object events (`sight_trainer_tests` docs).
pub(super) fn route_103_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        ROUTE_103,
        player,
        None,
    )
}

/// A battle-ready lead of `species`/`level` with a single `move_id`, mirroring
/// `route103_rival_test_support::lead`.
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
        assets::SpeciesId(species),
        level,
        ivs,
        0,
        vec![assets::MoveId(move_id)],
    )
    .expect("species/move must be in the dex")
}

/// `SPECIES_TREECKO`/`SLASH` at level 50 -- overwhelms Rhett's real level-15
/// Makuhita almost immediately, for the tests whose subject is "the battle
/// starts/concludes", not "who wins slowly".
pub(super) fn overwhelming_lead() -> BattlePokemon {
    lead(277, 50, 163)
}

/// `SPECIES_TREECKO`/`POUND` at level 1 -- heavily overmatched, for the
/// [`BattleOutcome::PlayerLost`] test.
pub(super) fn overmatched_lead() -> BattlePokemon {
    lead(277, 1, 1)
}

/// Play turns of `phase`'s in-progress sight-trainer battle, one per idle
/// [`OverworldPhase::step`] call, until it reports a terminal outcome or
/// `budget` turns have passed -- mirrors
/// `route103_rival_test_support::play_out_rival_battle`.
pub(super) fn play_out_sight_battle(
    phase: &mut OverworldPhase,
    budget: usize,
) -> Option<BattleOutcome> {
    for _ in 0..budget {
        phase.step(ButtonState::new());
        if let Some(outcome) = phase.sight_trainer_battle_outcome() {
            return Some(outcome);
        }
    }
    None
}

/// `TRAINER_MAY_ROUTE_103_TREECKO` (`crates/battle/src/battle/trainer.rs`'s
/// own `route103_rival::tests` fixtures use the identical id/RNG-seed/lead
/// combination below for an identical "must lose" scenario) -- the
/// proven-constructible stand-in party [`seed_battle`] and
/// `OverworldPhase::synthetic_sight_trainer` each borrow (`sight_trainer_tests`
/// docs, "The stand-in party").
pub(super) const STAND_IN_TRAINER: u16 = 532;

/// Seed `phase` with an in-progress sight-trainer battle directly, bypassing
/// [`OverworldPhase::begin_sight_trainer_approach_if_seen`]'s own construction
/// attempt (`sight_trainer_tests` docs, "The stand-in party"): a *real*
/// battle, built through the real `start_npc_trainer_battle`, against
/// [`STAND_IN_TRAINER`] -- but [`ActiveBattle`] (private to `overworld_phase`,
/// reachable here since this file is one of its own descendant modules) is
/// keyed to `trainer_id`, the real sight trainer the defeated-flag half
/// should end up keyed to.
pub(super) fn seed_battle(
    phase: &mut OverworldPhase,
    trainer_id: u16,
    player_lead: BattlePokemon,
    rng_seed: u32,
) {
    phase.rng = engine::rng::Rng::new(rng_seed);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        player_lead,
        assets::trainers::TrainerId(STAND_IN_TRAINER),
        &mut phase.rng,
    )
    .expect("the stand-in Route 103 rival must always construct");
    phase.party_lead = None;
    phase.active_battle = Some(ActiveBattle::SightTrainer {
        battle,
        trainer_id: assets::trainers::TrainerId(trainer_id),
    });
}

/// The trainer the active `SightTrainer` battle is keyed to, if any.
pub(super) fn active_sight_trainer_id(
    phase: &OverworldPhase,
) -> Option<assets::trainers::TrainerId> {
    match phase.active_battle.as_ref() {
        Some(ActiveBattle::SightTrainer { trainer_id, .. }) => Some(*trainer_id),
        _ => None,
    }
}

/// How many frames the exclamation-mark icon holds before the walk-up starts
/// (`sSpriteAnim_Icons1`'s `ANIMCMD_FRAME(0, 60)`, `trainer_see.c:150-154`)
/// -- transcribed independently of `sight_trainer_approach`'s own constant,
/// this crate's usual "each test file cites the upstream fact" convention.
pub(super) const EXCLAMATION_ICON_FRAMES: usize = 60;

/// Non-icon lock-handoff frames after the trigger frame for a standing
/// player, transcribed independently of `sight_trainer_approach`'s own
/// constants (`event_object_lock.c:130-146`, `script.c:80-87`).
pub(super) const LOCK_HANDOFF_AT_REST: usize = 1;
/// The same handoff counted after the frame an in-flight step drains on.
pub(super) const LOCK_HANDOFF_AFTER_DRAIN: usize = 2;

/// Rhett's own real object event out of the extracted `MAP_ROUTE103` data.
pub(super) fn rhetts_object_event() -> &'static assets::ObjectEvent {
    assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data")
        .object_events
        .iter()
        .find(|event| event.script == "Route103_EventScript_Rhett")
        .expect("Route 103 declares Rhett's own object event")
}

/// Seed `phase` with the approach `begin_sight_trainer_approach_if_seen`
/// would start for Rhett against a player `walk_tiles + 1` tiles away
/// (`InitTrainerApproachTask`'s own `approachDistance - 1`), carrying a real
/// stand-in battle (section docs).
pub(super) fn seed_approach(phase: &mut OverworldPhase, walk_tiles: u8) {
    phase.rng = engine::rng::Rng::new(7);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        overwhelming_lead(),
        assets::trainers::TrainerId(STAND_IN_TRAINER),
        &mut phase.rng,
    )
    .expect("the stand-in Route 103 rival must always construct");
    phase.party_lead = Some(overwhelming_lead());
    phase.sight_approach = Some(SightApproach::new(
        ObjectEventState::from_template(rhetts_object_event()),
        walk_tiles,
        "Whoa!\nHow'd you get into a space this small?",
        battle,
        assets::trainers::TrainerId(TRAINER_RHETT),
    ));
}

/// The approaching trainer's live object-event state, for the approach tests'
/// assertions.
pub(super) fn approaching_trainer(phase: &OverworldPhase) -> &ObjectEventState {
    phase
        .sight_approach
        .as_ref()
        .expect("the approach must still be running")
        .trainer()
}

/// How many consecutive frames the multi-frame RNG tests stand still for --
/// one wall-clock second at this port's 60 Hz frame budget, i.e. long past
/// the point where a per-frame leak would be obvious.
pub(super) const FRAMES_STANDING_STILL: usize = 60;
