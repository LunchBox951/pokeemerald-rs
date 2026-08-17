//! `sight_trainer_trigger` (issue #264, I-5 follow-up): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check, the headless battle handoff, and
//! the defeated-flag/win/loss posture. New sibling test module, the same
//! per-area split `route103_rival_tests` already uses (module docs there).
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split: a fabricated flat open room paired with the *real* `MAP_ROUTE103`
//! id, so `assets::MapEventsTable::resolve` hands back the real sight
//! trainers' own object events (positions, facings, ranges) without needing
//! a local pack. The room is sized to comfortably contain every
//! elevation-3 sight trainer at once (Rhett, Marcos, Daisy, Liv, Amy,
//! Andrew, Miguel); Isabelle and Pete are elevation 1 and so never trigger
//! against this fixture's uniform elevation-3 ground (an honest fixture
//! limitation, not a claim they are untestable -- `engine::overworld::trainer_sight`'s
//! own test suite already pins the elevation-compatibility rule generically).
//!
//! # A real battle, through the real cone (issue #293)
//!
//! Issue #264 could not test the win/loss/defeated-flag driver against a
//! real sight trainer at all: every one of the nine failed to construct,
//! because their level-up-derived movesets all reached a move the `battle`
//! crate could not execute. That file's [`seed_battle`] therefore borrowed a
//! Route 103 *rival*'s proven-constructible party as a stand-in and seeded
//! [`OverworldPhase::sight_trainer_battle`] directly, bypassing
//! [`OverworldPhase::begin_sight_trainer_battle_if_seen`] — and its own docs
//! recorded that "once a future move-coverage slice lets a real sight
//! trainer construct, the two halves should be merged back into one real
//! end-to-end test".
//!
//! That slice was issue #293, and this is that merge. Rhett's real party —
//! a level-15 Makuhita knowing Focus Energy, Sand Attack, Arm Thrust and
//! Vital Throw — now constructs, so every test below runs the **whole
//! chain**: stand in the real cone over the real object-event geometry, let
//! the unconditional per-frame check fire, let
//! `begin_sight_trainer_battle_if_seen` build the real
//! `CreateNPCTrainerParty` party off the shared stream, and drive the real
//! `battle::Battle` to a terminal outcome one turn per frame. No seeded
//! slot, no borrowed party.
//!
//! [`seed_battle`] is gone with it. What remains borrowed is nothing.

use assets::MapId;
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use crate::flow::tests::held;

use super::OverworldPhase;

/// `MAP_ROUTE103`, used throughout this file.
const ROUTE_103: MapId = MapId("MAP_ROUTE103");

/// `TRAINER_FLAGS_START` (module docs on `sight_trainer_trigger`) --
/// independently transcribed here too, the same "each module cites its own
/// constant" convention this crate's other sibling test files already use.
const TRAINER_FLAGS_START: u16 = 0x500;

/// `TRAINER_RHETT` (`include/constants/opponents.h`): a single-battle,
/// no-held-item, level-15 party -- this file's main subject for the
/// trigger/win/loss/defeated-flag tests. His own object event stands at
/// `(67, 5)`, elevation 3, facing south (`MOVEMENT_TYPE_FACE_DOWN`), sight
/// range 2.
const TRAINER_RHETT: u16 = 703;
const RHETT_TILE: (i32, i32) = (67, 5);

/// `SPECIES_MAKUHITA` (`include/constants/species.h:341`) -- the mon
/// `sParty_Rhett` fields, and since issue #293 one this engine can play
/// against move for move.
const MAKUHITA: u16 = 335;

/// `TRAINER_DAISY` (`include/constants/opponents.h`): still refused, for her
/// Shroomish's Leech Seed -- Absorb, Tackle and Stun Spore all execute
/// since issue #293, so it is the end-of-turn drain and nothing else that
/// makes her the stand-in for "a cone whose battle cannot be built"
/// (`sight_trainer_trigger`'s own verdict table pins the move id). Her
/// object event stands at `(71, 11)`, elevation 3,
/// `MOVEMENT_TYPE_FACE_DOWN_AND_RIGHT` -- whose
/// `gInitialMovementTypeFacingDirections` entry, and so the only facing
/// this port's static object events ever have, is **south** -- sight range
/// 3.
const DAISY_TILE: (i32, i32) = (71, 11);

