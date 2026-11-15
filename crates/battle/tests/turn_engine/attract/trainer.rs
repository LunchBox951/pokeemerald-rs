//! Attract used by the opponent: Daisy's `AI_SCRIPT_CHECK_BAD_MOVE` scoring
//! and a wild opponent's unscored pick.

use super::*;
use assets::trainers::TrainerId;

/// `TRAINER_DAISY`: `AI_SCRIPT_CHECK_BAD_MOVE` only
/// (`src/data/trainers.h:436`-`:445`).
const DAISY: TrainerId = TrainerId(36);
/// `SPECIES_PIKACHU`: Static as its only ability, so the trainer AI has no
/// ability to guess. Faster than [`MUDKIP`].
const PIKACHU: SpeciesId = SpeciesId::PIKACHU;
/// `SPECIES_NUMEL`: Oblivious as its only ability.
const NUMEL: SpeciesId = SpeciesId::NUMEL;
/// The simulated-damage draws `AI_CheckBadMove`'s initialisation spends, one
/// per move slot, then the highest-score pick.
const AI_SCORING_DRAWS: [u16; 5] = [0; 5];
const ATTRACT_PP: u8 = 15;
const SPLASH_PP: u8 = 40;

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

fn lead() -> BattlePokemon {
    mon(MUDKIP, MALE, &[ATTRACT, SPLASH])
}

#[test]
fn daisy_attracts_an_opposite_gender_target_and_the_player_is_infatuated_by_the_enemy() {
    let (battle, events) = daisy_turn(mon(PIKACHU, FEMALE, &[SPLASH]), lead(), &[ACCURACY_HITS]);

    assert!(
        events.contains(&BattleEvent::Infatuated {
            by_player: false,
            move_id: ATTRACT,
        }),
        "{events:?}"
    );
    assert_eq!(
        battle.player().volatiles().infatuated_by,
        Some(BattlerSide::Enemy)
    );
    assert_eq!(battle.enemy().moves()[0].pp, ATTRACT_PP - 1);
}

#[test]
fn daisy_never_picks_an_attract_that_scores_minus_ten() {
    for target in [
        mon(PIKACHU, MALE, &[SPLASH]),
        mon(NUMEL, FEMALE, &[SPLASH]),
        infatuated_by(mon(PIKACHU, FEMALE, &[SPLASH]), BattlerSide::Enemy),
    ] {
        let infatuated = target.volatiles().infatuated();
        let draws: &[u16] = if infatuated {
            // The infatuated player acts first and spends its coin.
            &[LOVE_LETS_MOVE_THROUGH]
        } else {
            &[]
        };
        let (battle, events) = daisy_turn_with_player_draws(target, lead(), draws);

        assert_eq!(battle.enemy().moves()[0].pp, ATTRACT_PP, "{events:?}");
        assert_eq!(
            battle.enemy().moves()[1].pp,
            SPLASH_PP - 1,
            "Splash was used: {events:?}"
        );
    }
}

/// Like [`daisy_turn`], with the player's own draws between the AI's scoring
/// and the enemy's action: the player is faster.
fn daisy_turn_with_player_draws(
    player: BattlePokemon,
    lead: BattlePokemon,
    player_draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    daisy_turn(player, lead, player_draws)
}

#[test]
fn a_wild_opponent_attracts_without_scoring_and_the_player_loses_its_move_to_the_coin() {
    let player = mon(MUDKIP, FEMALE, &[TACKLE]);
    let enemy = mon(RATTATA, MALE, &[ATTRACT]);

    let (battle, events, rng) = wild_turn(
        player,
        enemy,
        &[&PREFIX, &[ACCURACY_HITS], &[LOVE_IMMOBILIZES]],
    );

    rng.assert_exhausted();
    let infatuated = position(
        &events,
        &BattleEvent::Infatuated {
            by_player: false,
            move_id: ATTRACT,
        },
    );
    let love = position(&events, &in_love(true));
    let cancelled = position(&events, &immobilized(true, TACKLE));
    assert!(infatuated < love && love < cancelled, "{events:?}");
    assert_eq!(battle.player().moves()[0].pp, 35, "no PP is spent");
    assert_eq!(
        battle.player().volatiles().infatuated_by,
        Some(BattlerSide::Enemy)
    );
}
