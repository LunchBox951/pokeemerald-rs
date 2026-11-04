//! Intimidate through the paths no public action reaches: a voluntary enemy
//! switch out and back in, forced trainer replacements, and a direct drain of
//! the entry queue. The enemy's action is fixed by the test through the private
//! seam because no AI decision selects a switch yet, and every script is exact.

use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};

use super::trainer::fixed_ivs;
use super::{Battle, BattleEvent, BattlerSide, BattlerState, EnemyAction, PlayerAction};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::script_rng::SequenceRng;
use crate::stat_change::{set_stage, ChangedStat};
use crate::stat_stage::StatStage;

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

fn at_one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - 1);
    mon
}

fn fainted(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp);
    mon
}

fn script(groups: &[&[u16]]) -> SequenceRng {
    SequenceRng::new(groups.concat())
}

fn stage(value: i8) -> StatStage {
    StatStage::new(value).expect("in range")
}

fn attack_stage(mon: &BattlePokemon) -> StatStage {
    mon.stages().attack
}

/// Runs one turn with the enemy's action fixed by the test, delivering the
/// opening entry events after validation as `take_turn` does.
fn turn_with(
    battle: &mut Battle,
    player_move: usize,
    enemy_action: EnemyAction,
    rng: &mut impl BattleRng,
) -> Result<Vec<BattleEvent>, BattleError> {
    let player_action = battle.validate_player_action(PlayerAction::UseMove(player_move))?;
    let mut events = Vec::new();
    events.append(&mut battle.pending_entry_events);
    battle.start_turn(rng);
    battle.resolve_selected_actions(player_action, enemy_action, rng, &mut events)?;
    Ok(events)
}

fn intimidated(by_player: bool, new_stage: i8) -> BattleEvent {
    BattleEvent::Intimidated {
        by_player,
        new_stage: stage(new_stage),
    }
}

fn trainer_sent_out(species: SpeciesId, bench_remaining: usize) -> BattleEvent {
    BattleEvent::TrainerSentOut {
        species,
        bench_remaining,
    }
}

fn splash(by_player: bool) -> [BattleEvent; 2] {
    [
        BattleEvent::UsedMove {
            by_player,
            move_id: SPLASH,
        },
        BattleEvent::NothingHappened {
            by_player,
            move_id: SPLASH,
        },
    ]
}

/// Meowth against `[Magikarp, Mightyena]`, both Splashing.
fn meowth_against_magikarp_then_mightyena(rng: &mut SequenceRng) -> Battle {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::MEOWTH, 50, &[SPLASH, TACKLE]);
    let party = vec![
        mon(&dex, SpeciesId::MAGIKARP, 1, &[SPLASH]),
        mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH]),
    ];
    Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, rng).expect("a valid battle")
}

#[test]
fn both_initial_holders_queue_in_battler_order_without_a_draw() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH]);
    let party = vec![mon(&dex, SpeciesId::SALAMENCE, 20, &[SPLASH])];
    assert_ne!(player.stats().speed, party[0].stats().speed);
    let mut rng = script(&[&C]);

    let battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).expect("battle");

    assert_eq!(
        battle.pending_entry_events,
        [intimidated(true, -1), intimidated(false, -1)]
    );
    assert_eq!(attack_stage(battle.player()), stage(-1));
    assert_eq!(attack_stage(battle.enemy()), stage(-1));
    assert!(!battle
        .battler_state(BattlerSide::Player)
        .intimidate_pending());
    assert!(!battle
        .battler_state(BattlerSide::Enemy)
        .intimidate_pending());
    rng.assert_exhausted();
}

#[test]
fn the_floor_is_silent_and_a_re_entry_queues_again() {
    let mut rng = script(&[&C, &U, &U, &U]);
    let mut battle = meowth_against_magikarp_then_mightyena(&mut rng);
    set_stage(&mut battle.player, ChangedStat::Attack, stage(-5));

    let first = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).expect("switch in");
    assert_eq!(
        first,
        [
            &[
                trainer_sent_out(SpeciesId::MIGHTYENA, 1),
                intimidated(false, -6)
            ],
            splash(true).as_slice(),
        ]
        .concat(),
        "Mightyena enters before the player's move and drops Attack to the floor"
    );
    assert_eq!(attack_stage(battle.player()), stage(-6));

    let out = turn_with(&mut battle, 0, EnemyAction::Switch(0), &mut rng).expect("switch out");
    assert!(!out
        .iter()
        .any(|e| matches!(e, BattleEvent::Intimidated { .. })));
    let back = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).expect("switch back");
    assert!(
        !back
            .iter()
            .any(|e| matches!(e, BattleEvent::Intimidated { .. })),
        "an Attack already at -6 prints nothing: {back:?}"
    );
    assert_eq!(attack_stage(battle.player()), stage(-6));
    assert!(!battle
        .battler_state(BattlerSide::Enemy)
        .intimidate_pending());
    rng.assert_exhausted();
}

