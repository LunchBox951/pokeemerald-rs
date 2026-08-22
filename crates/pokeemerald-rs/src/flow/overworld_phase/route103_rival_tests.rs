//! `route103_rival_trigger` (issue #248, I-5): rival-sprite setup, the
//! A-press interaction trigger, and the headless per-frame driver. New
//! sibling test module alongside the per-area split of the former
//! monolithic `overworld_phase::tests` (issue #238) --
//! `crate::flow::save_continue_tests`/`crate::flow::start_menu_tests` set
//! the precedent for a per-area file at this crate's test-organization
//! level; this one sits inside `overworld_phase` itself (not `flow`
//! directly) because most of what it exercises --
//! [`super::OverworldPhase::begin_route103_rival_battle`],
//! [`super::route103_rival_trigger::is_rival_trigger`],
//! [`super::route103_rival_trigger::setup_rival_gfx_id_on_transition`] -- is
//! `pub(super)` to `overworld_phase`, not visible from `crate::flow`'s own
//! sibling test files.
//!
//! Mirrors `overworld_phase::first_battle_trigger_tests`' own "real events over a synthetic
//! grid" split (its module doc comment, and
//! `crate::flow::wild_encounter::tests::route_101_phase`/this file's own
//! sibling `first_battle_trigger` tests' `route_101_trigger_phase`): a
//! fabricated flat room paired with a *real* `MAP_ROUTE103` id, so
//! `assets::MapEventsTable::resolve` hands back the real rival object event
//! (`local_id` 2, `(10, 3)`, `MOVEMENT_TYPE_FACE_RIGHT`,
//! `"Route103_EventScript_Rival"`, `FLAG_HIDE_ROUTE_103_RIVAL`) without
//! needing a local pack for anything except the two real-pack tests that
//! are explicitly about pack-decoded reachability (the connection-crossing
//! walk and the rendered sprite).

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::PlayerGender;
use platform::{ButtonState, Buttons};

use crate::flow::tests::{held, pressed};

use super::route103_rival_trigger::{is_rival_trigger, setup_rival_gfx_id_on_transition};
use super::OverworldPhase;

/// `MAP_ROUTE103`, used throughout this file.
const ROUTE_103: MapId = MapId("MAP_ROUTE103");

/// `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`) -- independently
/// transcribed here too, the same "each module cites its own constant"
/// convention `route103_rival_trigger`'s own module docs explain.
const VAR_OBJ_GFX_ID_0: u16 = 0x4010;

/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`'s numeric id (female player's
/// rival).
const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`'s numeric id (male player's rival).
const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

