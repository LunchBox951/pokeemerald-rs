use super::{
    is_secondary_effect, spend_effect_chance_draw, trampoline_for_effect, SECONDARY_TRAMPOLINES,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::script_rng::SequenceRng;
use assets::{MoveEffect, MoveId};

/// `MOVE_TACKLE` — a plain `EFFECT_HIT` move: `MOVE_EFFECT_BYTE` is `0`,
/// `secondaryEffectChance` is `0`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_POISON_STING` — `EFFECT_POISON_HIT`, `secondaryEffectChance` 30.
const POISON_STING: MoveId = MoveId(40);
/// `MOVE_THUNDER_SHOCK` — `EFFECT_PARALYZE_HIT`, `secondaryEffectChance` 10.
const THUNDER_SHOCK: MoveId = MoveId(84);

/// The table's `EFFECT_*` ids are unique and sorted, so a duplicated or
/// misplaced row is caught rather than shadowed by the linear search.
#[test]
fn the_trampoline_table_is_sorted_and_free_of_duplicates() {
    let ids: Vec<u8> = SECONDARY_TRAMPOLINES.iter().map(|t| t.effect.0).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        ids, sorted,
        "SECONDARY_TRAMPOLINES must be sorted and unique"
    );
    assert_eq!(SECONDARY_TRAMPOLINES.len(), 31);
}

/// The five `MOVE_EFFECT_CERTAIN` rows are exactly the ones whose upstream
/// `setmoveeffect` carries that flag — the rows that take
/// `Cmd_seteffectwithchance`'s **draw-free** first branch. Fake Out
/// (`:2051`) is the one *target-side* CERTAIN: its guaranteed flinch means
/// a landed Fake Out spends zero draws where every other foe-hitting row
/// spends one.
#[test]
fn the_certain_rows_are_the_five_upstream_marks_certain() {
    let certain: Vec<&str> = SECONDARY_TRAMPOLINES
        .iter()
        .filter(|t| t.certain)
        .map(|t| t.move_effect)
        .collect();
    assert_eq!(
        certain,
        [
            "MOVE_EFFECT_RAPIDSPIN",       // EFFECT_RAPID_SPIN
            "MOVE_EFFECT_FLINCH",          // EFFECT_FAKE_OUT (| CERTAIN)
            "MOVE_EFFECT_ATK_DEF_DOWN",    // EFFECT_SUPERPOWER
            "MOVE_EFFECT_RECOIL_33",       // EFFECT_DOUBLE_EDGE
            "MOVE_EFFECT_SP_ATK_TWO_DOWN", // EFFECT_OVERHEAT
        ]
    );
    // Fake Out aside, every CERTAIN row is AFFECTS_USER upstream.
    assert!(SECONDARY_TRAMPOLINES
        .iter()
        .filter(|t| t.effect != MoveEffect(158))
        .all(|t| !t.certain || t.affects_user));
}

/// A trampoline effect is recognised; a plain hit effect is not.
#[test]
fn membership_matches_the_effects_that_write_a_move_effect_byte() {
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
    // An effect id past the table's end is not a trampoline either.
    assert!(!is_secondary_effect(MoveEffect(255)));
}

/// The draw is the **leading** operand of the `else if`, so it happens for a
/// plain move whose byte is `0` and for a type-immune hit alike — and its
/// value never changes the answer.
#[test]
fn a_plain_move_always_spends_exactly_one_discarded_draw() {
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

/// The fail-closed stub: a roll that lands on a trampoline byte refuses
/// rather than silently dropping the effect — **after** spending the draw
/// upstream also spends, so a caller that recovers still has a correctly
/// advanced stream.
#[test]
fn a_landed_roll_on_an_unported_byte_refuses_after_spending_the_draw() {
    let dex = Dex::new();
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().secondary_effect_chance,
        30
    );

    // `Random() % 100 < 30`: 29 lands, 30 does not.
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(POISON_STING))
    );
    assert_eq!(rng.draws(), 1, "the draw is spent before the refusal");

    let mut rng = SequenceRng::new([30]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut rng),
        Ok(()),
        "a roll that did not come under the chance changes nothing"
    );
    assert_eq!(rng.draws(), 1);

    // The third operand: a type-immune hit suppresses the effect even on a
    // landing roll -- and still spends the draw.
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, false, &mut rng),
        Ok(())
    );
    assert_eq!(rng.draws(), 1);
}

/// `Random() % 100` wraps a `u16`, so the modulus — not the raw value — is
/// what the chance is compared against.
#[test]
fn the_roll_is_the_value_modulo_one_hundred() {
    let dex = Dex::new();
    // 129 % 100 = 29 -> under 30 -> lands.
    let mut rng = SequenceRng::new([129]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(POISON_STING))
    );
    // 130 % 100 = 30 -> not under 30 -> passes.
    let mut rng = SequenceRng::new([130]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &mut rng),
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

/// `Cmd_seteffectwithchance`'s first branch (`battle_script_commands.c:2917`):
/// a CERTAIN byte on a hit that landed takes the draw-free path — delete the
/// early return at `spend_effect_chance_draw`'s branch 1 and this test's
/// empty `SequenceRng` panics. The complementary asymmetry is pinned too: a
/// CERTAIN byte on a `NO_EFFECT` hit falls through to the `else if`, whose
/// leading `Random() % 100` is spent even though nothing can be inflicted.
#[test]
fn a_certain_byte_on_a_landed_hit_spends_no_draw_but_a_no_effect_hit_spends_one() {
    const FAKE_OUT: MoveId = MoveId(252);
    let dex = Dex::new();

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, true, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(FAKE_OUT)),
        "the byte itself is still unported -- fail closed, draw-free"
    );
    assert_eq!(rng.draws(), 0);

    let mut rng = SequenceRng::new([9999]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, false, &mut rng),
        Ok(()),
        "NO_EFFECT discards the byte, but only after the leading roll"
    );
    assert_eq!(rng.draws(), 1);
}
