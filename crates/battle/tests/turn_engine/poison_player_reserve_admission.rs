//! Pre-battle admission of enemy Poison Sting against player reserves:
//! healthy reserves are validated, fainted reserves are skipped.

use crate::poison_support::*;
use assets::AbilityId;
use battle::{Battle, Dex};

/// A non-fainted player reserve is checked against the enemy's moveset
/// before the battle starts, like the active member, because it can become
/// the enemy's defender with no further checkpoint. A Synchronize reserve
/// facing Poison Sting is admitted like an active Synchronize defender.
#[test]
fn an_enemy_move_is_admitted_against_a_synchronize_reserve_before_the_battle_starts() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let reserve = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);
    assert_eq!(reserve.ability(), AbilityId::SYNCHRONIZE);
    let enemy = max_iv_mon(&dex, EKANS, 50, vec![TACKLE, POISON_STING]).with_ability_slot(1);
    // battle_main.c:3140 construction turn seed: the only draw.
    let mut rng = SequenceRng::new([0]);
    Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
        .expect("Synchronize's poison reflection is modelled, so the reserve is admitted");
    assert_eq!(rng.draws(), 1, "reserve admission draws only the turn seed");
    rng.assert_exhausted();
}

/// Discriminating pair: an enemy Serene Grace Poison Sting is refused against
/// a healthy poisonable reserve (the Poison-type lead is immune), and admitted
/// only because the same reserve is fainted. A fainted reserve is never sent
/// out (`Battle::send_out_next_player_reserve` skips fainted entries), so it
/// must not refuse construction over an enemy move it will never face.
#[test]
fn a_fainted_reserve_is_skipped_where_the_same_healthy_reserve_is_refused() {
    let dex = Dex::new();
    let lead = || max_iv_mon(&Dex::new(), EKANS, 10, vec![TACKLE]).with_ability_slot(1);
    let enemy = || max_iv_mon(&Dex::new(), 206, 10, vec![POISON_STING]);
    assert_eq!(enemy().ability(), AbilityId::SERENE_GRACE);
    let healthy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0u16; 4]);
    assert_eq!(
        Battle::new_with_player_reserves(
            Dex::new(),
            lead(),
            vec![healthy],
            enemy(),
            false,
            &mut rng
        )
        .err(),
        Some(battle::BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        )),
        "control: the healthy reserve is validated and refused"
    );
    let mut fainted = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);
    fainted.apply_damage(fainted.stats().max_hp);
    assert!(fainted.is_fainted());
    // battle_main.c:3140 construction turn seed: the only draw.
    let mut rng = SequenceRng::new([0]);
    Battle::new_with_player_reserves(dex, lead(), vec![fainted], enemy(), false, &mut rng)
        .expect("a fainted reserve is never sent out, so it is never validated");
    assert_eq!(rng.draws(), 1, "reserve admission draws only the turn seed");
    rng.assert_exhausted();
}
