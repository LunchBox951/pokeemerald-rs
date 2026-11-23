//! False Swipe (`EFFECT_FALSE_SWIPE`) through the ordinary hit script: its
//! HP loss is capped at one fewer than the target's HP by
//! `Cmd_adjustnormaldamage` (`battle_script_commands.c:1682-1686`), while
//! every ordinary draw is still spent and contact abilities react only to HP
//! actually lost (`TARGET_TURN_DAMAGED`, `battle_util.c:2756`-`:2827`).

use crate::poison_support::*;
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const FALSE_SWIPE: MoveId = MoveId::FALSE_SWIPE;
const SPLASH: MoveId = MoveId::SPLASH;
const PIKACHU: u16 = 25;
const CARVANHA: u16 = 330;
const GASTLY: u16 = 92;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
const NO_CRIT: u16 = 1;
const CRIT: u16 = 0;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 2;
/// Static's `Random() % 3` paralyses on zero.
const STATIC_TRIGGERS: u16 = 0;

/// Three turn-setup draws, then False Swipe's accuracy, critical, variance
/// and discarded effect-chance draws. The target's Splash draws nothing.
const SETUP: [u16; 3] = [CONSTRUCTION_TURN_NUMBER, TURN_START, ENEMY_MOVE_SELECTION];
const LANDS: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];
const LANDS_CRITICALLY: [u16; 4] = [ACCURACY_HITS, CRIT, DAMAGE_ROLL, DISCARDED_SECONDARY_EFFECT];

fn with_hp(mut mon: BattlePokemon, hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - hp);
    mon
}

fn script(landing: &[u16], extra: &[u16]) -> Vec<u16> {
    [SETUP.as_slice(), landing, extra].concat()
}

/// A level-10 Rattata outruns every level-5 target below, so it moves first
/// and the target's Splash follows.
fn take_false_swipe(
    enemy: BattlePokemon,
    dex: Dex,
    draws: Vec<u16>,
) -> (Battle, Vec<BattleEvent>, usize) {
    let player = max_iv_mon(&dex, RATTATA, 10, vec![FALSE_SWIPE]);
    let mut rng = SequenceRng::new(draws);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    let draws = rng.draws();
    (battle, events, draws)
}

fn player_hit(events: &[BattleEvent]) -> (u32, bool) {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: true,
                move_id: FALSE_SWIPE,
                damage,
                is_critical,
            } => Some((*damage, *is_critical)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no False Swipe hit in {events:?}"))
}

fn fainted(events: &[BattleEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, BattleEvent::Fainted { .. }))
}

#[test]
fn a_hit_that_would_faint_leaves_the_target_at_one_hp() {
    let dex = Dex::new();
    let enemy = with_hp(max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]), 2);

    let (battle, events, draws) = take_false_swipe(enemy, dex, script(&LANDS, &[]));

    assert_eq!(player_hit(&events), (1, false), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), 1);
    assert!(!fainted(&events), "{events:?}");
    assert_eq!(draws, 7);
}

#[test]
fn a_critical_hit_that_would_faint_leaves_the_target_at_one_hp() {
    let dex = Dex::new();
    let enemy = with_hp(max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]), 2);

    let (battle, events, draws) = take_false_swipe(enemy, dex, script(&LANDS_CRITICALLY, &[]));

    assert_eq!(player_hit(&events), (1, true), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), 1);
    assert!(!fainted(&events));
    assert_eq!(draws, 7);
}

#[test]
fn a_non_lethal_hit_deals_its_ordinary_damage() {
    let dex = Dex::new();
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]);
    let max_hp = enemy.stats().max_hp;

    let (battle, events, draws) = take_false_swipe(enemy, dex, script(&LANDS, &[]));

    let (damage, _) = player_hit(&events);
    assert!(damage > 0 && damage < max_hp - 1, "{events:?}");
    assert_eq!(battle.enemy().current_hp(), max_hp - damage);
    assert_eq!(draws, 7);
}

