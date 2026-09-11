//! Poison's secondary infliction and its end-of-turn residual, driven
//! through real turns.
//!
//! The draw shapes, the immunity guards, and the damage formula are pinned
//! in `battle::secondary` and `battle::status1`. What is pinned here is the
//! turn wiring those cannot reach: event order, residual order, and how a
//! residual knockout settles.

use crate::common::{max_iv_mon, slow_runner_rattata, SequenceRng};
use assets::MoveId;
use battle::status1::poison_residual_damage;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, Status1};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_POISON_STING` (`EFFECT_POISON_HIT`), 100 accuracy, 30% chance,
/// Poison type.
const POISON_STING: MoveId = MoveId(40);

/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: an ordinary Normal-type target, slower than Rattata.
const ZIGZAGOON: u16 = 288;
/// `SPECIES_EKANS`: mono Poison-type, immune to poison outright.
const EKANS: u16 = 23;
/// `SPECIES_ABRA`: faster than [`slow_runner_rattata`]'s Rattata, so a run
/// is never automatic, and too weak to end the battle before its residuals.
const ABRA: u16 = 63;

/// A draw that clears [`POISON_STING`]'s 30% secondary chance.
const POISON_CHANCE_HIT_DRAW: u16 = 29;
/// An escape roll that fails for [`slow_runner_rattata`] against [`ABRA`]
/// (`battle::escape`'s own tests pin the threshold).
const ESCAPE_ROLL_FAILS: u16 = 65000;

/// Both battlers use a damaging move, every roll on its default branch.
const BOTH_BATTLERS_ATTACK: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
/// [`BOTH_BATTLERS_ATTACK`] with the player's [`POISON_STING`] chance draw
/// clearing.
const POISON_STING_LANDS: [u16; 11] = [0, 0, 0, 0, 1, 0, POISON_CHANCE_HIT_DRAW, 0, 1, 0, 0];
/// The player's move fells the enemy before it acts.
const PLAYER_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];
/// A refused run, then the enemy's damaging move.
const FAILED_RUN_THEN_ENEMY_ATTACK: [u16; 8] = [0, 0, 0, ESCAPE_ROLL_FAILS, 0, 1, 0, 0];

#[test]
fn a_landed_poison_sting_reports_poisoned_immediately_after_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let hit_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    move_id: POISON_STING,
                    ..
                }
            )
        })
        .expect("Poison Sting must land: {events:?}");
    assert_eq!(
        events[hit_index + 1],
        BattleEvent::Poisoned {
            by_player: true,
            move_id: POISON_STING,
        },
        "Poisoned follows Hit immediately, matching seteffectwithchance \
         preceding tryfaintmon: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
}

#[test]
fn a_poison_type_target_is_never_poisoned_even_on_a_successful_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, EKANS, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Poisoned { .. })),
        "a Poison-type target must never carry Poisoned: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn end_of_turn_poison_damage_matches_the_pinned_formula_and_settles_no_faint() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_max_hp = player.stats().max_hp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let expected_damage = poison_residual_damage(player_max_hp);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            ))
            .count(),
        1,
        "exactly one residual tick for the one poisoned battler: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: expected_damage,
        }),
        "residual damage must match poison_residual_damage(max_hp): {events:?}"
    );
    // The enemy's own Tackle lands this same turn, so only a lower bound holds.
    assert!(battle.player().current_hp() <= player_max_hp - expected_damage);
    assert!(battle.outcome().is_none());
}

#[test]
fn both_battlers_poisoned_take_residual_damage_in_the_same_turn_order_their_moves_used() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let player_tick = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            )
        })
        .expect("the player must take a residual tick: {events:?}");
    let enemy_tick = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: false,
                    ..
                }
            )
        })
        .expect("the enemy must take a residual tick: {events:?}");
    assert!(
        player_tick < enemy_tick,
        "the faster Rattata's residual tick must precede the slower \
         Zigzagoon's, exactly like this turn's own move order: {events:?}"
    );
}

#[test]
fn a_lethal_residual_tick_faints_and_ends_the_battle_like_a_lethal_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);

    // Park the enemy exactly one residual tick above Tackle's damage.
    let tackle_damage = {
        let mut probe = SequenceRng::new([1, 0]);
        match battle::damage_core(&dex, TACKLE, &player, &enemy, false, &mut probe).unwrap() {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must deal damage against Zigzagoon: {other:?}"),
        }
    };
    let lethal_damage = poison_residual_damage(enemy.stats().max_hp);
    enemy.apply_damage(enemy.stats().max_hp - (tackle_damage + lethal_damage));
    enemy.set_status1(Status1::Poisoned);
    assert_eq!(
        enemy.current_hp(),
        tackle_damage + lethal_damage,
        "fixture sanity"
    );

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Hit {
            by_player: true,
            move_id: TACKLE,
            damage: tackle_damage,
            is_critical: false,
        }),
        "fixture sanity -- the direct hit must not itself faint the enemy: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: lethal_damage,
        }),
        "{events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the residual tick alone must faint the enemy: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.enemy().status1(),
        Status1::Healthy,
        "faint settlement must clear the corpse's primary status"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// Once a residual script's own `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) has decided the battle, `BattleTurnPassed`