/// `TRAINER_ANDREW` (`include/constants/opponents.h`): the subject of the
/// "does not trigger" geometry tests -- so a false positive there can never
/// be confused with Rhett's own fixtures -- and, since issue #293, the only
/// **multi-mon** party this port can field. His object event stands at
/// `(50, 8)`, elevation 3, facing south (`MOVEMENT_TYPE_WALK_DOWN_AND_UP`'s
/// own initial facing), sight range 3.
const ANDREW_TILE: (i32, i32) = (50, 8);
const TRAINER_ANDREW: u16 = 336;

/// `SPECIES_MAGIKARP` (`include/constants/species.h:133`) -- `sParty_Andrew`'s
/// level-5 lead, and again as its level-15 tail.
const MAGIKARP: u16 = 129;

/// `SPECIES_TENTACOOL` (`:76`) -- `sParty_Andrew`'s level-10 middle mon.
const TENTACOOL: u16 = 72;

/// `TRAINER_MIGUEL_1` (`include/constants/opponents.h`): a real
/// `TrainerParty::ItemDefaultMoves` party (module docs' item 6) --
/// [`crate::flow::npc_trainer_battle`] refuses to construct it. His object
/// event stands at `(56, 13)`, elevation 3, facing east, sight range 5.
const MIGUEL_TILE: (i32, i32) = (56, 13);

/// `TRAINER_AMY_AND_LIV_1` (`include/constants/opponents.h`): Amy's own
/// object event, `(64, 12)`, elevation 3, facing south, sight range 1 --
/// used for the double-battle refusal test.
const AMY_TILE: (i32, i32) = (64, 12);

/// A large-enough-for-every-elevation-3-trainer synthetic open room, paired
/// with the real `MAP_ROUTE103` object events (module docs).
fn route_103_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        ROUTE_103,
        player,
        None,
    )
}

/// A battle-ready lead of `species`/`level` with a single `move_id`, mirroring
/// `route103_rival_tests::lead`.
fn lead(species: u16, level: u8, move_id: u16) -> BattlePokemon {
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
fn overwhelming_lead() -> BattlePokemon {
    lead(277, 50, 163)
}

/// `SPECIES_TREECKO`/`POUND` at level 1 -- heavily overmatched, for the
/// [`BattleOutcome::PlayerLost`] test.
fn overmatched_lead() -> BattlePokemon {
    lead(277, 1, 1)
}

/// Play turns of `phase`'s in-progress sight-trainer battle, one per idle
/// [`OverworldPhase::step`] call, until it reports a terminal outcome or
/// `budget` turns have passed -- mirrors
/// `route103_rival_tests::play_out_rival_battle`.
fn play_out_sight_battle(phase: &mut OverworldPhase, budget: usize) -> Option<BattleOutcome> {
    for _ in 0..budget {
        phase.step(ButtonState::new());
        if let Some(outcome) = phase.sight_trainer_battle_outcome() {
            return Some(outcome);
        }
    }
    None
}

// -- Sight-cone geometry, through the real trigger -------------------------

/// Item 1 of the issue's own scope, now end to end (issue #293): standing
/// within a real sight trainer's cone starts the real battle on an ordinary
/// frame -- **no button press at all** (unlike the rival's own A-press
/// interaction trigger), matching upstream's
/// `CheckForTrainersWantingBattle` running unconditionally ahead of every
/// other per-frame check.
///
/// Everything here is real: Rhett's own object event out of
/// `MAP_ROUTE103`'s extracted `object_events`, his own
/// `gTrainers[TRAINER_RHETT]` row, and the level-15 Makuhita his party table
/// names, built through `CreateNPCTrainerParty`'s seeded personality and
/// `OT_ID_RANDOM_NO_SHINY` draws. Under issue #264 this test could only
/// assert that the handoff was *attempted* and failed.
#[test]
fn standing_in_a_real_trainers_cone_starts_the_real_battle() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    assert!(
        !phase.is_sight_trainer_battle_active(),
        "setup: no battle yet"
    );
    let before = phase.rng.state();
    phase.step(ButtonState::new());

    assert!(
        phase.is_sight_trainer_battle_active(),
        "Rhett's real party constructs since issue #293, so his cone really starts the fight"
    );
    assert!(phase.party_lead.is_none(), "the lead moved into the battle");
    assert_ne!(
        phase.rng.state(),
        before,
        "CreateNPCTrainerParty's per-mon OT-id draws really came off the shared stream"
    );
}

