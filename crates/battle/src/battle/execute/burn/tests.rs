//! Flame Body through the seams no public constructor reaches: a holder that
//! is not Fire-typed, which only a Trace copy produces.

use assets::{AbilityId, MoveId, SpeciesId};

use crate::battle::trainer::fixed_ivs;
use crate::battle::{Battle, BattleEvent, PlayerAction};
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;
use crate::script_rng::SequenceRng;
use crate::secondary::{
    flame_body_can_land, resolve_synchronize_burn_reflection, SynchronizeBurnReflectionOutcome,
};
use crate::status1::Status1;

const TACKLE: MoveId = MoveId::TACKLE;

fn mon(dex: &Dex, species: SpeciesId, level: u8) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, fixed_ivs(255), 0, vec![TACKLE]).expect("dex-resident")
}

#[test]
fn a_synchronize_attacker_passes_the_flame_body_burn_back_without_a_draw() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::RALTS, 50);
    assert_eq!(player.ability(), AbilityId::SYNCHRONIZE);
    let enemy = mon(&dex, SpeciesId::RALTS, 20);
    // Construction's turn number, the turn start and the enemy's move choice,
    // then the player's landed Tackle and its Flame Body roll, then the
    // enemy's landed Tackle: no draw is spent on the reflection.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    battle.enemy.trace_ability(AbilityId::FLAME_BODY);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.player().status1(), Status1::Burned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Burned, "{events:?}");
    let burned = events
        .iter()
        .position(|event| {
            *event
                == BattleEvent::BurnedByFlameBody {
                    by_player: true,
                    move_id: TACKLE,
                }
        })
        .expect("Flame Body burned the attacker");
    assert_eq!(
        events[burned + 1],
        BattleEvent::BurnedByAttackerSynchronize {
            by_player: true,
            move_id: TACKLE,
        },
        "{events:?}"
    );
}

#[test]
fn the_flame_body_guards_are_the_ability_origin_set() {
    let dex = Dex::new();
    let healthy = mon(&dex, SpeciesId::RATTATA, 10);
    assert!(flame_body_can_land(&healthy));

    let mut statused = healthy.clone();
    statused.set_status1(Status1::Paralysed);
    assert!(!flame_body_can_land(&statused));

    assert!(!flame_body_can_land(&mon(&dex, SpeciesId::MAGMAR, 10)));
    let goldeen = BattlePokemon::new(
        &dex,
        SpeciesId::GOLDEEN,
        10,
        fixed_ivs(255),
        1,
        vec![TACKLE],
    )
    .unwrap();
    assert_eq!(goldeen.ability(), AbilityId::WATER_VEIL);
    assert!(!flame_body_can_land(&goldeen));
    let dustox = mon(&dex, SpeciesId::DUSTOX, 10);
    assert_eq!(dustox.ability(), AbilityId::SHIELD_DUST);
    assert!(flame_body_can_land(&dustox), "Shield Dust is bypassed");
}

#[test]
fn the_synchronize_burn_reflection_checks_fire_typing_before_the_existing_status() {
    let dex = Dex::new();
    let mut holder = mon(&dex, SpeciesId::RALTS, 10);
    assert_eq!(
        resolve_synchronize_burn_reflection(&holder),
        SynchronizeBurnReflectionOutcome::Applied
    );
    holder.set_status1(Status1::Poisoned);
    assert_eq!(
        resolve_synchronize_burn_reflection(&holder),
        SynchronizeBurnReflectionOutcome::AlreadyStatused
    );
    let mut magmar = mon(&dex, SpeciesId::MAGMAR, 10);
    magmar.set_status1(Status1::Poisoned);
    assert_eq!(
        resolve_synchronize_burn_reflection(&magmar),
        SynchronizeBurnReflectionOutcome::TypeProtected
    );
}
