//! Unit tests for [`super`]'s stand-in answer to the level-up replacement
//! prompt (S-6, issue #304) — and, more importantly, for the wiring: a
//! headless driver must *answer* the question, because an unanswered one
//! wedges the battle it was asked in.

use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, Ivs, PpBonuses};
use engine::rng::Rng;

use super::settle_move_learn_prompts;
use crate::flow::npc_trainer_battle::advance_npc_trainer_battle;
use crate::flow::wild_encounter::SharedRng;

/// `TRAINER_MAY_ROUTE_103_MUDKIP` — the same real trainer the `battle`
/// crate's own trainer-battle pins use.
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);
const POUND: MoveId = MoveId(1);
const SCRATCH: MoveId = MoveId(10);
const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const TACKLE: MoveId = MoveId(33);
/// `MOVE_PECK` — Torchic's level-16 learnset entry.
const PECK: MoveId = MoveId(64);

/// A level-15 Torchic whose four slots are full and which is one experience
/// point short of level 16 — so the very next award crosses the threshold
/// and offers Peck with nowhere to put it.
fn torchic_one_point_from_a_full_moveset_level_up(dex: &Dex) -> BattlePokemon {
    let mut mon = BattlePokemon::new(
        dex,
        TORCHIC,
        15,
        Ivs::default(),
        0,
        vec![SCRATCH, GROWL, TACKLE, LEER],
    )
    .expect("a four-move Torchic is representable");
    let growth_rate = dex.species(TORCHIC).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    assert!(
        mon.apply_experience(dex, level_16 - 1 - mon.experience())
            .unwrap()
            .is_none(),
        "fixture sanity: stopping short of the threshold crosses no level"
    );
    mon
}

fn two_treecko_party(dex: &Dex) -> Vec<BattlePokemon> {
    (0..2)
        .map(|_| BattlePokemon::new(dex, TREECKO, 5, Ivs::default(), 0, vec![POUND, LEER]).unwrap())
        .collect()
}

/// The wiring, end to end through a real driver: the first knockout levels
/// the player into a full-moveset prompt *mid-battle*, and the battle keeps
/// playing. Without the driver's own
/// [`settle_move_learn_prompts`] call, the next turn would be refused with
/// `BattleError::MoveLearnPending` and the driver would abandon the fight —
/// so this pins the answer being given, not merely being available.
#[test]
fn an_npc_trainer_battle_answers_a_mid_battle_prompt_and_plays_on() {
    let dex = Dex::new();
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex);
    let original_moves: Vec<MoveId> = player.moves().iter().map(|slot| slot.move_id).collect();
    let party = two_treecko_party(&dex);

    let mut rng = Rng::new(1);
    let battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut SharedRng::new(&mut rng),
    )
    .expect("a two-mon Treecko party is one this engine can fight");
    let mut slot = Some(battle);
    let mut lead = None;
    let mut money = 0;

    let mut outcome = None;
    for _ in 0..60 {
        outcome = advance_npc_trainer_battle(&mut slot, &mut lead, &mut money, &mut rng);
        if outcome.is_some() {
            break;
        }
    }

    assert_eq!(
        outcome,
        Some(BattleOutcome::PlayerWon),
        "an unanswered prompt would have stalled the battle instead"
    );
    assert!(slot.is_none());
    let lead = lead.expect("the driver writes the player's mon back");
    assert!(lead.level() >= 16, "the award crossed the threshold");
    assert_eq!(
        lead.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        original_moves,
        "the stand-in answer is DECLINE, so the player's moveset is untouched"
    );
}

/// The answer itself, reported: one [`BattleEvent::MoveLearnDeclined`] per
/// prompt, then the aftermath the prompt deferred, and nothing left pending
/// afterwards.
#[test]
fn settling_declines_every_pending_prompt_and_reports_each_one() {
    let dex = Dex::new();
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex);
    let party = two_treecko_party(&dex);
    let mut rng = Rng::new(1);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut SharedRng::new(&mut rng),
    )
    .unwrap();

    // Play until the first knockout raises the prompt.
    for _ in 0..60 {
        let events = battle
            .take_turn(
                battle::PlayerAction::UseMove(0),
                &mut SharedRng::new(&mut rng),
            )
            .expect("no turn before the prompt can fail");
        if events
            .iter()
            .any(|event| matches!(event, BattleEvent::MoveLearnPrompt { .. }))
        {
            break;
        }
    }
    assert_eq!(
        battle.pending_move_learn().map(|pending| pending.move_id()),
        Some(PECK),
        "fixture sanity: the fight must have reached the prompt"
    );

    let events = settle_move_learn_prompts(&mut battle);

    assert_eq!(
        events,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            // The answer also releases the aftermath the prompt was holding
            // back -- here, the deferred forced send-out (upstream finishes
            // the level-up script before `HandleFaintedMonActions`' case 4
            // sends out the replacement).
            BattleEvent::TrainerSentOut {
                species: TREECKO,
                bench_remaining: 0,
            },
        ]
    );
    assert!(battle.pending_move_learn().is_none());
    assert!(
        settle_move_learn_prompts(&mut battle).is_empty(),
        "settling again with nothing pending is a no-op, not an error"
    );
}

