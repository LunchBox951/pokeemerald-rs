//! Target-ability reads: ambiguity refusal and the trapping-ability shortcut.

use crate::battle::trainer_ai::choose_trainer_action;
use crate::battle::trainer_ai::setup_first_turn::FIRST_TURN;
use crate::battle::trainer_ai::test_support::pokemon;
use crate::battle::trainer_ai::test_support::rng_with_maximum_simulated_damage;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::HYPER_VOICE;
use crate::battle::trainer_ai::test_support::POUND;
use crate::battle::trainer_ai::test_support::SELECT_FIRST_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SHEDINJA;
use crate::battle::trainer_ai::test_support::TRAPINCH;
use crate::battle::trainer_ai::test_support::TREECKO;
use crate::battle::trainer_ai::test_support::VOLTORB;
use crate::dex::Dex;
use crate::error::BattleError;
use assets::trainers::AiFlags;

/// Upstream's `get_ability AI_TARGET` reads history, a trapping ability,
/// or draws a fresh `Random() & 1` guess between two species slots
/// (`src/battle_ai_script_commands.c:1350-1396`). The engine models no
/// history, so a two-ability target without a trapping ability is refused
/// before any draw, whichever slot it holds: Voltorb in either slot and a
/// Hyper Cutter Trapinch.
#[test]
fn a_two_ability_target_is_refused_before_any_draw() {
    let dex = Dex::new();
    let enemy = pokemon(TREECKO, vec![HYPER_VOICE, GROWL]);
    for (species, slot) in [(VOLTORB, 0), (VOLTORB, 1), (TRAPINCH, 0)] {
        let target = pokemon(species, vec![POUND]).with_ability_slot(slot);
        let mut rng = rng_with_maximum_simulated_damage([]);
        let draws_before = rng.draws();

        let refused = choose_trainer_action(
            &dex,
            &enemy,
            &target,
            AiFlags::CHECK_BAD_MOVE,
            FIRST_TURN,
            &mut rng,
        );

        assert_eq!(refused, Err(BattleError::AmbiguousPlayerAbility(species)));
        assert_eq!(rng.draws(), draws_before);
    }
}

/// Upstream returns a true trapping ability before the two-slot guess,
/// with no draw (`src/battle_ai_script_commands.c:1368-1376`), so an Arena
/// Trap Trapinch is admitted and read directly while a Hyper Cutter
/// Trapinch stays refused.
#[test]
fn an_arena_trap_target_is_admitted_and_read_without_a_draw() {
    let dex = Dex::new();
    let enemy = pokemon(TREECKO, vec![HYPER_VOICE, GROWL]);
    let arena_trap = pokemon(TRAPINCH, vec![POUND]).with_ability_slot(1);
    let hyper_cutter = pokemon(TRAPINCH, vec![POUND]).with_ability_slot(0);

    assert_eq!(
        super::ai_get_ability(&arena_trap),
        Ok(assets::AbilityId::ARENA_TRAP)
    );
    assert_eq!(
        super::ensure_target_ability_unambiguous(&arena_trap),
        Ok(())
    );
    assert_eq!(
        super::ensure_target_ability_unambiguous(&hyper_cutter),
        Err(BattleError::AmbiguousPlayerAbility(TRAPINCH))
    );

    let mut admitted = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
    let mut single = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
    let trapping_choice = choose_trainer_action(
        &dex,
        &enemy,
        &arena_trap,
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut admitted,
    );
    let single_ability_choice = choose_trainer_action(
        &dex,
        &enemy,
        &pokemon(TREECKO, vec![POUND]),
        AiFlags::CHECK_BAD_MOVE,
        FIRST_TURN,
        &mut single,
    );
    assert!(trapping_choice.is_ok());
    assert_eq!(trapping_choice, single_ability_choice);
    assert_eq!(admitted.draws(), single.draws());
}

/// A single-ability target reads its sole ability and draws nothing.
#[test]
fn a_single_ability_target_is_read_without_a_draw() {
    let target = pokemon(SHEDINJA, vec![POUND]);
    assert_eq!(
        super::ai_get_ability(&target),
        Ok(assets::AbilityId::WONDER_GUARD)
    );
}
