//! Rough Skin contact damage (`MOVEEND_ON_DAMAGE_ABILITIES`,
//! `battle_util.c:2767`-`:2780`) consumes no RNG.

use crate::poison_support::*;
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

/// Rough Skin holder, slower than [`RATTATA`] except at levels 1, 2 and 5.
const CARVANHA: u16 = 330;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;

/// Construction, turn start and enemy move selection, then per acting battler
/// accuracy, crit (`1`: no crit), damage roll and a discarded secondary-effect
/// draw. [`RATTATA`] acts first unless both are level 1, 2 or 5, where Speed ties.
const BOTH_ACT: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];

const FIRST_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];

fn carvanha(dex: &Dex, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let mon = max_iv_mon(dex, CARVANHA, level, moves);
    assert_eq!(mon.ability(), AbilityId::ROUGH_SKIN);
    mon
}

/// Leaves `mon` with exactly `hp` HP.
fn with_hp(mut mon: BattlePokemon, hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - hp);
    mon
}

fn rough_skin_damage(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::RoughSkinDamage { .. }))
        .collect()
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

fn take_player_move(
    player: BattlePokemon,
    enemy: BattlePokemon,
    dex: Dex,
    script: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(script.iter().copied());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

#[test]
fn a_contact_hit_into_an_enemy_rough_skin_holder_hurts_the_player_by_a_sixteenth() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = carvanha(&dex, 50, vec![TACKLE]);
    let max_hp = player.stats().max_hp;
    let recoil = max_hp / 16;
    assert_ne!(max_hp % 16, 0, "the recoil must be floored");

    let (battle, events) = take_player_move(player, enemy, dex, &BOTH_ACT);

    let hurt = BattleEvent::RoughSkinDamage {
        by_player: true,
        move_id: TACKLE,
        damage: recoil,
    };
    assert_eq!(rough_skin_damage(&events), vec![&hurt], "{events:?}");
    assert!(!battle.player().is_fainted());
    let hit = BattleEvent::Hit {
        by_player: true,
        move_id: TACKLE,
        damage: battle_hit_damage(&events, true),
        is_critical: false,
    };
    assert!(position(&events, &hit) < position(&events, &hurt));
}

fn battle_hit_damage(events: &[BattleEvent], by_player: bool) -> u32 {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: side,
                damage,
                ..
            } if *side == by_player => Some(*damage),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no hit by player={by_player} in {events:?}"))
}

#[test]
fn a_contact_hit_into_a_player_rough_skin_holder_hurts_the_enemy_by_a_sixteenth() {
    let dex = Dex::new();
    let player = carvanha(&dex, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let max_hp = enemy.stats().max_hp;
    let recoil = max_hp / 16;

    let (battle, events) = take_player_move(player, enemy, dex, &BOTH_ACT);

    assert_eq!(
        rough_skin_damage(&events),
        vec![&BattleEvent::RoughSkinDamage {
            by_player: false,
            move_id: TACKLE,
            damage: recoil,
        }],
        "{events:?}"
    );
    assert!(!battle.enemy().is_fainted());
}

#[test]
fn recoil_is_floored_to_one_hp() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    let enemy = carvanha(&dex, 1, vec![TACKLE]);
    assert!(player.stats().max_hp < 16);

    let (_, events) = take_player_move(player, enemy, dex, &BOTH_ACT);

    assert!(
        events.contains(&BattleEvent::RoughSkinDamage {
            by_player: true,
            move_id: TACKLE,
            damage: 1,
        }),
        "{events:?}"
    );
}

#[test]
fn a_non_contact_hit_into_a_rough_skin_holder_draws_no_recoil() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 20, vec![WATER_GUN]);
    let enemy = carvanha(&dex, 20, vec![TACKLE]);

    let (_, events) = take_player_move(player, enemy, dex, &BOTH_ACT);

    assert!(rough_skin_damage(&events).is_empty(), "{events:?}");
}

