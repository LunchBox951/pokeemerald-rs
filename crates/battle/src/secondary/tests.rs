use super::{
    is_secondary_effect, spend_effect_chance_draw, trampoline_for_effect, EFFECT_DOUBLE_EDGE,
    EFFECT_FAKE_OUT, EFFECT_OVERHEAT, EFFECT_RAPID_SPIN, EFFECT_SUPERPOWER, SECONDARY_TRAMPOLINES,
};
use crate::damage::STRUGGLE;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::script_rng::SequenceRng;
use assets::{MoveEffect, MoveId};

const TACKLE: MoveId = MoveId(33);
const POISON_STING: MoveId = MoveId(40);
const THUNDER_SHOCK: MoveId = MoveId(84);
const FAKE_OUT: MoveId = MoveId(252);
const UNKNOWN_EFFECT: MoveEffect = MoveEffect(u8::MAX);

#[test]
fn trampoline_effects_are_sorted_and_free_of_duplicates() {
    let ids: Vec<u8> = SECONDARY_TRAMPOLINES
        .iter()
        .map(|trampoline| trampoline.effect.id())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        ids, sorted,
        "SECONDARY_TRAMPOLINES must be sorted and unique"
    );
    assert_eq!(SECONDARY_TRAMPOLINES.len(), 31);
}

#[test]
fn certain_trampolines_have_the_expected_effects_and_target_sides() {
    let certain: Vec<(MoveEffect, &str, bool)> = SECONDARY_TRAMPOLINES
        .iter()
        .filter(|trampoline| trampoline.certain)
        .map(|trampoline| {
            (
                trampoline.effect,
                trampoline.move_effect,
                trampoline.affects_user,
            )
        })
        .collect();
    assert_eq!(
        certain,
        [
            (EFFECT_RAPID_SPIN, "MOVE_EFFECT_RAPIDSPIN", true),
            (EFFECT_FAKE_OUT, "MOVE_EFFECT_FLINCH", false),
            (EFFECT_SUPERPOWER, "MOVE_EFFECT_ATK_DEF_DOWN", true),
            (EFFECT_DOUBLE_EDGE, "MOVE_EFFECT_RECOIL_33", true),
            (EFFECT_OVERHEAT, "MOVE_EFFECT_SP_ATK_TWO_DOWN", true),
        ]
    );
}

/// One expected trampoline row, re-derived from
/// `pokeemerald/data/battle_scripts_1.s` and
/// `pokeemerald/include/constants/battle_move_effects.h` rather than read back
/// from `SECONDARY_TRAMPOLINES`, so a corrupted row cannot agree by
/// construction.
#[derive(Debug, PartialEq, Eq)]
struct ExpectedRow {
    effect_id: u8,
    move_effect: &'static str,
    certain: bool,
    affects_user: bool,
}

impl ExpectedRow {
    const fn chance_on_target(effect_id: u8, move_effect: &'static str) -> Self {
        Self {
            effect_id,
            move_effect,
            certain: false,
            affects_user: false,
        }
    }

    const fn chance_on_user(effect_id: u8, move_effect: &'static str) -> Self {
        Self {
            effect_id,
            move_effect,
            certain: false,
            affects_user: true,
        }
    }

    const fn certain_on_target(effect_id: u8, move_effect: &'static str) -> Self {
        Self {
            effect_id,
            move_effect,
            certain: true,
            affects_user: false,
        }
    }

    const fn certain_on_user(effect_id: u8, move_effect: &'static str) -> Self {
        Self {
            effect_id,
            move_effect,
            certain: true,
            affects_user: true,
        }
    }
}

