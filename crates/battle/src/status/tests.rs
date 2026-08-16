//! [`crate::status`]'s own unit tests: the two immunity rules that differ
//! between poison and paralysis, the short-circuiting full-paralysis draw,
//! and the two arithmetic halves.

use super::{can_inflict, full_paralysis_roll, paralysis_speed, poison_turn_damage, Status1};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::pokemon::{BattlePokemon, Ivs};
use assets::{MoveId, SpeciesId, Type};

struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}
impl SequenceRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }
    fn draws(&self) -> usize {
        self.index
    }
}
impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .expect("SequenceRng exhausted");
        self.index += 1;
        v
    }
}

const TACKLE: MoveId = MoveId(33);

fn mon(dex: &Dex, species: u16) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), 15, Ivs::default(), 0, vec![TACKLE]).unwrap()
}

#[test]
fn is_any_distinguishes_healthy_from_every_real_status() {
    assert!(!Status1::Healthy.is_any());
    assert!(Status1::Poisoned.is_any());
    assert!(Status1::Paralysed.is_any());
    assert_eq!(Status1::default(), Status1::Healthy);
}

/// `ENDTURN_POISON`: `maxHP / 8`, floored at 1, truncating.
#[test]
fn poison_damage_is_an_eighth_of_max_hp_with_a_floor_of_one() {
    assert_eq!(poison_turn_damage(80), 10);
    assert_eq!(poison_turn_damage(87), 10, "truncating, not rounding");
    assert_eq!(poison_turn_damage(8), 1);
    // Under 8 max HP truncates to 0 and is bumped -- upstream's own
    // `if (gBattleMoveDamage == 0) gBattleMoveDamage = 1`.
    assert_eq!(poison_turn_damage(7), 1);
    assert_eq!(poison_turn_damage(1), 1);
}

#[test]
fn paralysis_quarters_the_speed_it_is_handed() {
    assert_eq!(paralysis_speed(40), 10);
    assert_eq!(paralysis_speed(43), 10, "truncating");
    assert_eq!(paralysis_speed(3), 0, "and it can reach zero");
}

/// The short-circuit is the load-bearing part: an unparalysed battler must
/// make **no draw at all**, or every pre-existing battle's RNG stream would
/// shift by one per move.
#[test]
fn the_full_paralysis_roll_draws_only_for_a_paralysed_battler() {
    for healthy in [Status1::Healthy, Status1::Poisoned] {
        // An empty sequence: any draw would panic.
        let mut rng = SequenceRng::new([]);
        assert!(!full_paralysis_roll(healthy, &mut rng));
        assert_eq!(rng.draws(), 0, "{healthy:?} must not reach the Random()");
    }

    // Paralysed: exactly one draw, and `% 4 == 0` immobilises.
    let mut rng = SequenceRng::new([0]);
    assert!(full_paralysis_roll(Status1::Paralysed, &mut rng));
    assert_eq!(rng.draws(), 1);

    // The other three residues let the move through -- still one draw.
    for value in [1u16, 2, 3, 5, 6, 7] {
        let mut rng = SequenceRng::new([value]);
        assert!(
            !full_paralysis_roll(Status1::Paralysed, &mut rng),
            "draw {value} is not 0 mod 4"
        );
        assert_eq!(rng.draws(), 1);
    }
    // ...and a multiple of 4 does, whatever its size.
    let mut rng = SequenceRng::new([4, 400, 0xFFFC]);
    assert!(full_paralysis_roll(Status1::Paralysed, &mut rng));
    assert!(full_paralysis_roll(Status1::Paralysed, &mut rng));
    assert!(full_paralysis_roll(Status1::Paralysed, &mut rng));
}

/// Poison's *type* immunity, which paralysis does not share.
#[test]
fn poison_type_and_steel_type_targets_cannot_be_poisoned() {
    let dex = Dex::new();
    // Tentacool (72) is Water/Poison; Magnemite (81) is Electric/Steel.
    for species in [72u16, 81] {
        let target = mon(&dex, species);
        assert!(
            target.types().contains(&Type::Poison) || target.types().contains(&Type::Steel),
            "fixture: species {species} must actually be Poison or Steel"
        );
        assert!(
            !can_inflict(&target, Status1::Poisoned),
            "species {species} is immune to poison"
        );
        // ...but not to paralysis: Gen 3 has no type immunity there at all.
        assert!(
            can_inflict(&target, Status1::Paralysed),
            "species {species} can still be paralysed"
        );
    }
}

/// The immunity paralysis is often *assumed* to have and does not: an
/// Electric-type is paralysable in Gen 3 (`case STATUS1_PARALYSIS:` guards
/// only on Limber and an existing status).
#[test]
fn an_electric_type_can_still_be_paralysed_in_gen_three() {
    let dex = Dex::new();
    let voltorb = mon(&dex, 100);
    assert!(voltorb.types().contains(&Type::Electric), "fixture");
    assert!(can_inflict(&voltorb, Status1::Paralysed));
}

/// Any existing status blocks a new one, not just the same one.
#[test]
fn an_already_statused_target_takes_no_second_status() {
    let dex = Dex::new();
    let mut marill = mon(&dex, 183);
    assert!(can_inflict(&marill, Status1::Paralysed));
    assert!(can_inflict(&marill, Status1::Poisoned));

    marill.set_status(Status1::Poisoned);
    assert!(
        !can_inflict(&marill, Status1::Paralysed),
        "a poisoned mon cannot also be paralysed"
    );
    assert!(
        !can_inflict(&marill, Status1::Poisoned),
        "nor poisoned twice"
    );

    marill.set_status(Status1::Paralysed);
    assert!(!can_inflict(&marill, Status1::Poisoned));
}

/// `Status1::Healthy` is not a status anything can be "inflicted" with --
/// the guard exists so a caller cannot accidentally ask.
#[test]
fn inflicting_healthy_is_never_possible() {
    let dex = Dex::new();
    let marill = mon(&dex, 183);
    assert!(!can_inflict(&marill, Status1::Healthy));
}

/// The Speed quarter really reaches turn order, through
/// `BattlePokemon::effective_speed`, and it is applied **after** the stage
/// scaling rather than before -- the two truncate differently.
#[test]
fn effective_speed_applies_the_stage_before_the_paralysis_quarter() {
    let dex = Dex::new();
    let mut mon = mon(&dex, 100); // Voltorb: fast enough to divide interestingly
    let base = mon.effective_speed();
    assert_eq!(base, mon.stats().speed, "no stage, no status");

    mon.set_status(Status1::Paralysed);
    assert_eq!(
        mon.effective_speed(),
        base / 4,
        "paralysis quarters the stage-scaled speed"
    );

    mon.stages_mut().speed = crate::stat_stage::StatStage::new(-1).unwrap();
    let staged = base * 2 / 3;
    assert_eq!(
        mon.effective_speed(),
        staged / 4,
        "the stage is applied first, then the quarter -- both truncating"
    );
}