/// `FLAG_HIDE_ROUTE_103_RIVAL` (`include/constants/flags.h:772`).
const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`) -- independently
/// transcribed here too, the same convention [`VAR_OBJ_GFX_ID_0`]'s own doc
/// comment explains. A fresh phase's `EventData` defaults every var to `0`
/// (Treecko), which is why most tests below need no explicit write at all.
const VAR_STARTER_MON: u16 = 0x4023;

/// The rival object event's own tile (`data/maps/Route103/map.json`,
/// `local_id` 2) -- `(10, 3)`, elevation 3, facing right.
const RIVAL_TILE: (i32, i32) = (10, 3);

/// An [`OverworldPhase`] over a **synthetic** flat, open room but the
/// *real* `MAP_ROUTE103` id (module docs) -- large enough to contain
/// [`RIVAL_TILE`] with room to stand beside it.
fn route_103_phase(player: PlayerState) -> OverworldPhase {
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
fn route_103_phase_facing_the_rival() -> OverworldPhase {
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
fn overwhelming_treecko_lead() -> BattlePokemon {
    lead(277, 50, 163)
}

/// `SPECIES_TREECKO`/`POUND` at level 1 -- heavily overmatched against any
/// level-5, type-advantaged rival, for the [`BattleOutcome::PlayerLost`]
/// tests.
fn overmatched_treecko_lead() -> BattlePokemon {
    lead(277, 1, 1)
}

/// Play turns of `phase`'s in-progress rival battle, one per idle
/// [`OverworldPhase::step`] call (mirroring how a real frame drives
/// [`OverworldPhase::advance_route103_rival_battle_frame`]), until it
/// reports a terminal outcome or `budget` turns have passed.
fn play_out_rival_battle(phase: &mut OverworldPhase, budget: usize) -> Option<BattleOutcome> {
    for _ in 0..budget {
        phase.step(ButtonState::new());
        if let Some(outcome) = phase.rival_battle_outcome() {
            return Some(outcome);
        }
    }
    None
}

// -- `setup_rival_gfx_id_on_transition` (module docs, step 1) ---------------

/// `Common_EventScript_SetupRivalGfxId`'s own `checkplayergender` pairing
/// (module docs): a male player's `VAR_OBJ_GFX_ID_0` becomes May's numeric
/// graphics id, a female player's becomes Brendan's.
#[test]
fn setup_rival_gfx_id_writes_the_opposite_genders_rival_id() {
    let mut male = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut male, PlayerGender::Male);
    assert_eq!(
        male.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a male player's rival is May"
    );

    let mut female = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut female, PlayerGender::Female);
    assert_eq!(
        female.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_BRENDAN_NORMAL_GFX_ID),
        "a female player's rival is Brendan"
    );
}

/// Upstream's own no-op: `checkplayergender`'s two `goto_if_eq`s never
/// match a raw byte outside `MALE`/`FEMALE`, so the var is left untouched
/// (module docs, [`Rival::for_gender`]'s own citation).
#[test]
fn setup_rival_gfx_id_leaves_an_unmodelled_gender_untouched() {
    let mut data = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut data, PlayerGender::Other(9));
    assert_eq!(data.var_get(VAR_OBJ_GFX_ID_0), Ok(0));
}

/// Entering any other map must never touch `VAR_OBJ_GFX_ID_0` -- gated on
/// the map, matching upstream's own map-scoped `MAP_SCRIPT_ON_TRANSITION`.
#[test]
fn setup_rival_gfx_id_is_a_no_op_off_route_103() {
    let mut data = EventData::new();
    setup_rival_gfx_id_on_transition(MapId("MAP_LITTLEROOT_TOWN"), &mut data, PlayerGender::Male);
    assert_eq!(data.var_get(VAR_OBJ_GFX_ID_0), Ok(0));
}

/// The var really is set the instant a synthetic phase is built on Route
/// 103, through the real [`OverworldPhase::new`]/[`OverworldPhase::for_test`]
/// construction path -- not just through the free function directly (the
/// other tests in this section). Item (b) of the issue's own test list.
/// `for_test`/`new` build a fresh save, whose default gender is always
/// [`PlayerGender::Male`] (`crate::new_game::DEFAULT_PLAYER_GENDER`); the
/// other genders are exercised directly against
/// [`setup_rival_gfx_id_on_transition`] above, the same "helper reachable
/// through the real construction path once, exhaustively at the helper
/// itself" split `first_battle_trigger`'s own tests use.
#[test]
fn a_phase_entering_route_103_carries_the_gfx_var_for_its_own_gender() {
    let phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    assert_eq!(
        phase.save2().player_gender,
        PlayerGender::Male,
        "setup: a fresh save's default gender"
    );
    assert_eq!(
        phase.save1().event_data.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a male player's rival is May, set the instant the phase enters Route 103"
    );
}

// -- `is_rival_trigger` (module docs, step 2) --------------------------------

/// Only Route 103's own rival script, only on Route 103 -- item (g) of the
/// issue's own test list: the trigger must not fire on other maps or
/// scripts.
#[test]
fn is_rival_trigger_only_matches_route_103s_own_rival_script() {
    assert!(is_rival_trigger(ROUTE_103, "Route103_EventScript_Rival"));
    assert!(
        !is_rival_trigger(MapId("MAP_LITTLEROOT_TOWN"), "Route103_EventScript_Rival"),
        "the same script name on a different map must not match"
    );
    assert!(
        !is_rival_trigger(ROUTE_103, "Route103_EventScript_Man"),
        "a different object event's script on Route 103 must not match"
    );
    assert!(
        !is_rival_trigger(ROUTE_103, "0x0"),
        "the no-script sentinel must not match"
    );
}

// -- The interaction trigger end to end (module docs, step 2) ---------------

/// Item (d) of the issue's own test list: facing the rival and pressing A
/// starts the trainer battle through the real [`OverworldPhase::step`]
/// path, consumes the frame correctly, and the hide flag is not yet set.
#[test]
fn facing_the_rival_and_pressing_a_starts_the_trainer_battle() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());

    assert!(!phase.is_rival_battle_active(), "setup: no battle yet");
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "an A press facing the rival must start the battle"
    );
    assert!(
        phase.party_lead.is_none(),
        "the lead moves into the battle, matching every other battle handoff"
    );
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(false),
        "the hide flag is a post-battle effect, not a trigger-time one"
    );
    assert_eq!(
        phase.rival_battle_outcome(),
        None,
        "the battle just started -- no outcome yet"
    );
}

/// Holding A through the frame after the battle starts must not re-run the
/// interaction lookup or start a second battle. Note what this does and
/// does not prove: a *held* A is not a fresh edge, so
/// [`OverworldPhase::interaction_tokens_this_frame`]'s `is_newly_pressed`
/// gate already discards it regardless of the battle -- the
/// battle-ownership short-circuit itself is pinned by
/// `input_during_the_rival_battle_neither_moves_the_player_nor_re_triggers`
/// below, which uses inputs that gate alone would otherwise act on.
#[test]
fn a_held_a_after_the_battle_starts_does_not_re_trigger_it() {
    let mut phase = route_103_phase_facing_the_rival();
    // Even levels on both sides (module docs' own
    // `one_turn_does_not_immediately_end_a_fresh_battle` precedent): the
    // point here is "no re-trigger", not "how long the fight lasts", so
    // the fixture must not itself end the battle on the very next turn.
    phase.party_lead = Some(lead(277, 5, 1));
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "setup: the fresh edge fired"
    );

    phase.step(held(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "the same battle instance must still be the one running"
    );
}

/// [`OverworldPhase::step`]'s battle-ownership check (its module docs: an
/// in-progress rival battle "owns the frame outright") really does
/// short-circuit everything below it --
/// `advance_route103_rival_battle_frame`'s place in the `||` chain, the
/// same coverage the sibling first-battle and wild-battle gates already
/// have. A held direction during the battle would otherwise turn or move
/// the player ([`super::input::held_direction`] runs on any ordinary
/// frame), and a fresh A edge would re-run the interaction lookup; with
/// the battle owning the frame, neither may observably happen.
#[test]
fn input_during_the_rival_battle_neither_moves_the_player_nor_re_triggers() {
    let mut phase = route_103_phase_facing_the_rival();
    // Even levels (the `one_turn_does_not_immediately_end_a_fresh_battle`
    // precedent): the battle must survive the frames this test steps.
    phase.party_lead = Some(lead(277, 5, 1));
    let position = phase.player.position();
    let facing = phase.player.facing();
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    // A held direction away from the current (East) facing: on an ordinary
    // frame this would at least turn the player in place.
    phase.step(held(Buttons::UP));
    assert!(phase.is_rival_battle_active());
    assert_eq!(
        phase.player.position(),
        position,
        "the battle owns the frame -- a held direction must not move the player"
    );
    assert_eq!(phase.player.facing(), facing, "nor even turn them in place");

    // A fresh A edge: on an ordinary frame this would re-run the
    // interaction lookup against the still-un-hidden rival.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "a fresh A edge during the battle must not reach the interaction lookup"
    );
    assert_eq!(phase.player.position(), position);
    assert_eq!(phase.player.facing(), facing);
}

/// Facing away from the rival must not start anything -- the ordinary
/// facing gate [`engine::overworld::facing_object_event`] already
/// enforces, unaffected by this trigger.
#[test]
fn facing_away_from_the_rival_does_not_start_the_battle() {
    let (rx, ry) = RIVAL_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx - 1, ry), 3, Direction::West));
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
    assert!(phase.party_lead.is_some(), "the lead must be untouched");
}

/// No party lead at all -- the same defensive `None` arm
/// [`OverworldPhase::begin_wild_battle`]/`begin_first_battle` document for a
/// bare test phase. Production always has one; this only matters here.
#[test]
fn the_trigger_with_no_party_lead_logs_and_starts_nothing() {
    let mut phase = route_103_phase_facing_the_rival();
    assert!(phase.party_lead.is_none(), "setup");
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
}

/// The trigger does not fire on other maps: a synthetic phase built on
/// Littleroot Town, with a fabricated lead, pressing A while facing empty
/// ground, must never start a rival battle -- and Mom's own dialog path
/// (already pinned in `overworld_phase::frame_tests`) is untouched
/// by this trigger's addition, since [`is_rival_trigger`] gates on
/// [`ROUTE_103`] before anything else runs.
#[test]
fn the_trigger_never_fires_off_route_103() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((4, 4), 3, Direction::South),
        None,
    );
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
}

// -- The headless driver and the win/loss decision (module docs) -----------

/// Item (e) of the issue's own test list: a concluded [`BattleOutcome::PlayerWon`]
/// battle retains its outcome, writes the lead back, sets
/// [`FLAG_HIDE_ROUTE_103_RIVAL`], and the rival is no longer
/// interactable/visible -- so the fight cannot be re-triggered.
#[test]
fn winning_the_rival_battle_hides_the_rival_and_makes_the_fight_unrepeatable() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    let outcome = play_out_rival_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    assert_eq!(phase.rival_battle_outcome(), Some(BattleOutcome::PlayerWon));
    assert!(
        !phase.is_rival_battle_active(),
        "a concluded battle empties its slot"
    );
    assert!(
        phase.party_lead.is_some(),
        "the driver writes the player's mon back"
    );
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true),
        "removeobject's real, ported effect (module docs)"
    );
    // `Cmd_getmoneyreward` (`pokeemerald/src/battle_script_commands.c:5641`)
    // credits the beaten trainer's own reward to the saved wallet -- here the
    // fresh save's default rival, a level-5 Torchic fought by a male player
    // who kept the default Treecko starter.
    let trainer = crate::flow::route103_rival::route103_rival_for(
        crate::flow::route103_rival::Rival::May,
        crate::flow::route103_rival::PlayerStarter::Treecko,
    );
    let reward = battle::trainer_money(battle::trainer_data(trainer).unwrap());
    assert_eq!(
        phase.save1().money,
        crate::new_game::STARTING_MONEY + reward,
        "a win must credit the trainer's prize money to the wallet (AddMoney)"
    );

    // The rival is no longer even *found* by the facing lookup, so a fresh
    // A press finds nothing to interact with -- the fight cannot restart.
    let lead_after = phase.party_lead.take();
    phase.party_lead = lead_after.clone();
    phase.step(pressed(Buttons::A));
    assert!(
        !phase.is_rival_battle_active(),
        "the hidden rival must not be found by a second A press"
    );
}

/// `AddMoney` (`pokeemerald/src/money.c:90-108`) saturates at `MAX_MONEY`
/// (`999999`) rather than wrapping or overshooting it -- pinned here by
/// starting a wallet close enough to the cap that the rival's own reward
/// would cross it.
#[test]
fn winning_the_rival_battle_saturates_money_at_the_upstream_cap() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.save1.money = 999_900;
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    let outcome = play_out_rival_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon), "setup: must win");
    assert_eq!(
        phase.save1().money,
        999_999,
        "a reward that would cross MAX_MONEY must clamp to it, not wrap or overshoot"
    );
}

/// **Replaces** the former `losing_the_rival_battle_does_not_hide_the_rival`
/// regression (issue #261, `route103_rival_trigger`'s module docs "The loss
/// decision" section): that test pinned the *interim* posture this crate
/// shipped before this issue -- a loss left a fainted lead standing right
/// next to a still-interactable rival, walled off from a rematch only by an
/// emergent `FaintedBattler` refusal one level down
/// ([`crate::flow::npc_trainer_battle::start_npc_trainer_battle`]). The real
/// upstream behaviour is reachable now
/// ([`super::white_out::OverworldPhase::white_out`]), so this pins that
/// instead: item (f) of the issue's own test list -- a
/// [`BattleOutcome::PlayerLost`] battle still concludes (the lead is written
/// back, same as before), the hide flag stays clear exactly as it does
/// upstream (`RivalEnd`'s branch is never reached on a loss, module docs),
/// but the write-back lead is healed and the player's money is halved in the
/// same frame, `DoWhiteOut`'s own ordering
/// (`pokeemerald/src/overworld.c:361-362`).
///
/// Pack-free by construction, same reasoning as
/// `crate::flow::wild_encounter::tests::a_lost_battle_now_heals_the_party_and_halves_money`:
/// every assertion is about state `white_out` writes *before* it attempts
/// the warp home, so it holds whether or not a local pack is extracted. The
/// warp landing itself (and thus whether the rival is still reachable at
/// all -- it is not, once the player is no longer standing on Route 103) is
/// pack-gated, pinned separately by
/// [`real_pack_losing_the_rival_battle_warps_home_to_the_default_heal_location`].
#[test]
fn losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overmatched_treecko_lead());
    phase.rng = engine::rng::Rng::new(2024);
    phase.save1.money = 2001;
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    let outcome = play_out_rival_battle(&mut phase, 64);
    assert_eq!(
        outcome,
        Some(BattleOutcome::PlayerLost),
        "a level 1 lead against a type-advantaged level 5 rival must lose"
    );
    assert_eq!(
        phase.rival_battle_outcome(),
        Some(BattleOutcome::PlayerLost)
    );
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(false),
        "a loss must not remove the rival -- upstream's `RivalEnd` branch is never reached \
         on a loss either"
    );
    assert_eq!(
        phase.save1().money,
        1000,
        "SetMoney(&gSaveBlock1Ptr->money, GetMoney(&gSaveBlock1Ptr->money) / 2) -- \
         2001 / 2 == 1000"
    );
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the driver writes the player's mon back, and white_out heals it in place");
    assert!(
        !lead.is_fainted(),
        "HealPlayerParty restores full HP -- a lost battle no longer leaves a fainted lead"
    );
    assert_eq!(lead.current_hp(), lead.stats().max_hp);
}

/// The trigger-time result-channel clear, carried forward from the tail of
/// the pre-#261 `losing_the_rival_battle_does_not_hide_the_rival` test that
/// [`losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`]
/// replaced: [`super::OverworldPhase::begin_route103_rival_battle`] clears
/// [`OverworldPhase::rival_battle_outcome`] the instant it fires, its own
/// doc comment's still-live "a new attempt owns its result channel from
/// trigger time onward" contract, so an in-progress battle can never report
/// an earlier attempt's terminal outcome.
///
/// The old test reached that assertion by re-triggering the fight after a
/// loss, which issue #261's white-out makes impossible (the player is warped
/// off Route 103 before another A press is possible). The stale outcome is
/// therefore seeded directly instead -- the same shape
/// `super::tests::an_aborted_first_battle_still_consumes_the_route_101_trigger`
/// already uses for `first_battle_outcome`'s identical contract on the
/// Route 101 side.
#[test]
fn beginning_a_rival_battle_clears_a_stale_outcome_from_an_earlier_attempt() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.rng = engine::rng::Rng::new(2024);
    // Terminal state from an earlier fight, sitting exactly where a fresh
    // attempt's own `None` belongs.
    phase.rival_battle_outcome = Some(BattleOutcome::PlayerLost);

    phase.step(pressed(Buttons::A));

    assert!(
        phase.is_rival_battle_active(),
        "setup: the trigger fired and built a battle"
    );
    assert_eq!(
        phase.rival_battle_outcome(),
        None,
        "`begin_route103_rival_battle` clears the stale `PlayerLost` at trigger time"
    );
}

/// The pack-gated companion to
/// [`losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`]:
/// the same loss must also land the player on the default heal location
/// (`crate::new_game::default_last_heal_location`) once `white_out`'s warp
/// actually resolves against a real map -- so a post-loss player is not
/// merely healed but genuinely off Route 103, the same displacement
/// upstream's own white-out produces. `#[ignore]`d like this crate's other
/// real-pack tests: run `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_losing_the_rival_battle_warps_home_to_the_default_heal_location() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overmatched_treecko_lead());
    phase.rng = engine::rng::Rng::new(2024);
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    let outcome = play_out_rival_battle(&mut phase, 64);
    assert_eq!(outcome, Some(BattleOutcome::PlayerLost), "setup: must lose");

    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
        "the white-out must land on the default heal location's own map, off Route 103 \
         entirely"
    );
    assert_eq!(phase.player.position(), (4, 2));
}