#[test]
fn every_trampoline_row_matches_its_upstream_script() {
    let rows: Vec<ExpectedRow> = SECONDARY_TRAMPOLINES
        .iter()
        .map(|trampoline| ExpectedRow {
            effect_id: trampoline.effect.id(),
            move_effect: trampoline.move_effect,
            certain: trampoline.certain,
            affects_user: trampoline.affects_user,
        })
        .collect();
    assert_eq!(
        rows,
        [
            ExpectedRow::chance_on_target(2, "MOVE_EFFECT_POISON"), // battle_scripts_1.s:319-321
            ExpectedRow::chance_on_target(4, "MOVE_EFFECT_BURN"),   // battle_scripts_1.s:362-364
            ExpectedRow::chance_on_target(5, "MOVE_EFFECT_FREEZE"), // battle_scripts_1.s:366-368
            ExpectedRow::chance_on_target(6, "MOVE_EFFECT_PARALYSIS"), // battle_scripts_1.s:370-372
            ExpectedRow::chance_on_target(31, "MOVE_EFFECT_FLINCH"), // battle_scripts_1.s:668-670
            ExpectedRow::chance_on_target(34, "MOVE_EFFECT_PAYDAY"), // battle_scripts_1.s:720-722
            ExpectedRow::chance_on_target(36, "MOVE_EFFECT_TRI_ATTACK"), // battle_scripts_1.s:731-733
            // Whirlpool's setup falls into the unflagged Wrap handler.
            ExpectedRow::chance_on_target(42, "MOVE_EFFECT_WRAP"), // battle_scripts_1.s:830-837
            ExpectedRow::chance_on_target(68, "MOVE_EFFECT_ATK_MINUS_1"), // battle_scripts_1.s:1040-1042
            ExpectedRow::chance_on_target(69, "MOVE_EFFECT_DEF_MINUS_1"), // battle_scripts_1.s:1044-1046
            ExpectedRow::chance_on_target(70, "MOVE_EFFECT_SPD_MINUS_1"), // battle_scripts_1.s:1048-1050
            ExpectedRow::chance_on_target(71, "MOVE_EFFECT_SP_ATK_MINUS_1"), // battle_scripts_1.s:1052-1054
            ExpectedRow::chance_on_target(72, "MOVE_EFFECT_SP_DEF_MINUS_1"), // battle_scripts_1.s:1056-1058
            ExpectedRow::chance_on_target(73, "MOVE_EFFECT_ACC_MINUS_1"), // battle_scripts_1.s:1060-1062
            ExpectedRow::chance_on_target(76, "MOVE_EFFECT_CONFUSION"), // battle_scripts_1.s:1071-1073
            ExpectedRow::chance_on_target(105, "MOVE_EFFECT_STEAL_ITEM"), // battle_scripts_1.s:1440-1442
            // Thaw Hit prepares Burn, not a Thaw-named effect.
            ExpectedRow::chance_on_target(125, "MOVE_EFFECT_BURN"), // battle_scripts_1.s:1679-1681
            ExpectedRow::certain_on_user(129, "MOVE_EFFECT_RAPIDSPIN"), // battle_scripts_1.s:1716-1718
            ExpectedRow::chance_on_user(138, "MOVE_EFFECT_DEF_PLUS_1"), // battle_scripts_1.s:1764-1766
            ExpectedRow::chance_on_user(139, "MOVE_EFFECT_ATK_PLUS_1"), // battle_scripts_1.s:1768-1770
            ExpectedRow::chance_on_user(140, "MOVE_EFFECT_ALL_STATS_UP"), // battle_scripts_1.s:1772-1774
            // Twister dispatches (battle_scripts_1.s:167) into the unflagged
            // flinch handler.
            ExpectedRow::chance_on_target(146, "MOVE_EFFECT_FLINCH"), // battle_scripts_1.s:1826-1832
            // Flinch/Minimize dispatches to Stomp (battle_scripts_1.s:171,
            // :1898-1901), which falls into the same unflagged flinch handler.
            ExpectedRow::chance_on_target(150, "MOVE_EFFECT_FLINCH"), // battle_scripts_1.s:1830-1832
            ExpectedRow::certain_on_target(158, "MOVE_EFFECT_FLINCH"), // battle_scripts_1.s:2048-2052
            ExpectedRow::certain_on_user(182, "MOVE_EFFECT_ATK_DEF_DOWN"), // battle_scripts_1.s:2388-2390
            ExpectedRow::chance_on_target(188, "MOVE_EFFECT_KNOCK_OFF"), // battle_scripts_1.s:2475-2477
            ExpectedRow::certain_on_user(198, "MOVE_EFFECT_RECOIL_33"), // battle_scripts_1.s:2567-2569
            // Blaze Kick dispatches (battle_scripts_1.s:221) into the
            // unflagged Burn handler.
            ExpectedRow::chance_on_target(200, "MOVE_EFFECT_BURN"), // battle_scripts_1.s:362-364
            ExpectedRow::chance_on_target(202, "MOVE_EFFECT_TOXIC"), // battle_scripts_1.s:2640-2642
            ExpectedRow::certain_on_user(204, "MOVE_EFFECT_SP_ATK_TWO_DOWN"), // battle_scripts_1.s:2648-2650
            // Poison Tail dispatches (battle_scripts_1.s:230) into the
            // unflagged Poison handler.
            ExpectedRow::chance_on_target(209, "MOVE_EFFECT_POISON"), // battle_scripts_1.s:319-321
        ]
    );
}

