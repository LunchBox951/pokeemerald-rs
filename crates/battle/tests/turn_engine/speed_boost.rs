//! Speed Boost's end-turn Speed stage through real turns: the
//! `ABILITYEFFECT_ENDTURN` case (`src/battle_util.c:2642-2652`) and the
//! `isFirstTurn` counter that gates it (`src/battle_main.c:3051,3220,3312,4875`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::stat_stage::StatStage;
use battle::status1::poison_residual_damage;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, MoveLearnDecision, PlayerAction, Status1};

const SPLASH: MoveId = MoveId::SPLASH;
const TACKLE: MoveId = MoveId::TACKLE;
const FOCUS_ENERGY: MoveId = MoveId::FOCUS_ENERGY;
const LEER: MoveId = MoveId::LEER;

/// Speed Boost in its only ability slot.
const NINJASK: u16 = 302;
/// Speed Boost in its first ability slot; the Speed Boost holder with the
/// lowest Speed here.
const YANMA: u16 = 193;
/// Faster than a level-10 [`NINJASK`] before any boost, slower after one.
const ZIGZAGOON: u16 = 288;
const MEOWTH: u16 = 52;
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

fn leave_one_hp(mon: &mut battle::BattlePokemon) {
    mon.apply_damage(mon.current_hp() - 1);
}

fn speed_boost_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::SpeedBoostActivated { .. }))
        .collect()
}

fn boosted(by_player: bool, stage: i8) -> BattleEvent {
    BattleEvent::SpeedBoostActivated {
        by_player,
        new_stage: StatStage::new(stage).unwrap(),
    }
}

fn splash_event(by_player: bool) -> BattleEvent {
    BattleEvent::NothingHappened {
        by_player,
        move_id: SPLASH,
    }
}

/// A level-10 Ninjask and a level-30 Zigzagoon, both knowing only Splash.
fn ninjask_versus_faster_zigzagoon(ninjask_is_player: bool, rng: &mut SequenceRng) -> Battle {
    let dex = Dex::new();
    let ninjask = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let other = max_iv_mon(&dex, ZIGZAGOON, 30, vec![SPLASH]);
    assert_eq!(ninjask.ability(), AbilityId::SPEED_BOOST);
    assert!(ninjask.stats().speed < other.stats().speed);
    let (player, enemy) = if ninjask_is_player {
        (ninjask, other)
    } else {
        (other, ninjask)
    };
    Battle::new(dex, player, enemy, false, rng).unwrap()
}

#[test]
fn an_initial_lead_boosts_at_the_end_of_turn_one_on_either_side_and_wins_the_next_order() {
    for ninjask_is_player in [true, false] {
        // Construction seed, then per turn the turn-start and wild move picks.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
        let mut battle = ninjask_versus_faster_zigzagoon(ninjask_is_player, &mut rng);

        let first = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        let ninjask_acts = splash_event(ninjask_is_player);
        let other_acts = splash_event(!ninjask_is_player);
        assert_eq!(
            first,
            vec![other_acts, ninjask_acts, boosted(ninjask_is_player, 1)],
            "the slower holder acts second and boosts after both actions: {first:?}"
        );

        let second = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            second,
            vec![ninjask_acts, other_acts, boosted(ninjask_is_player, 2)],
            "one stage at +1 beats the faster base Speed: {second:?}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn both_speed_boost_holders_boost_in_the_residual_order() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, YANMA, 10, vec![SPLASH]);
    assert_eq!(enemy.ability(), AbilityId::SPEED_BOOST);
    assert!(player.stats().speed > enemy.stats().speed);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            splash_event(true),
            splash_event(false),
            boosted(true, 1),
            boosted(false, 1),
        ]
    );
    rng.assert_exhausted();
}

