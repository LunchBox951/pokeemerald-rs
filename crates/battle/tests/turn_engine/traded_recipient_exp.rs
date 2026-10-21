//! `Cmd_getexp` boosts a traded recipient's award by one more truncating 150%,
//! applied after the trainer multiplier
//! (`src/battle_script_commands.c:3378`-`:3390`). Treecko's expYield is 65, so
//! a level-5 knockout awards 65*5/7 = 46 in the wild and 46*150/100 = 69 from
//! a trainer; a traded recipient receives 69 and 103 respectively.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::MoveId;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

const TREECKO: u16 = 277;
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const DOMESTIC_WILD_AWARD: u32 = 46;
const TRADED_WILD_AWARD: u32 = 69;
const DOMESTIC_TRAINER_AWARD: u32 = 69;
const TRADED_TRAINER_AWARD: u32 = 103;

/// A level-5 Treecko one hit from fainting.
fn nearly_fainted_treecko(dex: &Dex) -> BattlePokemon {
    let mut mon = max_iv_mon(dex, TREECKO, 5, vec![MoveId::POUND, MoveId::LEER]);
    mon.apply_damage(mon.current_hp() - 1);
    mon
}

fn wild_award(level: u8, traded: bool) -> (u32, u8) {
    let dex = Dex::new();
    let player =
        max_iv_mon(&dex, TREECKO, level, vec![MoveId::SLASH]).with_traded_recipient(traded);
    let enemy = nearly_fainted_treecko(&dex);
    let mut rng = SequenceRng::new([0; 64]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (exp_of(&events), battle.player().level())
}

fn trainer_award(level: u8, traded: bool) -> (u32, u8) {
    let dex = Dex::new();
    let player =
        max_iv_mon(&dex, TREECKO, level, vec![MoveId::SLASH]).with_traded_recipient(traded);
    let party = vec![nearly_fainted_treecko(&dex)];
    let mut rng = SequenceRng::new([0; 64]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (exp_of(&events), battle.player().level())
}

fn exp_of(events: &[BattleEvent]) -> u32 {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::ExpGained(exp) => Some(*exp),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the knockout awards experience: {events:?}"))
}

#[test]
fn a_domestic_wild_recipient_earns_the_plain_award() {
    assert_eq!(wild_award(50, false).0, DOMESTIC_WILD_AWARD);
}

#[test]
fn a_traded_wild_recipient_earns_the_boosted_award() {
    assert_eq!(wild_award(50, true).0, TRADED_WILD_AWARD);
}

#[test]
fn a_domestic_trainer_recipient_earns_the_trainer_award() {
    assert_eq!(trainer_award(50, false).0, DOMESTIC_TRAINER_AWARD);
}

/// The 150% follows the trainer 150%: 46 -> 69 -> 103, not 46*150/100 once.
#[test]
fn a_traded_trainer_recipient_stacks_the_bonus_after_the_trainer_multiplier() {
    assert_eq!(trainer_award(50, true).0, TRADED_TRAINER_AWARD);
}

/// A level-6 Treecko needs 57 experience for level 7: the plain 46 falls
/// short, the traded 69 crosses.
#[test]
fn the_wild_bonus_can_cross_a_level_the_plain_award_does_not() {
    assert_eq!(wild_award(6, false), (DOMESTIC_WILD_AWARD, 6));
    assert_eq!(wild_award(6, true), (TRADED_WILD_AWARD, 7));
}

/// A level-7 Treecko needs 78 experience for level 8: the trainer's 69 falls
/// short, the traded 103 crosses.
#[test]
fn the_trainer_bonus_can_cross_a_level_the_plain_award_does_not() {
    assert_eq!(trainer_award(7, false), (DOMESTIC_TRAINER_AWARD, 7));
    assert_eq!(trainer_award(7, true), (TRADED_TRAINER_AWARD, 8));
}
