//! [`crate::stat_change`]'s own unit tests: the transcribed thunk table, the
//! two tails' differing draw counts, the cap/floor outcomes, and Clear
//! Body's refusal (issue #322).

use super::{
    ensure_resolvable, is_stat_change_effect, resolve_stat_change_move, stat_change_for_effect,
    ChangedStat, StatChangeDirection, StatChangeOutcome, CLEAR_BODY, EFFECT_ATTACK_DOWN,
    EFFECT_DEFENSE_DOWN, STAT_CHANGE_EFFECTS, WHITE_SMOKE,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::stat_stage::StatStage;
use assets::{MoveEffect, MoveId, MoveTarget, SpeciesId};

/// A `BattleRng` fed from a fixed sequence, for pinning exact draw
/// order/count.
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

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
}

const GROWL: MoveId = MoveId(45);
const LEER: MoveId = MoveId(43);
const TAIL_WHIP: MoveId = MoveId(39);
const STRING_SHOT: MoveId = MoveId(81);
const SAND_ATTACK: MoveId = MoveId(28);
const SCREECH: MoveId = MoveId(103);
const GROWTH: MoveId = MoveId(74);
const HARDEN: MoveId = MoveId(106);
const TACKLE: MoveId = MoveId(33);

/// `SPECIES_TENTACOOL` (`72`): the one species this crate's dex carries
/// whose slot-0 ability is Clear Body (`gSpeciesInfo`,
/// [`crate::pokemon::BattlePokemon::ability`]'s tests).
const TENTACOOL: u16 = 72;

#[test]
fn the_table_covers_the_named_effects_and_nothing_outside_it() {
    assert!(is_stat_change_effect(EFFECT_ATTACK_DOWN));
    assert!(is_stat_change_effect(EFFECT_DEFENSE_DOWN));
    assert!(is_stat_change_effect(MoveEffect(20))); // SPEED_DOWN
    assert!(is_stat_change_effect(MoveEffect(23))); // ACCURACY_DOWN
    assert!(is_stat_change_effect(MoveEffect(59))); // DEFENSE_DOWN_2
    assert!(is_stat_change_effect(MoveEffect(13))); // SPECIAL_ATTACK_UP
    assert!(is_stat_change_effect(MoveEffect(11))); // DEFENSE_UP

    assert!(!is_stat_change_effect(MoveEffect(0))); // EFFECT_HIT
                                                    // The `_UP`/`_DOWN`-named ids whose real table entry is the plain hit
                                                    // script: they belong to `crate::hit`'s allow-list, not here (this
                                                    // module's own table docs).
    for hit_shaped in [12u8, 14, 15, 21, 22, 55, 56, 61, 63, 64] {
        assert!(
            !is_stat_change_effect(MoveEffect(hit_shaped)),
            "EFFECT id {hit_shaped} dispatches to BattleScript_EffectHit, not a stat tail"
        );
        assert!(
            crate::hit::is_ordinary_hit_effect(MoveEffect(hit_shaped)),
            "...and `crate::hit` must be the one that claims it"
        );
    }
    // EFFECT_MINIMIZE and EFFECT_DEFENSE_CURL raise a stat but run their own
    // scripts (this module's table docs).
    assert!(!is_stat_change_effect(MoveEffect(108)));
    assert!(!is_stat_change_effect(MoveEffect(156)));
}