#[test]
fn the_boost_stops_at_plus_six_and_announces_nothing_more() {
    for ninjask_is_player in [true, false] {
        let mut script = vec![0];
        script.extend([0, 0].repeat(7));
        let mut rng = SequenceRng::new(script);
        let mut battle = ninjask_versus_faster_zigzagoon(ninjask_is_player, &mut rng);

        for stage in 1..=6 {
            let events = battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap();
            assert!(
                events.contains(&boosted(ninjask_is_player, stage)),
                "turn {stage}: {events:?}"
            );
        }
        let capped = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(
            speed_boost_events(&capped).is_empty(),
            "a holder at +6 is silent, with no failure message: {capped:?}"
        );
        assert!(
            !capped
                .iter()
                .any(|event| matches!(event, BattleEvent::StatWontGoHigher { .. })),
            "{capped:?}"
        );
        let holder = if ninjask_is_player {
            battle.player()
        } else {
            battle.enemy()
        };
        assert_eq!(holder.stages().speed, StatStage::MAX);
        rng.assert_exhausted();
    }
}

#[test]
fn a_holder_boosts_before_its_own_poison_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    player.set_status1(Status1::Poisoned);
    let max_hp = player.stats().max_hp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 30, vec![SPLASH]);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        &events[2..],
        &[
            boosted(true, 1),
            BattleEvent::HurtByPoison {
                by_player: true,
                damage: poison_residual_damage(max_hp),
            },
        ],
        "{events:?}"
    );
    rng.assert_exhausted();
}

/// A failed escape still ends in the ordinary end-turn pass; a successful one
/// leaves the battle before it.
#[test]
fn a_failed_run_still_boosts_and_a_successful_run_does_not() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 30, vec![SPLASH]);
    // Construction seed, turn-start draw, wild move pick, then the escape draw.
    let mut rng = SequenceRng::new([0, 0, 0, 255]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(events.contains(&boosted(true, 1)), "{events:?}");
    assert_eq!(battle.outcome(), None);
    rng.assert_exhausted();

    let dex = Dex::new();
    let player = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 30, vec![SPLASH]);
    let mut rng = SequenceRng::new([0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(speed_boost_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
}

/// Meowth (Tackle, Splash) against a 1 HP level-5 Zigzagoon lead with a
/// level-10 Ninjask reserve: the enemy outspeeds and knocks the lead out.
fn player_action_ko_battle(rng: &mut SequenceRng) -> Battle {
    let dex = Dex::new();
    let mut lead = max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]);
    leave_one_hp(&mut lead);
    let reserve = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, MEOWTH, 20, vec![TACKLE, SPLASH]);
    assert!(lead.stats().speed < enemy.stats().speed);
    Battle::new_with_player_reserves(dex, lead, vec![reserve], enemy, false, rng).unwrap()
}

#[test]
fn a_player_replacement_sent_out_by_an_action_ko_skips_that_turn_and_boosts_the_next() {
    // Construction seed; turn one: turn start, Tackle pick, accuracy, no
    // crit, damage roll, discarded secondary draw; turn two: turn start and
    // Splash pick.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1]);
    let mut battle = player_action_ko_battle(&mut rng);

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        first.contains(&BattleEvent::PlayerSentOut {
            species: SpeciesId(NINJASK),
            reserves_remaining: 0,
        }),
        "{first:?}"
    );
    assert!(speed_boost_events(&first).is_empty(), "{first:?}");
    assert_eq!(battle.player().stages().speed, StatStage::NEUTRAL);

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(second.last(), Some(&boosted(true, 1)), "{second:?}");
    rng.assert_exhausted();
}

#[test]
fn a_player_replacement_sent_out_after_a_residual_ko_boosts_on_the_following_turn() {
    let dex = Dex::new();
    let mut lead = max_iv_mon(&dex, ZIGZAGOON, 30, vec![SPLASH]);
    leave_one_hp(&mut lead);
    lead.set_status1(Status1::Poisoned);
    let reserve = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, MEOWTH, 20, vec![SPLASH]);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    let mut battle =
        Battle::new_with_player_reserves(dex, lead, vec![reserve], enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        first.contains(&BattleEvent::PlayerSentOut {
            species: SpeciesId(NINJASK),
            reserves_remaining: 0,
        }),
        "{first:?}"
    );
    assert!(speed_boost_events(&first).is_empty(), "{first:?}");

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        second.contains(&boosted(true, 1)),
        "the end-of-turn cleanup already ran behind the replacement: {second:?}"
    );
    rng.assert_exhausted();
}

