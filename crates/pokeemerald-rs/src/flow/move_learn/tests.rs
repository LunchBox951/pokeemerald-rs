use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, Ivs, PpBonuses};
use engine::rng::Rng;

use super::settle_move_learn_prompts;
use crate::flow::npc_trainer_battle::advance_npc_trainer_battle;
use crate::flow::wild_encounter::SharedRng;

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);
const POUND: MoveId = MoveId(1);
const SCRATCH: MoveId = MoveId(10);
const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const TACKLE: MoveId = MoveId(33);
const PECK: MoveId = MoveId(64);
const MAX_TEST_TURNS: usize = 60;

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

fn play_until_move_learn_prompt(battle: &mut Battle, rng: &mut Rng) {
    for _ in 0..MAX_TEST_TURNS {
        let events = battle
            .take_turn(battle::PlayerAction::UseMove(0), &mut SharedRng::new(rng))
            .expect("no turn before the prompt can fail");
        if events
            .iter()
            .any(|event| matches!(event, BattleEvent::MoveLearnPrompt { .. }))
        {
            return;
        }
    }
    panic!("battle did not reach a move-learning prompt within {MAX_TEST_TURNS} turns");
}

#[test]
fn npc_trainer_driver_declines_a_mid_battle_prompt_and_finishes() {
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
    for _ in 0..MAX_TEST_TURNS {
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

#[test]
fn settling_declines_the_prompt_before_releasing_the_deferred_send_out() {
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

    play_until_move_learn_prompt(&mut battle, &mut rng);
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

#[test]
fn declining_a_prompt_preserves_the_pp_up_bits() {
    let dex = Dex::new();
    let bonuses = PpBonuses::from_bits(0b0000_1111);
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex)
        .with_pp_bonuses(&dex, bonuses)
        .unwrap();
    let original_moves: Vec<MoveId> = player.moves().iter().map(|slot| slot.move_id).collect();
    let party = two_treecko_party(&dex);

    // Prove the PP-Up fixture actually reaches and settles the Peck prompt;
    // otherwise the preservation assertions below would be vacuous.
    let mut probe_rng = Rng::new(1);
    let mut probe = Battle::new_trainer(
        dex.clone(),
        player.clone(),
        MAY_ROUTE_103_MUDKIP,
        party.clone(),
        &mut SharedRng::new(&mut probe_rng),
    )
    .unwrap();
    play_until_move_learn_prompt(&mut probe, &mut probe_rng);
    assert_eq!(
        probe.pending_move_learn().map(|pending| pending.move_id()),
        Some(PECK),
        "fixture sanity: the PP-Up fixture reaches the Peck prompt"
    );
    assert_eq!(
        settle_move_learn_prompts(&mut probe).first(),
        Some(&BattleEvent::MoveLearnDeclined { move_id: PECK }),
        "settling the prompt declines it"
    );
    assert_eq!(
        probe.player().pp_bonuses(),
        bonuses,
        "declining leaves the PP Ups alone"
    );

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
    for _ in 0..MAX_TEST_TURNS {
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
    let lead = lead.expect("the driver writes the player's mon back");
    assert!(lead.level() >= 16, "the award crossed the threshold");
    assert_eq!(
        lead.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        original_moves,
        "the declined prompt left the moveset untouched"
    );
    assert_eq!(lead.pp_bonuses(), bonuses);
}

#[test]
fn settling_a_prompt_does_not_advance_the_shared_rng() {
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
    play_until_move_learn_prompt(&mut battle, &mut rng);
    assert!(battle.pending_move_learn().is_some());

    let before = rng.state();
    let _ = settle_move_learn_prompts(&mut battle);
    assert_eq!(
        rng.state(),
        before,
        "answering the prompt must not move the stream"
    );
}

#[test]
fn npc_trainer_driver_credits_prize_released_by_the_final_prompt() {
    let dex = Dex::new();
    let player = torchic_one_point_from_a_full_moveset_level_up(&dex);
    let final_knockout_party =
        vec![BattlePokemon::new(&dex, TREECKO, 5, Ivs::default(), 0, vec![POUND, LEER]).unwrap()];
    let expected_money = {
        let mut rng = Rng::new(1);
        Battle::new_trainer(
            dex.clone(),
            player.clone(),
            MAY_ROUTE_103_MUDKIP,
            final_knockout_party.clone(),
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
        final_knockout_party,
        &mut SharedRng::new(&mut rng),
    )
    .unwrap();
    let mut slot = Some(battle);
    let mut lead = None;
    let mut money = 0;

    let mut outcome = None;
    for _ in 0..MAX_TEST_TURNS {
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