#[test]
fn a_voluntary_re_entry_drops_attack_again_before_the_players_move() {
    let mut rng = script(&[&C, &U, &U, &U]);
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::MEOWTH, 50, &[SPLASH]);
    let party = vec![
        mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH]),
        mon(&dex, SpeciesId::MAGIKARP, 1, &[SPLASH]),
    ];
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).expect("battle");
    assert_eq!(attack_stage(battle.player()), stage(-1));

    let opening = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).expect("turn");
    assert_eq!(opening[0], intimidated(false, -1));

    let out = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).expect("switch out");
    assert_eq!(out[0], trainer_sent_out(SpeciesId::MAGIKARP, 1));
    let back = turn_with(&mut battle, 0, EnemyAction::Switch(0), &mut rng).expect("switch in");
    assert_eq!(
        back[..2],
        [
            trainer_sent_out(SpeciesId::MIGHTYENA, 1),
            intimidated(false, -2)
        ]
    );
    assert_eq!(
        back[2],
        BattleEvent::UsedMove {
            by_player: true,
            move_id: SPLASH
        }
    );
    assert_eq!(attack_stage(battle.player()), stage(-2));
    rng.assert_exhausted();
}

#[test]
fn prevention_precedes_the_floor_check() {
    for (species, ability) in [
        (SpeciesId::BELDUM, AbilityId::CLEAR_BODY),
        (SpeciesId::TORKOAL, AbilityId::WHITE_SMOKE),
        (SpeciesId::MAWILE, AbilityId::HYPER_CUTTER),
    ] {
        let dex = Dex::new();
        let player = mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH]);
        let enemy = mon(&dex, species, 20, &[SPLASH]);
        assert_eq!(enemy.ability(), ability);
        let mut rng = script(&[&C]);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).expect("battle");
        set_stage(&mut battle.enemy, ChangedStat::Attack, StatStage::MIN);
        battle.battler_states[BattlerSide::Player.index()] =
            BattlerState::at_battle_start(AbilityId::INTIMIDATE);

        let mut events = Vec::new();
        battle.resolve_intimidate_queue(&mut events);

        assert_eq!(
            events,
            [BattleEvent::IntimidatePrevented {
                by_player: true,
                ability
            }]
        );
        assert_eq!(attack_stage(battle.enemy()), StatStage::MIN);
        assert!(!battle
            .battler_state(BattlerSide::Player)
            .intimidate_pending());
        rng.assert_exhausted();
    }
}

#[test]
fn a_benched_enemy_holder_waits_then_drops_the_survivors_attack() {
    let mut rng = script(&[&C, &U, &H, &U]);
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::MEOWTH, 50, &[TACKLE, SPLASH]);
    let party = vec![
        at_one_hp(mon(&dex, SpeciesId::MAGIKARP, 1, &[SPLASH])),
        mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH]),
    ];
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).expect("battle");
    assert_eq!(
        attack_stage(battle.player()),
        stage(0),
        "the bench is silent"
    );

    let knockout = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).expect("turn");
    let tail = &knockout[knockout.len() - 2..];
    assert_eq!(
        tail,
        [
            trainer_sent_out(SpeciesId::MIGHTYENA, 0),
            intimidated(false, -1)
        ]
    );
    assert_eq!(attack_stage(battle.player()), stage(-1));

    let next = turn_with(&mut battle, 1, EnemyAction::Move(0), &mut rng).expect("turn");
    assert!(!next
        .iter()
        .any(|e| matches!(e, BattleEvent::Intimidated { .. })));
    assert_eq!(attack_stage(battle.player()), stage(-1));
    rng.assert_exhausted();
}

#[test]
fn both_replacements_enter_before_either_drop_resolves() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::MEOWTH, 50, &[SPLASH]);
    let reserves = vec![mon(&dex, SpeciesId::MIGHTYENA, 20, &[SPLASH])];
    let party = vec![
        mon(&dex, SpeciesId::MAGIKARP, 1, &[SPLASH]),
        mon(&dex, SpeciesId::SALAMENCE, 20, &[SPLASH]),
    ];
    let mut rng = script(&[&C]);
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex,
        player,
        reserves,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .expect("battle");
    battle.player.apply_damage(battle.player.stats().max_hp);
    battle.enemy.apply_damage(battle.enemy.stats().max_hp);

    let mut events = Vec::new();
    battle.handle_fainted_mons(&mut events).expect("replaced");

    assert_eq!(
        events,
        [
            BattleEvent::PlayerSentOut {
                species: SpeciesId::MIGHTYENA,
                reserves_remaining: 0
            },
            trainer_sent_out(SpeciesId::SALAMENCE, 0),
            intimidated(true, -1),
            intimidated(false, -1),
        ]
    );
    let mut again = Vec::new();
    battle.handle_fainted_mons(&mut again).expect("settled");
    assert!(again.is_empty(), "the queue drains once: {again:?}");
    rng.assert_exhausted();
}

#[test]
fn a_finished_battle_leaves_the_queue_untouched() {
    let mut rng = script(&[&C]);
    let mut battle = meowth_against_magikarp_then_mightyena(&mut rng);
    battle.battler_states[BattlerSide::Player.index()] =
        BattlerState::at_battle_start(AbilityId::INTIMIDATE);
    battle.player = fainted(battle.player.clone());

    let mut events = Vec::new();
    battle.resolve_intimidate_queue(&mut events);

    assert!(events.is_empty());
    assert!(battle
        .battler_state(BattlerSide::Player)
        .intimidate_pending());
    rng.assert_exhausted();
}
