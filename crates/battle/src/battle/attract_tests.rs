//! Infatuation clearing on switches and faints, through the private seams no
//! public action reaches: a trainer switch, a player reserve's send-out and
//! the direct faint clear the drain and recoil paths use.

use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};

use super::trainer::fixed_ivs;
use super::{Battle, BattleEvent, BattlerSide, EnemyAction, PlayerAction};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::pokemon::BattlePokemon;

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const MEOWTH: SpeciesId = SpeciesId(52);
const PICHU: SpeciesId = SpeciesId(172);
const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);

/// Replays zero for every draw, so an odd love coin never arises.
struct ZeroRng;

impl BattleRng for ZeroRng {
    fn next_u16(&mut self) -> u16 {
        0
    }
}

fn mon(species: SpeciesId, level: u8) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        level,
        fixed_ivs(255),
        0,
        vec![MoveId::TACKLE],
    )
    .expect("dex-resident")
}

fn trainer_battle(reserves: Vec<BattlePokemon>) -> Battle {
    Battle::new_trainer_with_player_reserves(
        Dex::new(),
        mon(MEOWTH, 5),
        reserves,
        MAY_ROUTE_103_MUDKIP,
        vec![mon(TREECKO, 50), mon(TORCHIC, 50)],
        &mut ZeroRng,
    )
    .expect("a valid battle")
}

fn switch_turn(battle: &mut Battle, slot: usize) -> Vec<BattleEvent> {
    let action = battle
        .validate_player_action(PlayerAction::UseMove(0))
        .expect("a legal move");
    battle.start_turn(&mut ZeroRng);
    let mut events = Vec::new();
    battle
        .resolve_selected_actions(action, EnemyAction::Switch(slot), &mut ZeroRng, &mut events)
        .expect("a healthy slot");
    events
}

#[test]
fn the_attractor_switching_out_frees_the_battler_before_its_action() {
    let mut battle = trainer_battle(Vec::new());
    battle
        .player
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Enemy);
    battle
        .player
        .volatiles_mut()
        .set_seeded_by(BattlerSide::Enemy);

    let events = switch_turn(&mut battle, 1);

    assert!(!battle.player().volatiles().infatuated());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::InLove { .. })),
        "the player's action runs after the switch: {events:?}"
    );
    assert!(
        battle.player().volatiles().seeded(),
        "Leech Seed outlives its seeder's switch"
    );
}

#[test]
fn an_infatuated_battler_switching_out_leaves_its_replacement_free() {
    let mut battle = trainer_battle(Vec::new());
    battle
        .enemy
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Player);

    switch_turn(&mut battle, 1);

    assert!(!battle.enemy().volatiles().infatuated());
}

#[test]
fn a_player_reserve_replacing_the_attractor_frees_the_enemy() {
    let mut battle = trainer_battle(vec![mon(PICHU, 5)]);
    battle
        .enemy
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Player);
    battle.enemy.volatiles_mut().set_confusion(3);
    battle.player.apply_damage(battle.player.stats().max_hp);

    assert!(battle.send_out_next_player_reserve(&mut Vec::new()));

    assert!(!battle.enemy().volatiles().infatuated());
    assert!(
        battle.enemy().volatiles().confused(),
        "only the infatuation is released"
    );
}

#[test]
fn a_player_reserve_enters_free_of_the_infatuation_that_held_its_predecessor() {
    let mut battle = trainer_battle(vec![mon(PICHU, 5)]);
    battle
        .player
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Enemy);
    battle.player.apply_damage(battle.player.stats().max_hp);

    assert!(battle.send_out_next_player_reserve(&mut Vec::new()));

    assert!(!battle.player().volatiles().infatuated());
}

#[test]
fn a_direct_faint_clear_frees_the_opposing_battler_and_keeps_its_other_state() {
    for fainted_is_player in [true, false] {
        let mut battle = trainer_battle(Vec::new());
        let fainted_side = BattlerSide::from_is_player(fainted_is_player);
        let holder_side = BattlerSide::from_is_player(!fainted_is_player);
        {
            let holder = if fainted_is_player {
                &mut battle.enemy
            } else {
                &mut battle.player
            };
            holder.volatiles_mut().set_infatuated_by(fainted_side);
            holder.volatiles_mut().set_seeded_by(fainted_side);
        }

        battle.clear_fainted_battler_state(fainted_is_player);

        let holder = if fainted_is_player {
            battle.enemy()
        } else {
            battle.player()
        };
        assert!(!holder.volatiles().infatuated(), "{holder_side:?}");
        assert!(holder.volatiles().seeded(), "{holder_side:?}");
    }
}