/// A level-20 Meowth lead (Tackle, Splash) one hit from fainting against the
/// player; the trainer's bench holds a Ninjask.
fn trainer_ko_battle(rng: &mut SequenceRng, player_moves: Vec<MoveId>, level: u8) -> Battle {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MEOWTH, level, player_moves);
    let mut lead = max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]);
    leave_one_hp(&mut lead);
    let bench = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![lead, bench], rng).unwrap()
}

#[test]
fn a_trainer_replacement_sent_out_by_an_action_ko_skips_that_turn_and_boosts_the_next() {
    // Construction seed; turn one: turn start, four AI simulation draws, the
    // AI pick, then the player's accuracy, no crit, damage roll and a
    // discarded secondary draw; turn two: turn start, the same AI draws.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = trainer_ko_battle(&mut rng, vec![TACKLE, SPLASH], 20);
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        first.contains(&BattleEvent::TrainerSentOut {
            species: SpeciesId(NINJASK),
            bench_remaining: 0,
        }),
        "{first:?}"
    );
    assert!(speed_boost_events(&first).is_empty(), "{first:?}");
    assert_eq!(battle.enemy().stages().speed, StatStage::NEUTRAL);

    let second = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(second.last(), Some(&boosted(false, 1)), "{second:?}");
    rng.assert_exhausted();
}

/// A level-24 Ninjask one experience point short of level 25, which asks to
/// learn a move when it knocks out or outlasts the trainer's 1 HP lead. The
/// bench holds a Ninjask too.
fn prompting_battle(rng: &mut SequenceRng, poisoned_lead: bool) -> Battle {
    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId(NINJASK)).unwrap().growth_rate;
    let level_25 = assets::experience_for_level(growth_rate, 25).unwrap();
    let mut player = max_iv_mon(&dex, NINJASK, 24, vec![TACKLE, SPLASH, FOCUS_ENERGY, LEER]);
    assert!(player
        .apply_experience(&dex, level_25 - 1 - player.experience())
        .unwrap()
        .is_none());
    let mut lead = max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]);
    leave_one_hp(&mut lead);
    if poisoned_lead {
        lead.set_status1(Status1::Poisoned);
    }
    let bench = max_iv_mon(&dex, NINJASK, 10, vec![SPLASH]);
    Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![lead, bench], rng).unwrap()
}

#[test]
fn a_move_learn_prompt_after_an_action_ko_defers_the_residual_pass_and_its_cleanup() {
    let mut rng = SequenceRng::new(vec![0; 64]);
    let mut battle = prompting_battle(&mut rng, false);
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(battle.pending_move_learn().is_some(), "{first:?}");
    assert!(speed_boost_events(&first).is_empty(), "{first:?}");

    let mut resume = SequenceRng::new([]);
    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut resume)
        .unwrap();
    assert_eq!(
        speed_boost_events(&answered),
        vec![&boosted(true, 1)],
        "the held-back residual pass boosts the lead once and skips the \
         replacement sent out ahead of it: {answered:?}"
    );
    resume.assert_exhausted();

    let next = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        speed_boost_events(&next),
        vec![&boosted(true, 2), &boosted(false, 1)],
        "the cleanup behind the held pass already aged the replacement: {next:?}"
    );
}

#[test]
fn a_move_learn_prompt_after_a_residual_ko_holds_the_cleanup_until_the_replacement_is_out() {
    let mut rng = SequenceRng::new(vec![0; 64]);
    let mut battle = prompting_battle(&mut rng, true);
    let first = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert!(battle.pending_move_learn().is_some(), "{first:?}");
    assert_eq!(
        speed_boost_events(&first),
        vec![&boosted(true, 1)],
        "the player boosts before the poison tick that fells the lead: {first:?}"
    );

    let mut resume = SequenceRng::new([]);
    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut resume)
        .unwrap();
    assert!(speed_boost_events(&answered).is_empty(), "{answered:?}");
    resume.assert_exhausted();

    let next = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        speed_boost_events(&next),
        vec![&boosted(true, 2), &boosted(false, 1)],
        "the replacement was aged once, by the cleanup the prompt held: {next:?}"
    );
}
