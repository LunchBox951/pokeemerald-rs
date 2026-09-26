//! Turn-level coverage for `battle`'s drain, fixed-damage, multi-hit,
//! flag-only, and defense-curl move pipelines: whatever a unit test inside
//! those modules cannot show because it never runs through a real turn.
//!
//! Every script's first three draws are fixed: `Battle::new`'s battle-start
//! draw, `Battle::start_turn`'s turn-number refresh, and the AI's move
//! selection; each later turn repeats only the latter two. What follows is the acting move's own draws,
//! then the opponent's if it still acts. `SequenceRng` panics on exhaustion,
//! and the trailing `rng.draws() == script.len()` assertion, where present,
//! catches an under-drawing pipeline too.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::MoveId;
use battle::{
    Battle, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, StatStage, StatStages,
    Volatiles,
};

const TACKLE: MoveId = MoveId(33);
const ABSORB: MoveId = MoveId(71);
const SONIC_BOOM: MoveId = MoveId(49);
const DOUBLE_SLAP: MoveId = MoveId(3);
const SPLASH: MoveId = MoveId(150);
const FOCUS_ENERGY: MoveId = MoveId(116);
const CHARGE: MoveId = MoveId(268);
const DEFENSE_CURL: MoveId = MoveId(111);
/// An Electric move whose `EFFECT_ALWAYS_HIT` skips only the accuracy draw;
/// crit, damage-variance, and effect-chance draws still follow. Charge
/// itself draws nothing.
const SHOCK_WAVE: MoveId = MoveId(351);

/// Grass/Poison; Overgrow occupies ability slot 0, the slot every
/// [`max_iv_mon`] (even personality) call selects.
const BULBASAUR: u16 = 1;
const SQUIRTLE: u16 = 7;
const RATTATA: u16 = 19;
/// Clear Body occupies ability slot 0 and Liquid Ooze slot 1, so these
/// tests pass [`LIQUID_OOZE_PERSONALITY`] to field Liquid Ooze instead.
const TENTACOOL: u16 = 72;
const MACHOP: u16 = 66;

/// `CreateBoxMon` selects a two-ability species' ability slot as
/// `personality & 1`; every Liquid Ooze test passes this odd value to
/// [`max_iv_mon_with_personality`] to field Tentacool's slot-1 ability
/// instead of its slot-0 Clear Body.
const LIQUID_OOZE_PERSONALITY: u32 = 1;

