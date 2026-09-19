//! Move-resolution results and the events exposed to callers.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, STRUGGLE};

#[test]
fn every_move_event_names_the_move_that_was_used() {
    let dex = Dex::new();
    // Slow player (Bulbasaur), so the run fails and the *enemy* acts -- the
    // side whose move a caller cannot otherwise know, since it comes out of
    // the rejection loop rather than from the caller.
    let player = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch
    let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(10),
                ..
            }
        )),
        "the enemy's rejection-loop pick (Scratch, slot 1) must be named: {events:?}"
    );

    // And the player's own move on a miss (accuracy roll 95 -> 96 > 95).
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 95, 95]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events[0],
        BattleEvent::Missed {
            by_player: true,
            move_id: MoveId(33),
        }
    );
    // A miss still spends PP: BattleScript_PrintMoveMissed re-runs
    // attackstring/ppreduce (`battle_scripts_1.s:273`-`:275`), so the
    // deduction survives the failed accuracycheck at `:244`.
    assert_eq!(battle.player().moves()[0].pp, 34);
}

// The first mover's hit commits, and the forced Struggle that follows it in
// turn order executes in the same turn.

#[test]
fn a_forced_struggle_follows_the_first_movers_hit_in_the_same_turn() {
    let dex = Dex::new();
    // Rattata (speed 13) moves first; Bulbasaur (speed 11) second, with
    // every slot spent -- upstream forces Struggle for it at selection
    // time (drawing nothing), and its own turn-order slot runs right
    // after the first mover's.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]);
    let enemy_hp = enemy.current_hp();
    let player_hp = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]).current_hp();
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // 1 (battle start) + turn number + 4 (the player's ordinary hit) + 3
    // (the enemy's forced Struggle: accuracy, crit, damage-variance -- no
    // trailing effect-chance draw and no selection draw for the forced
    // pick). The script is exhausted, so a stray draw would panic.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 7,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: STRUGGLE,
                damage: 6,
                is_critical: false,
            },
            BattleEvent::Recoil {
                by_player: false,
                move_id: STRUGGLE,
                damage: 1,
            },
        ],
        "the first mover's ordinary hit and the forced Struggle that \
         follows it must both commit"
    );
    // ...and both really did commit: HP and PP moved on both sides.
    assert_eq!(battle.enemy().current_hp(), enemy_hp - 7 - 1);
    assert_eq!(battle.player().current_hp(), player_hp - 6);
    assert_eq!(battle.player().moves()[0].pp, 34);
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the forced pick spends no PP"
    );
    assert_eq!(rng.draws(), 9);
    assert!(battle.outcome().is_none());
}

#[test]
fn an_immune_first_hit_reports_no_effect_and_the_turn_continues() {
    let dex = Dex::new();
    // Rattata L10 (speed 22) outspeeds Gastly L5 (speed 14). The
    // player's Tackle cannot touch the Ghost (NoEffect), but the turn
    // does not end there: the second mover still acts.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 92, 5, vec![MoveId(33)]);
    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, the player's immune hit
    // (still 4 draws -- see crate::hit), the enemy's ordinary hit (4).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Gastly L5 Tackle into Rattata L10, hand computed: attack
    // (2*35+31)*5/100+5 = 10; defense (2*35+31)*10/100+5 = 15;
    // 10*35 = 350, *(2*5/5+2 = 4) = 1400, /15 = 93, /50 = 1, +2 = 3;
    // no STAB (Gastly is Ghost/Poison, Tackle Normal), neutral, 100%.
    assert_eq!(
        events,
        vec![
            BattleEvent::NoEffect {
                by_player: true,
                move_id: MoveId(33),
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(battle.player().current_hp(), player_hp_before - 3);
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "an immune hit deals nothing"
    );
    assert_eq!(rng.draws(), 11);
    assert!(battle.outcome().is_none(), "nobody fainted; no Ended event");
    // A type-immune hit still spends PP: ppreduce (`battle_scripts_1.s:247`)
    // runs before typecalc (`:251`) decides the immunity.
    assert_eq!(battle.player().moves()[0].pp, 34);
}

#[test]
fn an_overkill_hit_reports_only_the_hp_actually_lost() {
    let dex = Dex::new();
    // Level 50 Charmander's Tackle against a level 2 Rattata computes
    // far more damage than the Rattata's max HP; the Hit event must
    // report the HP actually lost (the cap), not the raw formula result.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);
    let enemy_max_hp = enemy.stats().max_hp;

    // Battle-start turn number; turn's turn number; opponent's move
    // pick; player's hit (accuracy pass / no crit / best damage roll /
    // effect chance).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let hit_damage = events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: true,
                damage,
                ..
            } => Some(*damage),
            _ => None,
        })
        .expect("the player's one-shot hit must be reported");
    assert_eq!(
        hit_damage, enemy_max_hp,
        "an overkill KO reports the defender's whole HP bar, never more"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn a_ground_multi_hit_move_does_not_affect_a_levitate_holder() {
    let dex = Dex::new();
    // `MOVE_BONE_RUSH` (`EFFECT_MULTI_HIT`, Ground) against Gastly, whose
    // only ability slot is Levitate (its second slot is `NONE`, so
    // `ability()` always resolves to Levitate regardless of personality).
    // Rattata L10 (speed 22) outspeeds Gastly L5 (speed 14), so the player
    // moves first.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(198)]);
    let enemy = max_iv_mon(&dex, 92, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, then the multi-hit pipeline's
    // four draws for the player's first (and only attempted) hit --
    // accuracy, hit-count offset, the first attempt's crit roll, and the
    // trailing effect chance, matching `Cmd_typecalc`'s Levitate branch
    // (`battle_script_commands.c:1375`-`:1383`), which zeroes the move
    // before `adjustnormaldamage` and the multi-hit script's
    // `jumpifmovehadnoeffect` stops the loop before a second hit is
    // attempted -- then the enemy's ordinary Tackle (4).
    let script = [0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            } | BattleEvent::MultiHit {
                by_player: true,
                ..
            }
        )),
        "Levitate makes every Ground hit ineffective, so no hit or hit-count \
         event may report from the player's side: {events:?}"
    );
    assert_eq!(
        events[0],
        BattleEvent::LevitateBlocked {
            by_player: true,
            move_id: MoveId(198),
        },
        "the multi-hit loop must stop at the Levitate-block branch on its \
         first attempt: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "a Levitate holder takes no Ground damage"
    );
    assert_eq!(rng.draws(), script.len());
    // A no-effect hit still spends PP: ppreduce runs before typecalc
    // decides the immunity, exactly as the ordinary single-hit case does.
    assert_eq!(battle.player().moves()[0].pp, 9);
}

