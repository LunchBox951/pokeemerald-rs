//! The move-end status-immunity cure (`ABILITYEFFECT_IMMUNITY`,
//! `battle_util.c:2873`-`:2951`, from `MOVEEND_IMMUNITY_ABILITIES`,
//! `battle_script_commands.c:4285`-`:4289`): every active battler whose ability
//! forbids a status or volatile it carries is cured when a move ends, with no
//! RNG draw. Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one.

use crate::common::{max_iv_mon_with_personality, script, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::battle::BattlerSide;
use battle::{
    Battle, BattleEvent, BattleOutcome, BattlePokemon, CuredCondition, Dex, PlayerAction, Status1,
};

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];
/// A hit: passing accuracy, non-critical, damage roll, discarded effect chance.
const H: [u16; 4] = [0, 1, 0, 0];

const SPLASH: MoveId = MoveId::SPLASH;

fn mon(species: SpeciesId, level: u8, moves: &[MoveId], ability_slot: u8) -> BattlePokemon {
    max_iv_mon_with_personality(&Dex::new(), species.0, level, moves.to_vec(), 0)
        .with_ability_slot(ability_slot)
}

fn used(by_player: bool, move_id: MoveId) -> [BattleEvent; 2] {
    [
        BattleEvent::UsedMove { by_player, move_id },
        BattleEvent::NothingHappened { by_player, move_id },
    ]
}

fn cured(by_player: bool, ability: AbilityId, condition: CuredCondition) -> BattleEvent {
    BattleEvent::AbilityCuredStatus {
        by_player,
        ability,
        condition,
    }
}

/// One status-immunity pairing: a holder, its ability, and what is cured.
struct Pairing {
    ability: AbilityId,
    condition: CuredCondition,
    make: fn() -> BattlePokemon,
}

fn status_holder(
    species: SpeciesId,
    ability_slot: u8,
    ability: AbilityId,
    status: Status1,
) -> BattlePokemon {
    let mut holder = mon(species, 20, &[SPLASH], ability_slot);
    assert_eq!(holder.ability(), ability);
    holder.set_status1(status);
    holder
}

fn pairings() -> Vec<Pairing> {
    vec![
        Pairing {
            ability: AbilityId::LIMBER,
            condition: CuredCondition::Paralysis,
            make: || status_holder(SpeciesId::PERSIAN, 0, AbilityId::LIMBER, Status1::Paralysed),
        },
        Pairing {
            ability: AbilityId::IMMUNITY,
            condition: CuredCondition::Poison,
            make: || {
                status_holder(
                    SpeciesId::SNORLAX,
                    0,
                    AbilityId::IMMUNITY,
                    Status1::Poisoned,
                )
            },
        },
        Pairing {
            ability: AbilityId::IMMUNITY,
            condition: CuredCondition::Poison,
            make: || {
                status_holder(
                    SpeciesId::SNORLAX,
                    0,
                    AbilityId::IMMUNITY,
                    Status1::BadPoisoned { counter: 5 },
                )
            },
        },
        Pairing {
            ability: AbilityId::WATER_VEIL,
            condition: CuredCondition::Burn,
            make: || {
                status_holder(
                    SpeciesId::GOLDEEN,
                    1,
                    AbilityId::WATER_VEIL,
                    Status1::Burned,
                )
            },
        },
        Pairing {
            ability: AbilityId::MAGMA_ARMOR,
            condition: CuredCondition::Freeze,
            make: || {
                status_holder(
                    SpeciesId::SLUGMA,
                    0,
                    AbilityId::MAGMA_ARMOR,
                    Status1::Frozen,
                )
            },
        },
        Pairing {
            ability: AbilityId::INSOMNIA,
            condition: CuredCondition::Sleep,
            make: || {
                status_holder(
                    SpeciesId::HOOTHOOT,
                    0,
                    AbilityId::INSOMNIA,
                    Status1::Asleep { turns: 3 },
                )
            },
        },
        Pairing {
            ability: AbilityId::VITAL_SPIRIT,
            condition: CuredCondition::Sleep,
            make: || {
                status_holder(
                    SpeciesId::MANKEY,
                    0,
                    AbilityId::VITAL_SPIRIT,
                    Status1::Asleep { turns: 3 },
                )
            },
        },
        Pairing {
            ability: AbilityId::OWN_TEMPO,
            condition: CuredCondition::Confusion,
            make: || {
                let mut holder = mon(SpeciesId::SPINDA, 20, &[SPLASH], 0);
                assert_eq!(holder.ability(), AbilityId::OWN_TEMPO);
                holder.volatiles_mut().set_confusion(3);
                holder
            },
        },
        Pairing {
            ability: AbilityId::OBLIVIOUS,
            condition: CuredCondition::Infatuation,
            make: || {
                let mut holder = mon(SpeciesId::ILLUMISE, 20, &[SPLASH], 0);
                assert_eq!(holder.ability(), AbilityId::OBLIVIOUS);
                holder.volatiles_mut().set_infatuated_by(BattlerSide::Enemy);
                holder
            },
        },
    ]
}

