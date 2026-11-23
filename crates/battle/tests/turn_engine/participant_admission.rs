//! Both constructors refuse always-refused abilities (Intimidate, Drizzle,
//! Drought, Sand Stream) before any RNG draw, and admit every status or
//! volatile pairing the move-end immunity pass cures (`battle_util.c:2873-2951`).

use crate::common::{admission_script, max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::battle::BattlerSide;
use battle::{Battle, BattleError, BattlePokemon, Dex, Status1};

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

fn mon(species: SpeciesId, slot: u8, status: Status1) -> BattlePokemon {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, species.0, 20, vec![MoveId::TACKLE]).with_ability_slot(slot);
    mon.set_status1(status);
    mon
}

fn healthy(species: SpeciesId) -> BattlePokemon {
    mon(species, 0, Status1::Healthy)
}

fn refused() -> [(BattlePokemon, AbilityId); 6] {
    [
        (healthy(SpeciesId::MIGHTYENA), AbilityId::INTIMIDATE),
        (healthy(SpeciesId::EKANS), AbilityId::INTIMIDATE),
        (
            mon(SpeciesId::MAWILE, 1, Status1::Healthy),
            AbilityId::INTIMIDATE,
        ),
        (healthy(SpeciesId::KYOGRE), AbilityId::DRIZZLE),
        (healthy(SpeciesId::GROUDON), AbilityId::DROUGHT),
        (healthy(SpeciesId::TYRANITAR), AbilityId::SAND_STREAM),
    ]
}

/// Every pairing the move-end immunity pass cures: a holder, its ability and
/// the condition it carries.
fn cured() -> Vec<(BattlePokemon, AbilityId)> {
    let mut pairings = vec![
        (
            mon(SpeciesId::PERSIAN, 0, Status1::Paralysed),
            AbilityId::LIMBER,
        ),
        (
            mon(SpeciesId::SNORLAX, 0, Status1::Poisoned),
            AbilityId::IMMUNITY,
        ),
        // Immunity cures bad poison too, whatever its stored counter
        // (`battle_util.c:2878`-`:2883`).
        (
            mon(SpeciesId::SNORLAX, 0, Status1::BadPoisoned { counter: 0 }),
            AbilityId::IMMUNITY,
        ),
        (
            mon(SpeciesId::SNORLAX, 0, Status1::BadPoisoned { counter: 5 }),
            AbilityId::IMMUNITY,
        ),
        (
            mon(SpeciesId::SNORLAX, 0, Status1::BadPoisoned { counter: 15 }),
            AbilityId::IMMUNITY,
        ),
        (
            mon(SpeciesId::GOLDEEN, 1, Status1::Burned),
            AbilityId::WATER_VEIL,
        ),
        (
            mon(SpeciesId::SLUGMA, 0, Status1::Frozen),
            AbilityId::MAGMA_ARMOR,
        ),
        (
            mon(SpeciesId::HOOTHOOT, 0, Status1::Asleep { turns: 3 }),
            AbilityId::INSOMNIA,
        ),
        (
            mon(SpeciesId::MANKEY, 0, Status1::Asleep { turns: 3 }),
            AbilityId::VITAL_SPIRIT,
        ),
    ];
    let mut spinda = healthy(SpeciesId::SPINDA);
    assert_eq!(spinda.ability(), AbilityId::OWN_TEMPO);
    spinda.volatiles_mut().set_confusion(3);
    pairings.push((spinda, AbilityId::OWN_TEMPO));
    let mut illumise = healthy(SpeciesId::ILLUMISE);
    assert_eq!(illumise.ability(), AbilityId::OBLIVIOUS);
    illumise
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Enemy);
    pairings.push((illumise, AbilityId::OBLIVIOUS));
    pairings
}

