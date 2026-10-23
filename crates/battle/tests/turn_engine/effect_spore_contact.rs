//! Effect Spore's contact status (`ABILITYEFFECT_ON_DAMAGE`,
//! `battle_util.c:2782`-`:2804`): a `Random() % 10` chance, then `Random() & 3`
//! redrawn until nonzero to select sleep, poison or paralysis.

use crate::poison_support::*;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Status1};

const SHROOMISH: u16 = 306;
const FASTER_THAN_HOLDER_LEVEL: u8 = 12;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 0;
const HIT_COUNT_TWO: u16 = 0;
const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;

/// The faster battler's whole action, ahead of its contact draws.
const FASTER_HITS: [u16; 7] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// The slower battler's whole action when it still acts after the contact draws.
const SLOWER_HITS: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// A paralysed battler's action first draws `Random() % 4`; a nonzero value acts.
const FULL_PARALYSIS_AVOIDED: u16 = 1;

/// `Random() % 10 == 0` triggers Effect Spore.
const SPORE_TRIGGERS: u16 = 0;
const SPORE_MISSES: u16 = 1;
const SELECTS_NOTHING: u16 = 0;
const SELECTS_SLEEP: u16 = 1;
const SELECTS_POISON: u16 = 2;
const SELECTS_BURN_BECOMING_PARALYSIS: u16 = 3;
/// `(Random() & 3) + 2` sleep turns for a draw of `n` is `n + 2`.
const SLEEP_DURATION_DRAW_FOR_FIVE_TURNS: u16 = 3;

const TRIGGER_SLEEP: [u16; 3] = [
    SPORE_TRIGGERS,
    SELECTS_SLEEP,
    SLEEP_DURATION_DRAW_FOR_FIVE_TURNS,
];
const TRIGGER_POISON: [u16; 2] = [SPORE_TRIGGERS, SELECTS_POISON];
const TRIGGER_PARALYSIS: [u16; 2] = [SPORE_TRIGGERS, SELECTS_BURN_BECOMING_PARALYSIS];

fn shroomish(dex: &Dex, level: u8) -> BattlePokemon {
    let mon = max_iv_mon(dex, SHROOMISH, level, vec![TACKLE]);
    assert_eq!(mon.ability(), AbilityId::EFFECT_SPORE);
    mon
}

fn rattata(dex: &Dex, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, RATTATA, level, moves)
}

/// Builds a species at a level that out-speeds a level-5 [`shroomish`] holder.
fn species_with_ability(dex: &Dex, species: SpeciesId, ability: AbilityId) -> BattlePokemon {
    let mon = max_iv_mon(dex, species.0, FASTER_THAN_HOLDER_LEVEL, vec![TACKLE]);
    assert_eq!(mon.ability(), ability);
    mon
}

fn synchronizer(dex: &Dex) -> BattlePokemon {
    species_with_ability(dex, SpeciesId(RALTS), AbilityId::SYNCHRONIZE)
}

/// Leaves `mon` with exactly `hp` HP.
fn with_hp(mut mon: BattlePokemon, hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - hp);
    mon
}

fn with_status(mut mon: BattlePokemon, status: Status1) -> BattlePokemon {
    mon.set_status1(status);
    mon
}

fn script(groups: &[&[u16]]) -> Vec<u16> {
    groups.concat()
}

/// Runs the player's first move for one turn on an exactly scripted RNG.
fn take_player_move(
    player: BattlePokemon,
    enemy: BattlePokemon,
    dex: Dex,
    draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(draws.iter().copied());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

/// A faster [`RATTATA`] attacker hits a slower [`shroomish`] holder, which
/// survives and acts afterwards, with `contact` between the two actions.
fn rattata_hits_surviving_holder(
    attacker: BattlePokemon,
    contact: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let holder = shroomish(&dex, 5);
    take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, contact, &SLOWER_HITS]),
    )
}

fn effect_spore_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::AsleepByEffectSpore { .. }
                    | BattleEvent::PoisonedByEffectSpore { .. }
                    | BattleEvent::ParalyzedByEffectSpore { .. }
            )
        })
        .collect()
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

const ASLEEP_EVENT: BattleEvent = BattleEvent::AsleepByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};
const POISONED_EVENT: BattleEvent = BattleEvent::PoisonedByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};
const PARALYZED_EVENT: BattleEvent = BattleEvent::ParalyzedByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};

#[test]
fn a_failed_chance_draws_once_and_leaves_the_attacker_healthy() {
    for failed_chance in [SPORE_MISSES, 9, 65_535] {
        let dex = Dex::new();
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        let (battle, events) = rattata_hits_surviving_holder(attacker, &[failed_chance]);

        assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
        assert!(effect_spore_events(&events).is_empty(), "{events:?}");
    }
}

