//! Trace through the paths the public turn drives only partly: forced
//! replacements on both sides, which `take_turn` reaches, and an enemy switch
//! out and back, which no selector chooses. The enemy's action is fixed by the
//! test through the private seam, and every script is exact: copying adds no
//! draw.

use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};

use super::admission::ensure_participant_admissible;
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

// A Trace holder that gains a primary status while it holds a copied ability
// keeps that status on the trainer bench, so a re-copy on its return can pair
// the status with an ability that forbids it. Upstream never holds that state:
// `ABILITYEFFECT_IMMUNITY` cures it at move end (`battle_util.c:2873`-`:2951`),
// which this engine does not model, so the return is refused as admission
// refuses the same pairing at construction. No selector emits the enemy switch
// today; these tests drive it through the same private seam as above.

/// Draws of a status move that passes its accuracy check.
const S: [u16; 1] = [0];
/// Dragon Rage's accuracy draw and the discarded effect chance.
const D: [u16; 2] = [0, 0];

/// A trainer battle one step before the statused Trace holder returns: the
/// player is a Gastly lead at one HP (Levitate, so the enemy's first copy is
/// harmless) with `reserve` behind it; the enemy leads with a Porygon2 that
/// has already copied Levitate, has `status_move` used on it by `fixture`, and
/// has been switched out for a Rattata whose Dragon Rage then sends the
/// reserve out. The returned battle has the holder benched in slot 0 with its
/// status and native Trace, and the reserve active.
struct Reentry {
    battle: Battle,
    rng: SequenceRng,
}

fn reentry_fixture(
    reserve: BattlePokemon,
    status_move: MoveId,
    setup: &[&[u16]],
    last_turn: &[&[u16]],
) -> Reentry {
    let dex = Dex::new();
    let player = at_one_hp(mon(&dex, SpeciesId::GASTLY, 100, &[status_move, SPLASH]));
    let party = vec![
        tracer(&dex, 10),
        mon(&dex, SpeciesId::RATTATA, 1, &[MoveId::DRAGON_RAGE, SPLASH]),
    ];
    let mut rng = script(
        &[
            &[&C[..], &U[..]][..],
            setup,
            &[&U[..], &U[..], &D[..]],
            last_turn,
        ]
        .concat(),
    );
    let battle = Battle::new_trainer_with_player_reserves(
        dex,
        player,
        vec![reserve],
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut rng,
    )
    .expect("every pairing is admitted while the holder is healthy");
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
    Reentry { battle, rng }
}

/// Plays the shared prefix after the status lands: the holder switches out,
/// then Dragon Rage sends the reserve out.
fn bench_the_statused_holder(fixture: &mut Reentry) {
    let Reentry { battle, rng } = fixture;
    turn_with(battle, 1, EnemyAction::Switch(1), rng).expect("the holder leaves");
    let events = turn_with(battle, 1, EnemyAction::Move(0), rng).expect("Dragon Rage");
    assert!(events.contains(&BattleEvent::Fainted { by_player: true }));
    assert_eq!(
        battle.trainer().unwrap().bench()[0].ability(),
        AbilityId::TRACE
    );
}

