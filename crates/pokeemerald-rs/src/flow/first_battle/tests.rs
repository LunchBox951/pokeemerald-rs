use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, Ivs, PlayerAction};
use engine::rng::Rng;

use super::{
    advance_first_battle, start_first_battle, FIRST_BATTLE_OPPONENT_LEVEL,
    FIRST_BATTLE_OPPONENT_SPECIES,
};

const TREECKO: SpeciesId = SpeciesId(277);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const POUND: MoveId = MoveId(1);
const FIRST_MOVE_SLOT: usize = 0;
const FIXED_PLAYER_PERSONALITY: u32 = 0;
const SCRIPTED_OPPONENT_LEVEL: u8 = 2;
const DOMINANT_PLAYER_LEVEL: u8 = 50;
const GROWL_SCENARIO_PLAYER_LEVEL: u8 = 2;
const DEFAULT_RNG_SEED: u32 = 1;
const GROWL_SCENARIO_RNG_SEED: u32 = 2;
const MAX_HEADLESS_TURNS: usize = 200;
const IV_BIT_MASK: u16 = 0x1F;
const SP_DEFENSE_IV_SHIFT: u32 = 10;

/// A placeholder save-owner id for scenarios that exercise something other
/// than opponent OT assignment (`opponent_ot_id_tests` pins that directly).
const TEST_PLAYER_TRAINER_ID: u32 = 0x1234_5678;

fn max_iv_player_mon(species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(
        &Dex::new(),
        species,
        level,
        ivs,
        FIXED_PLAYER_PERSONALITY,
        moves,
    )
    .expect("player mon: species/moves must be in the dex")
}