#[test]
fn the_chance_is_a_modulo_ten_roll_rather_than_a_raw_zero() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[10, SELECTS_POISON]);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
}

#[test]
fn a_non_contact_hit_into_an_effect_spore_holder_draws_no_roll() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![WATER_GUN]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[]);

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert!(effect_spore_events(&events).is_empty(), "{events:?}");
}

#[test]
fn a_selected_sleep_draws_its_two_to_five_turn_duration_after_the_selector() {
    for (duration_draw, turns) in [(0, 2), (1, 3), (2, 4), (3, 5)] {
        let dex = Dex::new();
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        let contact = [SPORE_TRIGGERS, SELECTS_SLEEP, duration_draw];
        let (battle, events) = rattata_hits_surviving_holder(attacker, &contact);

        assert_eq!(
            battle.player().status1(),
            Status1::Asleep { turns },
            "{events:?}"
        );
        assert_eq!(effect_spore_events(&events), vec![&ASLEEP_EVENT]);
    }
}

#[test]
fn a_selected_poison_poisons_the_attacker() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(effect_spore_events(&events), vec![&POISONED_EVENT]);
}

#[test]
fn a_selector_of_three_paralyses_the_attacker_instead_of_burning_it() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_PARALYSIS);

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(effect_spore_events(&events), vec![&PARALYZED_EVENT]);
}

#[test]
fn the_selector_keeps_only_its_low_two_bits() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[SPORE_TRIGGERS, 7]);

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
}

#[test]
fn a_zero_selector_is_redrawn_until_a_nonzero_one_arrives() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let contact = [
        SPORE_TRIGGERS,
        SELECTS_NOTHING,
        4,
        8,
        SELECTS_SLEEP,
        SLEEP_DURATION_DRAW_FOR_FIVE_TURNS,
    ];
    let (battle, events) = rattata_hits_surviving_holder(attacker, &contact);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
}

#[test]
fn each_status_lands_on_the_enemy_attacker_of_a_player_holder() {
    let enemy_event = |kind: fn(bool, MoveId) -> BattleEvent| kind(false, TACKLE);
    let outcomes = [
        (
            TRIGGER_SLEEP.to_vec(),
            Status1::Asleep { turns: 5 },
            enemy_event(|by_player, move_id| BattleEvent::AsleepByEffectSpore {
                by_player,
                move_id,
            }),
        ),
        (
            TRIGGER_POISON.to_vec(),
            Status1::Poisoned,
            enemy_event(|by_player, move_id| BattleEvent::PoisonedByEffectSpore {
                by_player,
                move_id,
            }),
        ),
        (
            TRIGGER_PARALYSIS.to_vec(),
            Status1::Paralysed,
            enemy_event(|by_player, move_id| BattleEvent::ParalyzedByEffectSpore {
                by_player,
                move_id,
            }),
        ),
    ];
    for (contact, expected_status, expected_event) in outcomes {
        let dex = Dex::new();
        let holder = shroomish(&dex, 5);
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        // Equal levels leave the faster Rattata first on the enemy side too.
        let (battle, events) = take_player_move(
            holder,
            attacker,
            dex,
            &script(&[&FASTER_HITS, &contact, &SLOWER_HITS]),
        );

        assert_eq!(battle.enemy().status1(), expected_status, "{events:?}");
        assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
        assert_eq!(effect_spore_events(&events), vec![&expected_event]);
    }
}

#[test]
fn a_holder_that_faints_to_the_hit_still_rolls_after_its_faint_settles() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 50, vec![TACKLE]);
    let holder = shroomish(&dex, 5);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, &TRIGGER_SLEEP]),
    );

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    assert!(
        position(&events, &BattleEvent::Fainted { by_player: false })
            < position(&events, &ASLEEP_EVENT),
        "{events:?}"
    );
}

