//! Trace through the paths no public action reaches: forced replacements on
//! both sides and an enemy switch out and back. The enemy's action is fixed
//! by the test through the private seam, and every script is exact: copying
//! adds no draw.

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

fn tracer(dex: &Dex, level: u8) -> BattlePokemon {
    let tracer = mon(dex, SpeciesId::PORYGON2, level, &[SPLASH]);
    assert_eq!(tracer.native_ability(), AbilityId::TRACE);
    tracer
}

fn at_one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - 1);
    mon
}

fn poisoned_at_one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.set_status1(Status1::Poisoned);
    at_one_hp(mon)
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

fn traced(by_player: bool, ability: AbilityId) -> BattleEvent {
    BattleEvent::TracedAbility { by_player, ability }
}

#[test]
fn a_player_replacement_copies_the_wild_opponents_ability_after_its_send_out() {
    let dex = Dex::new();
    let player = at_one_hp(mon(&dex, SpeciesId::RATTATA, 5, &[SPLASH]));
    let reserve = tracer(&dex, 5);
    let enemy = mon(&dex, SpeciesId::GASTLY, 100, &[TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let mut rng = script(&[&C, &U, &H]);
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();

    let events = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        events[events.len() - 3..],
        [
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId::PORYGON2,
                reserves_remaining: 0,
            },
            traced(true, AbilityId::LEVITATE),
        ]
    );
    assert_eq!(battle.player().ability(), AbilityId::LEVITATE);
    assert_eq!(battle.player().native_ability(), AbilityId::TRACE);
    rng.assert_exhausted();
}

#[test]
fn a_trainer_replacement_copies_the_players_ability_after_its_send_out() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::GASTLY, 100, &[TACKLE]);
    let lead = at_one_hp(mon(&dex, SpeciesId::RATTATA, 1, &[TACKLE]));
    let party = vec![lead, tracer(&dex, 10)];
    let mut rng = script(&[&C, &U, &H]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let events = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        events[events.len() - 3..],
        [
            BattleEvent::Fainted { by_player: false },
            BattleEvent::TrainerSentOut {
                species: SpeciesId::PORYGON2,
                bench_remaining: 0,
            },
            traced(false, AbilityId::LEVITATE),
        ]
    );
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
    rng.assert_exhausted();
}

#[test]
fn a_simultaneous_double_faint_sends_both_out_before_either_copy() {
    let dex = Dex::new();
    let player = poisoned_at_one_hp(mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]));
    let lead = poisoned_at_one_hp(mon(&dex, SpeciesId::GASTLY, 90, &[SPLASH]));
    let party = vec![lead, tracer(&dex, 11)];
    let mut rng = script(&[&C, &U]);
    let mut battle = Battle::new_trainer_with_player_reserves(
        dex.clone(),
        player,
        vec![tracer(&dex, 10)],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .unwrap();

    let events = turn_with(&mut battle, 0, EnemyAction::Move(0), &mut rng).unwrap();
    assert_eq!(
        events[events.len() - 4..],
        [
            BattleEvent::PlayerSentOut {
                species: SpeciesId::PORYGON2,
                reserves_remaining: 0,
            },
            BattleEvent::TrainerSentOut {
                species: SpeciesId::PORYGON2,
                bench_remaining: 0,
            },
            traced(true, AbilityId::TRACE),
            traced(false, AbilityId::TRACE),
        ]
    );
    rng.assert_exhausted();
}

#[test]
fn a_holder_switched_out_and_back_reloads_its_native_ability_and_copies_again() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]);
    let party = vec![
        tracer(&dex, 10),
        mon(&dex, SpeciesId::RATTATA, 1, &[SPLASH]),
    ];
    let mut rng = script(&[&C, &U, &U]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);

    let out = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).unwrap();
    assert!(!out
        .iter()
        .any(|e| matches!(e, BattleEvent::TracedAbility { .. })));
    let benched = &battle.trainer().unwrap().bench()[0];
    assert_eq!(
        benched.ability(),
        AbilityId::TRACE,
        "the bench holds the native slot"
    );
    assert_eq!(benched.ability_slot(), 0);

    let back = turn_with(&mut battle, 0, EnemyAction::Switch(0), &mut rng).unwrap();
    assert!(back.contains(&traced(false, AbilityId::LEVITATE)));
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
    rng.assert_exhausted();
}

const PSYBEAM: MoveId = MoveId::PSYBEAM;

/// #3455: the player's chance move is screened at selection
/// (`validate_player_action`) and again against the incoming defender, but
/// never against a later ability change of its own attacker. The sequence
/// that would need that re-screen (a Trace holder copying Serene Grace
/// *after* selection) cannot be built from the engine's own transitions: the
/// holder copies the lead's ability at battle start, settles, and a settled
/// holder never copies again, so the enemy's switch-in leaves it alone.
#[test]
fn a_settled_player_trace_holder_keeps_its_copy_when_the_enemy_switches_to_serene_grace() {
    let dex = Dex::new();
    let player = mon(&dex, SpeciesId::PORYGON2, 100, &[PSYBEAM]);
    let lead = mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]);
    let serene = BattlePokemon::new(
        &dex,
        SpeciesId::CHANSEY,
        100,
        fixed_ivs(255),
        1,
        vec![SPLASH],
    )
    .expect("dex-resident");
    assert_eq!(serene.ability(), AbilityId::SERENE_GRACE);
    // Accuracy, non-critical, damage roll, then an effect chance of 99 that
    // misses Psybeam's 10 percent.
    let mut rng = script(&[&C, &U, &[0, 1, 0, 99]]);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        vec![lead, serene],
        &mut rng,
    )
    .unwrap();
    assert_eq!(battle.player().native_ability(), AbilityId::TRACE);
    assert_eq!(battle.player().ability(), AbilityId::LEVITATE);
    assert!(!battle.battler_state(BattlerSide::Player).trace_pending());

    let events = turn_with(&mut battle, 0, EnemyAction::Switch(1), &mut rng).unwrap();

    assert_eq!(battle.enemy().ability(), AbilityId::SERENE_GRACE);
    assert_eq!(battle.player().ability(), AbilityId::LEVITATE);
    assert!(!events.iter().any(|event| matches!(
        event,
        BattleEvent::TracedAbility {
            by_player: true,
            ..
        }
    )));
    rng.assert_exhausted();
}

/// A queued Trace holder stays queued while its opponent has no ability to
/// copy or is fainted, and the drain itself is deferred by a finished battle
/// or a pending move-learn prompt (`trace.rs`, `resolve_trace_queue` and
/// `settle_next_trace`). Each deferral ends before the next turn's actions,
/// and no battleable species is ability-less, so a player Trace holder is
/// never still queued when the enemy switches mid-turn.
#[test]
fn every_battleable_species_has_an_ability_for_trace_to_copy() {
    let dex = Dex::new();
    let mut checked = 0;
    let count = u16::try_from(assets::species::SPECIES_COUNT).expect("species ids fit in u16");
    for id in 1..count {
        let species = SpeciesId(id);
        if BattlePokemon::new(&dex, species, 5, fixed_ivs(0), 0, vec![SPLASH]).is_err() {
            continue;
        }
        let mon = mon(&dex, species, 5, &[SPLASH]);
        assert_ne!(mon.native_ability(), AbilityId::NONE, "{species:?}");
        checked += 1;
    }
    assert!(checked > 380, "{checked}");
}
