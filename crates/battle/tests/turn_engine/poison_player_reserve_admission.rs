//! Pre-battle admission of enemy Poison Sting against Synchronize player
//! reserves.

use crate::poison_support::*;
use assets::AbilityId;
use battle::{Battle, Dex};

/// A non-fainted player reserve is checked against the enemy's moveset
/// before the battle starts, exactly like the active member: it may become
/// the enemy's defender with no further checkpoint once sent out. Now that
/// the reflection is modelled, a Synchronize reserve reachable by an enemy
/// Poison Sting is admitted just like an active-slot Synchronize defender.
#[test]
fn an_enemy_move_is_admitted_against_a_synchronize_reserve_before_the_battle_starts() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let reserve = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);
    assert_eq!(reserve.ability(), AbilityId::SYNCHRONIZE);
    let enemy = max_iv_mon(&dex, EKANS, 50, vec![TACKLE, POISON_STING]);
    let mut rng = SequenceRng::new([0; 32]);
    Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
        .expect("Synchronize's poison reflection is modelled, so the reserve is admitted");
}

/// A fainted reserve can never be sent out
/// (`Battle::send_out_next_player_reserve` skips every fainted entry), so it
/// models a player party that already lost a member before the battle and
/// must not refuse construction over an enemy move it will never face.
#[test]
fn a_fainted_synchronize_reserve_does_not_refuse_an_enemy_poison_sting() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut reserve = max_iv_mon(&dex, RALTS, 5, vec![TACKLE]);
    assert_eq!(reserve.ability(), AbilityId::SYNCHRONIZE);
    reserve.apply_damage(reserve.stats().max_hp);
    assert!(reserve.is_fainted());
    let enemy = max_iv_mon(&dex, EKANS, 50, vec![TACKLE, POISON_STING]);
    let mut rng = SequenceRng::new([0; 32]);
    Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
        .expect("a fainted reserve can never be sent out, so it is never validated as a defender");
}