#[test]
fn an_attacker_that_fainted_to_its_own_recoil_draws_no_roll() {
    let dex = Dex::new();
    let holder = shroomish(&dex, 40);
    let mut attacker = with_hp(rattata(&dex, 50, vec![TACKLE]), 1);
    for _ in 0..attacker.moves()[0].pp {
        attacker.deduct_pp(0).unwrap();
    }

    // The all-spent enemy's forced Struggle pick skips the selection draw.
    let mut rng = SequenceRng::new([
        CONSTRUCTION_TURN_NUMBER,
        TURN_START,
        ACCURACY_HITS,
        NO_CRIT,
        DAMAGE_ROLL,
    ]);
    let mut battle = Battle::new(dex, holder, attacker, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert!(effect_spore_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn sleep_poison_and_paralysis_rejections_spend_their_draws_and_stay_silent() {
    struct Rejection {
        label: &'static str,
        attacker: fn(&Dex) -> BattlePokemon,
        contact: &'static [u16],
        status: Status1,
    }
    let rejections = [
        Rejection {
            label: "Insomnia rejects sleep",
            attacker: |dex| species_with_ability(dex, SpeciesId::HOOTHOOT, AbilityId::INSOMNIA),
            contact: &[SPORE_TRIGGERS, SELECTS_SLEEP],
            status: Status1::Healthy,
        },
        Rejection {
            label: "Vital Spirit rejects sleep",
            attacker: |dex| species_with_ability(dex, SpeciesId::MANKEY, AbilityId::VITAL_SPIRIT),
            contact: &[SPORE_TRIGGERS, SELECTS_SLEEP],
            status: Status1::Healthy,
        },
        Rejection {
            label: "a primary status rejects sleep",
            attacker: |dex| with_status(rattata(dex, 5, vec![TACKLE]), Status1::Poisoned),
            contact: &[SPORE_TRIGGERS, SELECTS_SLEEP],
            status: Status1::Poisoned,
        },
        Rejection {
            label: "a Poison type rejects poison",
            attacker: |dex| species_with_ability(dex, SpeciesId::ZUBAT, AbilityId::INNER_FOCUS),
            contact: &[SPORE_TRIGGERS, SELECTS_POISON],
            status: Status1::Healthy,
        },
        Rejection {
            label: "a Steel type rejects poison",
            attacker: |dex| species_with_ability(dex, SpeciesId::ARON, AbilityId::STURDY),
            contact: &[SPORE_TRIGGERS, SELECTS_POISON],
            status: Status1::Healthy,
        },
        Rejection {
            label: "Immunity rejects poison",
            attacker: |dex| species_with_ability(dex, SpeciesId::ZANGOOSE, AbilityId::IMMUNITY),
            contact: &[SPORE_TRIGGERS, SELECTS_POISON],
            status: Status1::Healthy,
        },
        Rejection {
            label: "a primary status rejects poison",
            attacker: |dex| with_status(rattata(dex, 5, vec![TACKLE]), Status1::Poisoned),
            contact: &[SPORE_TRIGGERS, SELECTS_POISON],
            status: Status1::Poisoned,
        },
        Rejection {
            label: "Limber rejects paralysis",
            attacker: |dex| species_with_ability(dex, SpeciesId::DITTO, AbilityId::LIMBER),
            contact: &[SPORE_TRIGGERS, SELECTS_BURN_BECOMING_PARALYSIS],
            status: Status1::Healthy,
        },
        Rejection {
            label: "a primary status rejects paralysis",
            attacker: |dex| with_status(rattata(dex, 5, vec![TACKLE]), Status1::Poisoned),
            contact: &[SPORE_TRIGGERS, SELECTS_BURN_BECOMING_PARALYSIS],
            status: Status1::Poisoned,
        },
    ];
    for rejection in rejections {
        let dex = Dex::new();
        let attacker = (rejection.attacker)(&dex);
        let holder = shroomish(&dex, 5);
        let attacker_speed_wins = attacker.stats().speed > holder.stats().speed;
        assert!(attacker_speed_wins, "{}", rejection.label);

        let (battle, events) = take_player_move(
            attacker,
            holder,
            dex,
            &script(&[&FASTER_HITS, rejection.contact, &SLOWER_HITS]),
        );

        assert_eq!(
            battle.player().status1(),
            rejection.status,
            "{}",
            rejection.label
        );
        assert!(
            effect_spore_events(&events).is_empty(),
            "{}: {events:?}",
            rejection.label
        );
    }
}

#[test]
fn ability_effects_ignore_the_move_type_and_shield_dust_guards() {
    let dex = Dex::new();
    let attacker = species_with_ability(&dex, SpeciesId::GEODUDE, AbilityId::ROCK_HEAD);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_PARALYSIS);
    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");

    let dex = Dex::new();
    let attacker = species_with_ability(&dex, SpeciesId::WURMPLE, AbilityId::SHIELD_DUST);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_POISON);
    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
}

#[test]
fn a_synchronize_attacker_reflects_effect_spore_poison_onto_a_living_holder() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(synchronizer(&dex), &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Poisoned, "{events:?}");
    assert!(
        position(&events, &POISONED_EVENT)
            < position(
                &events,
                &BattleEvent::PoisonedByAttackerSynchronize {
                    by_player: true,
                    move_id: TACKLE,
                }
            ),
        "{events:?}"
    );
}

