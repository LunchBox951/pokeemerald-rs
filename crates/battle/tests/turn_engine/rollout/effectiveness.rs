//! The matchup message on a landed attempt. `BattleScript_EffectRollout`
//! continues into `BattleScript_HitFromCritCalc`
//! (`data/battle_scripts_1.s:1606`), whose `resultmessage` prints it after the
//! hit and before `tryfaintmon` (`:248`-`:266`).

use super::*;
use assets::Effectiveness;

/// Pure Fire, with Blaze: no contact ability spends a draw.
const CHARMANDER: u16 = 4;

/// Plays one landed player attempt of `move_id` into a splashing `species`.
fn landed_attempt(move_id: MoveId, species: u16) -> Vec<BattleEvent> {
    let mut rng = SequenceRng::new(turn_script(&[&CONSTRUCTION, &WILD_TURN_OVERHEAD, &LANDED]));
    let mut battle = wild_battle(
        attacker(&[move_id, SPLASH]),
        target(species, &[SPLASH]),
        &mut rng,
    );
    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    rng.assert_exhausted();
    events
}

/// The index of the player's hit and of the matchup event, if any.
fn hit_and_matchup(events: &[BattleEvent]) -> (usize, Option<(usize, Effectiveness)>) {
    let hit = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("no player hit in {events:?}"));
    let matchup = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            BattleEvent::TypeEffectiveness {
                by_player: true,
                effectiveness,
            } => Some((index, *effectiveness)),
            _ => None,
        });
    (hit, matchup)
}

#[test]
fn a_super_effective_rollout_reports_the_matchup_right_after_the_hit() {
    let events = landed_attempt(ROLLOUT, CHARMANDER);

    let (hit, matchup) = hit_and_matchup(&events);

    assert_eq!(
        matchup,
        Some((hit + 1, Effectiveness::SuperEffective)),
        "{events:?}"
    );
}

#[test]
fn a_not_very_effective_ice_ball_reports_the_matchup_right_after_the_hit() {
    let events = landed_attempt(ICE_BALL, SHARPEDO);

    let (hit, matchup) = hit_and_matchup(&events);

    assert_eq!(
        matchup,
        Some((hit + 1, Effectiveness::NotVeryEffective)),
        "{events:?}"
    );
}

#[test]
fn a_neutral_rollout_reports_no_matchup() {
    let events = landed_attempt(ROLLOUT, WAILORD);

    assert_eq!(hit_and_matchup(&events).1, None, "{events:?}");
}

#[test]
fn a_lethal_super_effective_rollout_reports_the_matchup_before_the_faint() {
    let events = landed_attempt(ROLLOUT, SHEDINJA);

    let (hit, matchup) = hit_and_matchup(&events);
    let faint = events
        .iter()
        .position(|event| matches!(event, BattleEvent::Fainted { .. }))
        .unwrap_or_else(|| panic!("no faint in {events:?}"));

    assert_eq!(
        matchup,
        Some((hit + 1, Effectiveness::SuperEffective)),
        "{events:?}"
    );
    assert!(hit + 1 < faint, "{events:?}");
}
