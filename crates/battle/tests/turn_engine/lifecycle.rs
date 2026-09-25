//! Battle lifecycle, terminal outcomes, and experience-award behavior.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, PlayerAction, MAX_LEVEL, STRUGGLE,
};

/// The friendliest possible outcome for whichever check a draw feeds: a
/// hit, a critical, or the maximum damage roll.
const MAX_ROLL: u16 = 0;
/// A roll every modeled critical-hit stage treats as non-critical.
const NO_CRIT_ROLL: u16 = 1;

const CHARMANDER: u16 = 4;
const RATTATA: u16 = 19;
const BULBASAUR: u16 = 1;
const TACKLE: MoveId = MoveId(33);

#[test]
fn take_turn_after_the_battle_ended_is_an_error() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    let mut rng = SequenceRng::new([
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(battle.outcome().is_some());
    assert_eq!(rng.draws(), 7);
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
    assert!(
        rejected.events().is_empty(),
        "a call rejected before the turn began has no events to report"
    );
    assert_eq!(rng.draws(), 7, "an already-over battle draws nothing");
}

#[test]
fn full_wild_battle_runs_to_a_faint_and_reports_victory() {
    const FULL_TURN: [u16; 10] = [
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
    ];
    const FINAL_TURN: [u16; 6] = [
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
    ];

    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(
        std::iter::once(MAX_ROLL)
            .chain(FULL_TURN)
            .chain(FULL_TURN)
            .chain(FINAL_TURN),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let mut turns = 0;
    let mut won = false;
    for _ in 0..20 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        turns += 1;
        if let Some(BattleEvent::Ended(outcome)) = events.last() {
            assert_eq!(*outcome, BattleOutcome::PlayerWon);
            won = true;
            break;
        }
    }
    assert!(won, "battle did not conclude within 20 turns");
    assert_eq!(turns, 3);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(battle.player().current_hp(), 9);
    assert_eq!(rng.draws(), 27);
}

#[test]
fn losing_the_battle_reports_defeat_and_awards_no_exp() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let player_max_hp = player.stats().max_hp;

    let mut rng = SequenceRng::new([
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "defeat reports the faint and the loss -- and no ExpGained: \
         exp is the winner's, and the player lost"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(
        battle.player().moves()[0].pp,
        35,
        "the fainted player's queued move never executed, so no PP moved"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn a_max_level_player_gains_no_exp_and_no_exp_event_on_victory() {
    let dex = Dex::new();
    // Cmd_getexp skips both the "gained EXP" message and MonGainEVs for a
    // MAX_LEVEL recipient (battle_script_commands.c:3351-3356).
    let player = max_iv_mon(&dex, CHARMANDER, MAX_LEVEL, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    let mut rng = SequenceRng::new([
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        MAX_ROLL,
        NO_CRIT_ROLL,
        MAX_ROLL,
        MAX_ROLL,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the win itself is unchanged: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::ExpGained(_))),
        "a MAX_LEVEL player gains no exp and sees no exp event: {events:?}"
    );
    assert_eq!(
        battle.player().evs(),
        battle::Evs::default(),
        "a MAX_LEVEL recipient is excluded from `MonGainEVs` \
         too -- the same `Cmd_getexp` case 2 guard that zeroes the exp \
         award skips the whole body, `gain_evs` included"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn a_fainted_battler_is_rejected_before_the_battle_start_draw() {
    let dex = Dex::new();
    let mut fainted = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    fainted.apply_damage(fainted.stats().max_hp);
    let healthy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(
            dex.clone(),
            fainted.clone(),
            healthy.clone(),
            false,
            &mut rng
        )
        .unwrap_err(),
        BattleError::FaintedBattler(true)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, healthy, fainted, false, &mut rng).unwrap_err(),
        BattleError::FaintedBattler(false)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");
}

/// `HandleFaintedMonActions` runs the pending level-up prompt to completion
/// before the deferred `Ended` event follows (`battle_util.c:1894-1951`).
#[test]
fn a_wild_knockouts_prompt_defers_the_battles_end_until_it_is_answered() {
    use battle::MoveLearnDecision;

    const TORCHIC: u16 = 280;
    const SCRATCH: MoveId = MoveId(10);
    const GROWL: MoveId = MoveId(45);
    const LEER: MoveId = MoveId(43);
    const PECK: MoveId = MoveId(64);
    const PECK_LEARN_LEVEL: u8 = 16;
    const EXPERIENCE_SHORT_OF_PECK_LEVEL: u32 = 1;

    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    let growth_rate = dex.species(player.species()).unwrap().growth_rate;
    let peck_level_experience =
        assets::experience_for_level(growth_rate, PECK_LEARN_LEVEL).unwrap();
    assert!(player
        .apply_experience(
            &dex,
            peck_level_experience - EXPERIENCE_SHORT_OF_PECK_LEVEL - player.experience()
        )
        .unwrap()
        .is_none());
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    let mut rng = SequenceRng::new([MAX_ROLL; 7]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::MoveLearnPrompt { move_id: PECK }),
        "the one-hit knockout's award must ask about Peck: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Ended(_))),
        "the battle's end waits on the answer: {events:?}"
    );
    assert_eq!(
        battle.outcome(),
        None,
        "no outcome while the question is open"
    );
    assert_eq!(battle.player().level(), PECK_LEARN_LEVEL);

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `Cmd_checkteamslost` only declares the loss once the player's whole-party
/// HP total is zero (`src/battle_script_commands.c:3534`-`:3564`); with a
/// reserve still standing, `BattleScript_HandleFaintedMon` reaches its
/// send-out branch instead (`data/battle_scripts_1.s:2830`-`:2896`).
#[test]
fn an_active_faint_with_a_healthy_reserve_sends_it_out_instead_of_ending_the_battle() {
    let dex = Dex::new();
    // Slow L5 Rattata against a fast L50 Charmander: the enemy overkills
    // the active before it can act, exactly like
    // `losing_the_battle_reports_defeat_and_awards_no_exp` -- but this
    // time a healthy L5 Squirtle reserve is waiting behind it.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let reserve = max_iv_mon(&dex, 7, 5, vec![MoveId(33)]);
    let reserve_max_hp = reserve.stats().max_hp;
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;

    // battle start, turn number, enemy pick, enemy hit (accuracy / no
    // crit / best roll / effect chance) -- identical to the immediate-loss
    // fixture; a stray draw after that panics the exhausted script.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId(7),
                reserves_remaining: 0,
            },
        ],
        "a healthy reserve replaces the fainted active instead of an \
         Ended event: {events:?}"
    );
    assert_eq!(
        battle.outcome(),
        None,
        "the player's party still has a standing member"
    );
    assert_eq!(battle.player().species(), SpeciesId(7));
    assert_eq!(
        battle.player().current_hp(),
        reserve_max_hp,
        "the reserve enters untouched by the active's faint"
    );
    assert_eq!(rng.draws(), 7);
}

/// Upstream leaves every `gPlayerParty` record in its slot through a
/// replacement and only repoints `gBattlerPartyIndexes`
/// (`src/battle_script_commands.c:4613`), so a caller must be able to
/// persist each reported member onto the slot it came from even when two
/// members are indistinguishable by species, personality, and original
/// trainer id -- the only thing left to tell them apart is position.
#[test]
fn identical_identity_members_report_in_their_original_party_order_after_a_replacement() {
    let dex = Dex::new();
    // Same species, personality, and OT id: only the slot tells these two
    // Rattata apart. The first is overkilled by the L50 Charmander before
    // it can act; the second enters untouched.
    let player = max_iv_mon_with_personality(&dex, 19, 5, vec![MoveId(33)], 7)
        .with_original_trainer_id(12_345);
    let reserve = max_iv_mon_with_personality(&dex, 19, 5, vec![MoveId(33)], 7)
        .with_original_trainer_id(12_345);
    let reserve_max_hp = reserve.stats().max_hp;
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::PlayerSentOut {
            species: SpeciesId(19),
            reserves_remaining: 0,
        }),
        "fixture sanity: the second Rattata replaced the first: {events:?}"
    );
    assert_eq!(battle.player().current_hp(), reserve_max_hp);

    let members: Vec<_> = battle.player_members().collect();
    assert_eq!(members.len(), 2);
    assert_eq!(
        members[0].current_hp(),
        0,
        "slot 0 is the Rattata that started active and fainted"
    );
    assert_eq!(
        members[1].current_hp(),
        reserve_max_hp,
        "slot 1 is the Rattata that was sent out and still stands"
    );
    assert_eq!(rng.draws(), 7);
}