/// A healthy L100 Rattata that outspeeds every L20 fixture here, even unparalysed.
fn fast_opponent() -> BattlePokemon {
    mon(SpeciesId::RATTATA, 100, &[SPLASH], 0)
}

/// Whether `holder` carries neither a status nor a confusion or infatuation.
fn is_clear(holder: &BattlePokemon) -> bool {
    holder.status1() == Status1::Healthy
        && !holder.volatiles().confused()
        && !holder.volatiles().infatuated()
}

#[test]
fn every_pairing_is_cured_after_the_first_move_whichever_side_holds_it() {
    for pairing in pairings() {
        for holder_is_player in [true, false] {
            let mut holder = (pairing.make)();
            if holder.volatiles().infatuated() && holder_is_player {
                holder.volatiles_mut().infatuated_by = Some(BattlerSide::Enemy);
            } else if holder.volatiles().infatuated() {
                holder.volatiles_mut().infatuated_by = Some(BattlerSide::Player);
            }
            let (player, enemy) = if holder_is_player {
                (holder, fast_opponent())
            } else {
                (fast_opponent(), holder)
            };
            assert_ne!(player.speed_for_turn_order(), enemy.speed_for_turn_order());
            let mut rng = SequenceRng::new(script(&[&C, &W]));
            let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
            let holder_of = |battle: &Battle| {
                if holder_is_player {
                    battle.player().clone()
                } else {
                    battle.enemy().clone()
                }
            };
            assert!(
                !is_clear(&holder_of(&battle)),
                "{:?}: construction cures nothing",
                pairing.ability
            );

            let events = battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap();

            let fast_side = !holder_is_player;
            assert_eq!(
                events,
                [
                    used(fast_side, SPLASH).to_vec(),
                    vec![cured(holder_is_player, pairing.ability, pairing.condition)],
                    used(holder_is_player, SPLASH).to_vec(),
                ]
                .concat(),
                "{:?} on the {} side",
                pairing.ability,
                if holder_is_player { "player" } else { "enemy" }
            );
            assert!(is_clear(&holder_of(&battle)));
            rng.assert_exhausted();
        }
    }
}

#[test]
fn the_scan_cures_the_player_before_the_enemy_whoever_moved() {
    // Snorlax (Immunity) is the slowest fixture; Goldeen (Water Veil) outspeeds it.
    let poisoned = || {
        status_holder(
            SpeciesId::SNORLAX,
            0,
            AbilityId::IMMUNITY,
            Status1::Poisoned,
        )
    };
    let burned = || {
        status_holder(
            SpeciesId::GOLDEEN,
            1,
            AbilityId::WATER_VEIL,
            Status1::Burned,
        )
    };
    let cure_poison = cured(true, AbilityId::IMMUNITY, CuredCondition::Poison);
    let cure_burn = cured(false, AbilityId::WATER_VEIL, CuredCondition::Burn);

    // The enemy moves first and the pass still starts with the player.
    let mut rng = SequenceRng::new(script(&[&C, &W]));
    let mut battle = Battle::new(Dex::new(), poisoned(), burned(), false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        [
            used(false, SPLASH).to_vec(),
            vec![cure_poison, cure_burn],
            used(true, SPLASH).to_vec(),
        ]
        .concat()
    );
    rng.assert_exhausted();

    // The player moves first and the pass still starts with the player.
    let cure_burn = cured(true, AbilityId::WATER_VEIL, CuredCondition::Burn);
    let cure_poison = cured(false, AbilityId::IMMUNITY, CuredCondition::Poison);
    let mut rng = SequenceRng::new(script(&[&C, &W]));
    let mut battle = Battle::new(Dex::new(), burned(), poisoned(), false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        [
            used(true, SPLASH).to_vec(),
            vec![cure_burn, cure_poison],
            used(false, SPLASH).to_vec(),
        ]
        .concat()
    );
    rng.assert_exhausted();
}

