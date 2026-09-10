use super::{
    ensure_admissible, is_poison_hit_effect, is_secondary_effect, spend_effect_chance_draw,
    trampoline_for_effect, EFFECT_DOUBLE_EDGE, EFFECT_FAKE_OUT, EFFECT_OVERHEAT, EFFECT_POISON_HIT,
    EFFECT_POISON_TAIL, EFFECT_RAPID_SPIN, EFFECT_SUPERPOWER, SECONDARY_TRAMPOLINES,
};
use crate::damage::STRUGGLE;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::status1::Status1;
use assets::{AbilityId, MoveEffect, MoveId, SpeciesId};

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

const TACKLE: MoveId = MoveId(33);
const POISON_STING: MoveId = MoveId(40);
const POISON_TAIL: MoveId = MoveId(342);
const THUNDER_SHOCK: MoveId = MoveId(84);
const FAKE_OUT: MoveId = MoveId(252);
const UNKNOWN_EFFECT: MoveEffect = MoveEffect(u8::MAX);

/// `SPECIES_ZIGZAGOON`, Normal-type: eligible for poison, no interfering
/// ability.
const ZIGZAGOON: u16 = 288;
/// `SPECIES_EKANS`: mono Poison-type, immune to poison outright.
const EKANS: u16 = 23;
/// `SPECIES_MAGNEMITE`: Electric/Steel, immune to poison outright.
const MAGNEMITE: u16 = 81;
/// `SPECIES_ZANGOOSE`: Immunity in its only ability slot.
const ZANGOOSE: u16 = 380;
/// `SPECIES_WURMPLE`: Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
/// `SPECIES_DUNSPARCE`: Serene Grace in its primary ability slot.
const DUNSPARCE: u16 = 206;
/// `SPECIES_RALTS`: Synchronize in its primary ability slot.
const RALTS: u16 = 392;
/// `SPECIES_DRATINI`: Dragon-type, Shed Skin in its only ability slot --
/// unlike `SPECIES_SEVIPER`, not itself Poison-typed, so this fixture
/// actually exercises the ability guard rather than the type guard.
const DRATINI: u16 = 147;
/// `SPECIES_MACHOP`: Guts in its primary ability slot.
const MACHOP: u16 = 66;
/// `SPECIES_MILOTIC`: Marvel Scale in its primary ability slot.
const MILOTIC: u16 = 329;

const PRIMARY_ABILITY_PERSONALITY: u32 = 0;

fn mon(dex: &Dex, species: u16) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SpeciesId(species),
        10,
        MAX_IVS,
        PRIMARY_ABILITY_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
}

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
fn only_effect_poison_hit_is_the_modelled_poison_trampoline() {
    let dex = Dex::new();
    assert!(is_poison_hit_effect(EFFECT_POISON_HIT));
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().effect,
        EFFECT_POISON_HIT
    );

    // Poison Tail prepares the identical `MOVE_EFFECT_POISON` symbolic effect
    // but is a distinct move-effect byte, and stays unported (issue #784's
    // boundary).
    assert!(!is_poison_hit_effect(EFFECT_POISON_TAIL));
    assert_eq!(
        dex.move_data(POISON_TAIL).unwrap().effect,
        EFFECT_POISON_TAIL
    );
    assert_eq!(
        trampoline_for_effect(EFFECT_POISON_TAIL)
            .unwrap()
            .move_effect,
        "MOVE_EFFECT_POISON",
        "fixture sanity: Poison Tail really does share the symbolic effect name"
    );
}

#[test]
fn a_plain_move_discards_one_draw_even_when_the_hit_has_no_effect() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    assert_eq!(dex.move_data(TACKLE).unwrap().secondary_effect_chance, 0);
    for had_effect in [true, false] {
        for value in [0u16, 29, 30, 99, u16::MAX] {
            let mut rng = SequenceRng::new([value]);
            assert_eq!(
                spend_effect_chance_draw(&dex, TACKLE, had_effect, &defender, &mut rng),
                Ok(false)
            );
            assert_eq!(rng.draws(), 1, "value {value}, had_effect {had_effect}");
        }
    }
}