/// never re-enters `DoBattlerEndTurnEffects` (`battle_main.c:3960-3966`).
#[test]
fn the_first_battlers_lethal_residual_tick_stops_the_second_battlers_from_running() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    enemy.set_status1(Status1::Poisoned);

    // Park the player, processed first, exactly one residual tick above the
    // enemy's Tackle damage.
    let tackle_damage = {
        let mut probe = SequenceRng::new([1, 0]);
        match battle::damage_core(&dex, TACKLE, &enemy, &player, false, &mut probe).unwrap() {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must deal damage against Rattata: {other:?}"),
        }
    };
    let player_lethal_damage = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - (tackle_damage + player_lethal_damage));

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: player_lethal_damage,
        }),
        "the player, processed first, must take its own residual tick: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "that tick alone must faint the player: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::HurtByPoison {
                by_player: false,
                ..
            }
        )),
        "the enemy's own residual tick must never run once the player's \
         already ended the battle this same pass: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
}

#[test]
fn a_failed_run_still_ticks_the_poison_residual() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, ABRA, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);
    let enemy_max_hp = enemy.stats().max_hp;

    let mut rng = SequenceRng::new(FAILED_RUN_THEN_ENEMY_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::Run, &mut rng)
        .unwrap_or_else(battle::TurnError::into_events);

    let expected_damage = poison_residual_damage(enemy_max_hp);
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: expected_damage,
        }),
        "a failed run does not skip end-of-turn residuals: {events:?}"
    );
}

/// `SPECIES_CHARMANDER`, a level-50 fixture that one-shots the level-5 wild
/// mon below with Tackle at any damage roll.
const CHARMANDER: u16 = 4;

/// A won battle routes through `HandleEndTurn_BattleWon`, never
/// `HandleEndTurn_ContinueBattle` and its `BattleTurnPassed`
/// (`src/battle_main.c:4937`-`:4952`), so `DoBattlerEndTurnEffects` never
/// runs for the turn the knockout ended.
#[test]
fn a_direct_hit_wild_ko_ends_the_battle_before_any_residual_can_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_hp_before = player.current_hp();
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new(PLAYER_ACTS_ALONE);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the fixture must knock the wild mon out with a direct hit: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })),
        "a knockout that already won the battle leaves no turn for a \
         residual tick to run in: {events:?}"
    );
    assert_eq!(battle.player().current_hp(), player_hp_before);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::ExpGained(_))),
        "the knockout still pays out: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `HandleFaintedMonActions`' `BattleScript_GiveExp`
/// (`src/battle_util.c:1912`-`:1923`) runs at the end of the killing move's
/// own script, while `DoBattlerEndTurnEffects` waits for `BattleTurnPassed`
/// (`src/battle_main.c:3960`-`:3968`).
#[test]
fn a_direct_hit_kos_reward_is_paid_before_the_residual_tick_that_fells_the_winner() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_lethal = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - player_lethal);
    let evs_before = player.evs();

    let lead = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let benched = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    // No draw in this turn decides anything the assertions below read.
    let mut rng = SequenceRng::new([0; 40]);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        vec![lead, benched],
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let exp_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::ExpGained(_)))
        .unwrap_or_else(|| panic!("the direct-hit knockout pays out: {events:?}"));
    let sent_out_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::TrainerSentOut { .. }))
        .unwrap_or_else(|| panic!("the bench replaces the fallen lead: {events:?}"));
    let tick_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the poisoned winner still ticks: {events:?}"));

    assert!(
        exp_index < tick_index && sent_out_index < tick_index,
        "the knockout is settled in full before the residual pass: {events:?}"
    );
    assert_eq!(
        events[tick_index],
        BattleEvent::HurtByPoison {
            by_player: true,
            damage: player_lethal,
        },
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "the fixture's tick must be lethal: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert!(
        battle.player().experience() > 0,
        "losing the turn must not take back the experience already awarded"
    );
    assert_ne!(
        battle.player().evs(),
        evs_before,
        "losing the turn must not take back the EVs already awarded"
    );
}