/// Every row's direction must agree with the real move data that carries it:
/// upstream's raising moves are all `MOVE_TARGET_USER` with `accuracy = 0`,
/// and its lowering moves all target the foe with a real accuracy. A row
/// transcribed with the wrong `up`/`down` would change *whose* stage moves,
/// so this checks the transcription against `gBattleMoves` rather than
/// against itself.
#[test]
fn every_transcribed_row_agrees_with_the_real_move_table() {
    let dex = Dex::new();
    let mut seen = 0usize;
    for move_id in 1..=354u16 {
        let mv = dex.move_data(MoveId(move_id)).unwrap();
        let Some(change) = stat_change_for_effect(mv.effect) else {
            continue;
        };
        seen += 1;
        assert_eq!(mv.power, 0, "move {move_id}: stat-change moves are 0-power");
        assert!(
            change.magnitude == 1 || change.magnitude == 2,
            "move {move_id}: no upstream thunk uses another magnitude"
        );
        match change.direction {
            StatChangeDirection::Raise => {
                assert_eq!(
                    mv.target,
                    MoveTarget::USER,
                    "move {move_id} raises, so it must be MOVE_TARGET_USER"
                );
                // Tail Glow (294) is upstream's sole raiser with a non-zero
                // `accuracy` byte, and the byte is inert: the raising script
                // has no `accuracycheck` command to read it, so Tail Glow
                // cannot miss any more than Growth can. Pinned as the one
                // exception rather than relaxed away, so a second one would
                // fail here.
                if move_id == 294 {
                    assert_eq!(mv.accuracy, 100, "Tail Glow's inert accuracy byte");
                } else {
                    assert_eq!(
                        mv.accuracy, 0,
                        "move {move_id} raises, so upstream leaves its accuracy byte at 0"
                    );
                }
            }
            StatChangeDirection::Lower => {
                assert_ne!(
                    mv.target,
                    MoveTarget::USER,
                    "move {move_id} lowers, so it targets the foe"
                );
                assert!(
                    mv.accuracy > 0,
                    "move {move_id} lowers, so it has a real accuracy to roll"
                );
            }
        }
    }
    assert!(
        seen >= 20,
        "the sweep must actually reach a useful number of real moves, saw {seen}"
    );
}

#[test]
fn ensure_resolvable_accepts_every_family_member_and_rejects_outsiders() {
    let dex = Dex::new();
    for move_id in [
        GROWL,
        LEER,
        TAIL_WHIP,
        STRING_SHOT,
        SAND_ATTACK,
        SCREECH,
        GROWTH,
        HARDEN,
    ] {
        assert_eq!(ensure_resolvable(&dex, move_id), Ok(()), "{move_id:?}");
    }
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
    let bad = MoveId(60_000);
    assert_eq!(
        ensure_resolvable(&dex, bad),
        Err(BattleError::UnknownMove(bad))
    );
}