#[test]
fn lookup_returns_metadata_for_table_members() {
    let dex = Dex::new();
    for (move_id, name) in [
        (POISON_STING, "MOVE_EFFECT_POISON"),
        (THUNDER_SHOCK, "MOVE_EFFECT_PARALYSIS"),
    ] {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(is_secondary_effect(effect), "move {}", move_id.0);
        assert_eq!(trampoline_for_effect(effect).unwrap().move_effect, name);
    }
    let plain = dex.move_data(TACKLE).unwrap().effect;
    assert!(!is_secondary_effect(plain));
    assert_eq!(trampoline_for_effect(plain), None);
    assert!(!is_secondary_effect(UNKNOWN_EFFECT));
}

#[test]
fn a_plain_move_discards_one_draw_even_when_the_hit_has_no_effect() {
    let dex = Dex::new();
    assert_eq!(dex.move_data(TACKLE).unwrap().secondary_effect_chance, 0);
    for had_effect in [true, false] {
        for value in [0u16, 29, 30, 99, u16::MAX] {
            let mut rng = SequenceRng::new([value]);
            assert_eq!(
                spend_effect_chance_draw(&dex, TACKLE, had_effect, &mut rng),
                Ok(())
            );
            assert_eq!(rng.draws(), 1, "value {value}, had_effect {had_effect}");
        }
    }
}

#[test]
fn a_successful_effect_chance_fails_closed_after_drawing() {
    let dex = Dex::new();
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().secondary_effect_chance,
        30
    );

    let mut successful_roll_rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut successful_roll_rng),
        Err(BattleError::UnportedSecondaryEffect(POISON_STING))
    );
    assert_eq!(successful_roll_rng.draws(), 1);

    let mut failed_roll_rng = SequenceRng::new([30]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut failed_roll_rng),
        Ok(())
    );
    assert_eq!(failed_roll_rng.draws(), 1);

    let mut ineffective_hit_rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, false, &mut ineffective_hit_rng),
        Ok(())
    );
    assert_eq!(ineffective_hit_rng.draws(), 1);
}

#[test]
fn a_successful_struggle_fails_closed_without_drawing() {
    let dex = Dex::new();
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, STRUGGLE, true, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(STRUGGLE))
    );
    assert_eq!(rng.draws(), 0);

    let mut ineffective_hit_rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, STRUGGLE, false, &mut ineffective_hit_rng),
        Ok(())
    );
    assert_eq!(ineffective_hit_rng.draws(), 1);
}

#[test]
fn effect_chance_uses_the_draw_modulo_one_hundred() {
    let dex = Dex::new();
    let mut successful_wrapped_roll_rng = SequenceRng::new([129]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut successful_wrapped_roll_rng,),
        Err(BattleError::UnportedSecondaryEffect(POISON_STING))
    );
    let mut failed_wrapped_roll_rng = SequenceRng::new([130]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut failed_wrapped_roll_rng,),
        Ok(())
    );
}

#[test]
fn an_unknown_move_is_rejected_without_drawing() {
    let dex = Dex::new();
    let unknown = MoveId(60_000);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, unknown, true, &mut rng),
        Err(BattleError::UnknownMove(unknown))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn a_certain_effect_draws_only_when_the_hit_has_no_effect() {
    let dex = Dex::new();

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, true, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(FAKE_OUT))
    );
    assert_eq!(rng.draws(), 0);

    let mut rng = SequenceRng::new([9999]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, false, &mut rng),
        Ok(())
    );
    assert_eq!(rng.draws(), 1);
}