#[test]
fn a_successful_unported_effect_chance_fails_closed_after_drawing() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    assert_eq!(
        dex.move_data(THUNDER_SHOCK)
            .unwrap()
            .secondary_effect_chance,
        10
    );

    let mut successful_roll_rng = SequenceRng::new([9]);
    assert_eq!(
        spend_effect_chance_draw(
            &dex,
            THUNDER_SHOCK,
            true,
            &defender,
            &mut successful_roll_rng
        ),
        Err(BattleError::UnportedSecondaryEffect(THUNDER_SHOCK))
    );
    assert_eq!(successful_roll_rng.draws(), 1);

    let mut failed_roll_rng = SequenceRng::new([10]);
    assert_eq!(
        spend_effect_chance_draw(&dex, THUNDER_SHOCK, true, &defender, &mut failed_roll_rng),
        Ok(false)
    );
    assert_eq!(failed_roll_rng.draws(), 1);

    let mut ineffective_hit_rng = SequenceRng::new([9]);
    assert_eq!(
        spend_effect_chance_draw(
            &dex,
            THUNDER_SHOCK,
            false,
            &defender,
            &mut ineffective_hit_rng
        ),
        Ok(false)
    );
    assert_eq!(ineffective_hit_rng.draws(), 1);
}

#[test]
fn poison_tail_stays_unported_like_every_other_trampoline() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    assert_eq!(
        dex.move_data(POISON_TAIL).unwrap().secondary_effect_chance,
        10
    );
    let mut rng = SequenceRng::new([9]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_TAIL, true, &defender, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(POISON_TAIL)),
        "Poison Tail prepares MOVE_EFFECT_POISON too, but only EFFECT_POISON_HIT resolves"
    );
}

#[test]
fn a_successful_struggle_fails_closed_without_drawing() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, STRUGGLE, true, &defender, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(STRUGGLE))
    );
    assert_eq!(rng.draws(), 0);

    let mut ineffective_hit_rng = SequenceRng::new([0]);
    assert_eq!(
        spend_effect_chance_draw(&dex, STRUGGLE, false, &defender, &mut ineffective_hit_rng),
        Ok(false)
    );
    assert_eq!(ineffective_hit_rng.draws(), 1);
}

#[test]
fn effect_chance_uses_the_draw_modulo_one_hundred() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    // Thunder Shock's chance is 10 (module fixture, pinned above): 109 % 100
    // == 9 succeeds, 110 % 100 == 10 does not.
    let mut successful_wrapped_roll_rng = SequenceRng::new([109]);
    assert_eq!(
        spend_effect_chance_draw(
            &dex,
            THUNDER_SHOCK,
            true,
            &defender,
            &mut successful_wrapped_roll_rng,
        ),
        Err(BattleError::UnportedSecondaryEffect(THUNDER_SHOCK))
    );
    let mut failed_wrapped_roll_rng = SequenceRng::new([110]);
    assert_eq!(
        spend_effect_chance_draw(
            &dex,
            THUNDER_SHOCK,
            true,
            &defender,
            &mut failed_wrapped_roll_rng,
        ),
        Ok(false)
    );
}

#[test]
fn an_unknown_move_is_rejected_without_drawing() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    let unknown = MoveId(60_000);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, unknown, true, &defender, &mut rng),
        Err(BattleError::UnknownMove(unknown))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn a_certain_effect_draws_only_when_the_hit_has_no_effect() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, true, &defender, &mut rng),
        Err(BattleError::UnportedSecondaryEffect(FAKE_OUT))
    );
    assert_eq!(rng.draws(), 0);

    let mut rng = SequenceRng::new([9999]);
    assert_eq!(
        spend_effect_chance_draw(&dex, FAKE_OUT, false, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn poison_sting_applies_poison_on_a_successful_roll_against_an_eligible_target() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    assert_eq!(
        dex.move_data(POISON_STING).unwrap().secondary_effect_chance,
        30
    );

    let mut succeeds = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut succeeds),
        Ok(true)
    );
    assert_eq!(succeeds.draws(), 1, "exactly one chance draw");

    let mut fails = SequenceRng::new([30]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut fails),
        Ok(false)
    );
    assert_eq!(fails.draws(), 1);
}