/// Two replacements in a row move the active member off party position 0,
/// so the second send-out must return each fainted member to its own slot
/// from the middle of the party, not just from the front.
#[test]
fn successive_replacements_keep_every_member_in_its_original_party_order() {
    let dex = Dex::new();
    // Three L5 Rattata told apart only by personality, each overkilled in
    // turn by the fast L50 Charmander.
    let player = max_iv_mon_with_personality(&dex, 19, 5, vec![MoveId(33)], 1);
    let second = max_iv_mon_with_personality(&dex, 19, 5, vec![MoveId(33)], 2);
    let third = max_iv_mon_with_personality(&dex, 19, 5, vec![MoveId(33)], 3);
    let third_max_hp = third.stats().max_hp;
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);

    // Turn 1: battle start, turn number, enemy pick, enemy hit. Turn 2:
    // turn number, enemy pick, enemy hit.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![second, third], enemy, false, &mut rng)
            .unwrap();
    for turn in 1..=2 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, BattleEvent::PlayerSentOut { .. })),
            "fixture sanity: turn {turn} ends in a replacement: {events:?}"
        );
    }
    assert_eq!(battle.outcome(), None);
    assert_eq!(battle.player().personality(), 3);

    let members: Vec<_> = battle.player_members().collect();
    let personalities: Vec<_> = members.iter().map(|m| m.personality()).collect();
    assert_eq!(personalities, vec![1, 2, 3], "the caller's order survives");
    let hp: Vec<_> = members.iter().map(|m| m.current_hp()).collect();
    assert_eq!(hp, vec![0, 0, third_max_hp]);
    assert_eq!(rng.draws(), 13);
}

