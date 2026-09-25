//! Which `BattleEvent` a resolved move reports, and how the turn continues
//! around it.
//!
//! The admission rules and formulas live in `battle::hit`, `battle::damage`,
//! `battle::multi_hit`, and `battle::ability`; a hit's own draw order is
//! pinned in `crate::hit`. What is pinned here is the turn wiring those
//! cannot reach on their own: event classification (`Hit`, `Missed`,
//! `NoEffect`, `LevitateBlocked`, `WonderGuardBlocked`, `MultiHit`), PP spent
//! on a miss or a block, forced Struggle, and an overkill hit's reported
//! damage.

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, STRUGGLE};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_SCRATCH`.
const SCRATCH: MoveId = MoveId(10);
/// `MOVE_BONE_RUSH` (`EFFECT_MULTI_HIT`, Ground, 80 accuracy).
const BONE_RUSH: MoveId = MoveId(198);
/// `MOVE_WATER_GUN` (`EFFECT_HIT`, Water, power 40): neutral against both
/// Bug and Ghost.
const WATER_GUN: MoveId = MoveId(55);
/// `MOVE_DRAGON_RAGE` (`EFFECT_DRAGON_RAGE`, fixed 40 damage): neutral
/// against Bug and Ghost (no chart row).
const DRAGON_RAGE: MoveId = MoveId(82);
/// `MOVE_FAINT_ATTACK` (`EFFECT_ALWAYS_HIT`, Dark, power 60): super
/// effective against Ghost, neutral against Bug.
const FAINT_ATTACK: MoveId = MoveId(185);
/// `MOVE_PIN_MISSILE` (`EFFECT_MULTI_HIT`, Bug): not very effective against
/// Ghost, neutral against Bug.
const PIN_MISSILE: MoveId = MoveId(42);
/// `MOVE_POISON_STING` (`EFFECT_POISON_HIT`, 30% secondary chance): not
/// very effective against Ghost, neutral against Bug.
const POISON_STING: MoveId = MoveId(40);

/// `SPECIES_BULBASAUR`.
const BULBASAUR: u16 = 1;
/// `SPECIES_CHARMANDER`.
const CHARMANDER: u16 = 4;
/// `SPECIES_RATTATA`: the fast mover against [`GASTLY`] and [`SHEDINJA`] in
/// this file's fixtures.
const RATTATA: u16 = 19;
/// `SPECIES_GASTLY`: Ghost/Poison (immune to Normal-type damage), Levitate
/// in its only ability slot. Base Speed 80 is higher than [`RATTATA`]'s 72,
/// but the level-10-versus-5 gap used here still makes Rattata faster.
const GASTLY: u16 = 92;
/// `SPECIES_DUNSPARCE`: Serene Grace in its primary ability slot (secondary
/// slot is Run Away).
const DUNSPARCE: u16 = 206;
/// `SPECIES_SHEDINJA`: Bug/Ghost, Wonder Guard in its only ability slot;
/// always 1 HP, so any landed hit faints it.
const SHEDINJA: u16 = 303;

/// A roll that fails a run attempt.
const RUN_ROLL_FAILS: u16 = 65000;
/// A roll that fails [`TACKLE`]'s 95 accuracy (`95 % 100 + 1 == 96`).
const TACKLE_ACCURACY_ROLL_MISSES: u16 = 95;
/// A roll that fails [`BONE_RUSH`]'s 80 accuracy (`80 % 100 + 1 == 81`).
const BONE_RUSH_ACCURACY_ROLL_MISSES: u16 = 80;

/// Battle start, turn number, the enemy's rejection-loop pick (index 1,
/// [`SCRATCH`]), the run roll failing, then the enemy's ordinary hit.
const RUN_FAILS_THEN_ENEMY_SCRATCH_HITS: [u16; 8] = [0, 0, 1, RUN_ROLL_FAILS, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, then both battlers'
/// [`TACKLE`] missing in turn order.
const BOTH_TACKLES_MISS: [u16; 5] = [
    0,
    0,
    0,
    TACKLE_ACCURACY_ROLL_MISSES,
    TACKLE_ACCURACY_ROLL_MISSES,
];
/// Battle start and turn number, then the player's ordinary hit (accuracy,
/// crit, damage-variance, effect-chance) and the enemy's forced Struggle
/// (accuracy, crit, damage-variance, no trailing effect-chance). Struggle
/// draws no selection pick: `choose_enemy_move` returns `None` before
/// drawing once every move is spent
/// (`crates/battle/src/battle/opponent_ai.rs`).
const FORCED_STRUGGLE_FOLLOWS_THE_FIRST_HIT: [u16; 9] = [0, 0, 0, 1, 0, 0, 0, 1, 0];
/// Battle start, turn number, the enemy's pick, then both battlers' ordinary
/// hits landing in turn order.
const BOTH_BATTLERS_HIT: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, then the player's one-shot
/// hit; the enemy faints before its own turn.
const PLAYER_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, then the player's first (and
/// only attempted) multi-hit swing -- accuracy, hit-count offset, crit, and
/// effect-chance -- stopped after one attempt, then the enemy's ordinary
/// hit.
const MULTI_HIT_STOPS_AT_FIRST_ATTEMPT: [u16; 11] = [0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, [`DRAGON_RAGE`]'s accuracy
/// and (discarded) effect-chance draws -- fixed damage skips the
/// crit/variance draws -- then the enemy's ordinary hit.
const FIXED_DAMAGE_MOVE_THEN_ENEMY_HIT: [u16; 9] = [0, 0, 0, 0, 0, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, then [`FAINT_ATTACK`]'s
/// always-hit crit, damage-variance, and effect-chance draws; the enemy
/// faints before its own turn.
const ALWAYS_HIT_FAINTS_BEFORE_ENEMY_TURN: [u16; 6] = [0, 0, 0, 1, 0, 0];
/// Battle start, turn number, the enemy's pick, [`BONE_RUSH`]'s single
/// failed accuracy roll (no hit-count, crit, or effect-chance draw behind a
/// miss), then the enemy's ordinary hit.
const MULTI_HIT_MOVE_MISSES_THEN_ENEMY_HIT: [u16; 8] =
    [0, 0, 0, BONE_RUSH_ACCURACY_ROLL_MISSES, 0, 1, 0, 0];

#[test]
fn every_move_event_names_the_move_that_was_used() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE, SCRATCH]);
    let mut rng = SequenceRng::new(RUN_FAILS_THEN_ENEMY_SCRATCH_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                move_id: SCRATCH,
                ..
            }
        )),
        "the enemy's rejection-loop pick must be named: {events:?}"
    );

    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(BOTH_TACKLES_MISS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events[0],
        BattleEvent::Missed {
            by_player: true,
            move_id: TACKLE,
        }
    );
    // A missed move still spends PP: `BattleScript_PrintMoveMissed` re-runs
    // `ppreduce` after the failed `accuracycheck`
    // (`battle_scripts_1.s:244`,`:273-275`).
    assert_eq!(battle.player().moves()[0].pp, 34);
}

#[test]
fn a_forced_struggle_follows_the_first_movers_hit_in_the_same_turn() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    let enemy_hp = enemy.current_hp();
    let player_hp = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]).current_hp();
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    let mut rng = SequenceRng::new(FORCED_STRUGGLE_FOLLOWS_THE_FIRST_HIT);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
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
    assert_eq!(battle.enemy().current_hp(), enemy_hp - 7 - 1);
    assert_eq!(battle.player().current_hp(), player_hp - 6);
    assert_eq!(battle.player().moves()[0].pp, 34);
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the forced pick spends no PP"
    );
    assert_eq!(rng.draws(), FORCED_STRUGGLE_FOLLOWS_THE_FIRST_HIT.len());
    assert!(battle.outcome().is_none());
}

#[test]
fn an_immune_first_hit_reports_no_effect_and_the_turn_continues() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(BOTH_BATTLERS_HIT);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let enemy_tackle_damage = 3;
    assert_eq!(
        events,
        vec![
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                damage: enemy_tackle_damage,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        battle.player().current_hp(),
        player_hp_before - enemy_tackle_damage
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "an immune hit deals nothing"
    );
    assert_eq!(rng.draws(), BOTH_BATTLERS_HIT.len());
    assert!(battle.outcome().is_none(), "nobody fainted; no Ended event");
    assert_eq!(battle.player().moves()[0].pp, 34);
}

#[test]
fn an_overkill_hit_reports_only_the_hp_actually_lost() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;

    let mut rng = SequenceRng::new(PLAYER_ACTS_ALONE);
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
    let player = max_iv_mon(&dex, RATTATA, 10, vec![BONE_RUSH]);
    let enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(MULTI_HIT_STOPS_AT_FIRST_ATTEMPT);
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
            move_id: BONE_RUSH,
        },
        "the multi-hit loop must stop at the Levitate-block branch on its \
         first attempt: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "a Levitate holder takes no Ground damage"
    );
    assert_eq!(rng.draws(), MULTI_HIT_STOPS_AT_FIRST_ATTEMPT.len());
    assert_eq!(battle.player().moves()[0].pp, 9);
}

/// Levitate selects the Ground-miss result rather than the ordinary
/// type-immunity result in `Cmd_typecalc` (`battle_script_commands.c:1375-1383`;
/// `battle_message.c:71,73`), so it must not collapse into
/// [`BattleEvent::NoEffect`].
#[test]
fn a_levitate_block_is_reported_distinctly_from_a_typing_immunity() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![BONE_RUSH]);
    let enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let enemy_hp_before = enemy.current_hp();
    let mut rng = SequenceRng::new(MULTI_HIT_STOPS_AT_FIRST_ATTEMPT);
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
            move_id: BONE_RUSH,
        }),
        "the Levitate block must be reported distinctly: {events:?}"
    );
}

/// Wonder Guard blocks any powered move that is not strictly super
/// effective, in `Cmd_typecalc` (`battle_script_commands.c:1409-1418`), so a
/// neutral Water Gun must leave Shedinja's HP untouched.
#[test]
fn wonder_guard_blocks_a_neutral_ordinary_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![WATER_GUN]);
    let enemy = max_iv_mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(BOTH_BATTLERS_HIT);
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
            move_id: WATER_GUN,
        },
        "the Wonder Guard block must be reported distinctly: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Wonder Guard takes no damage from a hit that is not strictly super \
         effective: {events:?}"
    );
    assert_eq!(rng.draws(), BOTH_BATTLERS_HIT.len());
}

#[test]
fn wonder_guard_blocks_a_neutral_fixed_damage_move() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![DRAGON_RAGE]);
    let enemy = max_iv_mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(FIXED_DAMAGE_MOVE_THEN_ENEMY_HIT);
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
            move_id: DRAGON_RAGE,
        },
        "the Wonder Guard block must be reported distinctly: {events:?}"
    );
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "Wonder Guard takes no fixed damage from a neutral matchup: {events:?}"
    );
    assert_eq!(rng.draws(), FIXED_DAMAGE_MOVE_THEN_ENEMY_HIT.len());
}

#[test]
fn wonder_guard_permits_a_super_effective_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![FAINT_ATTACK]);
    let enemy = max_iv_mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);

    let mut rng = SequenceRng::new(ALWAYS_HIT_FAINTS_BEFORE_ENEMY_TURN);
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

#[test]
fn wonder_guard_stops_a_multi_hit_move_on_its_first_attempt() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![PIN_MISSILE]);
    let enemy = max_iv_mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(MULTI_HIT_STOPS_AT_FIRST_ATTEMPT);
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
            move_id: PIN_MISSILE,
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
    assert_eq!(rng.draws(), MULTI_HIT_STOPS_AT_FIRST_ATTEMPT.len());
}

#[test]
fn wonder_guard_admits_a_serene_grace_poison_hit_move() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, DUNSPARCE, 10, vec![POISON_STING]);
    assert_eq!(player.ability(), AbilityId::SERENE_GRACE);
    let enemy = max_iv_mon(&dex, SHEDINJA, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(BOTH_BATTLERS_HIT);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: POISON_STING,
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
    assert_eq!(rng.draws(), BOTH_BATTLERS_HIT.len());
}

/// `Cmd_accuracycheck` reclassifies a failed accuracy roll through
/// `CheckWonderGuardAndLevitate` (`battle_script_commands.c:1175-1186`,
/// `:1435-1443`), so a multi-hit move that misses a Levitate holder must
/// reach the caller as [`BattleEvent::LevitateBlocked`], not
/// [`BattleEvent::Missed`], and must not roll a hit count.
#[test]
fn a_missed_ground_multi_hit_move_reports_levitate_rather_than_a_generic_miss() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![BONE_RUSH]);
    let enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let enemy_hp_before = enemy.current_hp();

    let mut rng = SequenceRng::new(MULTI_HIT_MOVE_MISSES_THEN_ENEMY_HIT);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::LevitateBlocked {
            by_player: true,
            move_id: BONE_RUSH,
        },
        "a failed accuracy roll must still report Levitate: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Missed {
                by_player: true,
                ..
            }
        )),
        "the generic miss must not survive reclassification: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before);
    assert_eq!(
        rng.draws(),
        MULTI_HIT_MOVE_MISSES_THEN_ENEMY_HIT.len(),
        "reclassifying the miss adds no draw"
    );
    assert_eq!(battle.player().moves()[0].pp, 9);
}
