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

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{AbilityId, MoveId, SpeciesId, Type};
    use battle::{BattleEvent, BattleRng, Dex, Ivs, PlayerAction};

    /// Every draw is zero: no tie and a hit, whatever the move.
    struct AllZero;

    impl BattleRng for AllZero {
        fn next_u16(&mut self) -> u16 {
            0
        }
    }

    /// A battle that has ended with the player's Color Change holder wearing Dragon: it
    /// splashed, Dragon Rage hit it for a fixed 40 and changed its type, then it ran.
    fn ended_with_the_holder_changed_to_dragon() -> Battle {
        let dex = Dex::new();
        let player = BattlePokemon::new(
            &dex,
            SpeciesId::KECLEON,
            50,
            Ivs::default(),
            0,
            vec![MoveId::SPLASH],
        )
        .unwrap();
        let enemy = BattlePokemon::new(
            &dex,
            SpeciesId::RATTATA,
            5,
            Ivs::default(),
            0,
            vec![MoveId::DRAGON_RAGE],
        )
        .unwrap();
        assert_eq!(player.ability(), AbilityId::COLOR_CHANGE);
        assert_eq!(player.species_types(), [Type::Normal; 2]);
        assert!(
            player.stats().speed > enemy.stats().speed,
            "running must succeed"
        );
        let mut rng = AllZero;
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(
            events.iter().any(|event| matches!(
                event,
                BattleEvent::TypeChanged {
                    move_id: MoveId::DRAGON_RAGE,
                    new_type: Type::Dragon,
                    ..
                }
            )),
            "{events:?}"
        );
        battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
        assert_eq!(
            battle.player().types(),
            [Type::Dragon; 2],
            "the override must still be live when the battle ends"
        );
        battle
    }

    #[test]
    fn a_color_changed_lead_returns_to_the_overworld_with_its_species_typing() {
        let mut slot = Some(ended_with_the_holder_changed_to_dragon());
        let mut lead = None;

        let outcome = finalize_battle_turn(&mut slot, false, &mut lead);

        assert_eq!(outcome, Some(BattleOutcome::PlayerRan));
        assert!(slot.is_none());
        assert_eq!(lead.unwrap().types(), [Type::Normal; 2]);
    }

    #[test]
    fn a_color_changed_party_member_returns_to_the_overworld_with_its_species_typing() {
        let mut slot = Some(ended_with_the_holder_changed_to_dragon());

        let end = finalize_party_battle_turn(&mut slot, false).expect("the battle ended");

        assert_eq!(end.outcome, Some(BattleOutcome::PlayerRan));
        assert!(slot.is_none());
        assert_eq!(end.members.len(), 1);
        assert_eq!(end.members[0].types(), [Type::Normal; 2]);
        assert_eq!(end.members[0].types(), end.members[0].species_types());
    }
}