/// An overkill Absorb heals half the HP the target actually lost --
/// upstream's `Cmd_datahpupdate` cap on `gHpDealt` -- never half of
/// `gBattleMoveDamage`'s raw formula output, a distinction only a real turn
/// can expose.
#[test]
fn an_overkill_absorb_drains_half_the_hp_actually_removed() {
    let dex = Dex::new();
    // A level-50 Bulbasaur's Absorb computes far more than 5 damage; the
    // attacker is 10 HP down, so the heal is not max-HP clamped either.
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    player.apply_damage(10);
    let mut enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;
    enemy.apply_damage(enemy_max_hp - 5);

    // Absorb's 3 (accuracy, crit, damage roll); the enemy faints and never
    // gets to act.
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
    let mut enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
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

/// Liquid Ooze turns the same magnitude into damage on the attacker, with
/// `LiquidOoze` replacing `Drained` -- never both -- as the script's
/// alternate message-table entry.
#[test]
fn liquid_ooze_turns_the_drain_into_damage_on_the_attacker() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    player.apply_damage(30);
    let enemy =
        max_iv_mon_with_personality(&dex, TENTACOOL, 5, vec![TACKLE], LIQUID_OOZE_PERSONALITY);
    let enemy_max_hp = enemy.stats().max_hp;
    assert_eq!(enemy.ability(), battle::LIQUID_OOZE);
    assert_eq!(enemy_max_hp, 20, "Tentacool's level-5 maximum HP");

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

/// `tryfaintmon BS_ATTACKER` runs before `tryfaintmon BS_TARGET`
/// (`data/battle_scripts_1.s:358`-`:359`), so a Liquid-Ooze attacker that
/// kills itself still faints first in the event stream even though both
/// faints are reported. A simultaneous double faint resolves through the
/// same loss handler as an outright defeat, never the win path
/// (`battle_script_commands.c:3560`-`:3573`, `battle_main.c:557`-`:559`).
#[test]
fn a_liquid_ooze_kill_faints_the_attacker_before_the_target() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let player_max_hp = player.stats().max_hp;
    // Leave the attacker on less HP than the 10 the ooze will take.
    player.apply_damage(player_max_hp - 6);
    // These survive until the reset assertions below; the crit draw of 1
    // reads as non-critical at Focus Energy's stage as well as at stage 0.
    player.stages_mut().attack = StatStage::new(2).unwrap();
    player.volatiles_mut().set_focus_energy();
    player.volatiles_mut().set_charge();
    let mut enemy =
        max_iv_mon_with_personality(&dex, TENTACOOL, 5, vec![TACKLE], LIQUID_OOZE_PERSONALITY);
    enemy.stages_mut().attack = StatStage::new(2).unwrap();
    enemy.volatiles_mut().set_focus_energy();
    enemy.volatiles_mut().set_charge();

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
            BattleEvent::Fainted { by_player: false },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "both tryfaintmons run and report -- the attacker's faint is \
         settled first in the event stream, but the target's still shows \
         up, and a simultaneous double faint resolves as a defeat, not a \
         win, no matter which side is the attacker"
    );
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(battle.enemy().current_hp(), 0, "the target died too");
    // Double-faint settlement must clear scratch state on both corpses like
    // the ordinary faint path does.
    assert_eq!(battle.player().stages(), StatStages::default());
    assert_eq!(battle.enemy().stages(), StatStages::default());
    assert_eq!(battle.player().volatiles(), Volatiles::default());
    assert_eq!(battle.enemy().volatiles(), Volatiles::default());
}

/// Mirrors the previous test with attacker and target swapped: the *enemy*
/// drains a Liquid-Ooze player here. `checkteamslost` scores a simultaneous
/// double faint as a loss from each side's own total HP
/// (`battle_script_commands.c:3560`-`:3573`), not from who dealt the
/// finishing blow, so this still resolves as `PlayerLost`, never
/// `PlayerWon`.
#[test]
fn a_liquid_ooze_kill_by_the_enemy_still_resolves_as_a_loss() {
    let dex = Dex::new();
    let player =
        max_iv_mon_with_personality(&dex, TENTACOOL, 5, vec![TACKLE], LIQUID_OOZE_PERSONALITY);
    let mut enemy = max_iv_mon(&dex, BULBASAUR, 50, vec![ABSORB]);
    let enemy_max_hp = enemy.stats().max_hp;
    enemy.apply_damage(enemy_max_hp - 6);

    // Bulbasaur (enemy, level 50) outruns Tentacool (player, level 5), so
    // the enemy's Absorb resolves first and the player's Tackle never
    // runs -- the battle is over before its turn-order slot comes up.
    // Absorb's 3 (accuracy, crit, damage roll) match the test above's
    // script: the arithmetic depends on species, not which struct field
    // holds it.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        [
            BattleEvent::Hit {
                by_player: false,
                move_id: ABSORB,
                damage: 20,
                is_critical: false,
            },
            BattleEvent::LiquidOoze {
                by_player: false,
                move_id: ABSORB,
                damage: 6,
            },
            BattleEvent::Fainted { by_player: false },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the enemy (the attacker here) faints first in the event stream, \
         but the outcome is still PlayerLost, not PlayerWon -- a \
         simultaneous double faint is a defeat no matter which side \
         dealt the finishing blow"
    );
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(battle.enemy().current_hp(), 0);
}