/// `Cmd_typecalc`'s Levitate branch (`battle_script_commands.c:1375-1383`)
/// sets `B_MSG_GROUND_MISS`, not the ordinary type-immunity string
/// (`battle_message.c:71` vs. `:73`), so it must not collapse into
/// [`BattleEvent::NoEffect`].
#[test]
fn a_levitate_block_is_reported_distinctly_from_a_typing_immunity() {
    let dex = Dex::new();
    // Bone Rush (Ground, MULTI_HIT) into Gastly, whose only ability is
    // Levitate; Rattata L10 outspeeds Gastly L5.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(198)]);
    let enemy = max_iv_mon(&dex, 92, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Levitate still takes no Ground damage"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::NoEffect {
                by_player: true,
                ..
            }
        )),
        "a Levitate block must not be reported as the ordinary typing \
         immunity event: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::LevitateBlocked {
            by_player: true,
            move_id: MoveId(198),
        }),
        "the Levitate block must be reported distinctly: {events:?}"
    );
}

/// `Cmd_typecalc`'s Wonder Guard branch (`battle_script_commands.c:1409-1418`)
/// blocks any powered move that is not strictly super effective, so a
/// neutral Water Gun must leave Shedinja's HP untouched.
#[test]
fn wonder_guard_blocks_a_neutral_ordinary_hit() {
    let dex = Dex::new();
    // Water Gun (Water, `EFFECT_HIT`, power 40) into Shedinja, whose only
    // ability slot is Wonder Guard (its second slot is `NONE`). Water is
    // neutral on both Bug and Ghost, so the type chart alone would let the
    // hit through. Rattata L10 (speed 22) outspeeds Shedinja L5, the same
    // pairing `an_immune_first_hit_reports_no_effect_and_the_turn_continues`
    // uses against Gastly.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(55)]);
    let enemy = max_iv_mon(&dex, 303, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, the player's blocked hit
    // (accuracy, crit, damage-variance, and effect-chance draws, exactly
    // like an ordinary hit -- see crate::hit), the enemy's ordinary Tackle
    // (4 draws).
    let script = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "no player hit may be reported against a Wonder Guard holder: {events:?}"
    );
    assert_eq!(
        events[0],
        BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: MoveId(55),
        },
        "the Wonder Guard block must be reported distinctly: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Wonder Guard takes no damage from a hit that is not strictly super \
         effective: {events:?}"
    );
    assert_eq!(rng.draws(), script.len());
}