#[test]
fn a_cure_leaves_the_primary_status_and_every_other_volatile_alone() {
    // Own Tempo and Oblivious clear only their own volatile. The holder then
    // acts under the other one and its paralysis: a full-paralysis draw and
    // the surviving volatile's coin, each passing.
    for (species, volatile_cleared) in [
        (SpeciesId::SPINDA, CuredCondition::Confusion),
        (SpeciesId::ILLUMISE, CuredCondition::Infatuation),
    ] {
        let mut holder = mon(species, 20, &[SPLASH], 0);
        holder.set_status1(Status1::Paralysed);
        holder.volatiles_mut().set_confusion(3);
        holder.volatiles_mut().set_infatuated_by(BattlerSide::Enemy);
        holder.volatiles_mut().set_focus_energy();
        holder.volatiles_mut().set_charge();
        let ability = holder.ability();
        let mut rng = SequenceRng::new(script(&[&C, &W, &[1, 1]]));
        let mut battle = Battle::new(Dex::new(), holder, fast_opponent(), false, &mut rng).unwrap();

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        let cure = cured(true, ability, volatile_cleared);
        assert!(events.contains(&cure), "{events:?}");
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, BattleEvent::AbilityCuredStatus { .. }))
                .count(),
            1
        );
        let player = battle.player();
        assert_eq!(player.status1(), Status1::Paralysed);
        assert!(player.volatiles().focus_energy);
        assert!(player.volatiles().charged_up());
        assert_eq!(
            player.volatiles().confused(),
            volatile_cleared != CuredCondition::Confusion
        );
        assert_eq!(
            player.volatiles().infatuated(),
            volatile_cleared != CuredCondition::Infatuation
        );
        rng.assert_exhausted();
    }
}

/// A burned Water Veil Goldeen against a slower Snorlax, both level 50.
fn burned_goldeen(moves: &[MoveId]) -> BattlePokemon {
    let mut goldeen = mon(SpeciesId::GOLDEEN, 50, moves, 1);
    assert_eq!(goldeen.ability(), AbilityId::WATER_VEIL);
    goldeen.set_status1(Status1::Burned);
    goldeen
}

fn snorlax() -> BattlePokemon {
    mon(SpeciesId::SNORLAX, 50, &[SPLASH], 0)
}

fn hit(by_player: bool, move_id: MoveId, damage: u32) -> BattleEvent {
    BattleEvent::Hit {
        by_player,
        move_id,
        damage,
        is_critical: false,
    }
}

