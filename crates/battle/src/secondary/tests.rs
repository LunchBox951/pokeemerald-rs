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
