//! The four move pipelines issue #321 added, driven through real turns.
//!
//! The arithmetic and the RNG-draw shapes are pinned at unit level inside
//! `battle`'s own `drain` / `fixed_damage` / `multi_hit` / `flag_move`
//! modules. What is pinned **here** is the wiring only a turn can show:
//! that the drain heals from the HP the target actually lost, that Liquid
//! Ooze's damage lands on the attacker in the script's message order, that
//! the multi-hit loop really stops at a knocked-out target without spending
//! the abandoned hits' draws, and that the volatiles a flag move sets are
//! read by the *next* move and expire on schedule.
//!
//! Every script is exact-length, so a pipeline that draws one time too many
//! panics rather than quietly desynchronising.

use crate::common::{max_iv_mon, SequenceRng, MAX_IVS};
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_ABSORB` (`EFFECT_ABSORB`).
const ABSORB: MoveId = MoveId(71);
/// `MOVE_SONIC_BOOM` (`EFFECT_SONICBOOM`).
const SONIC_BOOM: MoveId = MoveId(49);
/// `MOVE_DOUBLE_SLAP` (`EFFECT_MULTI_HIT`).
const DOUBLE_SLAP: MoveId = MoveId(3);
/// `MOVE_SPLASH` / `MOVE_FOCUS_ENERGY` / `MOVE_CHARGE`, the flag-only three.
const SPLASH: MoveId = MoveId(150);
const FOCUS_ENERGY: MoveId = MoveId(116);
const CHARGE: MoveId = MoveId(268);
/// `MOVE_SHOCK_WAVE` — `EFFECT_ALWAYS_HIT`, and the only Electric damaging
/// move this engine can execute, so the one that can show Charge acting.
const SHOCK_WAVE: MoveId = MoveId(351);

/// `SPECIES_BULBASAUR`: Grass/Poison, **Overgrow** in ability slot 0.
const BULBASAUR: u16 = 1;
/// `SPECIES_SQUIRTLE`: pure Water, one ability slot.
const SQUIRTLE: u16 = 7;
/// `SPECIES_TENTACOOL`: Clear Body in slot 0, **Liquid Ooze** in slot 1, so
/// an odd personality fields the ability this pipeline exposes.
const TENTACOOL: u16 = 72;
/// `SPECIES_MACHOP`: bulky and slow enough at level 20 to survive a charged
/// Shock Wave and still move second.
const MACHOP: u16 = 66;

/// A mon with an explicit personality, for the one fixture that needs
/// ability slot 1 (`personality & 1`).
fn mon_with_personality(
    dex: &Dex,
    species: u16,
    level: u8,
    personality: u32,
    moves: Vec<MoveId>,
) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, personality, moves).unwrap()
}

/// The `gHpDealt`-not-`gBattleMoveDamage` contract, driven through a real
/// turn: an overkill Absorb heals half the HP the target **actually lost**
/// — `Cmd_datahpupdate`'s cap — never half of the formula's raw output.
///
/// Swapping `execute_drain_move`'s clamped figure for the raw damage passes
/// every unit test in `battle::drain` and fails here, which is why this pin
/// lives at turn level.
#[test]
fn an_overkill_absorb_drains_half_the_hp_actually_removed() {
    let dex = Dex::new();
    // A level-50 Bulbasaur's Absorb computes far more than 5 damage; the
    // attacker is 10 HP down, so the heal is not max-HP clamped either.
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    player.apply_damage(10);
    let mut enemy = max_iv_mon(&dex, 19, 2, vec![TACKLE]); // Rattata
    let enemy_max_hp = enemy.stats().max_hp;
    enemy.apply_damage(enemy_max_hp - 5);

    // 1 (battle start) + turn number + enemy selection + Absorb's 3
    // (accuracy, crit, damage roll). The enemy faints, so it never acts and
    // the script ends exactly here.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Hit {
            by_player: true,
            move_id: ABSORB,
            damage: 5,
            is_critical: false,
        },
        "the hit reports the 5 HP the target had, not the raw formula: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::Drained {
            by_player: true,
            move_id: ABSORB,
            healed: 2,
        },
        "half of the 5 actually removed (truncating), never half of the \
         formula output: {events:?}"
    );
    assert_eq!(
        battle.player().current_hp(),
        player_max_hp - 10 + 2,
        "the reported heal is the applied heal"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(rng.draws(), 6);
}

/// A full-HP drainer still prints the drain string — upstream's
/// `printfromtable` is not gated on the heal doing anything — but the event
/// reports the `0` HP actually gained, because `Cmd_datahpupdate` clamps at
/// maximum HP.
#[test]
fn a_full_hp_absorb_still_reports_the_drain_and_heals_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, 19, 2, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;
    enemy.apply_damage(enemy_max_hp - 5);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Drained {
            by_player: true,
            move_id: ABSORB,
            healed: 0,
        }),
        "a full-HP drainer gains nothing and the event says so: {events:?}"
    );
    assert_eq!(battle.player().current_hp(), player_max_hp);
}

/// Liquid Ooze: the same magnitude, taken off the **attacker**, with the
/// script's other string (`B_MSG_ABSORB_OOZE`) in place of the drain one —
/// never both.
#[test]
fn liquid_ooze_turns_the_drain_into_damage_on_the_attacker() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    player.apply_damage(30);
    // Personality 1 is odd, so `CreateBoxMon`'s `abilityNum = personality &
    // 1` selects Tentacool's slot-1 ability, Liquid Ooze.
    let enemy = mon_with_personality(&dex, TENTACOOL, 5, 1, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;
    assert_eq!(enemy.ability(), battle::LIQUID_OOZE);
    assert_eq!(enemy_max_hp, 20, "Tentacool's level-5 maximum HP");

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // The overkill hit removes all 20 HP, so the drain magnitude is 10 --
    // and the sign flip makes those 10 the attacker's loss.
    assert_eq!(
        events[0],
        BattleEvent::Hit {
            by_player: true,
            move_id: ABSORB,
            damage: 20,
            is_critical: false,
        }
    );
    assert_eq!(
        events[1],
        BattleEvent::LiquidOoze {
            by_player: true,
            move_id: ABSORB,
            damage: 10,
        }
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::Drained { .. })),
        "upstream picks one string-table entry, never both: {events:?}"
    );
    assert_eq!(battle.player().current_hp(), player_max_hp - 30 - 10);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `tryfaintmon BS_ATTACKER` comes **before** `tryfaintmon BS_TARGET`
