//! Route 103 rival battle-outcome driver seam: win/loss decisions, starter and gender selection, and the Route 101 handoff.

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::{Direction, PlayerState};
use engine::save::PlayerGender;
use platform::{ButtonState, Buttons};

use crate::flow::tests::{held, pressed};

use super::route103_rival_test_support::{
    lead, overmatched_treecko_lead, overwhelming_treecko_lead, play_out_rival_battle,
    route_103_phase_facing_the_rival, FLAG_HIDE_ROUTE_103_RIVAL, TRAINER_FLAGS_START,
    VAR_STARTER_MON,
};
use super::OverworldPhase;

// -- The headless driver and the win/loss decision (module docs) -----------

/// Item (e) of the issue's own test list: a concluded [`BattleOutcome::PlayerWon`]
/// battle retains its outcome, writes the lead back, sets
/// [`FLAG_HIDE_ROUTE_103_RIVAL`] and the fought trainer's
/// [`TRAINER_FLAGS_START`] defeated flag, and the rival is no longer
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
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + trainer.0),
        Ok(true),
        "SetBattledTrainersFlags' real effect (src/battle_setup.c:1245-1250) -- a won trainer \
         battle must set the fought trainer's own defeated flag, not just the rival's bespoke \
         hide flag (issue #843)"
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

/// [`route103_rival::route103_rival_for`]'s full six-entry table, not just the
/// fresh-save default: whichever
/// of the six `TRAINER_*_ROUTE_103_*` ids a playthrough actually fights, a
/// win must set that exact trainer's own `TRAINER_FLAGS_START + id` flag
/// (`SetBattledTrainersFlags`, `src/battle_setup.c:1245-1250`) -- the same
/// contract [`winning_the_rival_battle_hides_the_rival_and_makes_the_fight_unrepeatable`]
/// pins for the default id alone.
#[test]
fn winning_the_rival_battle_sets_the_exact_fought_trainers_defeated_flag_for_every_starter_and_gender(
) {
    use crate::flow::route103_rival::{route103_rival_for, PlayerStarter, Rival};

    let combinations = [
        (PlayerGender::Male, 0, PlayerStarter::Treecko),
        (PlayerGender::Male, 1, PlayerStarter::Torchic),
        (PlayerGender::Male, 2, PlayerStarter::Mudkip),
        (PlayerGender::Female, 0, PlayerStarter::Treecko),
        (PlayerGender::Female, 1, PlayerStarter::Torchic),
        (PlayerGender::Female, 2, PlayerStarter::Mudkip),
    ];
    for (gender, starter_var, starter) in combinations {
        let mut phase = route_103_phase_facing_the_rival();
        phase.save2.player_gender = gender;
        phase
            .save1
            .event_data
            .var_set(VAR_STARTER_MON, starter_var)
            .expect("VAR_STARTER_MON is an ordinary var id");
        phase.party_lead = Some(overwhelming_treecko_lead());
        phase.step(pressed(Buttons::A));
        assert!(
            phase.is_rival_battle_active(),
            "setup: the battle started for {gender:?}/starter {starter_var}"
        );

        let outcome = play_out_rival_battle(&mut phase, 32);
        assert_eq!(
            outcome,
            Some(BattleOutcome::PlayerWon),
            "setup: must win for {gender:?}/starter {starter_var}"
        );

        let rival = Rival::for_gender(gender).expect("setup: Male/Female always has a rival");
        let trainer = route103_rival_for(rival, starter);
        assert_eq!(
            phase
                .save1()
                .event_data
                .flag_get(TRAINER_FLAGS_START + trainer.0),
            Ok(true),
            "trainer {trainer:?}'s own defeated flag must be set for {gender:?}/starter \
             {starter_var}"
        );
    }
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
    // `SetBattledTrainersFlags` sits in `CB2_EndTrainerBattle`'s non-defeat
    // branch, the same branch `RivalEnd`'s own effect is gated behind, so a
    // loss must not set it either (issue #843).
    let trainer = crate::flow::route103_rival::route103_rival_for(
        crate::flow::route103_rival::Rival::May,
        crate::flow::route103_rival::PlayerStarter::Treecko,
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + trainer.0),
        Ok(false),
        "a loss must not set the fought trainer's generic defeated flag either"
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
        BattlePokemon::new(&Dex::new(), SpeciesId(277), 1, ivs, 0, vec![MoveId::POUND])
            .expect("Treecko/Pound must be in the dex"),
    );

    for _ in 0..engine::overworld::WALK_FRAMES_PER_TILE {
        route_101.step(held(Buttons::RIGHT));
    }
    // The completed step is observed on the call after the walk animation
    // drains (`OverworldPhase::step`'s "Frame shape" docs, issue #1039).
    route_101.step(held(Buttons::RIGHT));
    assert!(
        route_101.is_first_battle_active(),
        "setup: the rescue trigger must fire"
    );
    let mut frames = 0;
    while route_101.is_first_battle_active() {
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