fn assert_refused_without_draws(
    result: &Result<Battle, BattleError>,
    ability: AbilityId,
    rng: &SequenceRng,
) {
    assert!(matches!(
        result,
        Err(BattleError::UnportedAbilityInteraction(found)) if *found == ability
    ));
    assert_eq!(rng.draws(), 0, "refusal precedes every constructor draw");
}

/// A trainer battle admits a participant, except that its AI cannot read a
/// two-ability player's ability: that error is separate from admission and
/// precedes every draw.
fn assert_admitted_or_ambiguous(
    result: &Result<Battle, BattleError>,
    ability: AbilityId,
    rng: &SequenceRng,
) {
    match result {
        Ok(_) => rng.assert_exhausted(),
        Err(BattleError::AmbiguousPlayerAbility(_)) => assert_eq!(rng.draws(), 0),
        Err(error) => panic!("{ability:?}: {error:?}"),
    }
}

fn wild(
    player: BattlePokemon,
    reserves: Vec<BattlePokemon>,
    enemy: BattlePokemon,
    first_battle: bool,
    rng: &mut SequenceRng,
) -> Result<Battle, BattleError> {
    Battle::new_with_player_reserves(Dex::new(), player, reserves, enemy, first_battle, rng)
}

fn trainer(
    player: BattlePokemon,
    reserves: Vec<BattlePokemon>,
    party: Vec<BattlePokemon>,
    rng: &mut SequenceRng,
) -> Result<Battle, BattleError> {
    Battle::new_trainer_with_player_reserves(
        Dex::new(),
        player,
        reserves,
        MAY_ROUTE_103_MUDKIP,
        party,
        rng,
    )
}

#[test]
fn wild_constructor_refuses_an_active_player_before_drawing() {
    for first_battle in [false, true] {
        for (bad, ability) in refused() {
            let mut rng = SequenceRng::new([]);
            let result = wild(
                bad,
                vec![],
                healthy(SpeciesId::ZIGZAGOON),
                first_battle,
                &mut rng,
            );
            assert_refused_without_draws(&result, ability, &rng);
        }
    }
}