#[test]
fn a_target_at_one_hp_loses_nothing_and_every_draw_is_still_spent() {
    for landing in [LANDS, LANDS_CRITICALLY] {
        let dex = Dex::new();
        let enemy = with_hp(max_iv_mon(&dex, ZIGZAGOON, 5, vec![SPLASH]), 1);

        let (battle, events, draws) = take_false_swipe(enemy, dex, script(&landing, &[]));

        assert_eq!(player_hit(&events).0, 0, "{events:?}");
        assert_eq!(battle.enemy().current_hp(), 1);
        assert!(!fainted(&events), "{events:?}");
        assert_eq!(draws, 7);
    }
}

#[test]
fn a_ghost_target_at_one_hp_is_still_immune() {
    let dex = Dex::new();
    let enemy = with_hp(max_iv_mon(&dex, GASTLY, 5, vec![SPLASH]), 1);

    let (battle, events, draws) = take_false_swipe(enemy, dex, script(&LANDS, &[]));

    assert!(
        events.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: FALSE_SWIPE,
        }),
        "{events:?}"
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: true,
            ..
        }
    )));
    assert_eq!(battle.enemy().current_hp(), 1);
    assert_eq!(draws, 7);
}

fn pikachu(dex: &Dex) -> BattlePokemon {
    let mon = max_iv_mon(dex, PIKACHU, 5, vec![SPLASH]);
    assert_eq!(mon.ability(), AbilityId::STATIC);
    mon
}

#[test]
fn static_does_not_react_to_a_hit_that_removes_no_hp() {
    let dex = Dex::new();
    // No contact draw is scripted: a stray Static roll would exhaust the RNG.
    let (battle, events, draws) =
        take_false_swipe(with_hp(pikachu(&dex), 1), dex, script(&LANDS, &[]));

    assert_eq!(player_hit(&events).0, 0, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::ParalyzedByStatic { .. })));
    assert_eq!(draws, 7);
}

#[test]
fn static_reacts_once_a_clamped_hit_removes_hp() {
    let dex = Dex::new();
    let (battle, events, draws) = take_false_swipe(
        with_hp(pikachu(&dex), 2),
        dex,
        script(&LANDS, &[STATIC_TRIGGERS]),
    );

    assert_eq!(player_hit(&events).0, 1, "{events:?}");
    assert_eq!(battle.enemy().current_hp(), 1);
    assert_eq!(battle.player().status1(), Status1::Paralysed);
    assert_eq!(draws, 8);
}

fn carvanha(dex: &Dex) -> BattlePokemon {
    let mon = max_iv_mon(dex, CARVANHA, 5, vec![SPLASH]);
    assert_eq!(mon.ability(), AbilityId::ROUGH_SKIN);
    mon
}

#[test]
fn rough_skin_does_not_react_to_a_hit_that_removes_no_hp() {
    let dex = Dex::new();
    let (battle, events, draws) =
        take_false_swipe(with_hp(carvanha(&dex), 1), dex, script(&LANDS, &[]));

    assert_eq!(player_hit(&events).0, 0, "{events:?}");
    assert_eq!(battle.player().current_hp(), battle.player().stats().max_hp);
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::RoughSkinDamage { .. })));
    assert_eq!(draws, 7);
}

#[test]
fn rough_skin_reacts_once_a_clamped_hit_removes_hp() {
    let dex = Dex::new();
    let (battle, events, draws) =
        take_false_swipe(with_hp(carvanha(&dex), 2), dex, script(&LANDS, &[]));

    let recoil = (battle.player().stats().max_hp / 16).max(1);
    assert_eq!(player_hit(&events).0, 1, "{events:?}");
    assert!(events.contains(&BattleEvent::RoughSkinDamage {
        by_player: true,
        move_id: FALSE_SWIPE,
        damage: recoil,
    }));
    assert_eq!(draws, 7);
}