/// ...and it is Rhett's *own* party that comes out, not some stand-in: the
/// species and level `sParty_Rhett` names (`src/data/trainer_parties.h`), on
/// the IVs its `.iv = 100` scales to.
#[test]
fn the_battle_the_cone_starts_fields_rhetts_real_party() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());

    let battle = phase
        .sight_trainer_battle
        .as_ref()
        .expect("the cone started a battle");
    assert_eq!(
        battle.enemy().species(),
        assets::SpeciesId(MAKUHITA),
        "sParty_Rhett fields SPECIES_MAKUHITA"
    );
    assert_eq!(battle.enemy().level(), 15);
    assert_eq!(
        battle.enemy().ivs().as_array(),
        [12; 6],
        "`.iv = 100` scales to 100 * 31 / 255 == 12 across the board"
    );
    assert_eq!(
        battle.trainer().map(battle::TrainerContext::id),
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "and it is a BATTLE_TYPE_TRAINER battle against Rhett himself"
    );
    // The moveset issue #293 unlocked, in learnset order.
    let known: Vec<u16> = battle
        .enemy()
        .moves()
        .iter()
        .map(|slot| slot.move_id.0)
        .collect();
    assert_eq!(
        known,
        vec![116, 28, 292, 233],
        "Focus Energy, Sand Attack, Arm Thrust, Vital Throw -- Makuhita's real level-15 \
         GiveBoxMonInitialMoveset result"
    );
}

/// How many consecutive frames the multi-frame RNG tests stand still for --
/// one wall-clock second at this port's 60 Hz frame budget, i.e. long past
/// the point where a per-frame leak would be obvious.
const FRAMES_STANDING_STILL: usize = 60;

