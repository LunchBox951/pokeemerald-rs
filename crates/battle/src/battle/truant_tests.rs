//! Truant through the trainer paths no public action reaches: a voluntary
//! switch out and back in, and forced replacements on the enemy side. The
//! enemy's action is fixed by the test through the private seam because no
//! AI decision selects a switch yet, and every script is exact.

use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};

use super::trainer::fixed_ivs;
use super::{Battle, BattleEvent, BattlerSide, EnemyAction, PlayerAction};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::script_rng::SequenceRng;
use crate::status1::Status1;

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const TACKLE: MoveId = MoveId::TACKLE;
const SPLASH: MoveId = MoveId::SPLASH;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A turn's seed draw; the test fixes the enemy action, so nothing else.
const U: [u16; 1] = [0];
/// Passing accuracy, non-critical, damage roll, discarded effect chance.
const H: [u16; 4] = [0, 1, 0, 0];

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, fixed_ivs(255), 0, moves.to_vec())
        .expect("dex-resident")
}

fn slaking(dex: &Dex, level: u8) -> BattlePokemon {
    let slaking = mon(dex, SpeciesId::SLAKING, level, &[TACKLE]);
    assert_eq!(slaking.ability(), AbilityId::TRUANT);
    slaking
}

fn sleeping(mut mon: BattlePokemon) -> BattlePokemon {
    mon.set_status1(Status1::Asleep { turns: 5 });
    mon
}

fn poisoned_at_one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.set_status1(Status1::Poisoned);
    mon.apply_damage(mon.stats().max_hp - 1);
    mon
}

fn script(groups: &[&[u16]]) -> SequenceRng {
    SequenceRng::new(groups.concat())
}

/// Runs one turn with the enemy's action fixed by the test.
fn turn_with(
    battle: &mut Battle,
    player_move: usize,
    enemy_action: EnemyAction,
    rng: &mut impl BattleRng,
) -> Result<Vec<BattleEvent>, BattleError> {
    let player_action = battle.validate_player_action(PlayerAction::UseMove(player_move))?;
    battle.start_turn(rng);
    let mut events = Vec::new();
    battle.resolve_selected_actions(player_action, enemy_action, rng, &mut events)?;
    Ok(events)
}

fn enemy_loafs(battle: &Battle) -> bool {
    battle.battler_state(BattlerSide::Enemy).truant_loafing()
}

fn player_loafs(battle: &Battle) -> bool {
    battle.battler_state(BattlerSide::Player).truant_loafing()
}

fn no_effect(by_player: bool) -> BattleEvent {
    BattleEvent::NoEffect {
        by_player,
        move_id: TACKLE,
    }
}

fn fast_asleep(by_player: bool) -> BattleEvent {
    BattleEvent::FastAsleep {
        by_player,
        move_id: TACKLE,
    }
}

fn loafing(by_player: bool) -> BattleEvent {
    BattleEvent::LoafingAround {
        by_player,
        move_id: TACKLE,
    }
}

fn splash(by_player: bool) -> BattleEvent {
    BattleEvent::NothingHappened {
        by_player,
        move_id: SPLASH,
    }
}

#[test]
fn a_holder_switched_out_with_its_counter_set_re_enters_able_to_act() {
    let dex = Dex::new();
    let player = sleeping(mon(&dex, SpeciesId::GASTLY, 1, &[TACKLE]));
    let party = vec![
        slaking(&dex, 10),
        sleeping(mon(&dex, SpeciesId::GASTLY, 2, &[TACKLE])),
    ];
    let mut rng = script(&[&C, &U, &H, &U, &U, &U, &H]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    let starting_pp = battle.enemy().moves()[0].pp;

    let first = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(first, vec![no_effect(false), fast_asleep(true)]);
    assert!(enemy_loafs(&battle), "the holder's counter is set");

    let out = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).unwrap();
    assert_eq!(
        out,
        vec![
            BattleEvent::TrainerSentOut {
                species: SpeciesId::GASTLY,
                bench_remaining: 1,
            },
            fast_asleep(true),
        ]
    );
    assert!(
        !enemy_loafs(&battle),
        "the counter does not travel with the benched member"
    );

    let back = turn_with(&mut battle, 0, EnemyAction::Switch(0), &mut rng).unwrap();
    assert_eq!(
        back,
        vec![
            BattleEvent::TrainerSentOut {
                species: SpeciesId::SLAKING,
                bench_remaining: 1,
            },
            fast_asleep(true),
        ]
    );
    assert!(
        !enemy_loafs(&battle),
        "switch-in set the counter and the same turn's end pass cleared it"
    );

    let acting = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(acting, vec![no_effect(false), fast_asleep(true)]);
    assert_eq!(battle.enemy().moves()[0].pp, starting_pp - 2);
    rng.assert_exhausted();
}