/// (`data/battle_scripts_1.s:358`-`:359`), so an attacker its own Liquid
/// Ooze victim killed faints first — and the battle ends in defeat even
/// though the target was also knocked out by the same move.
#[test]
fn a_liquid_ooze_kill_faints_the_attacker_before_the_target() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    // Leave the attacker on less HP than the 10 the ooze will take.
    player.apply_damage(player_max_hp - 6);
    let enemy = mon_with_personality(&dex, TENTACOOL, 5, 1, vec![TACKLE]);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        [
            BattleEvent::Hit {
                by_player: true,
                move_id: ABSORB,
                damage: 20,
                is_critical: false,
            },
            BattleEvent::LiquidOoze {
                by_player: true,
                move_id: ABSORB,
                damage: 6,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the attacker's faint is settled first, and it ends the battle"
    );
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(battle.enemy().current_hp(), 0, "the target died too");
}

/// Overgrow through a whole turn: the same Absorb from the same battler
/// deals 12 instead of 8 once its HP reaches a third of its maximum, and
/// drains proportionally more.
///
/// Bulbasaur level 5 has 21 maximum HP, so the gate (`hp <= maxHP / 3`) is
/// 7. Absorb's power goes 20 -> 30 and the damage 8 -> 12
/// (`battle::drain`'s unit tests carry the full hand computation).
#[test]
fn overgrow_boosts_the_players_absorb_once_its_hp_is_low() {
    // 1 (battle start) + turn number + enemy selection + Absorb's 3 +
    // the enemy's Tackle (4). Bulbasaur (speed 11) outruns Squirtle
    // (speed 10), so no speed-tie draw and the player moves first.
    let script = [0, 0, 0, 0, 1, 0, 0, 1, 0, 0];

    let run = |remaining_hp: u32| {
        let dex = Dex::new();
        let mut player = max_iv_mon(&dex, BULBASAUR, 5, vec![ABSORB]);
        assert_eq!(player.stats().max_hp, 21);
        player.apply_damage(21 - remaining_hp);
        let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

        let mut rng = SequenceRng::new(script);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(rng.draws(), script.len(), "the boost changes no draw count");
        (events, battle.player().current_hp())
    };

    // One HP above the gate: the ordinary figures.
    let (events, hp) = run(8);
    assert_eq!(
        &events[..2],
        [
            BattleEvent::Hit {
                by_player: true,
                move_id: ABSORB,
                damage: 8,
                is_critical: false,
            },
            BattleEvent::Drained {
                by_player: true,
                move_id: ABSORB,
                healed: 4,
            },
        ]
    );
    assert_eq!(
        hp,
        8 + 4 - 4,
        "healed 4, then took the enemy's 4-damage Tackle"
    );

    // Exactly at the gate: Overgrow fires.
    let (events, hp) = run(7);
    assert_eq!(
        &events[..2],
        [
            BattleEvent::Hit {
                by_player: true,
                move_id: ABSORB,
                damage: 12,
                is_critical: false,
            },
            BattleEvent::Drained {
                by_player: true,
                move_id: ABSORB,
                healed: 6,
            },
        ]
    );
    assert_eq!(hp, 7 + 6 - 4);
}

