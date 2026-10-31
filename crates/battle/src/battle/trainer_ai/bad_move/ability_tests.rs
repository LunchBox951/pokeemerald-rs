//! `AI_CheckBadMove` target-ability guards.

use crate::battle::opponent_ai::EnemyAction;
use crate::battle::trainer_ai::choose_trainer_action;
use crate::battle::trainer_ai::setup_first_turn::FIRST_TURN;
use crate::battle::trainer_ai::test_support::pokemon;
use crate::battle::trainer_ai::test_support::rng_with_maximum_simulated_damage;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::HYPER_VOICE;
use crate::battle::trainer_ai::test_support::LEER;
use crate::battle::trainer_ai::test_support::MR_MIME;
use crate::battle::trainer_ai::test_support::MUDKIP;
use crate::battle::trainer_ai::test_support::PINSIR;
use crate::battle::trainer_ai::test_support::POUND;
use crate::battle::trainer_ai::test_support::REGIROCK;
use crate::battle::trainer_ai::test_support::SELECT_FIRST_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SELECT_SECOND_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SHEDINJA;
use crate::battle::trainer_ai::test_support::TREECKO;
use crate::battle::trainer_ai::test_support::WATER_GUN;
use crate::battle::trainer_ai::test_support::WING_ATTACK;
use crate::dex::Dex;
use crate::pokemon::MAX_MON_MOVES;
use assets::trainers::AiFlags;

/// `AI_CheckBadMove` reads `get_ability AI_TARGET` on a powered hit
/// (`data/battle_ai_scripts.s:59-64`): a Wonder Guard target keeps a
/// merely-neutral hit's -10 score, while an exactly-super-effective hit
/// keeps its full score, matching the same ability at execution
/// (`crates/battle/src/hit.rs:194-198`).
#[test]
fn wonder_guard_discourages_a_neutral_hit_but_not_a_super_effective_one() {
    let enemy = pokemon(MUDKIP, vec![WATER_GUN, WING_ATTACK]);
    let player = pokemon(SHEDINJA, vec![POUND]);
    assert_eq!(player.ability(), assets::AbilityId::WONDER_GUARD);
    let mut rng = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);

    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();

    assert_eq!(action, EnemyAction::Move(1));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

/// `AI_CBM_AttackDown` reads `get_ability AI_TARGET` after the stage
/// floor check (`data/battle_ai_scripts.s:277-280`): Hyper Cutter scores
/// the drop -10 at a stage the target could otherwise still lose,
/// matching the `AbilityProtected` outcome at execution
/// (`crates/battle/src/stat_change.rs:374-379`).
#[test]
fn hyper_cutter_discourages_an_attack_drop_at_an_unlowered_stage() {
    let enemy = pokemon(TREECKO, vec![POUND, GROWL]);
    // Pinsir's only ability is Hyper Cutter, so the read is unambiguous.
    let player = pokemon(PINSIR, vec![POUND]);
    assert_eq!(player.ability(), assets::AbilityId::HYPER_CUTTER);
    // Both moves tie at 100 without the guard, and the tie-break draw
    // below would already pick slot 0 -- select the SECOND tied slot so
    // this fails without the guard instead of passing by coincidence.
    // The ability reads draw nothing.
    let mut rng = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);

    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();

    assert_eq!(action, EnemyAction::Move(0));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

/// `AI_CBM_DefenseDown` shares the Clear Body / White Smoke guard with
/// `AI_CBM_AttackDown` (`data/battle_ai_scripts.s:308-311`).
#[test]
fn clear_body_discourages_a_defense_drop_at_an_unlowered_stage() {
    let enemy = pokemon(TREECKO, vec![POUND, LEER]);
    // Regirock's only ability is Clear Body.
    let player = pokemon(REGIROCK, vec![POUND]);
    assert_eq!(player.ability(), crate::stat_change::CLEAR_BODY);
    // See the SECOND-tied-slot note above: both moves tie at 100 without
    // the guard, and the ability reads draw nothing.
    let mut rng = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);

    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();

    assert_eq!(action, EnemyAction::Move(0));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

/// The Soundproof guard (`data/battle_ai_scripts.s:92-103`) omits Hyper
/// Voice unlike execution's ten-move `SOUND_MOVES`
/// (`crates/battle/src/stat_change.rs:83-94`).
#[test]
fn soundproof_discourages_growl_but_not_hyper_voice() {
    let enemy = pokemon(TREECKO, vec![HYPER_VOICE, GROWL]);
    // Mr. Mime's only ability is Soundproof.
    let player = pokemon(MR_MIME, vec![POUND]);
    assert_eq!(player.ability(), assets::AbilityId::SOUNDPROOF);
    // Both moves tie at 100 without the guard; see the SECOND-tied-slot
    // note above. Hyper Voice is absent from the AI's own list, so it
    // scores unaffected either way; the ability reads draw nothing.
    let mut rng = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);

    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();

    assert_eq!(action, EnemyAction::Move(0));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}
