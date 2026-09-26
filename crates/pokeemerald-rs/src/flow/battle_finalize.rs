use battle::{Battle, BattleOutcome, BattlePokemon};

pub(super) fn finalize_battle_turn(
    battle_slot: &mut Option<Battle>,
    turn_failed: bool,
    lead: &mut Option<BattlePokemon>,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let outcome = battle.outcome();
    let battle_is_ongoing = !turn_failed && outcome.is_none();
    if battle_is_ongoing {
        return None;
    }

    let mut player_after_battle = battle.player().clone();
    player_after_battle.clear_battle_scratch();
    *lead = Some(player_after_battle);
    *battle_slot = None;
    outcome
}
