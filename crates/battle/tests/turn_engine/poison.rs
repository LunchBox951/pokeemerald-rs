//! [`Status1::Poisoned`], [`EFFECT_POISON_HIT`]'s secondary infliction, and
//! the end-of-turn poison residual, driven through real turns.
//!
//! Unit-level draw shapes and the residual damage formula are pinned inside
//! `battle::secondary` (the chance draw, the immunity guards, the ability
//! admission screens) and `battle::status1` (`poison_residual_damage`'s
//! eighth-of-max-HP floor). What is pinned **here** is the wiring only a
//! turn can show: that a landed hit's poison shows up as
//! [`BattleEvent::Poisoned`] right after [`BattleEvent::Hit`], that the
//! residual tick fires for both battlers in the same turn-order sequence
//! their moves used, and that a lethal residual tick settles the battle
//! exactly like a lethal hit does.

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
/// `SPECIES_ABRA`: base Speed 90, faster than [`slow_runner_rattata`]'s
/// Speed-72 Rattata even at the same level, but far too weak an attacker to
/// one-shot it with Tackle -- forces the failed-run fixture's RNG-driven
/// escape branch without the battle ending before residual ever runs.
const ABRA: u16 = 63;

#[test]
fn a_landed_poison_sting_reports_poisoned_immediately_after_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Poison Sting (accuracy hit, no crit, best
    // damage roll, a successful 30% chance draw), the enemy's ordinary
    // Tackle (4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 29, 0, 1, 0, 0]);
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

    // Same script shape as the landed case above: the chance roll still
    // succeeds (29 < 30), but Ekans's own typing silently blocks it.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 29, 0, 1, 0, 0]);
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

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Tackle (Rattata's 72 base Speed outpaces
    // Zigzagoon's 41, so the player acts first, 4 draws), then the enemy's
    // own Tackle (4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
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
    // The enemy's own Tackle also lands on the player this same turn, so
    // only a lower bound holds here: the residual tick alone removed at
    // least `expected_damage`, on top of whatever the direct hit took.
    assert!(battle.player().current_hp() <= player_max_hp - expected_damage);
    assert!(battle.outcome().is_none());
}

/// Neither battler's residual tick is lethal here, so both run in the same
/// order this turn's own moves used; contrast
/// [`the_first_battlers_lethal_residual_tick_stops_the_second_battlers_from_running`],
/// where the first tick ending the battle stops the second from running at
/// all.
#[test]
fn both_battlers_poisoned_take_residual_damage_in_the_same_turn_order_their_moves_used() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Tackle (faster: acts first, 4 draws), the
    // enemy's Tackle (4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
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

    // Tackle cannot miss at 100 accuracy (`accuracy::accuracy_check`'s own
    // draw-range pin), so the player's Tackle always lands this turn. Probe
    // its exact damage with the same crit/damage-roll draws the real turn
    // below uses, then park the enemy exactly one residual tick above that
    // -- it survives the direct hit with precisely `lethal_damage` HP left,
    // so the residual tick (not capped by a shorter remainder) is what
    // brings it to zero, not Tackle.
    let tackle_damage = {
        // Crit draw 1 (not 0) -- an ordinary, non-critical roll, matching
        // `ORDINARY_NO_CRIT_DRAW` in `hit`'s own unit tests -- then the best
        // damage-variance roll.
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

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Tackle (faster: acts first, accuracy/crit/
    // damage/chance, 4 draws matching the probe's crit and damage rolls),
    // the enemy's own Tackle (4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
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

/// `BattleTurnPassed`'s `if (gBattleOutcome == 0)` guard
/// (`battle_main.c:3960`-`:3966`) refuses to re-invoke
/// `DoBattlerEndTurnEffects` once a residual script's own `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) has set an outcome, so a battler whose
/// own residual tick ends the battle stops the tracker walk there --
/// leaving a *later* poisoned battler's own tick unrun this turn, no matter
/// how lethal it would have been.
#[test]
fn the_first_battlers_lethal_residual_tick_stops_the_second_battlers_from_running() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    enemy.set_status1(Status1::Poisoned);

    // Rattata's 72 base Speed outpaces Zigzagoon's 41, so the player is
    // processed first in residual, matching this turn's own move order.
    // Probe the enemy's Tackle damage against the player with the same
    // crit/damage-roll draws the real turn below uses, then park the
    // player exactly one residual tick above that -- it survives the
    // enemy's direct hit with precisely its own poison damage left, so the
    // player's residual tick (processed first) is what faints it.
    let tackle_damage = {
        let mut probe = SequenceRng::new([1, 0]);
        match battle::damage_core(&dex, TACKLE, &enemy, &player, false, &mut probe).unwrap() {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must deal damage against Rattata: {other:?}"),
        }
    };
    let player_lethal_damage = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - (tackle_damage + player_lethal_damage));

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Tackle (faster: acts first, 4 draws), the
    // enemy's own Tackle (accuracy/crit/damage/chance, 4 draws matching the
    // probe's crit and damage rolls).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
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
    // A player faster than the enemy escapes unconditionally
    // (`try_run_from_battle` returns `true` with no draw); this fixture
    // needs the RNG-driven branch instead, so it pairs `slow_runner_rattata`
    // against a same-level [`ABRA`], faster but far too weak an attacker to
    // one-shot it -- unlike the escape module's own level-50-Charmander
    // fixture, which would end the battle before residual ever ran. The
    // *enemy* carries the poison, not the player, so the fixture needs no
    // damage-survival accounting at all.
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, ABRA, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);
    let enemy_max_hp = enemy.stats().max_hp;

    // battle-start turn number, turn number, the enemy's move pick, the
    // escape roll (65000 & 0xFF = 232, fails against this pairing's
    // threshold -- see `crate::escape`'s own tests), then the enemy's Tackle
    // (accuracy / no crit / best roll / effect chance, 4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0, 0]);
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

/// `BattleScript_FaintTarget` (`data/battle_scripts_1.s:2817`-`:2823`)
/// carries no `checkteamslost`, so a direct-hit wild KO leaves
/// `gBattleOutcome` at `0` for the rest of the turn: `BattleTurnPassed`'s
/// `if (gBattleOutcome == 0)` gate (`src/battle_main.c:3960`-`:3966`) still
/// runs `DoBattlerEndTurnEffects`, and `ENDTURN_POISON`
/// (`src/battle_util.c:1525`-`:1535`) ticks the standing, poisoned winner
/// before `HandleFaintedMonActions` (`:3968`) hands out the experience and
/// ends the battle.
#[test]
fn a_poisoned_winner_still_takes_its_final_residual_tick_before_the_wild_ko_pays_out() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_max_hp = player.stats().max_hp;
    let player_hp_before = player.current_hp();
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the wild mon's
    // move pick, then the player's Tackle (accuracy, crit, damage roll,
    // discarded effect-chance roll). The wild mon never acts: the player's
    // hit kills it.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the fixture must knock the wild mon out with a direct hit: {events:?}"
    );

    let residual_index = events
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
        .expect("the poisoned winner takes its end-of-turn tick: {events:?}");
    assert_eq!(
        events[residual_index],
        BattleEvent::HurtByPoison {
            by_player: true,
            damage: poison_residual_damage(player_max_hp),
        },
    );
    assert_eq!(
        battle.player().current_hp(),
        player_hp_before - poison_residual_damage(player_max_hp),
    );

    let exp_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::ExpGained(_)))
        .expect("the KO awards experience: {events:?}");
    assert!(
        residual_index < exp_index,
        "the residual tick precedes the knockout's payout: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `MOVE_LEER`, a non-damaging stat drop: the trainer mon's only move, so
/// neither battler's direct action can change the other's HP this turn and
/// the residual pass alone decides the battle.
const LEER: MoveId = MoveId(43);
/// `MOVE_GROWL`, the player's own non-damaging move, for the same reason.
const GROWL: MoveId = MoveId(45);
/// `TRAINER_MAY_ROUTE_103_MUDKIP`.
const MAY_ROUTE_103_MUDKIP: assets::trainers::TrainerId = assets::trainers::TrainerId(529);

/// A trainer's **last** mon fainting to its own residual tick ends the
/// battle right there: `BattleScript_DoTurnDmgEnd`'s `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) runs inside the very script that
/// fainted it, and `BattleTurnPassed`'s `if (gBattleOutcome == 0)` guard
/// (`battle_main.c:3960`-`:3966`) then refuses to walk on to the next
/// battler -- so the player's own lethal tick never runs, exactly as
/// [`the_first_battlers_lethal_residual_tick_stops_the_second_battlers_from_running`]
/// pins for the wild case.
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

    // Neither move deals damage, so no draw here decides anything the
    // assertions below read; a long zero script covers the turn-number and
    // trainer-AI draws without pinning their count (that belongs to
    // `trainer_ai`'s own tests).
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