/// A fixed-damage move deals its literal through the turn and costs **2**
/// draws where an ordinary move costs 4 — no crit roll, no damage roll.
#[test]
fn sonic_boom_deals_a_flat_twenty_for_two_draws() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 50, vec![SONIC_BOOM]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;

    // 1 (battle start) + turn number + enemy selection + Sonic Boom's 2
    // (accuracy, the discarded effect chance) + the enemy's Tackle (4).
    let script = [0, 0, 0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Hit {
            by_player: true,
            move_id: SONIC_BOOM,
            damage: 20,
            is_critical: false,
        },
        "a level-50 attacker against a level-50 defender still deals 20"
    );
    assert_eq!(battle.enemy().current_hp(), enemy_max_hp - 20);
    assert_eq!(rng.draws(), script.len());
}

/// A multi-hit move reports every landed hit and then the hit-count string,
/// with the per-hit crit and damage rolls in between.
#[test]
fn double_slap_reports_each_hit_then_the_hit_count() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![DOUBLE_SLAP]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    // 1 (battle start) + turn number + enemy selection
    //   + Double Slap: accuracy, hit count (mask 0 -> 2 hits),
    //     then crit+roll per hit, then one trailing effect chance
    //   + the enemy's Tackle (4).
    let script = [0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let hit = BattleEvent::Hit {
        by_player: true,
        move_id: DOUBLE_SLAP,
        damage: 3,
        is_critical: false,
    };
    assert_eq!(
        &events[..3],
        [
            hit,
            hit,
            BattleEvent::MultiHit {
                by_player: true,
                move_id: DOUBLE_SLAP,
                hits: 2,
            },
        ],
        "two hits, then `Hit 2 time(s)!`: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 20 - 6);
    assert_eq!(rng.draws(), script.len());
}

/// The loop's `jumpifhasnohp BS_TARGET` guard is checked at the **top** of
/// each iteration, so the killing hit completes and is reported, the
/// hit-count string reports the hits that landed, and the abandoned
/// iterations spend **no draws at all**.
///
/// The script rolls 5 hits at a target that can absorb only two, and is
/// exactly long enough for those two: a third iteration's crit draw would
/// run off the end and panic.
#[test]
fn a_multi_hit_move_abandons_its_remaining_hits_without_spending_their_draws() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![DOUBLE_SLAP]);
    let mut enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    enemy.apply_damage(enemy.stats().max_hp - 5);

    // 1 (battle start) + turn number + enemy selection
    //   + accuracy, hit count (mask 3 -> redraw, mask 3 -> 5 hits),
    //     crit+roll, crit+roll, the trailing effect chance.
    let script = [0, 0, 0, 0, 3, 3, 1, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        [
            BattleEvent::Hit {
                by_player: true,
                move_id: DOUBLE_SLAP,
                damage: 3,
                is_critical: false,
            },
            // The second hit's formula damage is 3 again; the target had 2
            // left, so `gHpDealt` -- and the report -- is 2.
            BattleEvent::Hit {
                by_player: true,
                move_id: DOUBLE_SLAP,
                damage: 2,
                is_critical: false,
            },
            BattleEvent::MultiHit {
                by_player: true,
                move_id: DOUBLE_SLAP,
                hits: 2,
            },
            BattleEvent::Fainted { by_player: false },
            BattleEvent::ExpGained(47),
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ],
        "{events:?}"
    );
    assert_eq!(
        rng.draws(),
        script.len(),
        "the three abandoned hits cost nothing"
    );
}

