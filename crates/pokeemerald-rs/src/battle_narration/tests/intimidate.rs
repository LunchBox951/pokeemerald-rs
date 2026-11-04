//! Intimidate's entry lines: the holder is the scripting battler, the
//! opposing battler the target, and the prevented line names the protector.

use assets::AbilityId;
use battle::{BattleEvent, StatStage};

use super::fixtures::{battler, narrator, render, Fixture, RATTATA};
use crate::battle_narration::{expand, AssetNames, BattleKind, NarrationRequest, Narrator, Side};

/// `STRINGID_PKMNCUTSATTACKWITH` and `STRINGID_PREVENTEDFROMWORKING`
/// (`src/battle_message.c:276`, `:294`).
const SLOTS: &[(u16, &str)] = &[
    (
        205,
        "{B_SCR_ACTIVE_NAME_WITH_PREFIX}'s {B_SCR_ACTIVE_ABILITY}\ncuts {B_DEF_NAME_WITH_PREFIX}'s ATTACK!",
    ),
    (
        341,
        "{B_DEF_NAME_WITH_PREFIX}'s {B_DEF_ABILITY}\nprevented {B_SCR_ACTIVE_NAME_WITH_PREFIX}'s\\l{B_BUFF1} from working!",
    ),
];

fn lines(requests: &[NarrationRequest]) -> Vec<String> {
    let fixture = Fixture::new(SLOTS, &[]);
    requests
        .iter()
        .map(|request| render(&expand(&fixture.table(), &AssetNames, request).unwrap()))
        .map(|line| line.replace('’', "'"))
        .collect()
}

fn narrate(narrator: &mut Narrator, event: BattleEvent) -> Vec<NarrationRequest> {
    narrator.requests(&AssetNames, &event).unwrap()
}

fn dropped(by_player: bool) -> BattleEvent {
    BattleEvent::Intimidated {
        by_player,
        new_stage: StatStage::new(-1).unwrap(),
    }
}

fn prevented(by_player: bool, ability: AbilityId) -> BattleEvent {
    BattleEvent::IntimidatePrevented { by_player, ability }
}

#[test]
fn the_holders_ability_cuts_the_targets_attack_with_each_sides_prefix() {
    let mut wild = narrator(BattleKind::Wild);
    assert_eq!(
        lines(&narrate(&mut wild, dropped(true))),
        ["TORCHIC's INTIMIDATE\ncuts Wild RATTATA's ATTACK!"]
    );
    assert_eq!(
        lines(&narrate(&mut wild, dropped(false))),
        ["Wild RATTATA's INTIMIDATE\ncuts TORCHIC's ATTACK!"]
    );

    let mut trainer = narrator(BattleKind::Trainer);
    assert_eq!(
        lines(&narrate(&mut trainer, dropped(false))),
        ["Foe RATTATA's INTIMIDATE\ncuts TORCHIC's ATTACK!"]
    );
}

#[test]
fn a_prevented_drop_names_the_protector_and_the_holders_intimidate() {
    for (ability, name) in [
        (AbilityId::CLEAR_BODY, "CLEAR BODY"),
        (AbilityId::WHITE_SMOKE, "WHITE SMOKE"),
        (AbilityId::HYPER_CUTTER, "HYPER CUTTER"),
    ] {
        let mut wild = narrator(BattleKind::Wild);
        assert_eq!(
            lines(&narrate(&mut wild, prevented(true, ability))),
            [format!(
                "Wild RATTATA's {name}\nprevented TORCHIC's{{SCROLL}}INTIMIDATE from working!"
            )]
        );
        assert_eq!(
            lines(&narrate(&mut wild, prevented(false, ability))),
            [format!(
                "TORCHIC's {name}\nprevented Wild RATTATA's{{SCROLL}}INTIMIDATE from working!"
            )]
        );
    }
}

#[test]
fn an_entering_holder_is_named_by_its_nickname_and_ability() {
    let mut narrator = narrator(BattleKind::Trainer);
    narrator.stage_replacement(Side::Opponent, battler("FANG", Some(AbilityId::INTIMIDATE)));
    let sent_out = BattleEvent::TrainerSentOut {
        species: RATTATA,
        bench_remaining: 0,
    };
    narrator.requests(&AssetNames, &sent_out).unwrap();

    assert_eq!(
        lines(&narrate(&mut narrator, dropped(false))),
        ["Foe FANG's INTIMIDATE\ncuts TORCHIC's ATTACK!"]
    );
}
