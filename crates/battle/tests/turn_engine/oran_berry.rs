//! Oran Berry's `ITEMEFFECT_NORMAL` heal in the end-turn item pass (S-6):
//! `HOLD_EFFECT_RESTORE_HP` (`src/battle_util.c:3336`-`:3346`) restores its
//! 10 HP param, capped at max HP, to a living holder at or below half HP, then
//! `BattleScript_ItemHealHP_RemoveItem` consumes the Berry
//! (`data/battle_scripts_1.s:4356`-`:4364`). `ENDTURN_ITEMS1` runs after
//! `ENDTURN_ABILITIES` and before `ENDTURN_LEECH_SEED`
//! (`src/battle_util.c:1442`-`:1461`, `:1498`-`:1502`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::items::ItemId;
use assets::MoveId;
use battle::battle::BattlerSide;
use battle::status1::{burn_residual_damage, poison_residual_damage};
use battle::{
    leech_seed_residual_damage, Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1,
};

const SPLASH: MoveId = MoveId::SPLASH;
const TACKLE: MoveId = MoveId::TACKLE;
const LEVEL: u8 = 50;
/// `SPECIES_RATTATA`: faster than [`WURMPLE`] at equal levels.
const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: faster than [`WURMPLE`].
const ZIGZAGOON: u16 = 288;
/// `SPECIES_WURMPLE`: the slowest fixture.
const WURMPLE: u16 = 290;
/// `SPECIES_SHEDINJA`: one maximum HP, whatever its level.
const SHEDINJA: u16 = 303;
/// Construction's turn-number draw, the turn-start draw and the wild
/// opponent's move-slot pick, with unequal Speeds so no tie draw follows.
const TURN_OVERHEAD: [u16; 3] = [0; 3];
/// [`TURN_OVERHEAD`] then one attack that hits without a critical hit.
const ENEMY_ATTACKS_FIRST: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];

fn mon(species: u16, level: u8, item: ItemId) -> BattlePokemon {
    max_iv_mon(&Dex::new(), species, level, vec![SPLASH]).with_held_item(item)
}

fn at_hp(mut mon: BattlePokemon, current_hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.current_hp() - current_hp);
    mon
}

fn half(mon: &BattlePokemon) -> u32 {
    mon.stats().max_hp / 2
}

fn restored(by_player: bool, healed: u32) -> BattleEvent {
    BattleEvent::ItemRestoredHealth {
        by_player,
        item: ItemId::ORAN_BERRY,
        healed,
    }
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

fn has_restore(events: &[BattleEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, BattleEvent::ItemRestoredHealth { .. }))
}

/// One Splash-for-Splash turn against a wild enemy; asserts that the turn,
/// Berry included, drew only its overhead.
fn splash_turn(player: BattlePokemon, enemy: BattlePokemon) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(TURN_OVERHEAD);
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    assert_eq!(rng.draws(), 3, "the Berry draws nothing");
    (battle, events)
}

/// Runs the turn with `holder` on the chosen side against a Berry-free
/// opponent, the holder faster or slower than it.
fn holder_turn(
    holder: BattlePokemon,
    holder_is_player: bool,
    holder_is_faster: bool,
) -> (Battle, Vec<BattleEvent>) {
    let opponent = mon(
        if holder_is_faster { WURMPLE } else { RATTATA },
        LEVEL,
        ItemId::NONE,
    );
    if holder_is_player {
        splash_turn(holder, opponent)
    } else {
        splash_turn(opponent, holder)
    }
}

fn species_for(faster: bool) -> u16 {
    if faster {
        RATTATA
    } else {
        WURMPLE
    }
}

fn holder_at(species: u16, hp: impl Fn(&BattlePokemon) -> u32) -> BattlePokemon {
    let holder = mon(species, LEVEL, ItemId::ORAN_BERRY);
    let hp = hp(&holder);
    at_hp(holder, hp)
}

fn battler(battle: &Battle, is_player: bool) -> &BattlePokemon {
    if is_player {
        battle.player()
    } else {
        battle.enemy()
    }
}