/// Assert that `tile` really is inside the cone of the object event running
/// `script`, over this file's synthetic room and the real Route 103 object
/// events -- asked of the same `engine` geometry the trigger itself uses.
///
/// A fixture check, not a subject: every caller's *own* assertion is about
/// what happens once the cone has fired (or refused), and a tile that is in
/// no cone at all would satisfy a "no battle started" assertion for entirely
/// the wrong reason. Issue #293's review found exactly that -- a transcribed
/// tile off the wrong object event -- so the check is now in front of it.
fn assert_cone_reaches(script: &str, tile: (i32, i32)) {
    let scene = crate::overworld::tests::synthetic_scene(80, 16);
    let header = assets::MapHeaderTable::new()
        .header(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let events = assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let event = events
        .object_events
        .iter()
        .find(|event| event.script == script)
        .unwrap_or_else(|| panic!("Route 103 declares an object event running {script}"));
    assert!(
        engine::overworld::trainer_can_see_player(
            event,
            &scene.runtime(ROUTE_103, header, events),
            &PlayerState::new(tile, 3, Direction::North),
            &engine::event_data::EventData::new(),
        ),
        "{script}: {tile:?} must really be inside the cone, or whatever the \
         caller asserts about the cone firing proves nothing"
    );
}

/// Issue #264's review finding F1, pinned: a cone whose battle *cannot be
/// constructed* is re-reached on every single frame the player stands in it
/// (there is no button gate on this check), so the refusal has to cost
/// nothing at all -- not "nothing on the first frame", nothing ever. Before
/// the pre-flight screen (`crate::flow::npc_trainer_battle`'s module docs)
/// this leaked `CreateNPCTrainerParty`'s per-mon OT-id draws sixty times a
/// second off the one stream every wild encounter and battle turn shares.
///
/// The specimens moved with the coverage boundary at issue #293 -- Rhett's
/// party constructs now, so he is the subject of the *win* tests below
/// instead, and Daisy takes his place here. All three refusal shapes are
/// still covered: an unimplemented moveset (Daisy's Leech Seed, module docs
/// item 7), a party that is refused for a move *and* holds an unrunnable
/// item (Miguel, item 6), and a double battle refused before construction is
/// even attempted (Amy, item 5).
///
/// Each tile is checked against the real cone *first* (issue #293 review):
/// "no battle started" is a claim about a refusal, and a tile that is in no
/// cone at all satisfies it for the wrong reason. `DAISY_TILE` had in fact
/// been transcribed off the wrong object event, so this test's Daisy row was
/// passing vacuously until [`assert_cone_reaches`] was put in front of it.
#[test]
fn standing_in_a_cone_for_many_frames_never_touches_the_rng_stream() {
    let (dx, dy) = DAISY_TILE;
    let (mx, my) = MIGUEL_TILE;
    let (ax, ay) = AMY_TILE;
    let cases = [
        (
            "Daisy (unimplemented moveset)",
            "Route103_EventScript_Daisy",
            (dx, dy + 1),
        ),
        (
            "Miguel (unimplemented moveset, held item behind it)",
            "Route103_EventScript_Miguel",
            (mx + 2, my),
        ),
        (
            "Amy (double battle)",
            "Route103_EventScript_Amy",
            (ax, ay + 1),
        ),
    ];
    for (name, script, tile) in cases {
        assert_cone_reaches(script, tile);
        let mut phase = route_103_phase(PlayerState::new(tile, 3, Direction::North));
        phase.party_lead = Some(overwhelming_lead());
        let before = phase.rng.state();
        for frame in 0..FRAMES_STANDING_STILL {
            phase.step(ButtonState::new());
            assert!(
                !phase.is_sight_trainer_battle_active(),
                "{name}: frame {frame} must still refuse"
            );
            assert_eq!(
                phase.rng.state(),
                before,
                "{name}: frame {frame} moved the shared RNG stream -- a refused cone must \
                 draw nothing, on every frame, forever"
            );
        }
        assert!(
            phase.party_lead.is_some(),
            "{name}: the lead is never consumed by a refusal"
        );
    }
}

/// A player one tile beyond a trainer's own sight range must not trigger.
#[test]
fn a_player_beyond_range_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    // Andrew's own range is 3; four tiles south is one past it.
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(phase.party_lead.is_some(), "the lead must be untouched");
}

/// A player off a trainer's own facing axis must not trigger, even standing
/// right beside them.
#[test]
fn a_player_off_the_facing_axis_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax + 1, ay), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

/// The sight check itself draws nothing -- pinned against a refused trigger
/// (out of range) so the RNG state is directly comparable before/after.
#[test]
fn a_refused_sight_check_draws_nothing() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert_eq!(
        phase.rng.state(),
        before,
        "the geometry check must draw nothing"
    );
}

// -- The fainted-lead fail-closed screen ------------------------------------

/// The same fail-closed screen the rival trigger applies: a fainted lead
/// must not start a sight-trainer battle, and the screen itself draws
/// nothing.
#[test]
fn a_fainted_lead_cannot_start_a_sight_trainer_battle_and_draws_nothing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    let mut fainted = overmatched_lead();
    fainted.apply_damage(u32::MAX);
    assert!(fainted.is_fainted(), "setup: the lead really is fainted");
    phase.party_lead = Some(fainted);

    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(phase.rng.state(), before);
}