#[test]
fn a_synchronize_attacker_reflects_effect_spore_paralysis_onto_a_living_holder() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(
        synchronizer(&dex),
        &script(&[&TRIGGER_PARALYSIS, &[FULL_PARALYSIS_AVOIDED]]),
    );

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
    assert!(
        position(&events, &PARALYZED_EVENT)
            < position(
                &events,
                &BattleEvent::ParalyzedByAttackerSynchronize {
                    by_player: true,
                    move_id: TACKLE,
                }
            ),
        "{events:?}"
    );
}

#[test]
fn a_synchronize_attacker_never_reflects_effect_spore_sleep() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(synchronizer(&dex), &TRIGGER_SLEEP);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy, "{events:?}");
}

#[test]
fn a_synchronize_attacker_does_not_reflect_onto_a_holder_that_already_has_a_status() {
    let dex = Dex::new();
    let attacker = synchronizer(&dex);
    let holder = with_status(shroomish(&dex, 5), Status1::Paralysed);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[
            &FASTER_HITS,
            &TRIGGER_POISON,
            &[FULL_PARALYSIS_AVOIDED],
            &SLOWER_HITS,
        ]),
    );

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
}

#[test]
fn a_synchronize_attacker_does_not_reflect_onto_a_holder_that_fainted_to_the_hit() {
    let dex = Dex::new();
    let attacker = max_iv_mon(&dex, RALTS, 50, vec![TACKLE]);
    let holder = shroomish(&dex, 5);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, &TRIGGER_POISON]),
    );

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::PoisonedByAttackerSynchronize { .. })),
        "{events:?}"
    );
}

/// Two Double Slap strikes of a surviving holder; each strike is crit, damage,
/// then its contact draws.
fn double_slap_through_two_strikes(first: &[u16], second: &[u16]) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = shroomish(&dex, 40);
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        first,
        &[NO_CRIT, DAMAGE_ROLL],
        second,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
        &SLOWER_HITS,
    ]);
    take_player_move(attacker, holder, dex, &draws)
}

#[test]
fn a_multi_hit_contact_move_rolls_effect_spore_after_every_hit() {
    let (battle, events) = double_slap_through_two_strikes(&[SPORE_MISSES], &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(
        effect_spore_events(&events),
        vec![&BattleEvent::PoisonedByEffectSpore {
            by_player: true,
            move_id: DOUBLE_SLAP,
        }]
    );
    let hits: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(hits.len(), 2, "{events:?}");
    let poisoned_at = events
        .iter()
        .position(|event| matches!(event, BattleEvent::PoisonedByEffectSpore { .. }))
        .unwrap();
    assert!(hits[0] < hits[1] && hits[1] < poisoned_at, "{events:?}");
    assert!(events.contains(&BattleEvent::MultiHit {
        by_player: true,
        move_id: DOUBLE_SLAP,
        hits: 2,
    }));
}

#[test]
fn a_second_strike_spends_its_roll_even_when_the_first_already_poisoned() {
    let (battle, events) = double_slap_through_two_strikes(&TRIGGER_POISON, &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(effect_spore_events(&events).len(), 1, "{events:?}");
}

#[test]
fn effect_spore_sleep_ends_the_remaining_strikes_but_reports_the_landed_ones() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = shroomish(&dex, 40);
    // The second strike's crit and damage draws are absent: sleep ends the loop.
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        &TRIGGER_SLEEP,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
        &SLOWER_HITS,
    ]);
    let (battle, events) = take_player_move(attacker, holder, dex, &draws);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    let hits = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .count();
    assert_eq!(hits, 1, "{events:?}");
    assert!(events.contains(&BattleEvent::MultiHit {
        by_player: true,
        move_id: DOUBLE_SLAP,
        hits: 1,
    }));
}

#[test]
fn a_multi_hit_strike_that_kills_the_holder_publishes_its_status_before_the_faint() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 60, vec![DOUBLE_SLAP]);
    let holder = with_hp(shroomish(&dex, 40), 1);
    let draws = script(&[
        &[
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
        ],
        &TRIGGER_POISON,
        &[DOUBLE_SLAP_EFFECT_CHANCE],
    ]);
    let (battle, events) = take_player_move(attacker, holder, dex, &draws);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    let poisoned = position(
        &events,
        &BattleEvent::PoisonedByEffectSpore {
            by_player: true,
            move_id: DOUBLE_SLAP,
        },
    );
    let summary = position(
        &events,
        &BattleEvent::MultiHit {
            by_player: true,
            move_id: DOUBLE_SLAP,
            hits: 1,
        },
    );
    let fainted = position(&events, &BattleEvent::Fainted { by_player: false });
    assert!(poisoned < summary && summary < fainted, "{events:?}");
}