/// Every `(holder_is_player, holder_is_faster)` pairing, so the pass runs
/// first and second on each side.
const ALL_ORDERS: [(bool, bool); 4] = [(true, true), (true, false), (false, true), (false, false)];

#[test]
fn a_holder_at_exactly_half_hp_heals_ten_and_consumes_the_berry_in_either_order() {
    for (is_player, faster) in ALL_ORDERS {
        let holder = holder_at(species_for(faster), half);
        let hp = holder.current_hp();
        assert_eq!(hp, holder.stats().max_hp / 2, "fixture");

        let (battle, events) = holder_turn(holder, is_player, faster);

        assert!(events.contains(&restored(is_player, 10)), "{events:?}");
        let healed = battler(&battle, is_player);
        assert_eq!(healed.current_hp(), hp + 10);
        assert_eq!(healed.held_item(), ItemId::NONE, "the Berry is consumed");
        assert_eq!(battle.outcome(), None);
    }
}

#[test]
fn a_holder_above_half_hp_keeps_its_berry_and_its_hp() {
    for (is_player, faster) in ALL_ORDERS {
        let holder = holder_at(species_for(faster), |mon| half(mon) + 1);
        let hp = holder.current_hp();

        let (battle, events) = holder_turn(holder, is_player, faster);

        assert!(!has_restore(&events), "{events:?}");
        let kept = battler(&battle, is_player);
        assert_eq!(kept.current_hp(), hp);
        assert_eq!(kept.held_item(), ItemId::ORAN_BERRY);
    }
}

#[test]
fn a_one_hp_holder_whose_half_rounds_to_zero_does_not_trigger() {
    // `hp <= maxHP / 2` with maxHP 1 is `1 <= 0`.
    let holder = mon(SHEDINJA, LEVEL, ItemId::ORAN_BERRY);
    assert_eq!(holder.stats().max_hp, 1, "fixture");

    let (battle, events) = holder_turn(holder, true, false);

    assert!(!has_restore(&events), "{events:?}");
    assert_eq!(battle.player().held_item(), ItemId::ORAN_BERRY);
}

#[test]
fn the_heal_is_capped_at_the_holders_max_hp() {
    for (is_player, faster) in ALL_ORDERS {
        let holder = mon(species_for(faster), 2, ItemId::ORAN_BERRY);
        let max_hp = holder.stats().max_hp;
        let holder = at_hp(holder, max_hp / 2);
        let missing = max_hp - max_hp / 2;
        assert!(missing < 10, "fixture: the cap, not the param, limits it");

        let (battle, events) = holder_turn(holder, is_player, faster);

        assert!(events.contains(&restored(is_player, missing)), "{events:?}");
        assert_eq!(battler(&battle, is_player).current_hp(), max_hp);
    }
}

#[test]
fn the_berry_heals_once_then_is_gone() {
    let holder = holder_at(WURMPLE, |_| 20);
    let opponent = mon(RATTATA, LEVEL, ItemId::NONE);
    let mut rng = SequenceRng::new(TURN_OVERHEAD);
    let mut battle = Battle::new(Dex::new(), holder, opponent, false, &mut rng).unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(first.contains(&restored(true, 10)), "{first:?}");
    assert_eq!(battle.player().current_hp(), 30);
    assert!(battle.player().current_hp() <= half(battle.player()));

    let mut rng = SequenceRng::new([0, 0]);
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    assert!(!has_restore(&second), "{second:?}");
    assert_eq!(battle.player().current_hp(), 30);
}

#[test]
fn a_holder_fainted_during_the_action_phase_is_not_healed() {
    // The faster enemy fells the 1 HP holder; the reserve keeps the battle on.
    let holder = at_hp(mon(WURMPLE, LEVEL, ItemId::ORAN_BERRY), 1);
    let reserve = mon(ZIGZAGOON, LEVEL, ItemId::NONE);
    let enemy = max_iv_mon(&Dex::new(), RATTATA, LEVEL, vec![TACKLE]);
    let mut rng = SequenceRng::new(ENEMY_ATTACKS_FIRST);
    let mut battle =
        Battle::new_with_player_reserves(Dex::new(), holder, vec![reserve], enemy, false, &mut rng)
            .unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "{events:?}"
    );
    assert!(!has_restore(&events), "{events:?}");
    assert_eq!(battle.outcome(), None);
}