/// Start Rhett's **real** battle the way real play does: stand the player in
/// his real cone with `player_lead` in the party and take one ordinary
/// frame, letting [`OverworldPhase::begin_sight_trainer_battle_if_seen`] do
/// the whole `CreateNPCTrainerParty` construction off `phase`'s own shared
/// stream (issue #293 — see the module docs for what this replaced).
///
/// `rng_seed` reseeds that stream first, so a scenario is reproducible: the
/// party's OT ids, the battle's turn-number draw, and every AI and damage
/// roll after it all come out of it in upstream's order.
fn start_rhetts_battle(phase: &mut OverworldPhase, player_lead: BattlePokemon, rng_seed: u32) {
    let (rx, ry) = RHETT_TILE;
    phase.player = PlayerState::new((rx, ry + 1), 3, Direction::North);
    phase.rng = engine::rng::Rng::new(rng_seed);
    phase.party_lead = Some(player_lead);
    phase.step(ButtonState::new());
    assert!(
        phase.is_sight_trainer_battle_active(),
        "setup: Rhett's real cone must start his real battle"
    );
    assert_eq!(
        phase.sight_trainer_id,
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "setup: keyed to the real TRAINER_RHETT, which the defeated flag depends on"
    );
}

/// A second real trainer through the same cone, and the first **multi-mon**
/// party this port has ever fielded: Andrew's Magikarp / Tentacool /
/// Magikarp (`sParty_Andrew`).
///
/// Worth its own test rather than folding into Rhett's: it is the only
/// sight-trainer battle that exercises the forced post-faint send-out
/// (`OpponentHandleChoosePokemon` -> `TrainerContext::send_out_next`) from
/// the cone, and the only one whose moveset spans four of the seven move
/// pipelines at once -- Splash (`status_move`), Poison Sting (`secondary`,
/// with the poison it inflicts), Supersonic (`primary_status`, confusion)
/// and Tackle (`hit`).
#[test]
fn andrews_cone_starts_his_real_three_mon_party_and_plays_it_out() {
    let (ax, ay) = ANDREW_TILE;
    // Andrew faces south with sight range 3; two tiles south is inside it.
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 2), 3, Direction::North));
    phase.rng = engine::rng::Rng::new(7);
    phase.party_lead = Some(overwhelming_lead());

    phase.step(ButtonState::new());
    assert!(
        phase.is_sight_trainer_battle_active(),
        "Andrew's real party constructs since issue #293"
    );
    let battle = phase
        .sight_trainer_battle
        .as_ref()
        .expect("the cone started a battle");
    assert_eq!(
        battle.enemy().species(),
        assets::SpeciesId(MAGIKARP),
        "sParty_Andrew leads with a level-5 Magikarp"
    );
    assert_eq!(battle.enemy().level(), 5);
    let bench: Vec<(u16, u8)> = battle
        .trainer()
        .expect("a sight-trainer battle has a trainer context")
        .bench()
        .iter()
        .map(|mon| (mon.species().0, mon.level()))
        .collect();
    assert_eq!(
        bench,
        vec![(TENTACOOL, 10), (MAGIKARP, 15)],
        "and two more behind it, in `sParty_Andrew`'s own party order -- the level-10 \
         Tentacool before the level-15 Magikarp, which is the order \
         `TrainerContext::send_out_next` will field them in \
         (`src/data/trainer_parties.h` sParty_Andrew)"
    );

    let outcome = play_out_sight_battle(&mut phase, 64);
    assert_eq!(
        outcome,
        Some(BattleOutcome::PlayerWon),
        "an overwhelming lead must beat all three"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_ANDREW),
        Ok(true),
        "and the defeated flag is keyed to Andrew, not to whoever was last out"
    );
}

// -- Frame ownership ---------------------------------------------------------

/// An in-progress sight-trainer battle owns the frame outright: a held
/// direction must not move the player, and the sight check must not
/// re-fire (or, since the player never left the cone, at least must not
/// disturb the running battle).
#[test]
fn an_in_progress_sight_battle_owns_the_frame() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    // A level-15 Treecko with Pound against Rhett's real level-15 Makuhita:
    // the point here is frame ownership, not how long the fight lasts.
    start_rhetts_battle(&mut phase, lead(277, 15, 1), 1);
    let position = phase.player.position();

    phase.step(held(Buttons::UP));
    assert!(phase.is_sight_trainer_battle_active());
    assert_eq!(
        phase.player.position(),
        position,
        "the battle owns the frame -- a held direction must not move the player"
    );
}

