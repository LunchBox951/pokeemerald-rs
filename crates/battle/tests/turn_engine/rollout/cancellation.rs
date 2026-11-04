//! What releases a series besides a miss: the cancellers that cancel
//! (`data/battle_scripts_1.s:3777`, `:3802`), accepted sleep
//! (`src/battle_script_commands.c:2296`), and a faint.

use super::*;
use assets::AbilityId;
use battle::{BattleOutcome, Status1};

const FULL_PARALYSIS: u16 = 0;
const PARALYSIS_AVOIDED: u16 = 1;
/// `Random() & 1` odd lets a confused battler through, even self-hits.
const CONFUSION_LETS_MOVE_THROUGH: u16 = 1;
const CONFUSION_SELF_HIT: u16 = 0;
const SELF_HIT_VARIANCE: u16 = 0;
const SPORE_TRIGGERS: u16 = 0;
const SPORE_SELECTS_SLEEP: u16 = 1;
const SLEEP_FIVE_TURNS: u16 = 3;

#[test]
fn full_paralysis_cancels_the_series_before_pp_or_any_move_draw() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    player.set_status1(Status1::Paralysed);
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &[PARALYSIS_AVOIDED],
        &LANDED,
        &WILD_TURN_OVERHEAD,
        &[FULL_PARALYSIS],
        &WILD_TURN_OVERHEAD,
        &[PARALYSIS_AVOIDED],
    ]));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(events.contains(&BattleEvent::FullyParalyzed {
        by_player: true,
        move_id: ROLLOUT,
    }));
    assert_eq!(player_lock(&battle), (None, None));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 1);

    // The next selection is free.
    let events = take(&mut battle, PlayerAction::UseMove(1), &mut rng);
    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::NothingHappened {
                by_player: true,
                ..
            }
        )),
        "Splash: {events:?}"
    );
    rng.assert_exhausted();
}

#[test]
fn a_confusion_self_hit_cancels_the_series_without_the_moves_draws() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    player.volatiles_mut().set_confusion(3);
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &[CONFUSION_LETS_MOVE_THROUGH],
        &LANDED,
        &WILD_TURN_OVERHEAD,
        &[CONFUSION_SELF_HIT, SELF_HIT_VARIANCE],
    ]));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(player_lock(&battle), (None, None));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 1);
    assert!(
        battle.player().volatiles().confused(),
        "only the series is cancelled, not the confusion"
    );
    rng.assert_exhausted();
}

#[test]
fn snapping_out_of_confusion_keeps_the_series_going() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    player.volatiles_mut().set_confusion(2);
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &[CONFUSION_LETS_MOVE_THROUGH],
        &LANDED,
        // The second tick reaches zero: no coin is drawn.
        &WILD_TURN_OVERHEAD,
        &LANDED,
    ]));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    let first = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    let second = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(second.contains(&BattleEvent::SnappedOutOfConfusion { by_player: true }));
    assert!(player_hit_damage(&second) > player_hit_damage(&first));
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(3)));
    rng.assert_exhausted();
}

fn effect_spore_battle(player: BattlePokemon, rng: &mut SequenceRng) -> (Battle, Vec<BattleEvent>) {
    let holder = target(SHROOMISH, &[SPLASH]);
    assert_eq!(holder.ability(), AbilityId::EFFECT_SPORE);
    let mut battle = wild_battle(player, holder, rng);
    let events = take(&mut battle, PlayerAction::UseMove(0), rng);
    (battle, events)
}

#[test]
fn sleep_accepted_by_effect_spore_cancels_the_series_it_just_extended() {
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &[SPORE_TRIGGERS, SPORE_SELECTS_SLEEP, SLEEP_FIVE_TURNS],
    ]));

    let (battle, events) = effect_spore_battle(attacker(&[ROLLOUT, SPLASH]), &mut rng);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::AsleepByEffectSpore { .. })),
        "{events:?}"
    );
    assert!(battle.player().status1().is_asleep());
    assert_eq!(player_lock(&battle), (None, None));
    rng.assert_exhausted();
}

#[test]
fn sleep_effect_spore_cannot_inflict_leaves_the_series_alone() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    player.set_status1(Status1::Poisoned);
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &[SPORE_TRIGGERS, SPORE_SELECTS_SLEEP],
    ]));

    let (battle, _events) = effect_spore_battle(player, &mut rng);

    assert_eq!(battle.player().status1(), Status1::Poisoned);
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));
    rng.assert_exhausted();
}

#[test]
fn an_attacker_that_faints_to_rough_skin_keeps_no_series() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    player.apply_damage(player.current_hp() - 1);
    let holder = target(SHARPEDO, &[SPLASH]);
    assert_eq!(holder.ability(), AbilityId::ROUGH_SKIN);
    let mut rng = SequenceRng::new(turn_script(&[&CONSTRUCTION, &WILD_TURN_OVERHEAD, &LANDED]));
    let mut battle = wild_battle(player, holder, &mut rng);

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(events.contains(&BattleEvent::Fainted { by_player: true }));
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(player_lock(&battle), (None, None));
    rng.assert_exhausted();
}