#[test]
fn a_paralysed_trace_holder_cannot_recopy_limber_on_its_return() {
    let dex = Dex::new();
    let reserve = mon(&dex, SpeciesId::PERSIAN, 100, &[SPLASH]);
    assert_eq!(reserve.ability(), AbilityId::LIMBER);
    let mut fixture = reentry_fixture(reserve, MoveId::THUNDER_WAVE, &[&S, &U], &[&U]);
    let Reentry { battle, rng } = &mut fixture;

    let events = turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Thunder Wave");
    assert!(events.contains(&BattleEvent::Paralyzed {
        by_player: true,
        move_id: MoveId::THUNDER_WAVE,
    }));
    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
    bench_the_statused_holder(&mut fixture);
    let Reentry { battle, rng } = &mut fixture;
    assert_eq!(battle.player().ability(), AbilityId::LIMBER);
    let holder = battle.trainer().unwrap().bench()[0].clone();
    assert_eq!(holder.status1(), Status1::Paralysed);
    let before = battle.clone();

    let refused = turn_with(battle, 0, EnemyAction::Switch(0), rng);

    // Copying Limber here would give the oracle's own refused pairing.
    let mut projected = holder;
    projected.trace_ability(AbilityId::LIMBER);
    assert_eq!(
        ensure_participant_admissible(&projected),
        Err(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    assert_eq!(
        refused,
        Err(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    assert_eq!(battle.enemy(), before.enemy());
    assert_eq!(
        battle.trainer().unwrap().bench(),
        before.trainer().unwrap().bench()
    );
    rng.assert_exhausted();
}

#[test]
fn a_yawned_to_sleep_trace_holder_cannot_recopy_a_sleep_immunity_on_its_return() {
    for (species, ability) in [
        (SpeciesId::DROWZEE, AbilityId::INSOMNIA),
        (SpeciesId::MANKEY, AbilityId::VITAL_SPIRIT),
    ] {
        let dex = Dex::new();
        let reserve = mon(&dex, species, 100, &[SPLASH]);
        assert_eq!(reserve.ability(), ability);
        let mut fixture = reentry_fixture(reserve, MoveId::YAWN, &[&U, &[0]], &[&U]);
        let Reentry { battle, rng } = &mut fixture;

        let drowsy = turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Yawn");
        assert!(drowsy.contains(&BattleEvent::MadeDrowsy {
            by_player: true,
            move_id: MoveId::YAWN,
        }));
        let asleep = turn_with(battle, 1, EnemyAction::Move(0), rng).expect("the yawn lands");
        assert!(asleep.contains(&BattleEvent::FellAsleepFromYawn { by_player: false }));
        assert!(battle.enemy().status1().is_asleep());
        assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
        bench_the_statused_holder(&mut fixture);
        let Reentry { battle, rng } = &mut fixture;
        assert_eq!(battle.player().ability(), ability);
        let before = battle.clone();

        let refused = turn_with(battle, 0, EnemyAction::Switch(0), rng);

        assert_eq!(
            refused,
            Err(BattleError::UnportedAbilityInteraction(ability))
        );
        assert_eq!(battle.enemy(), before.enemy());
        assert_eq!(
            battle.trainer().unwrap().bench(),
            before.trainer().unwrap().bench()
        );
        rng.assert_exhausted();
    }
}

#[test]
fn a_statused_trace_holder_still_recopies_an_ability_its_status_allows() {
    let dex = Dex::new();
    let reserve = mon(&dex, SpeciesId::GASTLY, 100, &[SPLASH]);
    let mut fixture = reentry_fixture(reserve, MoveId::THUNDER_WAVE, &[&S, &U], &[&U]);
    let Reentry { battle, rng } = &mut fixture;
    turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Thunder Wave");
    bench_the_statused_holder(&mut fixture);
    let Reentry { battle, rng } = &mut fixture;

    let back = turn_with(battle, 0, EnemyAction::Switch(0), rng).expect("Levitate is allowed");

    assert!(back.contains(&traced(false, AbilityId::LEVITATE)));
    assert_eq!(battle.enemy().ability(), AbilityId::LEVITATE);
    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    rng.assert_exhausted();
}

#[test]
fn a_refused_trace_return_spends_no_draw_and_changes_nothing() {
    let dex = Dex::new();
    let reserve = mon(&dex, SpeciesId::PERSIAN, 100, &[SPLASH]);
    let mut fixture = reentry_fixture(reserve, MoveId::THUNDER_WAVE, &[&S, &U], &[]);
    let Reentry { battle, rng } = &mut fixture;
    turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Thunder Wave");
    bench_the_statused_holder(&mut fixture);
    let Reentry { battle, rng } = &mut fixture;
    rng.assert_exhausted();
    let before = format!("{battle:?}");
    let refused = Err(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER));

    // The selected action is refused before it reorders, draws, or switches.
    let action = battle
        .validate_player_action(PlayerAction::UseMove(0))
        .unwrap();
    let mut events = Vec::new();
    let mut empty = script(&[]);
    assert_eq!(
        battle.resolve_selected_actions(action, EnemyAction::Switch(0), &mut empty, &mut events),
        refused
    );
    // So is the switch itself, which a forced replacement also reaches.
    assert_eq!(battle.switch_enemy_to(0, &mut events), refused);

    assert!(events.is_empty());
    assert_eq!(format!("{battle:?}"), before);
}

#[test]
fn a_forced_replacement_cannot_send_in_a_statused_trace_holder_that_copies_a_cure() {
    let dex = Dex::new();
    let reserve = mon(&dex, SpeciesId::PERSIAN, 100, &[TACKLE, SPLASH]);
    let mut fixture = reentry_fixture(reserve, MoveId::THUNDER_WAVE, &[&S, &U], &[&U, &H]);
    let Reentry { battle, rng } = &mut fixture;
    turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Thunder Wave");
    bench_the_statused_holder(&mut fixture);
    let Reentry { battle, rng } = &mut fixture;

    // Tackle faints the Rattata; the only member left to replace it is the
    // paralysed Trace holder, and Persian's Limber would be its copy.
    let refused = turn_with(battle, 0, EnemyAction::Move(1), rng);

    assert_eq!(
        refused,
        Err(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    assert!(battle.enemy().is_fainted());
    assert_eq!(battle.enemy().species(), SpeciesId::RATTATA);
    let holder = &battle.trainer().unwrap().bench()[0];
    assert_eq!(holder.status1(), Status1::Paralysed);
    assert_eq!(holder.ability(), AbilityId::TRACE);
    rng.assert_exhausted();
}

const PSYBEAM: MoveId = MoveId::PSYBEAM;

/// A settled holder never copies again, so no ability change can reach the
/// player's attacker after its move was screened at selection.
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

/// A queued holder waits only on an opponent with no ability to copy, so
/// this keeps a player holder from still being queued at an enemy switch.
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

/// The refused replacement leaves the fainted Rattata active, so a later
/// turn reaches the faint pass again; `Cmd_getexp` pays one faint once.
#[test]
fn a_refused_forced_replacement_does_not_pay_the_faint_again_on_a_later_turn() {
    let dex = Dex::new();
    let reserve = mon(&dex, SpeciesId::PERSIAN, 99, &[TACKLE, SPLASH]);
    let mut fixture = reentry_fixture(reserve, MoveId::THUNDER_WAVE, &[&S, &U], &[&U, &H]);
    let Reentry { battle, rng } = &mut fixture;
    turn_with(battle, 0, EnemyAction::Move(0), rng).expect("Thunder Wave");
    bench_the_statused_holder(&mut fixture);
    let Reentry { battle, rng } = &mut fixture;
    let before = battle.player().experience();

    let refused = turn_with(battle, 0, EnemyAction::Move(1), rng);
    assert_eq!(
        refused,
        Err(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    let paid = battle.player().experience();
    assert!(paid > before, "the faint pays once");

    let mut retry_rng = SequenceRng::new(vec![0; 64]);
    let retry = battle
        .take_turn(PlayerAction::UseMove(0), &mut retry_rng)
        .expect_err("the replacement is refused again");
    assert_eq!(
        retry.error,
        BattleError::UnportedAbilityInteraction(AbilityId::LIMBER)
    );
    assert!(!retry
        .events
        .iter()
        .any(|event| matches!(event, BattleEvent::ExpGained { .. })));
    assert_eq!(battle.player().experience(), paid);
}