/// One ordinary turn must not immediately end an even-level fight.
#[test]
fn one_turn_does_not_immediately_end_a_fresh_battle() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    start_rhetts_battle(&mut phase, lead(277, 15, 1), 1); // a level-15 Treecko with Pound

    phase.step(ButtonState::new());
    assert!(
        phase.is_sight_trainer_battle_active(),
        "one ordinary turn must not end an even-level fight outright"
    );
}

// -- Win: the defeated flag, and unrepeatability ----------------------------

/// Item 4 of the issue's own scope: winning sets
/// `FLAG_TRAINER_FLAGS_START + TRAINER_RHETT`, and the fight cannot restart
/// -- unlike the rival, the trainer stays standing (no hide flag), but a
/// fresh approach into the same cone starts nothing.
#[test]
fn winning_sets_the_defeated_flag_and_the_fight_cannot_restart() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    start_rhetts_battle(&mut phase, overwhelming_lead(), 1);

    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        Some(BattleOutcome::PlayerWon)
    );
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "a concluded battle empties its slot"
    );
    assert!(
        phase.party_lead.is_some(),
        "the driver writes the player's mon back"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "SetBattledTrainersFlags's real effect (module docs item 4)"
    );
    assert_eq!(
        phase.save1().money,
        crate::new_game::STARTING_MONEY,
        "a win must not touch the player's money -- only white_out (a loss) halves it"
    );

    // Standing in the same cone again must not restart the fight -- the
    // trainer is still standing (no hide flag, module docs item 4), but
    // `already_defeated` refuses before the geometry even matters (and, as
    // of this issue, before Rhett's own real construction gap would matter
    // either).
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

/// The defeated flag survives a save/continue round trip: `SaveBlock1`'s own
/// `event_data` is carried wholesale into a freshly reconstructed phase
/// (`OverworldPhase::from_saved`'s own docs), so the win recorded above
/// stays won. Checked directly against the resumed phase's own `event_data`
/// rather than by stepping it back into Rhett's cone: Rhett's own real
/// construction currently fails regardless of the flag (module docs item
/// 7), so a `step`-based assertion here could not actually distinguish "the
/// flag survived" from "construction always refuses anyway" -- the flag
/// read is the one assertion that can.
#[test]
fn the_defeated_flag_survives_a_save_continue_round_trip() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    start_rhetts_battle(&mut phase, overwhelming_lead(), 1);
    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon), "setup: must win");
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "setup: the flag is set"
    );

    let block1 = phase.save1().clone();
    let block2 = phase.save2().clone();
    let scene = crate::overworld::tests::synthetic_scene(80, 16);
    let resumed = OverworldPhase::from_saved(scene, ROUTE_103, block1, block2);

    assert_eq!(
        resumed
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "SaveBlock1::event_data is carried wholesale into a continued phase, the defeated \
         flag included"
    );
}

// -- Loss: white-out ----------------------------------------------------------

/// A loss heals the party, halves the player's money, and leaves the
/// defeated flag clear (`SetBattledTrainersFlags` only runs on a win) --
/// mirrors `route103_rival_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`.
#[test]
fn losing_heals_halves_money_and_leaves_the_defeated_flag_clear() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    phase.save1.money = 2001;
    // A level-1 Treecko with Pound against Rhett's real level-15 Makuhita
    // cannot win.
    start_rhetts_battle(&mut phase, overmatched_lead(), 2024);

    let outcome = play_out_sight_battle(&mut phase, 64);
    assert_eq!(
        outcome,
        Some(BattleOutcome::PlayerLost),
        "a level-1 lead against a real level-15 trainer must lose"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(false),
        "a loss must not set the defeated flag"
    );
    assert_eq!(phase.save1().money, 1000, "2001 / 2 == 1000");
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the driver writes the player's mon back, and white_out heals it in place");
    assert!(
        !lead.is_fainted(),
        "a lost battle no longer leaves a fainted lead"
    );
}