/// Unlike the test above, every player member here is already fainted when
/// the active goes down, so `Cmd_checkteamslost`'s whole-party HP total is
/// zero and `Battle::player_members` must still report both members' final
/// identity and state (`src/battle_script_commands.c:3534`-`:3564`).
#[test]
fn a_fainted_player_with_no_usable_reserve_still_loses_and_reports_both_members() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut reserve = max_iv_mon_with_personality(&dex, 7, 5, vec![MoveId(33)], 5);
    reserve.apply_damage(reserve.stats().max_hp);
    assert!(
        reserve.is_fainted(),
        "fixture sanity: the reserve is dead on arrival"
    );
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "no usable reserve still ends the battle: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));

    let members: Vec<_> = battle.player_members().collect();
    assert_eq!(members.len(), 2, "both party members are reported");
    assert_eq!(members[0].species(), SpeciesId(19));
    assert_eq!(
        members[0].current_hp(),
        0,
        "the active's faint is final state"
    );
    assert_eq!(members[1].species(), SpeciesId(7));
    assert_eq!(members[1].personality(), 5);
    assert_eq!(
        members[1].current_hp(),
        0,
        "the reserve's pre-battle faint survives to the final report too"
    );
    assert_eq!(rng.draws(), 7);
}

/// A Struggle-recoil faint (#1262) has no code path of its own: recoil
/// settles through the same `Battle::settle_faint` as any other faint, so
/// it reaches the same reserve-or-exhaustion decision in
/// `Battle::handle_fainted_mons`.
#[test]
fn a_struggle_recoil_faint_takes_the_same_reserve_or_exhaustion_decision() {
    let dex = Dex::new();
    // Rattata (speed 13) outruns Bulbasaur (speed 11), so the player's
    // forced Struggle resolves first. Depleting its only move's PP forces
    // Struggle at selection (`AreAllMovesUnusable`); starting at 1 HP
    // guarantees the certain quarter-damage recoil (floored to a minimum
    // of 1) faints it, regardless of the exact damage Struggle deals.
    let mut player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    for _ in 0..player.moves()[0].pp {
        player.deduct_pp(0).unwrap();
    }
    assert_eq!(player.moves()[0].pp, 0, "fixture sanity: Tackle is spent");
    player.apply_damage(player.stats().max_hp - 1);
    let reserve = max_iv_mon(&dex, 7, 5, vec![MoveId(33)]);
    let reserve_max_hp = reserve.stats().max_hp;
    let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]);

    // battle start, turn number, enemy's Tackle pick (not forced, so it
    // draws a slot), then the player's forced Struggle: accuracy, crit,
    // damage roll -- no effect-chance draw and no selection draw for a
    // forced pick (`a_forced_struggle_follows_the_first_movers_hit_in_the_same_turn`).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                move_id: STRUGGLE,
                ..
            }
        )),
        "the forced Struggle must land: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Recoil {
            by_player: true,
            move_id: STRUGGLE,
            damage: 1,
        }),
        "the player's own HP bar (1) caps the certain recoil: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, BattleEvent::Fainted { by_player: true }))
            .count(),
        1,
        "the recoil faints the player exactly once: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::PlayerSentOut {
            species: SpeciesId(7),
            reserves_remaining: 0,
        }),
        "the reserve replaces the recoil-fainted active: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, BattleEvent::Ended(_))),
        "a standing reserve keeps the battle open: {events:?}"
    );
    assert_eq!(battle.outcome(), None);
    assert_eq!(battle.player().species(), SpeciesId(7));
    assert_eq!(battle.player().current_hp(), reserve_max_hp);
}