#[test]
fn a_hit_growl_lowers_the_targets_attack_by_one_and_draws_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 3, vec![GROWL]); // Zigzagoon/Growl
    let defender = mon(&dex, 290, 3, vec![TACKLE]); // Wurmple
                                                    // Growl is 100 accuracy: roll = draw%100+1, always 1..=100 <= 100.
    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change,
        new_stage,
        capped,
    } = outcome
    else {
        panic!("Growl must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Attack);
    assert_eq!(change.direction, StatChangeDirection::Lower);
    assert_eq!(change.delta(), -1);
    assert!(!change.affects_user());
    assert_eq!(new_stage, StatStage::new(-1).unwrap());
    assert!(!capped);
    assert_eq!(rng.draws(), 1);
}

/// Screech is the `-2` case, and the magnitude has to survive into the
/// stage: a transcription that dropped it would land on `-1` here.
#[test]
fn a_hit_screech_lowers_defense_by_two() {
    let dex = Dex::new();
    let attacker = mon(&dex, 100, 15, vec![SCREECH]); // Voltorb/Screech
    let defender = mon(&dex, 290, 15, vec![TACKLE]);
    // Screech is 85 accuracy: draw 0 -> roll 1 <= 85 -> hit.
    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, SCREECH, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change, new_stage, ..
    } = outcome
    else {
        panic!("Screech must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Defense);
    assert_eq!(change.delta(), -2);
    assert_eq!(new_stage, StatStage::new(-2).unwrap());
    assert_eq!(rng.draws(), 1);
}

/// Sand Attack reaches a stage the pre-#322 slice could not name at all: the
/// target's **accuracy**.
#[test]
fn sand_attack_lowers_the_targets_accuracy_stage() {
    let dex = Dex::new();
    let attacker = mon(&dex, 296, 15, vec![SAND_ATTACK]); // Makuhita
    let defender = mon(&dex, 290, 15, vec![TACKLE]);
    let mut rng = SequenceRng::new([0]);
    let outcome =
        resolve_stat_change_move(&dex, SAND_ATTACK, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change, new_stage, ..
    } = outcome
    else {
        panic!("Sand Attack must connect: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::Accuracy);
    assert_eq!(new_stage, StatStage::new(-1).unwrap());
}

/// The raising tail has no `accuracycheck` command at all, so a stat-raising
/// move **cannot miss and costs the shared stream nothing** -- the single
/// biggest behavioural difference between the two families. The `SequenceRng`
/// is empty, so any draw at all would panic.
#[test]
fn a_stat_raising_move_draws_nothing_and_raises_the_users_own_stage() {
    let dex = Dex::new();
    let attacker = mon(&dex, 315, 14, vec![GROWTH]); // Roselia/Growth
    let defender = mon(&dex, 290, 14, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    let outcome = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    let StatChangeOutcome::Applied {
        change,
        new_stage,
        capped,
    } = outcome
    else {
        panic!("Growth cannot miss: {outcome:?}");
    };
    assert_eq!(change.stat, ChangedStat::SpAttack);
    assert_eq!(change.direction, StatChangeDirection::Raise);
    assert!(
        change.affects_user(),
        "Growth raises the *user's* Sp. Attack"
    );
    assert_eq!(new_stage, StatStage::new(1).unwrap());
    assert!(!capped);
    assert_eq!(
        rng.draws(),
        0,
        "BattleScript_EffectStatUp has no accuracycheck"
    );
}

/// ...and the stage it reads is the *user's*, not the target's: a raise
/// resolved against an already-boosted user must see that boost, and a
/// boosted target must be irrelevant.
#[test]
fn a_raise_reads_the_users_stage_and_a_drop_reads_the_targets() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, 315, 14, vec![GROWTH, GROWL]);
    let mut defender = mon(&dex, 290, 14, vec![TACKLE]);
    attacker.stages_mut().sp_attack = StatStage::new(4).unwrap();
    attacker.stages_mut().attack = StatStage::new(-3).unwrap();
    defender.stages_mut().sp_attack = StatStage::MIN;
    defender.stages_mut().attack = StatStage::new(2).unwrap();

    let mut rng = SequenceRng::new([]);
    let raise = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(raise, StatChangeOutcome::Applied { new_stage, .. }
            if new_stage == StatStage::new(5).unwrap()),
        "the raise must start from the *attacker's* +4, not the defender's floor: {raise:?}"
    );

    let mut rng = SequenceRng::new([0]);
    let drop = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(drop, StatChangeOutcome::Applied { new_stage, .. }
            if new_stage == StatStage::new(1).unwrap()),
        "the drop must start from the *defender's* +2, not the attacker's -3: {drop:?}"
    );
}

#[test]
fn a_missed_lowering_move_reports_miss_and_still_draws_once() {
    let dex = Dex::new();
    let attacker = mon(&dex, 290, 3, vec![STRING_SHOT]); // Wurmple/String Shot
    let defender = mon(&dex, 288, 3, vec![TACKLE]);
    // String Shot is 95 accuracy: draw 95 -> roll 96 > 95 -> miss.
    let mut rng = SequenceRng::new([95]);
    let outcome =
        resolve_stat_change_move(&dex, STRING_SHOT, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(outcome, StatChangeOutcome::Miss);
    assert_eq!(rng.draws(), 1);
}

#[test]
fn a_floored_drop_and_a_capped_raise_both_report_capped_without_moving_the_stage() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, 315, 14, vec![GROWTH]);
    let mut defender = mon(&dex, 290, 14, vec![TACKLE]);
    attacker.stages_mut().sp_attack = StatStage::MAX;
    defender.stages_mut().attack = StatStage::MIN;

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap(),
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(GROWTH).unwrap().effect).unwrap(),
            new_stage: StatStage::MAX,
            capped: true,
        }
    );
    assert_eq!(rng.draws(), 0);

    let growler = mon(&dex, 288, 3, vec![GROWL]);
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_stat_change_move(&dex, GROWL, &growler, &defender, &mut rng).unwrap(),
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            new_stage: StatStage::MIN,
            capped: true,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "the floored case still costs the accuracy roll"
    );
}

/// A `-2` drop against a target one stage above the floor clamps rather than
/// underflowing, and reports `capped: false` -- it *did* move.
#[test]
fn a_two_stage_drop_clamps_at_the_floor_without_reporting_capped() {
    let dex = Dex::new();
    let attacker = mon(&dex, 100, 15, vec![SCREECH]);
    let mut defender = mon(&dex, 290, 15, vec![TACKLE]);
    defender.stages_mut().defense = StatStage::new(-5).unwrap();

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, SCREECH, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::Applied {
            change: stat_change_for_effect(dex.move_data(SCREECH).unwrap().effect).unwrap(),
            new_stage: StatStage::MIN,
            capped: false,
        },
        "-5 - 2 clamps to -6 (ChangeStatBuffs' add-then-clamp), and the stage really moved"
    );
}

