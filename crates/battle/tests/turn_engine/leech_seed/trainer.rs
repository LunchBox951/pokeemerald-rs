//! Leech Seed through Daisy's `AI_SCRIPT_CHECK_BAD_MOVE` trainer path.

use super::*;

/// `TRAINER_DAISY`: `AI_SCRIPT_CHECK_BAD_MOVE` only
/// (`src/data/trainers.h:436`-`:445`).
const DAISY: assets::trainers::TrainerId = assets::trainers::TrainerId(36);
/// The simulated-damage draws `AI_CheckBadMove`'s initialisation spends, one
/// per move slot, then the highest-score pick.
const AI_SCORING_DRAWS: [u16; 5] = [0; 5];

fn daisy_turn(
    player: BattlePokemon,
    lead: BattlePokemon,
    action_draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(
        [0, 0]
            .into_iter()
            .chain(AI_SCORING_DRAWS)
            .chain(action_draws.iter().copied()),
    );
    let mut battle = Battle::new_trainer(Dex::new(), player, DAISY, vec![lead], &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

#[test]
fn daisys_ai_never_picks_a_seed_that_scores_minus_ten_and_executes_the_one_that_does_not() {
    let lead = || mon(WURMPLE, &[LEECH_SEED, SPLASH]);

    // Against a Grass player the seed scores 90 and Splash 100.
    let (battle, events) = daisy_turn(mon(SHROOMISH, &[SPLASH]), lead(), &[]);
    assert_eq!(battle.enemy().moves()[0].pp, 10, "{events:?}");
    assert_eq!(
        battle.enemy().moves()[1].pp,
        39,
        "Splash was used: {events:?}"
    );

    // Against a non-Grass player both score 100 and the first wins the pick.
    let (battle, events) = daisy_turn(mon(PIKACHU, &[SPLASH]), lead(), &[0]);
    assert!(
        events.contains(&BattleEvent::Seeded {
            by_player: false,
            move_id: LEECH_SEED,
        }),
        "{events:?}"
    );
    assert_eq!(
        battle.player().volatiles().leech_seed_source,
        Some(BattlerSide::Enemy)
    );
}

#[test]
fn a_seeded_trainer_battler_that_faints_hands_its_replacement_no_seed() {
    let lead = with_hp(seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player), 1);
    let bench = mon(ZIGZAGOON, &[SPLASH]);
    let mut rng = SequenceRng::new([0, 0].into_iter().chain(AI_SCORING_DRAWS));
    let mut battle = Battle::new_trainer(
        Dex::new(),
        mon(PIKACHU, &[SPLASH]),
        DAISY,
        vec![lead, bench],
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::Fainted { by_player: false }));
    assert_eq!(battle.enemy().species().0, ZIGZAGOON, "{events:?}");
    assert!(!battle.enemy().volatiles().seeded());
}