/// A healthy reserve keeps the player's whole-party HP total above zero, so
/// a simultaneous double faint clears `Cmd_checkteamslost`'s LOST bit while
/// still setting WON: `BattleScript_HandleFaintedMon` skips straight past
/// its send-out branch once the outcome is decided
/// (`src/battle_script_commands.c:3560`-`:3577`,
/// `data/battle_scripts_1.s:2830`-`:2832`). This is the same fixture as
/// `pipelines::a_liquid_ooze_kill_faints_the_attacker_before_the_target`,
/// with a reserve added.
#[test]
fn a_simultaneous_double_faint_with_a_healthy_reserve_still_wins() {
    const ABSORB: MoveId = MoveId(71);
    const BULBASAUR: u16 = 1;
    const TENTACOOL: u16 = 72;

    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    // Leave the attacker on less HP than the 6 the ooze will take, so
    // Absorb's own drain-turned-damage faints it in the same exchange that
    // its hit faints the level-5 Tentacool target.
    player.apply_damage(player_max_hp - 6);
    let reserve = max_iv_mon(&dex, 7, 5, vec![MoveId(33)]);
    let reserve_max_hp = reserve.stats().max_hp;
    // Personality 1 lands Tentacool on ability slot 1, Liquid Ooze.
    let enemy = max_iv_mon_with_personality(&dex, TENTACOOL, 5, vec![MoveId(33)], 1);

    // battle start, turn number, enemy pick, Absorb's hit (accuracy / no
    // crit / best roll / effect chance).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, BattleEvent::Fainted { .. }))
            .count(),
        2,
        "both battlers faint in the same exchange: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::PlayerSentOut { .. })),
        "the enemy's own exhaustion decides the battle before any \
         player-side send-out runs: {events:?}"
    );
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerWon)),
        "a standing reserve keeps the player's party from exhaustion, so \
         the enemy's own defeat wins the battle outright: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(
        battle.player().species(),
        SpeciesId(BULBASAUR),
        "no replacement occurred: the battle ended before reaching the \
         player's own send-out branch"
    );
    assert_eq!(battle.player().current_hp(), 0);
    let members: Vec<_> = battle.player_members().collect();
    assert_eq!(members[1].species(), SpeciesId(7));
    assert_eq!(
        members[1].current_hp(),
        reserve_max_hp,
        "the reserve never entered, so it is untouched"
    );
}

/// `Cmd_getexp` gates both the EV/message step and the exp-application step
/// on the recipient's own HP (`src/battle_script_commands.c:3367`,
/// `:3431`): a party member at zero HP is skipped, so a player that
/// fainted in the same exchange that felled the enemy gains neither exp nor
/// EVs and prints no "gained EXP" string. Same fixture as
/// `a_simultaneous_double_faint_with_a_healthy_reserve_still_wins`.
#[test]
fn a_fainted_player_gains_no_exp_or_evs_from_a_simultaneous_double_faint() {
    const ABSORB: MoveId = MoveId(71);
    const BULBASAUR: u16 = 1;
    const TENTACOOL: u16 = 72;
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    player.apply_damage(player_max_hp - 6);
    let experience_before = player.experience();
    let evs_before = player.evs();
    let reserve = max_iv_mon(&dex, 7, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon_with_personality(&dex, TENTACOOL, 5, vec![MoveId(33)], 1);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(battle.player().current_hp(), 0, "the player fainted");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::ExpGained(_))),
        "a zero-HP recipient is skipped before the exp string: {events:?}"
    );
    assert_eq!(
        battle.player().experience(),
        experience_before,
        "a fainted recipient gains no experience"
    );
    assert_eq!(
        battle.player().evs(),
        evs_before,
        "`MonGainEVs` is inside the HP-guarded branch"
    );
}
