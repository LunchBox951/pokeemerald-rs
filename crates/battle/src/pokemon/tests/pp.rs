//! Pins PP-Up bonus adoption: packed bonus bits, the raised max PP they
//! unlock, and healing to that raised maximum.

use super::super::PpBonuses;
use super::shared::sample_mon;
use crate::dex::Dex;
use crate::error::BattleError;

const THREE_PP_UPS_ON_FIRST_SLOT: PpBonuses = PpBonuses::from_bits(0b0000_0011);
const THREE_PP_UPS_ON_LAST_SLOT: PpBonuses = PpBonuses::from_bits(0b1100_0000);
const TACKLE_MAX_PP_WITH_THREE_UPS: u8 = 56;

#[test]
fn a_new_mon_carries_no_pp_ups() {
    let dex = Dex::new();
    let mon = sample_mon(&dex);
    assert_eq!(mon.pp_bonuses(), PpBonuses::NONE);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), mon.moves()[0].pp);
    assert_eq!(
        mon.max_pp(&dex, 1),
        Err(BattleError::InvalidMoveSlot(1)),
        "a slot this mon does not have has no maximum"
    );
}

#[test]
fn adopting_pp_bonuses_raises_and_fills_the_slot() {
    let dex = Dex::new();
    let base_pp = sample_mon(&dex).moves()[0].pp;
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();

    assert_eq!(mon.pp_bonuses().get(0), 3);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), TACKLE_MAX_PP_WITH_THREE_UPS);
    assert_eq!(mon.moves()[0].pp, TACKLE_MAX_PP_WITH_THREE_UPS);
    assert!(mon.moves()[0].pp > base_pp);
}

#[test]
fn pp_bonus_bits_for_unfilled_slots_are_carried_untouched() {
    let dex = Dex::new();
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_LAST_SLOT)
        .unwrap();
    assert_eq!(mon.pp_bonuses().bits(), THREE_PP_UPS_ON_LAST_SLOT.bits());
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), mon.moves()[0].pp);
}

#[test]
fn heal_restores_pp_to_the_pp_up_adjusted_maximum() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;

    for _ in 0..20 {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 36);

    mon.heal(&dex).unwrap();
    assert_eq!(mon.moves()[0].pp, TACKLE_MAX_PP_WITH_THREE_UPS);
    assert!(
        mon.moves()[0].pp > base_pp,
        "healing to base PP would silently strip the PP Ups"
    );
}

#[test]
fn an_upgraded_slot_spends_its_whole_upgraded_capacity() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();
    for _ in 0..TACKLE_MAX_PP_WITH_THREE_UPS {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 0);
    assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
}
