use battle::{Battle, BattleOutcome, BattlePokemon};

pub(super) fn finalize_battle_turn(
    battle_slot: &mut Option<Battle>,
    turn_failed: bool,
    lead: &mut Option<BattlePokemon>,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let outcome = battle.outcome();
    if battle_is_ongoing(battle, turn_failed) {
        return None;
    }

    let mut player_after_battle = battle.player().clone();
    player_after_battle.clear_battle_scratch();
    *lead = Some(player_after_battle);
    *battle_slot = None;
    outcome
}

/// A battle that carried its player's whole saved party: every member in the order the
/// flow handed them over, with the battle's own scratch state cleared.
#[derive(Debug)]
pub(super) struct PartyBattleEnd {
    pub(super) outcome: Option<BattleOutcome>,
    pub(super) members: Vec<BattlePokemon>,
    /// How many leading `members` were sent out, the starting active one included.
    pub(super) entered: usize,
}

/// [`finalize_battle_turn`] for a battle whose party had reserves: on an ending, empties
/// the slot and returns every player-side member, since the active one at the end need not
/// be the one that started.
pub(super) fn finalize_party_battle_turn(
    battle_slot: &mut Option<Battle>,
    turn_failed: bool,
) -> Option<PartyBattleEnd> {
    if battle_is_ongoing(battle_slot.as_ref()?, turn_failed) {
        return None;
    }
    let battle = battle_slot.take()?;
    let members = battle
        .player_members()
        .cloned()
        .map(|mut member| {
            member.clear_battle_scratch();
            member
        })
        .collect();
    Some(PartyBattleEnd {
        outcome: battle.outcome(),
        members,
        entered: battle.player_position() + 1,
    })
}

fn battle_is_ongoing(battle: &Battle, turn_failed: bool) -> bool {
    !turn_failed && battle.outcome().is_none()
}