/// The driver never attempts to run -- the same headline requirement
/// `crate::flow::route103_rival::tests::the_driver_never_attempts_to_run`
/// pins at the construction layer, checked here at the trigger layer: one
/// turn against a fresh battle must not immediately end it via a refused
/// `Run`.
#[test]
fn one_turn_does_not_immediately_end_a_fresh_battle() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(lead(277, 5, 1)); // a level-5 Treecko with Pound
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active());

    phase.step(ButtonState::new());
    assert!(
        phase.is_rival_battle_active(),
        "one ordinary turn must not end an even-level fight outright"
    );
}

// -- The real `VAR_STARTER_MON` -> `PlayerStarter` derivation (issue #251) --

/// [`begin_route103_rival_battle`] refuses to start a battle when
/// `VAR_STARTER_MON` reads a value with no `PlayerStarter` mapping (module
/// docs) -- logged and no-op, not a panic. A fresh phase's `VAR_STARTER_MON`
/// defaults to `0` (Treecko), so this needs an explicit out-of-range write
/// to exercise -- unlike the species-derived predecessor this replaces, the
/// lead's own species no longer matters to this check at all (a real
/// Treecko lead is used here precisely to prove that).
#[test]
fn an_out_of_range_starter_var_starts_no_battle() {
    let mut phase = route_103_phase_facing_the_rival();
    phase
        .save1
        .event_data
        .var_set(VAR_STARTER_MON, 3)
        .expect("VAR_STARTER_MON is an ordinary var id");
    phase.party_lead = Some(lead(277, 5, 1));
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
    assert!(
        phase.party_lead.is_some(),
        "a refused trigger must not consume the lead"
    );
}