/// The same Absorb from the same battler deals more, and drains
/// proportionally more, once its HP crosses Overgrow's third-of-max-HP
/// gate -- pinned as named constants in `battle::drain`'s own unit tests;
/// this pin exercises only the turn-level wiring.
#[test]
fn overgrow_boosts_the_players_absorb_once_its_hp_is_low() {
    /// `hp <= max_hp / 3` -- Bulbasaur's level-5 21 maximum HP puts this at 7.
    const OVERGROW_HP_GATE: u32 = 7;

    // Absorb's 3 + the enemy's Tackle (4); Bulbasaur (speed 11) outruns
    // Squirtle (speed 10), so no speed-tie draw.
    let script = [0, 0, 0, 0, 1, 0, 0, 1, 0, 0];

    let run = |remaining_hp: u32| {
        let dex = Dex::new();
        let mut player = max_iv_mon(&dex, BULBASAUR, 5, vec![ABSORB]);
        let player_max_hp = player.stats().max_hp;
        assert_eq!(player_max_hp, 21, "Bulbasaur's level-5 maximum HP");
        player.apply_damage(player_max_hp - remaining_hp);
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
    let (events, hp) = run(OVERGROW_HP_GATE + 1);
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
        OVERGROW_HP_GATE + 1 + 4 - 4,
        "healed 4, then took the enemy's 4-damage Tackle"
    );

    // Exactly at the gate: Overgrow fires.
    let (events, hp) = run(OVERGROW_HP_GATE);
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
    assert_eq!(hp, OVERGROW_HP_GATE + 6 - 4);
}

/// A fixed-damage move deals its literal figure and spends only 2 draws
/// (accuracy, then the discarded effect chance) where an ordinary damaging
/// move spends 4 -- no crit roll, no damage roll.
#[test]
fn sonic_boom_deals_a_flat_twenty_for_two_draws() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 50, vec![SONIC_BOOM]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 50, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;

    // Sonic Boom's 2 + the enemy's Tackle (4).
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

    // Double Slap: accuracy, hit count (mask 0 -> 2 hits), crit+roll per
    // hit, one trailing effect chance; then the enemy's Tackle (4).
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

/// `jumpifhasnohp BS_TARGET` is checked at the top of each loop iteration:
/// the hit that knocks the target out still completes and is reported, but
/// the iterations after it are abandoned and spend no draws at all -- a
/// third iteration's crit draw would run off the end of this script and
/// panic.
#[test]
fn a_multi_hit_move_abandons_its_remaining_hits_without_spending_their_draws() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![DOUBLE_SLAP]);
    let mut enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);
    enemy.apply_damage(enemy.stats().max_hp - 5);

    // Accuracy, hit count (mask 3 -> redraw, mask 3 again -> 5 hits), then
    // crit+roll for each of the two hits this 5-HP target can actually
    // absorb, and the trailing effect chance.
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

#[test]
fn splash_reports_that_nothing_happened_and_draws_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![SPLASH]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    // Splash's zero + the enemy's Tackle (4).
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