#[test]
fn a_fainted_rough_skin_holder_still_hurts_the_attacker() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = carvanha(&dex, 5, vec![TACKLE]);
    let recoil = player.stats().max_hp / 16;

    let (battle, events) = take_player_move(player, enemy, dex, &FIRST_ACTS_ALONE);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    let holder_fainted = position(&events, &BattleEvent::Fainted { by_player: false });
    let hurt = position(
        &events,
        &BattleEvent::RoughSkinDamage {
            by_player: true,
            move_id: TACKLE,
            damage: recoil,
        },
    );
    assert!(holder_fainted < hurt, "{events:?}");
}

#[test]
fn an_attacker_knocked_out_by_the_recoil_faints_and_the_holder_never_acts() {
    let dex = Dex::new();
    let player = with_hp(max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]), 1);
    let enemy = carvanha(&dex, 20, vec![TACKLE]);

    let (battle, events) = take_player_move(player, enemy, dex, &FIRST_ACTS_ALONE);

    assert!(battle.player().is_fainted(), "{events:?}");
    assert!(!battle.enemy().is_fainted(), "{events:?}");
    let hurt = position(
        &events,
        &BattleEvent::RoughSkinDamage {
            by_player: true,
            move_id: TACKLE,
            damage: 1,
        },
    );
    let attacker_fainted = position(&events, &BattleEvent::Fainted { by_player: true });
    assert_eq!(attacker_fainted, hurt + 1, "{events:?}");
}

#[test]
fn a_double_knockout_settles_the_holder_before_the_attacker() {
    let dex = Dex::new();
    let player = with_hp(max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]), 1);
    let enemy = carvanha(&dex, 5, vec![TACKLE]);

    let (battle, events) = take_player_move(player, enemy, dex, &FIRST_ACTS_ALONE);

    assert!(battle.player().is_fainted() && battle.enemy().is_fainted());
    let holder = position(&events, &BattleEvent::Fainted { by_player: false });
    let attacker = position(&events, &BattleEvent::Fainted { by_player: true });
    assert!(holder < attacker, "{events:?}");
}

#[test]
fn a_multi_hit_contact_move_hurts_the_attacker_after_every_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 60, vec![DOUBLE_SLAP]);
    let enemy = carvanha(&dex, 40, vec![TACKLE]);
    let recoil = player.stats().max_hp / 16;

    // Double Slap's hit count draw is 0, which rolls two hits.
    let script = [0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0];
    let (battle, events) = take_player_move(player, enemy, dex, &script);

    let hurt = BattleEvent::RoughSkinDamage {
        by_player: true,
        move_id: DOUBLE_SLAP,
        damage: recoil,
    };
    assert_eq!(rough_skin_damage(&events), vec![&hurt, &hurt], "{events:?}");
    assert!(!battle.player().is_fainted());
}

#[test]
fn a_multi_hit_sequence_stops_when_the_recoil_knocks_the_attacker_out() {
    let dex = Dex::new();
    let player = with_hp(max_iv_mon(&dex, RATTATA, 60, vec![DOUBLE_SLAP]), 1);
    let enemy = carvanha(&dex, 40, vec![TACKLE]);

    let script = [0, 0, 0, 0, 0, 1, 0, 0];
    let (battle, events) = take_player_move(player, enemy, dex, &script);

    assert!(battle.player().is_fainted(), "{events:?}");
    assert_eq!(rough_skin_damage(&events).len(), 1, "{events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::MultiHit { .. })),
        "{events:?}"
    );
}

#[test]
fn recoil_on_the_final_rolled_hit_is_capped_at_the_attackers_remaining_hp() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 60, vec![DOUBLE_SLAP]);
    let recoil = player.stats().max_hp / 16;
    let player = with_hp(player, recoil + 1);
    let enemy = carvanha(&dex, 40, vec![TACKLE]);

    let script = [0, 0, 0, 0, 0, 1, 0, 1, 0, 0];
    let (battle, events) = take_player_move(player, enemy, dex, &script);

    let hurt = |damage| BattleEvent::RoughSkinDamage {
        by_player: true,
        move_id: DOUBLE_SLAP,
        damage,
    };
    assert_eq!(
        rough_skin_damage(&events),
        vec![&hurt(recoil), &hurt(1)],
        "{events:?}"
    );
    assert!(battle.player().is_fainted(), "{events:?}");
}