/// The same Wonder Guard branch reaches fixed damage, which upstream also
/// gates on `gBattleMoves[gCurrentMove].power` -- always nonzero for Dragon
/// Rage (`crates/battle/src/fixed_damage.rs`).
#[test]
fn wonder_guard_blocks_a_neutral_fixed_damage_move() {
    let dex = Dex::new();
    // Dragon Rage (Dragon, `EFFECT_DRAGON_RAGE`, literal 40) into Shedinja:
    // the chart has no Dragon-versus-Bug or Dragon-versus-Ghost row, so the
    // matchup is neutral and only Wonder Guard can block it.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(82)]);
    let enemy = max_iv_mon(&dex, 303, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, Dragon Rage's accuracy and
    // (discarded) effect-chance draws, the enemy's ordinary Tackle (4).
    let script = [0, 0, 0, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "no player hit may be reported against a Wonder Guard holder: {events:?}"
    );
    assert_eq!(
        events[0],
        BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: MoveId(82),
        },
        "the Wonder Guard block must be reported distinctly: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Wonder Guard takes no fixed damage from a neutral matchup: {events:?}"
    );
    assert_eq!(rng.draws(), script.len());
}

/// Wonder Guard permits a strictly super-effective hit through unblocked, so
/// the block above is a targeted admission check, not a general immunity.
#[test]
fn wonder_guard_permits_a_super_effective_hit() {
    let dex = Dex::new();
    // Faint Attack (Dark, `EFFECT_ALWAYS_HIT`, power 60) is super effective
    // against Shedinja's Ghost type and neutral against its Bug type, so the
    // aggregate bucket is `SuperEffective` and Wonder Guard must let it
    // through. Shedinja's HP is always 1, so any landed hit faints it.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(185)]);
    let enemy = max_iv_mon(&dex, 303, 5, vec![MoveId(33)]);

    // battle start, turn number, enemy pick, Faint Attack's always-hit crit,
    // damage-variance, and effect-chance draws -- no accuracy draw, and no
    // enemy turn once Shedinja faints.
    let script = [0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "a strictly super-effective hit must land: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::WonderGuardBlocked { .. })),
        "Wonder Guard must not block a strictly super-effective hit: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// The multi-hit pipeline reaches the same Wonder Guard check through
/// [`crate::hit::damage_before_roll`]'s shared boundary, mirroring
/// `a_ground_multi_hit_move_does_not_affect_a_levitate_holder`.
#[test]
fn wonder_guard_stops_a_multi_hit_move_on_its_first_attempt() {
    let dex = Dex::new();
    // Pin Missile (Bug, `EFFECT_MULTI_HIT`) into Shedinja: Bug has no chart
    // row against Bug and is not-very-effective against Ghost, so the
    // aggregate bucket is `NotVeryEffective`, not `SuperEffective`.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(42)]);
    let enemy = max_iv_mon(&dex, 303, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, then the multi-hit pipeline's
    // four draws for the player's first (and only attempted) hit --
    // accuracy, hit-count offset, the first attempt's crit roll, and the
    // trailing effect chance -- then the enemy's ordinary Tackle (4).
    let script = [0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            } | BattleEvent::MultiHit {
                by_player: true,
                ..
            }
        )),
        "Wonder Guard blocks every attempted hit, so no hit or hit-count \
         event may report from the player's side: {events:?}"
    );
    assert_eq!(
        events[0],
        BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: MoveId(42),
        },
        "the multi-hit loop must stop at the Wonder Guard branch on its \
         first attempt: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "a Wonder Guard holder takes no damage from a not-very-effective \
         multi-hit move"
    );
    assert_eq!(rng.draws(), script.len());
}

/// Serene Grace's preflight refusal (`secondary::ensure_admissible`) must
/// not fire when Wonder Guard would foreclose the secondary anyway: the
/// block carries `MOVE_RESULT_MISSED`, so the poison chance never gets a
/// chance to apply (`battle_script_commands.c:1409-1418`).
#[test]
fn wonder_guard_admits_a_serene_grace_poison_hit_move() {
    let dex = Dex::new();
    // Dunsparce (species 206): Serene Grace in its primary ability slot.
    // Poison Sting (MoveId 40, `EFFECT_POISON_HIT`) is not-very-effective
    // against Shedinja's Ghost type and has no chart row against Bug, so
    // only Wonder Guard blocks it.
    let player = max_iv_mon(&dex, 206, 10, vec![MoveId(40)]);
    let enemy = max_iv_mon(&dex, 303, 5, vec![MoveId(33)]);
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, the player's blocked hit
    // (accuracy, crit, damage-variance, and effect-chance draws, exactly
    // like an ordinary hit -- see crate::hit), the enemy's ordinary Tackle
    // (4 draws).
    let script = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: MoveId(40),
        },
        "the turn must run and report Wonder Guard's block, not refuse \
         admission over Serene Grace: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Wonder Guard takes no damage from a hit that is not strictly \
         super effective: {events:?}"
    );
    assert_eq!(rng.draws(), script.len());
}