#[test]
fn a_replacement_after_an_action_faint_acts_on_its_first_turn() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::GASTLY, 100, &[TACKLE, SPLASH]);
    let mut lead = mon(&dex, SpeciesId::RATTATA, 1, &[TACKLE]);
    lead.apply_damage(lead.stats().max_hp - 1);
    let party = vec![lead, slaking(&dex, 10)];
    let mut rng = script(&[&C, &U, &H, &U, &H, &U]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let first = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        first,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                damage: 1,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: false },
            BattleEvent::TrainerSentOut {
                species: SpeciesId::SLAKING,
                bench_remaining: 0,
            },
        ]
    );
    assert!(!enemy_loafs(&battle), "the same turn's end pass cleared it");

    let second = turn_with(&mut battle, 1, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(second, vec![splash(true), no_effect(false)]);
    assert!(enemy_loafs(&battle));

    let third = turn_with(&mut battle, 1, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(third, vec![splash(true), loafing(false)]);
    assert!(!enemy_loafs(&battle));
    rng.assert_exhausted();
}

#[test]
fn a_replacement_after_a_residual_faint_loafs_on_its_first_turn() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]);
    let lead = poisoned_at_one_hp(mon(&dex, SpeciesId::RATTATA, 1, &[SPLASH]));
    let party = vec![lead, slaking(&dex, 10)];
    let mut rng = script(&[&C, &U, &U, &U, &H]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let first = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        first,
        vec![
            splash(true),
            splash(false),
            BattleEvent::HurtByPoison {
                by_player: false,
                damage: 1,
            },
            BattleEvent::Fainted { by_player: false },
            BattleEvent::TrainerSentOut {
                species: SpeciesId::SLAKING,
                bench_remaining: 0,
            },
        ]
    );
    assert!(enemy_loafs(&battle), "no end pass followed the send-out");

    let second = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(second, vec![splash(true), loafing(false)]);
    assert!(!enemy_loafs(&battle));

    let third = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(third, vec![splash(true), no_effect(false)]);
    rng.assert_exhausted();
}

#[test]
fn a_double_residual_faint_sends_the_player_out_first_and_both_holders_loaf() {
    let dex = Dex::new();
    let player = poisoned_at_one_hp(mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]));
    let reserve = slaking(&dex, 10);
    let lead = poisoned_at_one_hp(mon(&dex, SpeciesId::GASTLY, 90, &[SPLASH]));
    let party = vec![lead, slaking(&dex, 11)];
    let mut rng = script(&[&C, &U, &U]);
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex,
        player,
        vec![reserve],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .unwrap();

    let first = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        first,
        vec![
            splash(true),
            splash(false),
            BattleEvent::HurtByPoison {
                by_player: true,
                damage: 1,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::HurtByPoison {
                by_player: false,
                damage: 1,
            },
            BattleEvent::Fainted { by_player: false },
            BattleEvent::PlayerSentOut {
                species: SpeciesId::SLAKING,
                reserves_remaining: 0,
            },
            BattleEvent::TrainerSentOut {
                species: SpeciesId::SLAKING,
                bench_remaining: 0,
            },
        ]
    );
    assert!(player_loafs(&battle) && enemy_loafs(&battle));

    let second = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(second, vec![loafing(false), loafing(true)]);
    assert!(!player_loafs(&battle) && !enemy_loafs(&battle));
    rng.assert_exhausted();
}
