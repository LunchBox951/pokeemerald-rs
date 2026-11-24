//! `AI_TryToFaint_TryToEncourageQuickAttack`: a Quick Attack finisher scores
//! +6 in total, every other finisher +4, and a Quick Attack that cannot finish
//! follows the ordinary branches with no bonus and no extra draw
//! (`data/battle_ai_scripts.s:2616-2635`).

use super::{
    choose_trainer_action, ensure_scoreable, estimated_damage, score_try_to_faint, MoveScores,
    PERCENT_SCALE,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 5;
const INITIAL: i8 = 100;
/// `PERCENT_SCALE - draw % 16`: a zero draw keeps the full damage roll.
const FULL_DAMAGE_DRAW: u16 = 0;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);

fn mon(species: SpeciesId, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

fn at_hp(mut target: BattlePokemon, hp: u32) -> BattlePokemon {
    target.apply_damage(target.current_hp() - hp);
    assert_eq!(target.current_hp(), hp);
    target
}

fn damage_of(move_id: MoveId, user: &BattlePokemon, target: &BattlePokemon) -> u32 {
    estimated_damage(&Dex::new(), move_id, user, target, PERCENT_SCALE).unwrap()
}

/// Runs `AI_TryToFaint` on `slot` alone after the four simulated-damage draws,
/// returning the slot's score and the total draws consumed.
fn try_to_faint(
    slot: usize,
    user: &BattlePokemon,
    target: &BattlePokemon,
    tail: &[u16],
) -> (i8, usize) {
    let mut script = vec![FULL_DAMAGE_DRAW; MAX_MON_MOVES];
    script.extend_from_slice(tail);
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(user, &mut rng);
    let move_id = user.moves()[slot].move_id;
    score_try_to_faint(
        &Dex::new(),
        &mut scores,
        slot,
        move_id,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    rng.assert_exhausted();
    (scores.values[slot], rng.draws())
}

#[test]
fn a_quick_attack_finisher_scores_six_and_one_hp_more_earns_no_bonus_or_draw() {
    let enemy = mon(TREECKO, &[MoveId::QUICK_ATTACK, MoveId::POUND]);
    let quick_attack_damage =
        damage_of(MoveId::QUICK_ATTACK, &enemy, &mon(MUDKIP, &[MoveId::POUND]));
    let pound_damage = damage_of(MoveId::POUND, &enemy, &mon(MUDKIP, &[MoveId::POUND]));
    assert_eq!(
        quick_attack_damage, pound_damage,
        "both are 40-power Normal"
    );

    let target = at_hp(mon(MUDKIP, &[MoveId::POUND]), quick_attack_damage);
    assert_eq!(try_to_faint(0, &enemy, &target, &[]), (INITIAL + 6, 4));
    assert_eq!(try_to_faint(1, &enemy, &target, &[]), (INITIAL + 4, 4));

    // Neither move is weaker than the other and a Normal move is never
    // quadruple-effective, so the script ends untouched without a roll.
    let target = at_hp(mon(MUDKIP, &[MoveId::POUND]), quick_attack_damage + 1);
    assert_eq!(try_to_faint(0, &enemy, &target, &[]), (INITIAL, 4));
    assert_eq!(try_to_faint(1, &enemy, &target, &[]), (INITIAL, 4));

    // A stronger rival slot still demotes a Quick Attack that cannot finish.
    let enemy = mon(TREECKO, &[MoveId::QUICK_ATTACK, MoveId::SLASH]);
    let slash_damage = damage_of(MoveId::SLASH, &enemy, &mon(MUDKIP, &[MoveId::POUND]));
    assert!(slash_damage > quick_attack_damage);
    assert_eq!(try_to_faint(0, &enemy, &target, &[]), (INITIAL - 1, 4));
    assert_eq!(try_to_faint(1, &enemy, &target, &[]), (INITIAL + 4, 4));
}

#[test]
fn the_extra_bonus_decides_the_chosen_slot_at_the_exact_threshold() {
    let dex = Dex::new();
    let flags = AiFlags::TRY_TO_FAINT;
    // Quick Attack in slot 0 and an identical-damage Pound in slot 1: without
    // the bonus the two finishers tie and the last draw (1) picks slot 1.
    let enemy = mon(TREECKO, &[MoveId::QUICK_ATTACK, MoveId::POUND]);
    let damage = damage_of(MoveId::QUICK_ATTACK, &enemy, &mon(MUDKIP, &[MoveId::POUND]));
    let select_second_tied_slot = 1;
    let script = [FULL_DAMAGE_DRAW; MAX_MON_MOVES]
        .into_iter()
        .chain([select_second_tied_slot]);

    let finishable = at_hp(mon(MUDKIP, &[MoveId::POUND]), damage);
    let mut rng = SequenceRng::new(script.clone());
    let action = choose_trainer_action(&dex, &enemy, &finishable, flags, 0, &mut rng).unwrap();
    assert_eq!(action, EnemyAction::Move(0));
    rng.assert_exhausted();
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);

    // One HP more and the tie returns, so the same draw now picks slot 1.
    let healthy = at_hp(mon(MUDKIP, &[MoveId::POUND]), damage + 1);
    let mut rng = SequenceRng::new(script);
    let action = choose_trainer_action(&dex, &enemy, &healthy, flags, 0, &mut rng).unwrap();
    assert_eq!(action, EnemyAction::Move(1));
    rng.assert_exhausted();
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

#[test]
fn try_to_faint_admits_quick_attack_and_charge() {
    let dex = Dex::new();
    for flags in [
        AiFlags::TRY_TO_FAINT,
        AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::CHECK_VIABILITY)
            .union(AiFlags::SETUP_FIRST_TURN),
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::QUICK_ATTACK, flags),
            Ok(()),
            "{flags:?}"
        );
        assert_eq!(
            ensure_scoreable(&dex, MoveId::CHARGE, flags),
            Ok(()),
            "{flags:?}"
        );
    }
}