#[test]
fn start_first_battle_builds_the_fixed_opponent_with_first_battle_rules() {
    let mut rng = Rng::new(DEFAULT_RNG_SEED);
    let lead = max_iv_player_mon(TREECKO, DOMINANT_PLAYER_LEVEL, vec![POUND]);
    let mut battle = start_first_battle(lead, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("construction must succeed");

    assert_eq!(FIRST_BATTLE_OPPONENT_SPECIES, ZIGZAGOON);
    assert_eq!(FIRST_BATTLE_OPPONENT_LEVEL, SCRIPTED_OPPONENT_LEVEL);
    assert_eq!(battle.enemy().species(), ZIGZAGOON);
    assert_eq!(battle.enemy().level(), SCRIPTED_OPPONENT_LEVEL);
    assert!(!battle.enemy().is_fainted());
    let opponent_moves: Vec<MoveId> = battle
        .enemy()
        .moves()
        .iter()
        .map(|known_move| known_move.move_id)
        .collect();
    assert_eq!(
        opponent_moves,
        vec![TACKLE, GROWL],
        "a level-2 Zigzagoon's real level-up learnset is Tackle + Growl"
    );

    let failure = battle
        .take_turn(PlayerAction::Run, &mut super::SharedRng::new(&mut rng))
        .unwrap_err();
    assert_eq!(
        failure.error(),
        BattleError::RunForbidden,
        "first_battle must have reached Battle::new as true"
    );
}

#[test]
fn start_first_battle_preserves_the_frame_free_upstream_rng_order() {
    let mut rng = Rng::new(DEFAULT_RNG_SEED);
    let lead = max_iv_player_mon(TREECKO, DOMINANT_PLAYER_LEVEL, vec![POUND]);
    let battle = start_first_battle(lead, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("construction must succeed");

    let mut expected_rng = Rng::new(DEFAULT_RNG_SEED);
    let expected_personality = expected_rng.next_u32();
    let first_iv_draw = expected_rng.next_u16();
    let second_iv_draw = expected_rng.next_u16();
    let _unmodelled_held_item_draw = expected_rng.next_u16();
    let expected_turn_number = expected_rng.next_u16();

    assert_eq!(battle.enemy().personality(), expected_personality);
    assert_eq!(battle.enemy().ivs().hp, (first_iv_draw & IV_BIT_MASK) as u8);
    assert_eq!(
        battle.enemy().ivs().sp_defense,
        ((second_iv_draw >> SP_DEFENSE_IV_SHIFT) & IV_BIT_MASK) as u8
    );
    assert_eq!(battle.random_turn_number(), expected_turn_number);
    assert_ne!(
        battle.player().effective_speed(),
        battle.enemy().effective_speed(),
        "this pin assumes no speed tie; a tie would cost one extra draw"
    );
    assert_eq!(
        rng.state(),
        expected_rng.state(),
        "exactly personality (2) + ivs (2) + held item (1) + turn number (1) = 6 draws, no more"
    );
}

#[test]
fn advance_first_battle_plays_to_a_terminal_outcome_without_ever_running() {
    let mut rng = Rng::new(DEFAULT_RNG_SEED);
    let lead = max_iv_player_mon(TREECKO, DOMINANT_PLAYER_LEVEL, vec![POUND]);
    let battle = start_first_battle(lead, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("construction must succeed");

    let mut battle_slot = Some(battle);
    let mut player_lead = None;
    let mut turn_count = 0;
    let outcome = loop {
        if let Some(outcome) = advance_first_battle(&mut battle_slot, &mut player_lead, &mut rng) {
            break outcome;
        }
        turn_count += 1;
        assert!(
            turn_count < MAX_HEADLESS_TURNS,
            "the headless driver must terminate"
        );
    };

    assert!(
        matches!(outcome, BattleOutcome::PlayerWon | BattleOutcome::WildFled),
        "a level-50 Treecko is never going to lose to a level-2 Zigzagoon: got {outcome:?}"
    );
    assert!(battle_slot.is_none(), "a terminal battle empties its slot");
    let lead = player_lead.expect("a terminal battle writes the player lead back");
    assert_eq!(lead.species(), TREECKO);
    assert_eq!(
        lead.stages(),
        battle::StatStages::default(),
        "in-battle stat stages must not leak back into the overworld copy"
    );
}

#[test]
fn advance_first_battle_clears_stat_stages_after_growl_modifies_them() {
    let mut rng = Rng::new(GROWL_SCENARIO_RNG_SEED);
    let lead = max_iv_player_mon(TREECKO, GROWL_SCENARIO_PLAYER_LEVEL, vec![POUND]);
    let battle = start_first_battle(lead, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("construction must succeed");

    let mut battle_slot = Some(battle);
    let mut player_lead = None;
    let mut saw_modified_stages = false;
    let mut turn_count = 0;
    loop {
        if battle_slot
            .as_ref()
            .is_some_and(|battle| battle.player().stages() != battle::StatStages::default())
        {
            saw_modified_stages = true;
        }
        if advance_first_battle(&mut battle_slot, &mut player_lead, &mut rng).is_some() {
            break;
        }
        assert!(
            battle_slot.is_some(),
            "the driver aborted instead of playing this fight to an outcome"
        );
        turn_count += 1;
        assert!(
            turn_count < MAX_HEADLESS_TURNS,
            "the headless driver must terminate"
        );
    }

    assert!(
        saw_modified_stages,
        "Growl must land before the battle ends, or this test pins nothing"
    );
    let lead = player_lead.expect("a terminal battle writes the player lead back");
    assert_eq!(
        lead.stages(),
        battle::StatStages::default(),
        "in-battle stat stages must not leak back into the overworld copy"
    );
}

#[test]
fn advance_first_battle_aborts_and_writes_back_when_the_lead_has_no_pp() {
    let mut rng = Rng::new(DEFAULT_RNG_SEED);
    let mut lead = max_iv_player_mon(TREECKO, DOMINANT_PLAYER_LEVEL, vec![POUND]);
    let starting_pp = lead.moves()[FIRST_MOVE_SLOT].pp;
    assert!(starting_pp > 0, "a freshly built mon starts with PP");
    for _ in 0..starting_pp {
        lead.deduct_pp(FIRST_MOVE_SLOT)
            .expect("draining a slot that still has PP");
    }
    assert_eq!(lead.moves()[FIRST_MOVE_SLOT].pp, 0);

    let battle = start_first_battle(lead, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("construction must succeed");
    let mut battle_slot = Some(battle);
    let mut player_lead = None;

    let outcome = advance_first_battle(&mut battle_slot, &mut player_lead, &mut rng);

    assert!(
        outcome.is_none(),
        "an aborted turn has no battle outcome: {outcome:?}"
    );
    assert!(
        battle_slot.is_none(),
        "an aborted battle must empty its slot"
    );
    let lead = player_lead.expect("an aborted battle writes the player lead back");
    assert_eq!(lead.species(), TREECKO);
    assert_eq!(
        lead.moves()[FIRST_MOVE_SLOT].pp,
        0,
        "the drained PP persists into the overworld copy"
    );
    assert_eq!(lead.stages(), battle::StatStages::default());
}

#[test]
fn advancing_an_absent_first_battle_does_nothing() {
    let mut battle_slot: Option<Battle> = None;
    let mut player_lead = None;
    let mut rng = Rng::new(DEFAULT_RNG_SEED);

    assert!(advance_first_battle(&mut battle_slot, &mut player_lead, &mut rng).is_none());
    assert!(player_lead.is_none());
    assert_eq!(
        rng.state(),
        Rng::new(DEFAULT_RNG_SEED).state(),
        "an absent battle consumes no RNG draw"
    );
}
