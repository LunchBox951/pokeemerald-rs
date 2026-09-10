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

#[test]
fn every_trampoline_row_matches_its_upstream_script() {
    // Each row is `(effect id, prepared MOVE_EFFECT_*, certain, affects_user)`,
    // independently re-derived from `pokeemerald/data/battle_scripts_1.s` and
    // `pokeemerald/include/constants/battle_move_effects.h`, not from
    // `SECONDARY_TRAMPOLINES` itself, so a corrupted row does not agree by
    // construction.
    let rows: Vec<(u8, &str, bool, bool)> = SECONDARY_TRAMPOLINES
        .iter()
        .map(|trampoline| {
            (
                trampoline.effect.id(),
                trampoline.move_effect,
                trampoline.certain,
                trampoline.affects_user,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (2, "MOVE_EFFECT_POISON", false, false), // battle_scripts_1.s:319-321
            (4, "MOVE_EFFECT_BURN", false, false),   // battle_scripts_1.s:362-364
            (5, "MOVE_EFFECT_FREEZE", false, false), // battle_scripts_1.s:366-368
            (6, "MOVE_EFFECT_PARALYSIS", false, false), // battle_scripts_1.s:370-372
            (31, "MOVE_EFFECT_FLINCH", false, false), // battle_scripts_1.s:668-670
            (34, "MOVE_EFFECT_PAYDAY", false, false), // battle_scripts_1.s:720-722
            (36, "MOVE_EFFECT_TRI_ATTACK", false, false), // battle_scripts_1.s:731-733
            // Whirlpool's setup falls into the unflagged Wrap handler.
            (42, "MOVE_EFFECT_WRAP", false, false), // battle_scripts_1.s:830-837
            (68, "MOVE_EFFECT_ATK_MINUS_1", false, false), // battle_scripts_1.s:1040-1042
            (69, "MOVE_EFFECT_DEF_MINUS_1", false, false), // battle_scripts_1.s:1044-1046
            (70, "MOVE_EFFECT_SPD_MINUS_1", false, false), // battle_scripts_1.s:1048-1050
            (71, "MOVE_EFFECT_SP_ATK_MINUS_1", false, false), // battle_scripts_1.s:1052-1054
            (72, "MOVE_EFFECT_SP_DEF_MINUS_1", false, false), // battle_scripts_1.s:1056-1058
            (73, "MOVE_EFFECT_ACC_MINUS_1", false, false), // battle_scripts_1.s:1060-1062
            (76, "MOVE_EFFECT_CONFUSION", false, false), // battle_scripts_1.s:1071-1073
            (105, "MOVE_EFFECT_STEAL_ITEM", false, false), // battle_scripts_1.s:1440-1442
            // Thaw Hit prepares Burn, not a Thaw-named effect.
            (125, "MOVE_EFFECT_BURN", false, false), // battle_scripts_1.s:1679-1681
            // Both MOVE_EFFECT_CERTAIN and MOVE_EFFECT_AFFECTS_USER are set.
            (129, "MOVE_EFFECT_RAPIDSPIN", true, true), // battle_scripts_1.s:1716-1718
            // AFFECTS_USER but chance-based (not CERTAIN).
            (138, "MOVE_EFFECT_DEF_PLUS_1", false, true), // battle_scripts_1.s:1764-1766
            (139, "MOVE_EFFECT_ATK_PLUS_1", false, true), // battle_scripts_1.s:1768-1770
            (140, "MOVE_EFFECT_ALL_STATS_UP", false, true), // battle_scripts_1.s:1772-1774
            // Twister dispatches (battle_scripts_1.s:167) into the unflagged
            // flinch handler.
            (146, "MOVE_EFFECT_FLINCH", false, false), // battle_scripts_1.s:1826-1832
            // Flinch/Minimize dispatches to Stomp (battle_scripts_1.s:171,
            // :1898-1901), which falls into the same unflagged flinch handler.
            (150, "MOVE_EFFECT_FLINCH", false, false), // battle_scripts_1.s:1830-1832
            // CERTAIN without AFFECTS_USER.
            (158, "MOVE_EFFECT_FLINCH", true, false), // battle_scripts_1.s:2048-2052
            // Both flags explicit.
            (182, "MOVE_EFFECT_ATK_DEF_DOWN", true, true), // battle_scripts_1.s:2388-2390
            (188, "MOVE_EFFECT_KNOCK_OFF", false, false),  // battle_scripts_1.s:2475-2477
            // Both flags explicit.
            (198, "MOVE_EFFECT_RECOIL_33", true, true), // battle_scripts_1.s:2567-2569
            // Blaze Kick dispatches (battle_scripts_1.s:221) into the
            // unflagged Burn handler.
            (200, "MOVE_EFFECT_BURN", false, false), // battle_scripts_1.s:362-364
            (202, "MOVE_EFFECT_TOXIC", false, false), // battle_scripts_1.s:2640-2642
            // Both flags explicit.
            (204, "MOVE_EFFECT_SP_ATK_TWO_DOWN", true, true), // battle_scripts_1.s:2648-2650
            // Poison Tail dispatches (battle_scripts_1.s:230) into the
            // unflagged Poison handler.
            (209, "MOVE_EFFECT_POISON", false, false), // battle_scripts_1.s:319-321
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