/// Declining does not touch the `ppBonuses` byte: only a replacement clears
/// a slot's PP Ups (`RemoveMonPPBonus`).
#[test]
fn declining_leaves_every_slots_pp_ups_alone() {
    let dex = Dex::new();
    let bonuses = PpBonuses::from_bits(0b0000_1111);
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex)
        .with_pp_bonuses(&dex, bonuses)
        .unwrap();
    let party = two_treecko_party(&dex);
    let mut rng = Rng::new(1);
    let battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut SharedRng::new(&mut rng),
    )
    .unwrap();
    let mut slot = Some(battle);
    let mut lead = None;
    let mut money = 0;

    for _ in 0..60 {
        if advance_npc_trainer_battle(&mut slot, &mut lead, &mut money, &mut rng).is_some() {
            break;
        }
    }

    let lead = lead.expect("the driver writes the player's mon back");
    assert_eq!(lead.pp_bonuses(), bonuses);
}

/// The stand-in costs no RNG, so a battle that raises a prompt sits at the
/// same point in the shared stream as one that does not — the property
/// every `crate::flow` handoff is written around.
#[test]
fn settling_a_prompt_draws_nothing_from_the_shared_stream() {
    let dex = Dex::new();
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex);
    let party = two_treecko_party(&dex);
    let mut rng = Rng::new(7);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut SharedRng::new(&mut rng),
    )
    .unwrap();
    for _ in 0..60 {
        let events = battle
            .take_turn(
                battle::PlayerAction::UseMove(0),
                &mut SharedRng::new(&mut rng),
            )
            .unwrap();
        if events
            .iter()
            .any(|event| matches!(event, BattleEvent::MoveLearnPrompt { .. }))
        {
            break;
        }
    }
    assert!(battle.pending_move_learn().is_some());

    let before = rng.state();
    let _ = settle_move_learn_prompts(&mut battle);
    assert_eq!(
        rng.state(),
        before,
        "answering the prompt must not move the stream"
    );
}

/// A prompt raised by the *final* knockout defers the money payout into the
/// settlement's own events (`Cmd_getmoneyreward` runs only after the
/// level-up script, ask included, has finished), so the driver must credit
/// `MoneyGained` from there too — a wallet that only scanned the turn's
/// events would silently drop the prize.
#[test]
fn a_prize_deferred_behind_the_final_knockouts_prompt_is_still_credited() {
    let dex = Dex::new();
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex);
    // One-mon party: the knockout that raises the prompt is also the one
    // that decides the battle, so the payout waits on the answer.
    let party =
        vec![BattlePokemon::new(&dex, TREECKO, 5, Ivs::default(), 0, vec![POUND, LEER]).unwrap()];
    let expected_money = {
        let mut rng = Rng::new(1);
        Battle::new_trainer(
            dex.clone(),
            player.clone(),
            MAY_ROUTE_103_MUDKIP,
            party.clone(),
            &mut SharedRng::new(&mut rng),
        )
        .unwrap()
        .trainer()
        .expect("a trainer battle")
        .money()
    };

    let mut rng = Rng::new(1);
    let battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        party,
        &mut SharedRng::new(&mut rng),
    )
    .unwrap();
    let mut slot = Some(battle);
    let mut lead = None;
    let mut money = 0;

    let mut outcome = None;
    for _ in 0..60 {
        outcome = advance_npc_trainer_battle(&mut slot, &mut lead, &mut money, &mut rng);
        if outcome.is_some() {
            break;
        }
    }

    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    assert_eq!(
        money, expected_money,
        "the deferred MoneyGained must reach the wallet"
    );
    assert_ne!(money, 0, "fixture sanity: the trainer pays a real prize");
}