// -- Honest cuts: Miguel's held item, Amy & Liv's double battle -------------

/// Item 6 of the issue's own scope: Miguel's cone reaches, but his real
/// held-item party refuses to construct -- no battle starts, the lead is
/// untouched, and the refusal draws nothing (the held-item error is raised
/// before any RNG draw, `crate::flow::npc_trainer_battle`'s own docs).
#[test]
fn miguels_held_item_party_refuses_to_construct() {
    let (mx, my) = MIGUEL_TILE;
    // Miguel faces east; two tiles east of him is within his own range-5 cone.
    let mut phase = route_103_phase(PlayerState::new((mx + 2, my), 3, Direction::West));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Miguel's held-item party must refuse to construct (not modelled)"
    );
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(
        phase.rng.state(),
        before,
        "the held-item refusal must draw nothing"
    );
}

/// Item 5 of the issue's own scope: Amy's cone reaches, but her real party
/// is a double battle this port cannot field (no doubles support, and at
/// most one tracked party mon) -- no battle starts.
#[test]
fn amys_double_battle_party_is_refused() {
    let (ax, ay) = AMY_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Amy & Liv's shared double-battle party must never be selected"
    );
    assert!(phase.party_lead.is_some());
}

// -- No party lead ------------------------------------------------------------

/// No party lead at all -- the same defensive `None` arm every other
/// battle-trigger test file pins.
#[test]
fn the_trigger_with_no_party_lead_logs_and_starts_nothing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    assert!(phase.party_lead.is_none(), "setup");
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- The trigger never fires off Route 103 -----------------------------------

