//! Cases where Effect Spore draws no roll or its status is refused: a fainted
//! attacker, the silent sleep, poison and paralysis rejections that still
//! spend their draws, and the ability guards (Shield Dust, Rock Head) that do
//! not apply to an ability effect.

use super::*;

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