#[test]
fn the_heal_precedes_the_leech_seed_drain_and_the_poison_and_burn_ticks() {
    for (is_player, faster) in ALL_ORDERS {
        for status in [Status1::Healthy, Status1::Poisoned, Status1::Burned] {
            let mut holder = holder_at(species_for(faster), |_| 40);
            holder.volatiles_mut().set_seeded_by(if is_player {
                BattlerSide::Enemy
            } else {
                BattlerSide::Player
            });
            holder.set_status1(status);
            let max_hp = holder.stats().max_hp;
            let seed = leech_seed_residual_damage(max_hp);

            let (battle, events) = holder_turn(holder, is_player, faster);

            let heal = position(&events, &restored(is_player, 10));
            let sap = position(
                &events,
                &BattleEvent::SappedByLeechSeed {
                    by_player: is_player,
                    damage: seed,
                },
            );
            assert!(heal < sap, "{events:?}");
            let status_tick = match status {
                Status1::Poisoned => {
                    let damage = poison_residual_damage(max_hp);
                    let tick = BattleEvent::HurtByPoison {
                        by_player: is_player,
                        damage,
                    };
                    assert!(sap < position(&events, &tick), "{events:?}");
                    damage
                }
                Status1::Burned => {
                    let damage = burn_residual_damage(max_hp);
                    let tick = BattleEvent::HurtByBurn {
                        by_player: is_player,
                        damage,
                    };
                    assert!(sap < position(&events, &tick), "{events:?}");
                    damage
                }
                _ => 0,
            };
            assert_eq!(
                battler(&battle, is_player).current_hp(),
                40 + 10 - seed - status_tick,
                "{status:?}"
            );
        }
    }
}

#[test]
fn a_leech_seed_heal_from_a_faster_battler_lifts_a_slower_holder_past_the_threshold() {
    // The Rattata acts first, so its drain heals the slower Wurmple holder
    // before the holder's own item pass reads its HP.
    let mut seeded_rattata = mon(RATTATA, LEVEL, ItemId::NONE);
    seeded_rattata
        .volatiles_mut()
        .set_seeded_by(BattlerSide::Player);
    let holder = holder_at(WURMPLE, half);
    let hp = holder.current_hp();
    let drain = leech_seed_residual_damage(seeded_rattata.stats().max_hp);

    let (battle, events) = splash_turn(holder, seeded_rattata);

    assert!(!has_restore(&events), "{events:?}");
    assert_eq!(battle.player().current_hp(), hp + drain);
    assert_eq!(battle.player().held_item(), ItemId::ORAN_BERRY);
}

#[test]
fn a_faster_holder_heals_before_the_slower_seeded_battlers_drain_reaches_it() {
    // Holder fastest: its item pass runs first, at half HP, before the
    // slower seeded Wurmple's drain heals it.
    let mut seeded_wurmple = mon(WURMPLE, LEVEL, ItemId::NONE);
    seeded_wurmple
        .volatiles_mut()
        .set_seeded_by(BattlerSide::Player);
    let holder = holder_at(RATTATA, half);
    let hp = holder.current_hp();
    let drain = leech_seed_residual_damage(seeded_wurmple.stats().max_hp);

    let (battle, events) = splash_turn(holder, seeded_wurmple);

    let heal = position(&events, &restored(true, 10));
    let seeder_heal = position(
        &events,
        &BattleEvent::LeechSeedHealed {
            by_player: true,
            healed: drain,
        },
    );
    assert!(heal < seeder_heal, "{events:?}");
    assert_eq!(battle.player().current_hp(), hp + 10 + drain);
    assert_eq!(battle.player().held_item(), ItemId::NONE);
}