/// A synthetic phase on a different map, at coordinates that would match
/// Rhett's own tile numerically, must never start a sight-trainer battle --
/// `MapEventsTable::resolve` hands back that map's own (empty, in this
/// fixture) object events, not Route 103's.
#[test]
fn the_trigger_never_fires_off_route_103() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((rx, ry + 1), 3, Direction::North),
        None,
    );
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- Real-pack terrain (issue #264's own item 1: real collision/elevation) -

/// The geometry tests above run over a synthetic, fully open room; this
/// confirms the same cone fires against Route 103's own *real* extracted
/// terrain -- collision bits, elevation, **and object-event occupancy**, not
/// just declared coordinates -- and that the real battle really starts over
/// it.
///
/// The tile matters, and vetting it means all three (issue #264 review, F5):
/// this test originally stood the player at `(67, 7)`, whose real decoded
/// cell is indeed open ground at elevation 3 -- but Route 103 declares an
/// `OBJ_EVENT_GFX_CUTTABLE_TREE` object event standing on it
/// (`data/maps/Route103/map.json`), so no player can ever be there without
/// HM01. `(67, 6)`, one tile south of Rhett, is genuinely occupiable: open
/// ground, elevation 3, and no object event of any kind. It is also
/// distance **1**, which after this same review's F3 fix is the newly
/// guarded case -- the whole `GetCollisionAtCoords` chain applies to the
/// player's own tile with no intermediate tile ahead of it to catch
/// anything first (`engine::overworld::trainer_sight`'s own docs).
///
/// Issue #264 could only assert that the handoff was *attempted* and failed,
/// so this test used to stand still for sixty frames and pin an untouched
/// RNG stream. Rhett's real party constructs since issue #293 -- his row in
/// `sight_trainer_trigger`'s verdict table is `Constructs(335, 15)` -- so
/// keeping the old assertion would have pinned the opposite of what the
/// engine now does, and contradicted this file's own synthetic-terrain
/// [`standing_in_a_real_trainers_cone_starts_the_real_battle`]. It is
/// inverted rather than deleted: the real-terrain half is the part no other
/// test covers. The per-frame no-draw property it used to carry lives on in
/// [`standing_in_a_cone_for_many_frames_never_touches_the_rng_stream`],
/// against cones that really are still refused.
///
/// `#[ignore]`d like this crate's other real-pack tests: run
/// `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_rhetts_cone_starts_his_real_battle_over_real_terrain() {
    let scene = crate::overworld::load_room(
        ROUTE_103,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let (rx, ry) = RHETT_TILE;
    let tile = (rx, ry + 1);
    let player = PlayerState::new(tile, 3, Direction::North);

    // The cone genuinely reaches, over the real decoded grid -- asked of the
    // same `engine` geometry the trigger uses, with the real object events.
    let header = assets::MapHeaderTable::new()
        .header(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let events = assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let rhett = events
        .object_events
        .iter()
        .find(|event| event.script == "Route103_EventScript_Rhett")
        .expect("Route 103 declares Rhett's own object event");
    assert!(
        engine::overworld::trainer_can_see_player(
            rhett,
            &scene.runtime(ROUTE_103, header, events),
            &player,
            &engine::event_data::EventData::new(),
        ),
        "Rhett's real cone must reach a player standing one tile south of him over real \
         terrain -- including the final tile's own collision/impassability arms"
    );

    let mut phase = OverworldPhase::for_test(scene, ROUTE_103, player, None);
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    phase.step(ButtonState::new());
    assert!(
        phase.is_sight_trainer_battle_active(),
        "Rhett's real party constructs since issue #293, so his cone starts the fight over \
         real terrain exactly as it does over the synthetic room"
    );
    assert_eq!(
        phase.sight_trainer_id,
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "and it is keyed to the real TRAINER_RHETT"
    );
    assert_eq!(
        phase
            .sight_trainer_battle
            .as_ref()
            .expect("the cone started a battle")
            .enemy()
            .species(),
        assets::SpeciesId(MAKUHITA),
        "fielding sParty_Rhett's own level-15 Makuhita"
    );
    assert!(phase.party_lead.is_none(), "the lead moved into the battle");
    assert_ne!(
        phase.rng.state(),
        before,
        "CreateNPCTrainerParty's per-mon OT-id draws really came off the shared stream"
    );
}

/// `advance_npc_trainer_battle`'s documented `None` ambiguity, resolved the
/// way its docs demand (issue #293 review): a failed turn clears the battle
/// slot *without* an outcome, and the trigger must treat that as terminal
/// rather than ongoing. Before the fix, the still-active cone rebuilt
/// Rhett's party on the very next frame -- `CreateNPCTrainerParty`'s
/// per-mon draws off the shared stream, sixty times a second, forever --
/// and the player was trapped in a restart loop.
///
/// The failure is manufactured through the same per-turn gate a real player
/// would hit: the driver always picks move slot 0, so a lead whose only
/// move has no PP left fails `validate_player_move` on turn one.
#[test]
fn a_battle_that_ends_without_an_outcome_disengages_instead_of_restarting() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    let mut drained = overwhelming_lead();
    for _ in 0..drained.moves()[0].pp {
        drained
            .deduct_pp(0)
            .expect("slot 0 exists with PP to spend");
    }
    start_rhetts_battle(&mut phase, drained, 1);

    // Turn one: the no-PP turn fails, the battle dies without an outcome.
    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "the failed turn cleared the battle slot"
    );
    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        None,
        "no outcome was ever reached"
    );
    assert!(
        phase.party_lead.is_some(),
        "the lead is written back standing, exactly as the driver left it"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(false),
        "a fight that never resolved must not be recorded as won"
    );
    assert_eq!(phase.sight_trainer_id, None, "the engagement is dropped");

    // Still standing in Rhett's cone: the fight must NOT restart, and the
    // shared stream must not move -- the errored trainer is skipped like a
    // defeated one for the rest of the session.
    let before = phase.rng.state();
    for frame in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
        assert!(
            !phase.is_sight_trainer_battle_active(),
            "frame {frame}: an errored engagement must not restart"
        );
        assert_eq!(
            phase.rng.state(),
            before,
            "frame {frame}: no construction draws after disengaging"
        );
    }
    assert!(phase.party_lead.is_some());
}