/// `MOVE_LEER`, non-damaging: with [`GROWL`], no direct action can change
/// HP, so the residual pass alone decides the battle.
const LEER: MoveId = MoveId(43);
/// `MOVE_GROWL`, the player's own non-damaging move.
const GROWL: MoveId = MoveId(45);
/// `TRAINER_MAY_ROUTE_103_MUDKIP`.
const MAY_ROUTE_103_MUDKIP: assets::trainers::TrainerId = assets::trainers::TrainerId(529);

/// `BattleScript_DoTurnDmgEnd`'s `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) runs inside the residual script that
/// fainted the trainer's last mon, so `BattleTurnPassed`'s outcome guard
/// (`battle_main.c:3960`-`:3966`) never reaches the player's own tick.
#[test]
fn a_trainer_last_mons_lethal_residual_tick_ends_the_battle_before_the_players_own_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, ZIGZAGOON, 20, vec![GROWL]);
    player.set_status1(Status1::Poisoned);
    let player_lethal = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - player_lethal);

    let mut enemy = max_iv_mon(&dex, RATTATA, 20, vec![LEER]);
    enemy.set_status1(Status1::Poisoned);
    let enemy_lethal = poison_residual_damage(enemy.stats().max_hp);
    enemy.apply_damage(enemy.stats().max_hp - enemy_lethal);
    assert!(
        enemy.stats().speed > player.stats().speed,
        "fixture sanity -- the trainer's mon must be processed first in residual"
    );

    // No draw in this turn decides anything the assertions below read.
    let mut rng = SequenceRng::new([0; 40]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![enemy], &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: enemy_lethal,
        }),
        "the faster trainer mon takes its residual tick first: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "that tick alone must faint the trainer's last mon: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::HurtByPoison {
                by_player: true,
                ..
            }
        )),
        "the player's own residual tick must never run once the trainer's \
         last mon has already lost the battle for them: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Fainted { by_player: true })),
        "the player must not faint after the battle is already won: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `SPECIES_TORCHIC`, whose level-16 learnset entry is Peck.
const TORCHIC: u16 = 280;
/// `SPECIES_TREECKO`.
const TREECKO: u16 = 277;
/// `MOVE_SCRATCH`.
const SCRATCH: MoveId = MoveId(10);
/// `MOVE_POUND`.
const POUND: MoveId = MoveId(1);
/// `MOVE_PECK`, the level-16 entry Torchic has no free slot for.
const PECK: MoveId = MoveId(64);

/// A level-up prompt interrupts the residual pass without dropping it:
/// upstream answers the box inside `BattleScript_GiveExp` before
/// `DoBattlerEndTurnEffects` runs (`src/battle_util.c:1912-1923`,
/// `src/battle_main.c:3960-3968`), so the deferred tick arrives with
/// [`Battle::resolve_move_learn`]'s answer, after the deferred replacement.
#[test]
fn a_level_up_prompt_defers_the_residual_tick_to_the_answer_rather_than_dropping_it() {
    let dex = Dex::new();
    let growth_rate = dex.species(assets::SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    player.set_status1(Status1::Poisoned);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
    ];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let mut events = Vec::new();
    for _ in 0..8 {
        events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() {
            break;
        }
    }
    assert!(
        events.contains(&BattleEvent::MoveLearnPrompt { move_id: PECK }),
        "the knockout's award must ask about Peck: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })),
        "the residual pass waits on the answer with everything else: {events:?}"
    );

    let hp_before = battle.player().current_hp();
    let expected = poison_residual_damage(battle.player().stats().max_hp);
    let answered = battle
        .resolve_move_learn(battle::MoveLearnDecision::Decline)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            BattleEvent::TrainerSentOut {
                species: assets::SpeciesId(TREECKO),
                bench_remaining: 0,
            },
            BattleEvent::HurtByPoison {
                by_player: true,
                damage: expected,
            },
        ],
        "the answer releases the replacement, then the deferred tick"
    );
    assert_eq!(battle.player().current_hp(), hp_before - expected);
    assert_eq!(battle.outcome(), None, "the battle plays on");
}