/// **Test-ratchet replacement.** Before issue #251,
/// `a_fainted_lead_cannot_start_the_rival_battle_and_draws_nothing` drove a
/// *directly written* fainted lead onto the rival tile and proved the
/// fail-closed screen refused it without drawing -- pinning the screen
/// `begin_route103_rival_battle` carried solely because a lost Route 101
/// first battle could leave a fainted lead standing in the overworld with
/// nothing to heal it (`CB2_EndFirstBattle` has no `IsPlayerDefeated`
/// branch).
///
/// `overworld_phase::first_battle_conclusion::OverworldPhase::conclude_first_battle`
/// (issue #251) now heals that lead, and writes the real `VAR_STARTER_MON`,
/// the instant the Route 101 battle ends -- so the state the screen existed
/// to refuse can no longer reach Route 103 at all, and the screen has been
/// removed (`super`'s own module docs, "The loss decision"). This test
/// proves the *strictly stronger* claim the deleted one could not: not "a
/// fainted lead is refused here," but "a lead reaching here from a real
/// lost first battle is never fainted, and the rival it fights is the one
/// `VAR_STARTER_MON` -- not the lead's incidental species -- actually
/// names."
///
/// Drives the Route 101 loss through the real trigger/driver on its own
/// synthetic `MAP_ROUTE101` phase (the same crafted level-1, zero-Defense-IV
/// Treecko and seed `1` as
/// `crate::flow::wild_encounter::tests::a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted`
/// -- that test's own doc comment has the full derivation), then carries
/// the concluded lead and `VAR_STARTER_MON` over onto this file's own
/// `MAP_ROUTE103` fixture -- the same "each test file owns its own map
/// fixture" split this whole suite already keeps, since no single synthetic
/// room can be both real maps at once.
#[test]
fn a_lost_route_101_first_battle_still_lets_the_healed_lead_fight_the_rival() {
    use engine::rng::Rng;

    const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);
    const ROUTE_101_TRIGGER_ELEVATION: u8 = 3;

    let mut route_101 = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        PlayerState::new(
            (ROUTE_101_TRIGGER_TILE.0 - 1, ROUTE_101_TRIGGER_TILE.1),
            ROUTE_101_TRIGGER_ELEVATION,
            Direction::East,
        ),
        None,
    );
    route_101.rng = Rng::new(1);
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: 0,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    route_101.party_lead = Some(
        BattlePokemon::new(&Dex::new(), SpeciesId(277), 1, ivs, 0, vec![MoveId(1)])
            .expect("Treecko/Pound must be in the dex"),
    );

    for _ in 0..engine::overworld::WALK_FRAMES_PER_TILE {
        route_101.step(held(Buttons::RIGHT));
    }
    assert!(
        route_101.first_battle.is_some(),
        "setup: the rescue trigger must fire"
    );
    let mut frames = 0;
    while route_101.first_battle.is_some() {
        route_101.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 20, "setup: the crafted loss must resolve quickly");
    }
    assert_eq!(
        route_101.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: this seed/lead combination must really lose"
    );
    let healed_lead = route_101
        .party_lead
        .clone()
        .expect("conclude_first_battle heals in place");
    assert!(
        !healed_lead.is_fainted(),
        "setup: the conclusion must have healed the lead"
    );
    let starter_var = route_101
        .save1
        .event_data
        .var_get(VAR_STARTER_MON)
        .expect("VAR_STARTER_MON is an ordinary var id");
    assert_eq!(
        starter_var, 0,
        "setup: the conclusion wrote Treecko's own encoding"
    );

    // Carry the concluded state over onto this file's own Route 103
    // fixture -- the healed lead and the real VAR_STARTER_MON the
    // conclusion wrote, nothing else.
    let mut route_103 = route_103_phase_facing_the_rival();
    route_103.party_lead = Some(healed_lead);
    route_103
        .save1
        .event_data
        .var_set(VAR_STARTER_MON, starter_var)
        .expect("VAR_STARTER_MON is an ordinary var id");

    let before = route_103.rng.state();
    route_103.step(pressed(Buttons::A));
    assert!(
        route_103.is_rival_battle_active(),
        "a lead healed by a real conclusion must start the rival battle -- no fail-closed \
         screen stands in the way any more"
    );
    assert_ne!(
        route_103.rng.state(),
        before,
        "the party build really drew -- nothing refused it before it could"
    );
    assert!(
        route_103.party_lead.is_none(),
        "the lead moved into the battle, same as any other successful trigger"
    );

    // And the battle plays to a real terminal outcome -- the healed lead is
    // not merely accepted, it can actually fight.
    let outcome = play_out_rival_battle(&mut route_103, 50);
    assert!(
        outcome.is_some(),
        "the rival battle carried from a healed post-conclusion lead must reach a real \
         terminal outcome, not stall"
    );
}