#[test]
fn wild_constructor_refuses_an_active_enemy_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = wild(healthy(SpeciesId::ZIGZAGOON), vec![], bad, false, &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn wild_constructor_refuses_a_later_player_reserve_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let reserves = vec![healthy(SpeciesId::TREECKO), bad];
        let result = wild(
            healthy(SpeciesId::MUDKIP),
            reserves,
            healthy(SpeciesId::ZIGZAGOON),
            false,
            &mut rng,
        );
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_an_active_player_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = trainer(bad, vec![], vec![healthy(SpeciesId::TREECKO)], &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_the_active_enemy_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = trainer(healthy(SpeciesId::MUDKIP), vec![], vec![bad], &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_a_later_bench_member_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let party = vec![
            healthy(SpeciesId::TREECKO),
            healthy(SpeciesId::TREECKO),
            bad,
        ];
        let result = trainer(healthy(SpeciesId::MUDKIP), vec![], party, &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_a_later_player_reserve_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let reserves = vec![healthy(SpeciesId::TORCHIC), bad];
        let result = trainer(
            healthy(SpeciesId::MUDKIP),
            reserves,
            vec![healthy(SpeciesId::TREECKO)],
            &mut rng,
        );
        assert_refused_without_draws(&result, ability, &rng);
    }
}

const EFFECT_SPORE_HOLDERS: [SpeciesId; 4] = [
    SpeciesId::PARAS,
    SpeciesId::PARASECT,
    SpeciesId::SHROOMISH,
    SpeciesId::BRELOOM,
];

/// Truant's whole species line: its loafing alternation is modelled, so both
/// are admitted wherever they stand.
const TRUANT_HOLDERS: [SpeciesId; 2] = [SpeciesId::SLAKOTH, SpeciesId::SLAKING];

/// The constructor script for `player` against `enemy`: the order tiebreak is
/// drawn only when their turn-order Speeds tie.
fn admission_script_for(player: &BattlePokemon, enemy: &BattlePokemon) -> Vec<u16> {
    admission_script(player.speed_for_turn_order() == enemy.speed_for_turn_order())
}

#[test]
fn effect_spore_holders_are_admitted_on_every_participant_path() {
    for species in EFFECT_SPORE_HOLDERS {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            let holder = || {
                let holder = mon(species, 0, status);
                assert_eq!(holder.ability(), AbilityId::EFFECT_SPORE);
                holder
            };
            let opponent = || healthy(SpeciesId::ZIGZAGOON);
            let lead = || healthy(SpeciesId::MUDKIP);

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            wild(holder(), vec![], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&opponent(), &holder()));
            wild(opponent(), vec![], holder(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            wild(lead(), vec![holder()], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            trainer(holder(), vec![], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &holder()));
            trainer(lead(), vec![], vec![holder()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(
                lead(),
                vec![],
                vec![opponent(), opponent(), holder()],
                &mut rng,
            )
            .unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(lead(), vec![holder()], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();
        }
    }
}

/// Cute Charm's whole species pool: its contact infatuation is modelled
/// (`battle_util.c:2851`-`:2870`), so every holder is admitted wherever it stands.
const CUTE_CHARM_HOLDERS: [SpeciesId; 3] =
    [SpeciesId::CLEFAIRY, SpeciesId::SKITTY, SpeciesId::DELCATTY];

#[test]
fn cute_charm_holders_are_admitted_on_every_participant_path() {
    for species in CUTE_CHARM_HOLDERS {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            let holder = || {
                let holder = mon(species, 0, status);
                assert_eq!(holder.ability(), AbilityId::CUTE_CHARM);
                holder
            };
            let opponent = || healthy(SpeciesId::ZIGZAGOON);
            let lead = || healthy(SpeciesId::MUDKIP);

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            wild(holder(), vec![], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&opponent(), &holder()));
            wild(opponent(), vec![], holder(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            wild(lead(), vec![holder()], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            trainer(holder(), vec![], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &holder()));
            trainer(lead(), vec![], vec![holder()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(
                lead(),
                vec![],
                vec![opponent(), opponent(), holder()],
                &mut rng,
            )
            .unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(lead(), vec![holder()], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();
        }
    }
}

#[test]
fn truant_holders_are_admitted_on_every_participant_path() {
    for species in TRUANT_HOLDERS {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            let holder = || {
                let holder = mon(species, 0, status);
                assert_eq!(holder.ability(), AbilityId::TRUANT);
                holder
            };
            let opponent = || healthy(SpeciesId::ZIGZAGOON);
            let lead = || healthy(SpeciesId::MUDKIP);

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            wild(holder(), vec![], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&opponent(), &holder()));
            wild(opponent(), vec![], holder(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            wild(lead(), vec![holder()], opponent(), false, &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
            trainer(holder(), vec![], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &holder()));
            trainer(lead(), vec![], vec![holder()], &mut rng).unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(
                lead(),
                vec![],
                vec![opponent(), opponent(), holder()],
                &mut rng,
            )
            .unwrap();
            rng.assert_exhausted();

            let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
            trainer(lead(), vec![holder()], vec![opponent()], &mut rng).unwrap();
            rng.assert_exhausted();
        }
    }
}

#[test]
fn a_fainted_reserve_is_admitted_because_it_can_never_enter() {
    assert_ne!(
        healthy(SpeciesId::MUDKIP).speed_for_turn_order(),
        healthy(SpeciesId::ZIGZAGOON).speed_for_turn_order(),
        "the active pair must not tie on Speed, so construction draws no tiebreak and admission_script(false) fits"
    );
    let pairings = || refused().into_iter().chain(cured());
    for (index, (mut bad, _)) in pairings().enumerate() {
        bad.apply_damage(u32::MAX);
        assert!(bad.is_fainted());
        let mut rng = SequenceRng::new(admission_script(false));
        wild(
            healthy(SpeciesId::MUDKIP),
            vec![bad],
            healthy(SpeciesId::ZIGZAGOON),
            false,
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
        let (mut bad, _) = pairings().nth(index).unwrap();
        bad.apply_damage(u32::MAX);
        let mut rng = SequenceRng::new(admission_script(false));
        trainer(
            healthy(SpeciesId::MUDKIP),
            vec![],
            vec![healthy(SpeciesId::ZIGZAGOON), bad],
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
    }
}

#[test]
fn every_cured_pairing_is_admitted_on_every_participant_path() {
    for (index, (holder, ability)) in cured().into_iter().enumerate() {
        assert_eq!(holder.ability(), ability);
        let holder = || cured().remove(index).0;
        let opponent = || healthy(SpeciesId::ZIGZAGOON);
        let lead = || healthy(SpeciesId::MUDKIP);

        let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
        let battle = wild(holder(), vec![], opponent(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        assert_eq!(
            battle.player().status1(),
            holder().status1(),
            "construction cures nothing: the pass runs at a move end"
        );

        let mut rng = SequenceRng::new(admission_script_for(&opponent(), &holder()));
        wild(opponent(), vec![], holder(), false, &mut rng).unwrap();
        rng.assert_exhausted();

        let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
        wild(lead(), vec![holder()], opponent(), false, &mut rng).unwrap();
        rng.assert_exhausted();

        let mut rng = SequenceRng::new(admission_script_for(&holder(), &opponent()));
        let result = trainer(holder(), vec![], vec![opponent()], &mut rng);
        assert_admitted_or_ambiguous(&result, ability, &rng);

        let mut rng = SequenceRng::new(admission_script_for(&lead(), &holder()));
        trainer(lead(), vec![], vec![holder()], &mut rng).unwrap();
        rng.assert_exhausted();

        let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
        trainer(
            lead(),
            vec![],
            vec![opponent(), opponent(), holder()],
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();

        let mut rng = SequenceRng::new(admission_script_for(&lead(), &opponent()));
        let result = trainer(lead(), vec![holder()], vec![opponent()], &mut rng);
        assert_admitted_or_ambiguous(&result, ability, &rng);
    }
}

#[test]
fn confusion_is_admitted_for_every_holder_including_own_tempo() {
    for species in [SpeciesId::ZIGZAGOON, SpeciesId::SPINDA] {
        let mut holder = healthy(species);
        holder.volatiles_mut().set_confusion(3);
        let enemy = healthy(SpeciesId::ZIGZAGOON);
        assert_eq!(
            holder.speed_for_turn_order(),
            enemy.speed_for_turn_order(),
            "the fixture ties on Speed, so construction draws the seed and the order tiebreak"
        );
        let mut rng = SequenceRng::new(admission_script(true));
        let battle = wild(holder, vec![], enemy, false, &mut rng).unwrap();
        assert!(battle.player().volatiles().confused());
        rng.assert_exhausted();
    }
}

#[test]
fn an_always_refused_ability_is_refused_whatever_the_status() {
    for (species, ability) in [
        (SpeciesId::MIGHTYENA, AbilityId::INTIMIDATE),
        (SpeciesId::KYOGRE, AbilityId::DRIZZLE),
        (SpeciesId::GROUDON, AbilityId::DROUGHT),
        (SpeciesId::TYRANITAR, AbilityId::SAND_STREAM),
    ] {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            always_refused_whatever(species, ability, status);
        }
    }
}

fn always_refused_whatever(species: SpeciesId, ability: AbilityId, status: Status1) {
    let mut rng = SequenceRng::new([]);
    let result = wild(
        mon(species, 0, status),
        vec![],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    );
    assert_refused_without_draws(&result, ability, &rng);
    let mut rng = SequenceRng::new([]);
    let result = trainer(
        mon(species, 0, status),
        vec![],
        vec![healthy(SpeciesId::ZIGZAGOON)],
        &mut rng,
    );
    assert_refused_without_draws(&result, ability, &rng);
}

#[test]
fn the_selected_ability_slot_decides_not_the_species_pool() {
    for (species, slot, ability) in [
        (SpeciesId::EKANS, 1, AbilityId::SHED_SKIN),
        (SpeciesId::MAWILE, 0, AbilityId::HYPER_CUTTER),
        (SpeciesId::YANMA, 1, AbilityId::COMPOUND_EYES),
        (SpeciesId::RALTS, 0, AbilityId::SYNCHRONIZE),
        (SpeciesId::KIRLIA, 0, AbilityId::SYNCHRONIZE),
        (SpeciesId::GARDEVOIR, 0, AbilityId::SYNCHRONIZE),
    ] {
        let holder = mon(species, slot, Status1::Healthy);
        assert_eq!(holder.ability(), ability);
        let enemy = healthy(SpeciesId::ZIGZAGOON);
        assert_ne!(holder.speed_for_turn_order(), enemy.speed_for_turn_order());
        let mut rng = SequenceRng::new(admission_script(false));
        wild(holder, vec![], enemy, false, &mut rng).unwrap();
        rng.assert_exhausted();
    }

    let snorlax = mon(SpeciesId::SNORLAX, 1, Status1::Poisoned);
    assert_eq!(snorlax.ability(), AbilityId::THICK_FAT);
    let enemy = healthy(SpeciesId::ZIGZAGOON);
    assert_ne!(snorlax.speed_for_turn_order(), enemy.speed_for_turn_order());
    let mut rng = SequenceRng::new(admission_script(false));
    wild(snorlax, vec![], enemy, false, &mut rng).unwrap();
    rng.assert_exhausted();
}

/// Speed Boost is modelled: its end-turn stage and first-turn counter live in
/// `Battle`, so every side and bench slot admits it while unrelated refusals stay.
#[test]
fn speed_boost_holders_are_admitted_on_every_side_and_slot() {
    let tie_script = |player: &BattlePokemon, enemy: &BattlePokemon| {
        SequenceRng::new(admission_script(
            player.speed_for_turn_order() == enemy.speed_for_turn_order(),
        ))
    };
    for species in [SpeciesId::NINJASK, SpeciesId::YANMA] {
        let holder = || healthy(species);
        assert_eq!(holder().ability(), AbilityId::SPEED_BOOST);
        let foe = || healthy(SpeciesId::ZIGZAGOON);

        let mut rng = tie_script(&holder(), &foe());
        wild(holder(), vec![], foe(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &holder());
        wild(foe(), vec![], holder(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &foe());
        wild(foe(), vec![holder()], foe(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        // The trainer AI cannot score against a two-ability player (Yanma) it
        // cannot tell apart, an unrelated refusal that stays.
        if species == SpeciesId::NINJASK {
            let mut rng = tie_script(&foe(), &foe());
            trainer(foe(), vec![holder()], vec![foe()], &mut rng).unwrap();
            rng.assert_exhausted();
        }
        let mut rng = tie_script(&foe(), &foe());
        trainer(foe(), vec![], vec![foe(), holder()], &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &holder());
        trainer(foe(), vec![], vec![holder()], &mut rng).unwrap();
        rng.assert_exhausted();
    }
}

#[test]
fn a_speed_boost_holder_does_not_excuse_an_unrelated_refusal() {
    let holder = || healthy(SpeciesId::NINJASK);
    let mut rng = SequenceRng::new([]);
    let result = wild(
        holder(),
        vec![healthy(SpeciesId::MIGHTYENA)],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    );
    assert_refused_without_draws(&result, AbilityId::INTIMIDATE, &rng);
    let mut rng = SequenceRng::new([]);
    let result = trainer(
        healthy(SpeciesId::ZIGZAGOON),
        vec![],
        vec![holder(), healthy(SpeciesId::KYOGRE)],
        &mut rng,
    );
    assert_refused_without_draws(&result, AbilityId::DRIZZLE, &rng);
}

/// Color Change is modelled: its type change lives in the on-damage seam and the
/// battle-only typing, so every side and bench slot admits it, whatever the
/// holder's status, while unrelated refusals stay.
#[test]
fn color_change_holders_are_admitted_on_every_side_and_slot() {
    let tie_script = |player: &BattlePokemon, enemy: &BattlePokemon| {
        SequenceRng::new(admission_script(
            player.speed_for_turn_order() == enemy.speed_for_turn_order(),
        ))
    };
    for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
        let holder = || mon(SpeciesId::KECLEON, 0, status);
        assert_eq!(holder().ability(), AbilityId::COLOR_CHANGE);
        let foe = || healthy(SpeciesId::ZIGZAGOON);

        let mut rng = tie_script(&holder(), &foe());
        wild(holder(), vec![], foe(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &holder());
        wild(foe(), vec![], holder(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &foe());
        wild(foe(), vec![holder()], foe(), false, &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &foe());
        trainer(foe(), vec![holder()], vec![foe()], &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &foe());
        trainer(foe(), vec![], vec![foe(), holder()], &mut rng).unwrap();
        rng.assert_exhausted();
        let mut rng = tie_script(&foe(), &holder());
        trainer(foe(), vec![], vec![holder()], &mut rng).unwrap();
        rng.assert_exhausted();
    }
}

/// Flame Body is modelled: its contact burn lives in the on-damage seam, so
/// every side and bench slot admits it, whatever the holder's status, while
/// unrelated refusals stay.
#[test]
fn flame_body_holders_are_admitted_on_every_side_and_slot() {
    let tie_script = |player: &BattlePokemon, enemy: &BattlePokemon| {
        SequenceRng::new(admission_script(
            player.speed_for_turn_order() == enemy.speed_for_turn_order(),
        ))
    };
    for (species, slot) in [(SpeciesId::MAGMAR, 0), (SpeciesId::SLUGMA, 1)] {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            let holder = || mon(species, slot, status);
            assert_eq!(holder().ability(), AbilityId::FLAME_BODY);
            let foe = || healthy(SpeciesId::ZIGZAGOON);

            let mut rng = tie_script(&holder(), &foe());
            wild(holder(), vec![], foe(), false, &mut rng).unwrap();
            rng.assert_exhausted();
            let mut rng = tie_script(&foe(), &holder());
            wild(foe(), vec![], holder(), false, &mut rng).unwrap();
            rng.assert_exhausted();
            let mut rng = tie_script(&foe(), &foe());
            wild(foe(), vec![holder()], foe(), false, &mut rng).unwrap();
            rng.assert_exhausted();
            // The trainer AI cannot score against a two-ability player
            // (Slugma) it cannot tell apart, an unrelated refusal that stays.
            if species == SpeciesId::MAGMAR {
                let mut rng = tie_script(&foe(), &foe());
                trainer(foe(), vec![holder()], vec![foe()], &mut rng).unwrap();
                rng.assert_exhausted();
            }
            let mut rng = tie_script(&foe(), &foe());
            trainer(foe(), vec![], vec![foe(), holder()], &mut rng).unwrap();
            rng.assert_exhausted();
            let mut rng = tie_script(&foe(), &holder());
            trainer(foe(), vec![], vec![holder()], &mut rng).unwrap();
            rng.assert_exhausted();
        }
    }
}

/// Admitting Flame Body weakens no other refusal: a Flame Body holder beside
/// an Intimidate holder is still refused for Intimidate.
#[test]
fn a_flame_body_holder_does_not_excuse_an_unrelated_refusal() {
    let mut rng = SequenceRng::new([]);
    let result = wild(
        healthy(SpeciesId::MAGMAR),
        vec![healthy(SpeciesId::MIGHTYENA)],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    );
    assert_refused_without_draws(&result, AbilityId::INTIMIDATE, &rng);
}
