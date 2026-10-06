//! Poison's interaction with status-keyed stats: Guts and Marvel Scale
//! stay live on a newly poisoned defender.

use crate::common::{max_iv_mon_with_personality, SECONDARY_ABILITY_PERSONALITY};
use crate::poison_support::*;
use assets::AbilityId;
use battle::{Battle, BattleEvent, Dex, PlayerAction, Status1};

/// `SPECIES_MAKUHITA`: Fighting-type, Guts in ability slot 1 (Thick Fat is
/// slot 0), slower than [`MILOTIC`].
const MAKUHITA: u16 = 335;
/// `SPECIES_MILOTIC`: Water-type, Marvel Scale in its only ability slot,
/// faster than [`MAKUHITA`].
const MILOTIC: u16 = 329;

/// A damage roll draw producing the maximum 100% roll
/// (`damage::apply_damage_roll`'s `roll_reduction = draw % 16`).
const BEST_DAMAGE_DRAW: u16 = 0;
/// A damage roll draw producing the minimum 85% roll.
const WORST_DAMAGE_DRAW: u16 = 15;
/// A crit draw that never crits.
const NO_CRIT_DRAW: u16 = 1;

/// Upstream's `STATUS1_POISON` case does not guard Guts
/// (`battle_script_commands.c:2299-2340`).
#[test]
fn poison_sting_newly_poisons_a_guts_defender_who_then_hits_harder() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MILOTIC, 5, vec![POISON_STING]);
    let enemy = max_iv_mon_with_personality(
        &dex,
        MAKUHITA,
        5,
        vec![TACKLE],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(
        enemy.ability(),
        AbilityId::GUTS,
        "fixture sanity: personality 25 fields the secondary ability slot"
    );

    // Battle::new, turn start, and enemy selection, then each mover's
    // accuracy/crit/damage/effect-chance draws (`secondary::spend_effect_chance_draw`).
    let mut rng = SequenceRng::new([
        0,
        0,
        0,
        0,
        NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
        0,
        NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        0,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("Guts is modelled, so the pick is admitted");

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
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    assert!(
        events.contains(&BattleEvent::Hit {
            by_player: false,
            move_id: TACKLE,
            damage: 5,
            is_critical: false,
        }),
        "the now-poisoned Guts holder's own Tackle must observe the 150% \
         raw Attack boost this same turn: {events:?}"
    );
}

/// Upstream's `STATUS1_POISON` case does not guard Marvel Scale
/// (`battle_script_commands.c:2299-2340`).
#[test]
fn poison_sting_newly_poisons_a_marvel_scale_defender_who_then_takes_less_damage() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, MAKUHITA, 5, vec![POISON_STING, TACKLE]);
    let enemy = max_iv_mon(&dex, MILOTIC, 5, vec![TACKLE]);
    assert_eq!(
        enemy.ability(),
        AbilityId::MARVEL_SCALE,
        "fixture sanity: the only ability slot fields Marvel Scale"
    );

    // Battle::new, then two turns' worth of turn start, enemy selection,
    // and each mover's accuracy/crit/damage/effect-chance draws
    // (`secondary::spend_effect_chance_draw`).
    let mut rng = SequenceRng::new([
        0,
        0,
        0,
        0,
        NO_CRIT_DRAW,
        WORST_DAMAGE_DRAW,
        0,
        0,
        NO_CRIT_DRAW,
        WORST_DAMAGE_DRAW,
        POISON_CHANCE_HIT_DRAW,
        0,
        0,
        0,
        NO_CRIT_DRAW,
        WORST_DAMAGE_DRAW,
        0,
        0,
        NO_CRIT_DRAW,
        BEST_DAMAGE_DRAW,
        0,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let turn_one = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("Marvel Scale is modelled, so the pick is admitted");
    assert!(
        turn_one.contains(&BattleEvent::Poisoned {
            by_player: true,
            move_id: POISON_STING,
        }),
        "{turn_one:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);

    let turn_two = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert!(
        turn_two.contains(&BattleEvent::Hit {
            by_player: true,
            move_id: TACKLE,
            damage: 3,
            is_critical: false,
        }),
        "the poisoned Marvel Scale holder's raw Defense must be raised \
         150% against the player's Tackle: {turn_two:?}"
    );
}