/// Focus Energy's crit-stage volatile is a latch that raises every later
/// move's crit chance: a draw of 4 does not crit at stage 0 (1/16) but does
/// at stage 2 (1/4). The
/// control run swaps in Splash so every other draw stays identical,
/// isolating the volatile as the only explanation for the difference.
#[test]
fn focus_energy_raises_the_next_moves_crit_chance() {
    let script = [
        0, // battle start
        0, 0, 0, 1, 0, 0, // turn 1: the flag move's zero + the enemy's Tackle (4)
        0, 0, 0, 4, 0, 0, 0, 1, 0, 0, // turn 2: Tackle (crit draw 4) + enemy's Tackle (4)
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

/// A second Focus Energy fails from the script's own `jumpifstatus2` gate
/// (`data/battle_scripts_1.s:889`), not an unreachable branch in
/// `Cmd_setfocusenergy`.
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

/// Charge doubles an Electric move for its own turn and exactly one turn
/// after, then expires -- `ENDTURN_CHARGE` decrements the timer once per
/// end of turn (`src/battle_util.c:1743`-`:1745`).
#[test]
fn charge_doubles_the_next_turns_electric_move_and_then_expires() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 20, vec![CHARGE, SHOCK_WAVE]);
    let enemy = max_iv_mon(&dex, MACHOP, 20, vec![TACKLE]);

    // turn 1: Charge's zero + the enemy's Tackle (4).
    // turns 2-3: Shock Wave's 3 (EFFECT_ALWAYS_HIT skips the accuracy
    //            draw) + the enemy's Tackle (4).
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

/// A failed run still burns the turn, and upstream still runs
/// `DoBattlerEndTurnEffects` for it -- gated only on `gBattleOutcome == 0`,
/// which a merely-failed escape never sets
/// (`src/battle_main.c:3961`-`:3968`) -- so Charge's timer ticks down on
/// this turn exactly as it would on any other.
#[test]
fn a_failed_run_still_ticks_the_charge_timer_down() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MACHOP, 20, vec![CHARGE, SHOCK_WAVE]);
    let enemy = max_iv_mon(&dex, BULBASAUR, 20, vec![TACKLE]);

    // Bulbasaur is faster, so it acts first in both non-Run turns.
    // turn 1: Bulbasaur's Tackle (4) + Charge (0 draws).
    // turn 2: the escape roll (200; always fails -- Machop's raw speed 25
    //         is slower than Bulbasaur's 29, so speedVar = 25*128/29 = 110
    //         and 110 > 200 is false) + Bulbasaur's Tackle (4). This is the
    //         turn the fix must still tick Charge's timer down on.
    // turn 3: Bulbasaur's Tackle (4) + Shock Wave's 3 (EFFECT_ALWAYS_HIT
    //         skips the accuracy draw).
    let script = [
        0, // battle start
        0, 0, 0, 1, 0, 0, // turn 1
        0, 0, 200, 0, 1, 0, 0, // turn 2
        0, 0, 0, 1, 0, 0, 1, 0, 0, // turn 3
    ];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        turn1[1],
        BattleEvent::ChargingPower {
            by_player: true,
            move_id: CHARGE,
        }
    );

    let turn2 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        turn2[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        },
        "the escape roll must fail for this test to exercise the residual \
         path at all"
    );

    let turn3 = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    let shock_wave_damage = match turn3[1] {
        BattleEvent::Hit {
            by_player: true,
            damage,
            ..
        } => damage,
        ref other => panic!("expected the player's Shock Wave hit, got {other:?}"),
    };
    assert_eq!(
        shock_wave_damage, 5,
        "Charge's timer must have ticked down across the failed-run turn \
         (2 -> 1 on the Charge turn, 1 -> 0 on the failed-run turn), so \
         this Shock Wave lands plain, not doubled -- an engine that skips \
         residuals on a failed run leaves the timer at 1 and doubles this \
         hit to 10"
    );
    assert_eq!(rng.draws(), script.len());
}

#[test]
fn defense_curl_sets_its_volatile_and_raises_defense_and_draws_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![DEFENSE_CURL]);
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    // Defense Curl's zero + the enemy's Tackle (4).
    let script = [0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::StatRose {
            by_player: true,
            move_id: DEFENSE_CURL,
            stat: ChangedStat::Defense,
            new_stage: StatStage::new(1).unwrap(),
            magnitude: 1,
        }
    );
    assert!(
        battle.player().volatiles().defense_curl,
        "setdefensecurlbit must have run"
    );
    assert_eq!(battle.player().stages().defense, StatStage::new(1).unwrap());
    assert_eq!(battle.player().moves()[0].pp, 39, "a PP was still spent");
    assert_eq!(rng.draws(), script.len());
}

/// `setdefensecurlbit` precedes `statbuffchange` in the script
/// unconditionally (`data/battle_scripts_1.s:2017`-`:2019`), so the volatile
/// is written even when Defense is already at its ceiling and the only
/// resulting event is `StatWontGoHigher` -- the upstream command has no
/// failure branch of its own to skip.
#[test]
fn defense_curl_still_sets_its_volatile_when_defense_is_already_capped() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, BULBASAUR, 5, vec![DEFENSE_CURL]);
    player.stages_mut().defense = StatStage::MAX;
    let enemy = max_iv_mon(&dex, SQUIRTLE, 5, vec![TACKLE]);

    let script = [0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::StatWontGoHigher {
            by_player: true,
            move_id: DEFENSE_CURL,
            stat: ChangedStat::Defense,
        }
    );
    assert!(
        battle.player().volatiles().defense_curl,
        "the volatile write must not be skipped just because the raise is capped"
    );
    assert_eq!(battle.player().stages().defense, StatStage::MAX);
    assert_eq!(rng.draws(), script.len());
}