/// Splash spends a PP, prints, and does nothing else — **including drawing
/// nothing**, which the exact-length script enforces.
#[test]
fn splash_reports_that_nothing_happened_and_draws_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    // 1 (battle start) + turn number + enemy selection + Splash's *zero*
    // + the enemy's Tackle (4).
    let script = [0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::NothingHappened {
            by_player: true,
            move_id: SPLASH,
        }
    );
    assert_eq!(battle.player().moves()[0].pp, 39, "a PP was still spent");
    assert_eq!(rng.draws(), script.len());
}

/// Focus Energy's `STATUS2_FOCUS_ENERGY` is worth `+2` crit-chance stages to
/// the **next** move: a draw of `4` does not crit at stage 0 (1/16) but does
/// at stage 2 (1/4). The control run replaces Focus Energy with Splash and
/// keeps every other draw identical, so only the volatile can explain the
/// difference.
#[test]
fn focus_energy_raises_the_next_moves_crit_chance() {
    // turn 1: turn number, enemy selection, the flag move's zero, the
    //         enemy's Tackle (4).
    // turn 2: turn number, enemy selection, Tackle's 4 (crit draw `4`),
    //         the enemy's Tackle (4).
    let script = [
        0, // battle start
        0, 0, 0, 1, 0, 0, // turn 1
        0, 0, 0, 4, 0, 0, 0, 1, 0, 0, // turn 2
    ];

    let run = |first_move: MoveId| {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, BULBASAUR, 5, vec![first_move, TACKLE]);
        let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
        let mut rng = SequenceRng::new(script);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let first = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        let second = battle
            .take_turn(PlayerAction::UseMove(1), &mut rng)
            .unwrap();
        assert_eq!(
            rng.draws(),
            script.len(),
            "the flag move itself draws nothing"
        );
        (first, second)
    };

    let (first, second) = run(FOCUS_ENERGY);
    assert_eq!(
        first[0],
        BattleEvent::GettingPumped {
            by_player: true,
            move_id: FOCUS_ENERGY,
        }
    );
    assert!(
        matches!(
            second[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                is_critical: true,
                ..
            }
        ),
        "draw 4 crits at stage 2: {second:?}"
    );

    let (_, control) = run(SPLASH);
    assert!(
        matches!(
            control[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                is_critical: false,
                ..
            }
        ),
        "the same draw does not crit at stage 0: {control:?}"
    );
}

/// Using Focus Energy twice fails the second time — the script's own
/// `jumpifstatus2` at `data/battle_scripts_1.s:889`, not
/// `Cmd_setfocusenergy`'s unreachable `else`.
#[test]
fn a_second_focus_energy_reports_that_it_failed() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![FOCUS_ENERGY]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    let script = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        first[0],
        BattleEvent::GettingPumped {
            by_player: true,
            move_id: FOCUS_ENERGY,
        }
    );
    assert_eq!(
        second[0],
        BattleEvent::ButItFailed {
            by_player: true,
            move_id: FOCUS_ENERGY,
        }
    );
    assert_eq!(rng.draws(), script.len());
}

/// Charge doubles an Electric move for the Charge turn and exactly **one**
/// turn after it, then expires — `ENDTURN_CHARGE` decrements the timer once
/// per end of turn (`src/battle_util.c:1743`-`:1745`).
#[test]
fn charge_doubles_the_next_turns_electric_move_and_then_expires() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 20, vec![CHARGE, SHOCK_WAVE]);
    let enemy = max_iv_mon(&dex, MACHOP, 20, vec![TACKLE]);

    // turn 1: turn number, enemy selection, Charge's zero, the enemy's
    //         Tackle (4).
    // turns 2 and 3: turn number, enemy selection, Shock Wave's 3 (it is
    //         EFFECT_ALWAYS_HIT, so no accuracy draw), the enemy's Tackle (4).
    let script = [
        0, // battle start
        0, 0, 0, 1, 0, 0, // turn 1
        0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 2
        0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 3
    ];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        first[0],
        BattleEvent::ChargingPower {
            by_player: true,
            move_id: CHARGE,
        }
    );

    let charged = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    let expired = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();

    let damage_of = |events: &[BattleEvent]| match events[0] {
        BattleEvent::Hit {
            by_player: true,
            damage,
            ..
        } => damage,
        ref other => panic!("expected the player's hit, got {other:?}"),
    };
    let boosted = damage_of(&charged);
    let plain = damage_of(&expired);
    assert_eq!(
        boosted,
        2 * plain,
        "Charge doubles the post-crit figure before typecalc, so the type \
         multiplier scales both equally: {boosted} vs {plain}"
    );
    assert_eq!(rng.draws(), script.len(), "Charge itself draws nothing");
}
