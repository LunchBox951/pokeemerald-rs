//! [`crate::secondary`]'s own unit tests: the trampoline table, the chance
//! roll's exact threshold, and the two effects' shapes.

use super::{
    ensure_resolvable, is_secondary_effect, secondary_chance_roll, secondary_for_effect,
    SecondaryEffect, EFFECT_POISON_HIT, EFFECT_SPEED_DOWN_HIT,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::stat_change::{ChangedStat, StatChangeDirection};
use crate::status::Status1;
use assets::MoveId;

struct One(u16, usize);
impl BattleRng for One {
    fn next_u16(&mut self) -> u16 {
        self.1 += 1;
        self.0
    }
}

const POISON_STING: MoveId = MoveId(40);
const CONSTRICT: MoveId = MoveId(132);
const TACKLE: MoveId = MoveId(33);

#[test]
fn the_two_moves_carry_the_two_effect_ids_and_their_real_chances() {
    let dex = Dex::new();
    let sting = dex.move_data(POISON_STING).unwrap();
    assert_eq!(sting.effect, EFFECT_POISON_HIT);
    assert_eq!(sting.secondary_effect_chance, 30);
    assert_eq!(sting.power, 15, "a real damaging move, not a status one");

    let constrict = dex.move_data(CONSTRICT).unwrap();
    assert_eq!(constrict.effect, EFFECT_SPEED_DOWN_HIT);
    assert_eq!(constrict.secondary_effect_chance, 10);
    assert_eq!(constrict.power, 10);

    assert!(is_secondary_effect(EFFECT_POISON_HIT));
    assert!(is_secondary_effect(EFFECT_SPEED_DOWN_HIT));
    assert!(!is_secondary_effect(dex.move_data(TACKLE).unwrap().effect));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    assert_eq!(ensure_resolvable(&dex, POISON_STING), Ok(()));
}

#[test]
fn each_trampoline_maps_to_the_move_effect_byte_it_writes() {
    assert_eq!(
        secondary_for_effect(EFFECT_POISON_HIT),
        Some(SecondaryEffect::Poison)
    );
    assert_eq!(
        secondary_for_effect(EFFECT_SPEED_DOWN_HIT),
        Some(SecondaryEffect::SpeedDown)
    );
    assert_eq!(secondary_for_effect(assets::MoveEffect(0)), None);

    // Poison changes `status1` and no stage; the speed drop is the mirror.
    assert_eq!(SecondaryEffect::Poison.status(), Some(Status1::Poisoned));
    assert_eq!(SecondaryEffect::Poison.stat_change(), None);
    assert_eq!(SecondaryEffect::SpeedDown.status(), None);
    let change = SecondaryEffect::SpeedDown
        .stat_change()
        .expect("the speed drop is a stat change");
    assert_eq!(change.stat, ChangedStat::Speed);
    assert_eq!(change.direction, StatChangeDirection::Lower);
    assert_eq!(change.delta(), -1);
    assert!(
        !change.affects_user(),
        "the *target's* Speed falls, not the user's"
    );
}

/// `Random() % 100 < percentChance`: strictly less, so a chance of 30 fires
/// on residues 0..=29 and not on 30.
#[test]
fn the_chance_roll_is_strictly_less_than_and_always_draws_once() {
    for (chance, draw, expected) in [
        (30u8, 0u16, true),
        (30, 29, true),
        (30, 30, false),
        (30, 99, false),
        (30, 129, true), // 129 % 100 == 29
        (10, 9, true),
        (10, 10, false),
    ] {
        let mut rng = One(draw, 0);
        assert_eq!(
            secondary_chance_roll(chance, &mut rng),
            expected,
            "chance {chance}, draw {draw}"
        );
        assert_eq!(rng.1, 1, "exactly one draw, whatever the verdict");
    }
}

/// A `0` chance never fires -- but still draws, which is precisely the
/// discarded step-7 roll every plain `EFFECT_HIT` move already takes.
#[test]
fn a_zero_chance_still_costs_its_draw() {
    for draw in [0u16, 1, 99] {
        let mut rng = One(draw, 0);
        assert!(!secondary_chance_roll(0, &mut rng));
        assert_eq!(rng.1, 1);
    }
}