#[test]
fn the_burn_still_halves_the_hit_that_precedes_the_cure() {
    let goldeen = burned_goldeen(&[MoveId::TACKLE]);
    let enemy = snorlax();
    assert!(goldeen.speed_for_turn_order() > enemy.speed_for_turn_order());
    let mut rng = SequenceRng::new(script(&[&C, &W, &H]));
    let mut battle = Battle::new(Dex::new(), goldeen, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Level 50 max-IV Goldeen Attack 87 against Snorlax Defense 85, Tackle's
    // 35 power: (22 * 35 * 87 / 85) / 50 = 15, halved by the burn to 7, plus
    // 2 is 9; a healthy Goldeen deals 17 (`pokemon.c:3259`-`:3264`, `:3371`).
    assert_eq!(
        events,
        [
            vec![
                BattleEvent::UsedMove {
                    by_player: true,
                    move_id: MoveId::TACKLE
                },
                hit(true, MoveId::TACKLE, 9),
                cured(true, AbilityId::WATER_VEIL, CuredCondition::Burn),
            ],
            used(false, SPLASH).to_vec(),
        ]
        .concat()
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().current_hp(), battle.player().stats().max_hp);
    rng.assert_exhausted();
}

#[test]
fn a_multi_hit_move_is_cured_between_strikes() {
    let goldeen = burned_goldeen(&[MoveId::FURY_ATTACK]);
    // Accuracy, a two-hit count, then each strike's non-critical roll and
    // damage roll, and the discarded final effect chance.
    let strikes = [0, 0, 1, 0, 1, 0, 0];
    let mut rng = SequenceRng::new(script(&[&C, &W, &strikes]));
    let mut battle = Battle::new(Dex::new(), goldeen, snorlax(), false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Fury Attack's 15 power: (22 * 15 * 87 / 85) / 50 = 6, halved by the
    // burn to 3, plus 2 is 5 for the first strike; the cured second strike
    // deals 8.
    assert_eq!(
        events,
        [
            vec![
                BattleEvent::UsedMove {
                    by_player: true,
                    move_id: MoveId::FURY_ATTACK
                },
                hit(true, MoveId::FURY_ATTACK, 5),
                cured(true, AbilityId::WATER_VEIL, CuredCondition::Burn),
                hit(true, MoveId::FURY_ATTACK, 8),
                BattleEvent::MultiHit {
                    by_player: true,
                    move_id: MoveId::FURY_ATTACK,
                    hits: 2
                },
            ],
            used(false, SPLASH).to_vec(),
        ]
        .concat()
    );
    rng.assert_exhausted();
}

#[test]
fn a_miss_is_followed_by_the_cure_too() {
    let goldeen = burned_goldeen(&[MoveId::TACKLE]);
    // Accuracy 95 against a roll of 99 + 1.
    let mut rng = SequenceRng::new(script(&[&C, &W, &[99]]));
    let mut battle = Battle::new(Dex::new(), goldeen, snorlax(), false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        [
            vec![
                BattleEvent::UsedMove {
                    by_player: true,
                    move_id: MoveId::TACKLE
                },
                BattleEvent::Missed {
                    by_player: true,
                    move_id: MoveId::TACKLE
                },
                cured(true, AbilityId::WATER_VEIL, CuredCondition::Burn),
            ],
            used(false, SPLASH).to_vec(),
        ]
        .concat()
    );
    rng.assert_exhausted();
}

#[test]
fn a_knockout_is_reported_before_the_survivors_cure_and_the_outcome() {
    let mut enemy = snorlax();
    enemy.apply_damage(enemy.current_hp() - 1);
    let goldeen = burned_goldeen(&[MoveId::TACKLE]);
    let mut rng = SequenceRng::new(script(&[&C, &W, &H]));
    let mut battle = Battle::new(Dex::new(), goldeen, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::UsedMove {
                by_player: true,
                move_id: MoveId::TACKLE
            },
            hit(true, MoveId::TACKLE, 1),
            BattleEvent::Fainted { by_player: false },
            cured(true, AbilityId::WATER_VEIL, CuredCondition::Burn),
            BattleEvent::ExpGained {
                amount: 1100,
                boosted: false
            },
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    rng.assert_exhausted();
}

/// A level 5 Snorlax outspeeded by every level 20 fixture, whatever its status.
fn slow_opponent() -> BattlePokemon {
    mon(SpeciesId::SNORLAX, 5, &[SPLASH], 0)
}

/// Plays a turn in which the holder acts first, drawing `canceller` after the
/// opening `[C, W]`, and returns its events.
fn cancelled_turn(holder: BattlePokemon, canceller: &[u16]) -> Vec<BattleEvent> {
    let enemy = slow_opponent();
    assert!(holder.speed_for_turn_order() > enemy.speed_for_turn_order());
    let mut rng = SequenceRng::new(script(&[&C, &W, canceller]));
    let mut battle = Battle::new(Dex::new(), holder, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    events
}

#[test]
fn a_cancelled_action_still_runs_the_pass() {
    for pairing in pairings() {
        let (canceller, stopped): (&[u16], Vec<BattleEvent>) = match pairing.condition {
            // Three turns left decrement to two: still fast asleep.
            CuredCondition::Sleep => (
                &[],
                vec![BattleEvent::FastAsleep {
                    by_player: true,
                    move_id: SPLASH,
                }],
            ),
            // `Random() % 5` nonzero: still frozen solid.
            CuredCondition::Freeze => (
                &[1],
                vec![BattleEvent::FrozenSolid {
                    by_player: true,
                    move_id: SPLASH,
                }],
            ),
            // `Random() % 4 == 0`: fully paralysed.
            CuredCondition::Paralysis => (
                &[0],
                vec![BattleEvent::FullyParalyzed {
                    by_player: true,
                    move_id: SPLASH,
                }],
            ),
            // An even coin: the self-hit, whose damage roll follows.
            CuredCondition::Confusion => (
                &[0, 0],
                vec![
                    BattleEvent::Confused { by_player: true },
                    BattleEvent::ConfusionSelfHit {
                        by_player: true,
                        // Level 20 Spinda, Attack and Defense 35: 8 + 2.
                        damage: 10,
                    },
                ],
            ),
            // An even coin immobilizes.
            CuredCondition::Infatuation => (
                &[0],
                vec![
                    BattleEvent::InLove { by_player: true },
                    BattleEvent::ImmobilizedByLove {
                        by_player: true,
                        move_id: SPLASH,
                    },
                ],
            ),
            CuredCondition::Poison | CuredCondition::Burn => continue,
        };
        let events = cancelled_turn((pairing.make)(), canceller);
        let expected = [
            stopped,
            vec![cured(true, pairing.ability, pairing.condition)],
            used(false, SPLASH).to_vec(),
        ]
        .concat();
        assert_eq!(events, expected, "{:?}", pairing.ability);
    }
}

#[test]
fn a_condition_that_ends_by_itself_first_leaves_nothing_to_cure() {
    let woken = status_holder(
        SpeciesId::HOOTHOOT,
        0,
        AbilityId::INSOMNIA,
        Status1::Asleep { turns: 1 },
    );
    let mut snapped = mon(SpeciesId::SPINDA, 20, &[SPLASH], 0);
    snapped.volatiles_mut().set_confusion(1);
    let thawed = status_holder(
        SpeciesId::SLUGMA,
        0,
        AbilityId::MAGMA_ARMOR,
        Status1::Frozen,
    );

    // The last turn of sleep wakes with no draw, the last of confusion
    // snaps out with none, and a zero `Random() % 5` thaws.
    for (holder, canceller, ended) in [
        (woken, &[][..], BattleEvent::WokeUp { by_player: true }),
        (
            snapped,
            &[][..],
            BattleEvent::SnappedOutOfConfusion { by_player: true },
        ),
        (thawed, &[0][..], BattleEvent::Thawed { by_player: true }),
    ] {
        let events = cancelled_turn(holder, canceller);
        assert_eq!(
            events,
            [
                vec![ended],
                used(true, SPLASH).to_vec(),
                used(false, SPLASH).to_vec()
            ]
            .concat()
        );
    }
}

#[test]
fn every_toxic_counter_is_cured_as_poison() {
    for counter in [0, 5, 15] {
        let holder = status_holder(
            SpeciesId::SNORLAX,
            0,
            AbilityId::IMMUNITY,
            Status1::BadPoisoned { counter },
        );
        let mut rng = SequenceRng::new(script(&[&C, &W]));
        let mut battle = Battle::new(Dex::new(), holder, fast_opponent(), false, &mut rng).unwrap();

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert!(
            events.contains(&cured(true, AbilityId::IMMUNITY, CuredCondition::Poison)),
            "counter {counter}: {events:?}"
        );
        assert_eq!(battle.player().status1(), Status1::Healthy);
        let hp = battle.player().stats().max_hp;
        assert_eq!(battle.player().current_hp(), hp, "no toxic damage follows");
        rng.assert_exhausted();
    }
}

/// A Serene Grace attacker whose unported chance-doubling matters for a
/// poison or burn secondary that can still land.
fn serene_grace_attacker(move_id: MoveId) -> BattlePokemon {
    let attacker = mon(SpeciesId::DUNSPARCE, 20, &[move_id], 0);
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    attacker
}

#[test]
fn a_status_the_pass_will_cure_does_not_unlock_the_serene_grace_screens() {
    // The defender's own earlier move end can cure the status the screen reads
    // at construction, so the secondary can still land on a healthy target.
    let paralysed_limber =
        || status_holder(SpeciesId::PERSIAN, 0, AbilityId::LIMBER, Status1::Paralysed);
    let burned_water_veil = || {
        status_holder(
            SpeciesId::GOLDEEN,
            1,
            AbilityId::WATER_VEIL,
            Status1::Burned,
        )
    };
    let poisoned_immunity = || {
        status_holder(
            SpeciesId::SNORLAX,
            0,
            AbilityId::IMMUNITY,
            Status1::Poisoned,
        )
    };
    for (attacker, defender) in [
        (
            serene_grace_attacker(MoveId::POISON_STING),
            paralysed_limber(),
        ),
        (
            serene_grace_attacker(MoveId::POISON_STING),
            burned_water_veil(),
        ),
        (
            serene_grace_attacker(MoveId::SACRED_FIRE),
            paralysed_limber(),
        ),
        (
            serene_grace_attacker(MoveId::SACRED_FIRE),
            poisoned_immunity(),
        ),
    ] {
        let ability = defender.ability();
        let mut rng = SequenceRng::new([]);
        let refusal = Battle::new(Dex::new(), defender, attacker, false, &mut rng).err();
        assert_eq!(
            refusal,
            Some(battle::BattleError::UnportedAbilityInteraction(
                AbilityId::SERENE_GRACE
            )),
            "{ability:?}"
        );
        rng.assert_exhausted();
    }

    // Immunity blocks the poison outright, so there is nothing left to screen.
    let attacker = serene_grace_attacker(MoveId::POISON_STING);
    let defender = poisoned_immunity();
    let tie = defender.speed_for_turn_order() == attacker.speed_for_turn_order();
    let mut rng = SequenceRng::new(crate::common::admission_script(tie));
    Battle::new(Dex::new(), defender, attacker, false, &mut rng).unwrap();
    rng.assert_exhausted();
}