#[test]
fn an_ineffective_hit_never_poisons_regardless_of_the_roll() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, false, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(
        rng.draws(),
        1,
        "the draw still happens; only its result is discarded"
    );
}

#[test]
fn a_poison_type_defender_is_immune_but_still_consumes_the_draw() {
    let dex = Dex::new();
    let defender = mon(&dex, EKANS);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_steel_type_defender_is_immune_but_still_consumes_the_draw() {
    let dex = Dex::new();
    let defender = mon(&dex, MAGNEMITE);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn an_already_statused_defender_refuses_a_second_status_but_still_draws() {
    let dex = Dex::new();
    let mut defender = mon(&dex, ZIGZAGOON);
    defender.set_status1(Status1::Paralysed);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn an_immunity_ability_defender_blocks_poison_silently_but_still_draws() {
    let dex = Dex::new();
    let defender = mon(&dex, ZANGOOSE);
    assert_eq!(defender.ability(), AbilityId::IMMUNITY);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_shield_dust_defender_blocks_poison_silently_but_still_draws() {
    let dex = Dex::new();
    let defender = mon(&dex, WURMPLE);
    assert_eq!(defender.ability(), AbilityId::SHIELD_DUST);
    let mut rng = SequenceRng::new([29]);
    assert_eq!(
        spend_effect_chance_draw(&dex, POISON_STING, true, &defender, &mut rng),
        Ok(false)
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_serene_grace_attacker_is_refused_before_any_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, DUNSPARCE);
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    let defender = mon(&dex, ZIGZAGOON);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &defender),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        ))
    );
}

#[test]
fn a_serene_grace_attacker_is_admitted_once_poison_could_never_land_anyway() {
    let dex = Dex::new();
    let attacker = mon(&dex, DUNSPARCE);
    let immune_defender = mon(&dex, EKANS);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &immune_defender),
        Ok(()),
        "a Poison-type target can never be poisoned, so Serene Grace's doubled \
         threshold changes nothing observable"
    );
}

#[test]
fn a_synchronize_defender_is_refused_unless_the_attacker_is_already_statused() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let defender = mon(&dex, RALTS);
    assert_eq!(defender.ability(), AbilityId::SYNCHRONIZE);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &defender),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SYNCHRONIZE
        ))
    );

    let mut statused_attacker = mon(&dex, ZIGZAGOON);
    statused_attacker.set_status1(Status1::Poisoned);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &statused_attacker, &defender),
        Ok(()),
        "an attacker that already carries a primary status leaves the reflection \
         nothing to write"
    );
}

#[test]
fn a_shed_skin_defender_is_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let defender = mon(&dex, DRATINI);
    assert_eq!(defender.ability(), AbilityId::SHED_SKIN);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &defender),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SHED_SKIN
        ))
    );
}

#[test]
fn a_guts_defender_is_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let defender = mon(&dex, MACHOP);
    assert_eq!(defender.ability(), AbilityId::GUTS);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &defender),
        Err(BattleError::UnportedAbilityInteraction(AbilityId::GUTS))
    );
}

#[test]
fn a_marvel_scale_defender_is_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let defender = mon(&dex, MILOTIC);
    assert_eq!(defender.ability(), AbilityId::MARVEL_SCALE);
    assert_eq!(
        ensure_admissible(&dex, POISON_STING, &attacker, &defender),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::MARVEL_SCALE
        ))
    );
}

#[test]
fn ensure_admissible_is_a_no_op_for_every_non_poison_hit_move() {
    let dex = Dex::new();
    let attacker = mon(&dex, DUNSPARCE);
    let defender = mon(&dex, RALTS);
    assert_eq!(
        ensure_admissible(&dex, TACKLE, &attacker, &defender),
        Ok(())
    );
    assert_eq!(
        ensure_admissible(&dex, POISON_TAIL, &attacker, &defender),
        Ok(()),
        "Poison Tail is not EFFECT_POISON_HIT, so it never reaches these guards"
    );
}

#[test]
fn ensure_admissible_propagates_an_unknown_move() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let defender = mon(&dex, ZIGZAGOON);
    let unknown = MoveId(60_000);
    assert_eq!(
        ensure_admissible(&dex, unknown, &attacker, &defender),
        Err(BattleError::UnknownMove(unknown))
    );
}