/// [`Rival::for_gender`]'s own `None` arm reaches all the way through
/// [`OverworldPhase::begin_route103_rival_battle`]: an unmodelled gender
/// starts no battle either.
#[test]
fn an_unmodelled_player_gender_starts_no_battle() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.save2.player_gender = PlayerGender::Other(3);
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
    assert!(phase.party_lead.is_some());
}

// -- Real-pack reachability --------------------------------------------------

/// Item (a) of the issue's own test list: walking off Route 101's own
/// north edge crosses into Oldale Town, and walking off Oldale's own north
/// edge in turn crosses into Route 103 -- completing the chain
/// `walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
/// (`crate::flow::overworld_phase::connections_tests`) already proves the
/// first leg of.
///
/// Starts one ordinary tile south of Route 101's own north edge rather than
/// walking that map's whole interior: Route 101's own `x = 10`/`11` column
/// (the one the Littleroot crossing above lands on) runs into real,
/// unrelated collision around `y = 6`/`13` (Birch/rescue-scene set
/// dressing this map carries -- checked against the real extracted grid,
/// not assumed), so a single held-direction walk the length of the map
/// cannot reach the edge. That is Route 101's own pre-existing layout, no
/// part of this issue's own scope; starting adjacent to each edge (the
/// same convention `route_103_phase_facing_the_rival`, and this file's own
/// sibling `first_battle_trigger` tests' `route_101_trigger_phase`, already
/// use) still exercises the real crossing math and the real grid data at
/// both new boundaries, which is this test's actual subject. Oldale's own
/// `x = 10` column *is* fully walkable end to end (also checked against
/// the real grid), so that whole leg is walked for real.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_north_from_route_101_crosses_oldale_town_into_route_103() {
    let route101 = MapId("MAP_ROUTE101");
    let oldale = MapId("MAP_OLDALE_TOWN");
    let scene = crate::overworld::load_room(
        route101,
        crate::overworld::PlayerCharacter::Brendan,
        &EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // One ordinary tile south of Route 101's own north edge (module docs).
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, route101, player, None);

    let walk_north_one_tile = |phase: &mut OverworldPhase| {
        phase.step(held(Buttons::UP));
        for _ in 1..engine::overworld::WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // One ordinary step north lands on the map's own last interior row.
    walk_north_one_tile(&mut phase);
    assert_eq!(phase.map_id, route101, "still on Route 101's own grid");
    assert_eq!(phase.player.position(), (10, 0));

    // The second step crosses Route 101's own north edge into Oldale Town.
    walk_north_one_tile(&mut phase);
    assert_eq!(
        phase.map_id, oldale,
        "walking off Route 101's north edge must cross into Oldale Town"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "offset 0 carries x straight across, landing on Oldale's own south edge \
         (height 20, so y = 19)"
    );

    // 19 more steps walk the length of Oldale's own column (y: 19 -> 0),
    // for real -- this column has no interior collision.
    for _ in 0..19 {
        walk_north_one_tile(&mut phase);
        assert_eq!(phase.map_id, oldale, "still on Oldale's own grid");
    }
    assert_eq!(phase.player.position(), (10, 0));

    // A stale temp flag "from the departed map": `ClearTempFieldEventData`
    // (`overworld.c:798`, `LoadMapFromCameraTransition`) runs on a
    // connection crossing exactly like on a warp, so Route 103's
    // cuttable-tree flags (`FLAG_TEMP_12`/`_13`,
    // `assets::object_event_flags`) can never arrive pre-set and keep a
    // tree hidden. `0x12` is `FLAG_TEMP_12`.
    phase.save1.event_data.flag_set(0x12).unwrap();

    // The final step crosses Oldale's own north edge into Route 103 -- I-5's
    // own traversal target.
    walk_north_one_tile(&mut phase);
    assert_eq!(
        phase.map_id, ROUTE_103,
        "walking off Oldale's north edge must cross into Route 103"
    );
    assert_eq!(
        phase.player.position(),
        (10, 21),
        "offset 0 carries x straight across, landing on Route 103's own south edge \
         (height 22, so y = 21)"
    );
    assert_eq!(phase.player.elevation(), 3);
    assert!(!phase.player.in_transit());
    // The temp flag set on Oldale's grid above did not survive the
    // crossing's map load.
    assert_eq!(
        phase.save1().event_data.flag_get(0x12),
        Ok(false),
        "a connection crossing is a map load -- `ClearTempFieldEventData`'s port must \
         clear the temp flag range before the entered map's transition effects run"
    );
    // The crossing also ran this port's on-transition effects for Route
    // 103, so the rival's own gfx var is already primed on arrival.
    assert_eq!(
        phase.save1().event_data.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a fresh (default-gender) phase's rival is May the instant the crossing lands"
    );
    // ... and `cross_connection` decoded the arrival scene against the
    // *transitioned* store, not the pre-transition one -- the ordering
    // `warp_to`/`cross_connection` (`super::connections`) were structured
    // around. `OBJ_EVENT_GFX_VAR_0` binds a sprite only when the var
    // already held a rival id at decode time, so this is the one
    // production-path probe that the rival is actually drawable the instant
    // the player walks in, not just that the var got written.
    assert!(
        phase.scene.binds_sprite("OBJ_EVENT_GFX_VAR_0"),
        "the post-crossing rebind must decode against the transitioned event data -- \
         the rival binds a sprite the instant the crossing lands"
    );
}

// The rendered *contents* of the `OBJ_EVENT_GFX_VAR_0` binding -- which
// sheet and palette bank the rival draws from -- are pinned in
// `crate::overworld::tests::real_pack_route_103_rival_binds_to_the_opposite_protagonists_sheet`,
// not here: `OverworldScene`'s sprite/OAM internals are private to the
// `overworld` module tree, and `flow::overworld_phase` sees only the
// yes/no `OverworldScene::binds_sprite` probe the crossing walk above
// uses.