#[test]
fn a_rejected_move_draws_nothing() {
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 3, vec![TACKLE]);
    let defender = mon(&dex, 290, 3, vec![TACKLE]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_stat_change_move(&dex, TACKLE, &attacker, &defender, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// The table's own shape: no duplicate effect ids, and every id really is a
/// `u8` the upstream header defines.
#[test]
fn the_transcribed_table_has_no_duplicate_rows() {
    for (index, (id, _)) in STAT_CHANGE_EFFECTS.iter().enumerate() {
        assert!(
            !STAT_CHANGE_EFFECTS[..index]
                .iter()
                .any(|(other, _)| other == id),
            "EFFECT id {} appears twice",
            id.0
        );
    }
}

/// Clear Body's guard (issue #322): a Tentacool defender's Attack drop is
/// blocked after the accuracy draw, and the stage does not move.
#[test]
fn a_clear_body_holders_drop_is_blocked_after_the_accuracy_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 15, vec![GROWL]);
    let defender = mon(&dex, TENTACOOL, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        CLEAR_BODY,
        "fixture sanity: personality 0 selects slot 0, Clear Body"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            ability: CLEAR_BODY,
        }
    );
    assert_eq!(
        rng.draws(),
        1,
        "a blocked drop still costs its one accuracy draw"
    );
}

/// White Smoke shares Clear Body's guard in one upstream condition
/// (`battle_script_commands.c:6987`-`:6989`), so it must block identically:
/// a Torkoal defender (`SPECIES_TORKOAL`, single-ability slot 0, White
/// Smoke) refuses the drop after the same single accuracy draw.
#[test]
fn a_white_smoke_holders_drop_is_blocked_identically() {
    const TORKOAL: u16 = 321;
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 15, vec![GROWL]);
    let defender = mon(&dex, TORKOAL, 15, vec![TACKLE]);
    assert_eq!(
        defender.ability(),
        WHITE_SMOKE,
        "fixture sanity: Torkoal's only ability slot is White Smoke"
    );

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert_eq!(
        outcome,
        StatChangeOutcome::AbilityProtected {
            change: stat_change_for_effect(dex.move_data(GROWL).unwrap().effect).unwrap(),
            ability: WHITE_SMOKE,
        }
    );
    assert_eq!(rng.draws(), 1);
}

/// The guard fires even when the held stage is already at the floor:
/// upstream's ability branch is checked *before* the at-floor test, so a
/// Clear Body holder never sees `B_MSG_STAT_WONT_DECREASE` for an
/// opponent-inflicted drop, only the ability message.
#[test]
fn clear_body_blocks_even_when_already_at_the_floor() {
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 15, vec![GROWL]);
    let mut defender = mon(&dex, TENTACOOL, 15, vec![TACKLE]);
    defender.stages_mut().attack = StatStage::MIN;

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::AbilityProtected { ability, .. } if ability == CLEAR_BODY),
        "{outcome:?}"
    );
}

/// Clear Body only guards a *drop*: a holder using a raising move on itself
/// is unaffected, because `ChangeStatBuffs`' ability block sits inside the
/// decrease branch only (module docs).
#[test]
fn clear_body_never_guards_its_holders_own_raise() {
    let dex = Dex::new();
    let attacker = mon(&dex, TENTACOOL, 15, vec![GROWTH]);
    let defender = mon(&dex, 290, 15, vec![TACKLE]);
    assert_eq!(attacker.ability(), CLEAR_BODY);

    let mut rng = SequenceRng::new([]);
    let outcome = resolve_stat_change_move(&dex, GROWTH, &attacker, &defender, &mut rng).unwrap();
    assert!(
        matches!(outcome, StatChangeOutcome::Applied { capped: false, .. }),
        "Clear Body must not block its own holder's raise: {outcome:?}"
    );
}

/// A defender without Clear Body is unaffected: the ordinary Applied path
/// still runs.
#[test]
fn a_non_clear_body_holders_drop_is_unaffected() {
    let dex = Dex::new();
    let attacker = mon(&dex, 288, 3, vec![GROWL]);
    let defender = mon(&dex, 290, 3, vec![TACKLE]); // Wurmple: Shield Dust, not Clear Body
    assert_ne!(defender.ability(), CLEAR_BODY);

    let mut rng = SequenceRng::new([0]);
    let outcome = resolve_stat_change_move(&dex, GROWL, &attacker, &defender, &mut rng).unwrap();
    assert!(matches!(outcome, StatChangeOutcome::Applied { .. }));
}
