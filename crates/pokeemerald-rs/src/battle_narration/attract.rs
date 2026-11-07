//! Attract's messages: the result of the move and the infatuation canceller.
//!
//! The result events name the move's user and its target
//! (`BattleScript_EffectAttract` prints `STRINGID_PKMNFELLINLOVE` for the
//! defender, `data/battle_scripts_1.s:1653`; Oblivious prints its own ability
//! for the defender, `:4140`-`:4144`). The canceller events name the
//! infatuated actor as the attacker and, for `STRINGID_PKMNINLOVE`, the
//! battler it is in love with as the scripting battler
//! (`AtkCanceler_UnableToUseMove`, `src/battle_util.c:2203`;
//! `data/battle_scripts_1.s:3840`-`:3849`). Cute Charm's contact infatuation
//! prints `STRINGID_PKMNSXINFATUATEDY` naming the holder as the defender and
//! the infatuated attacker (`BattleScript_CuteCharmActivates`,
//! `data/battle_scripts_1.s:4193`-`:4197`).

use assets::AbilityId;
use battle::BattleEvent;

use super::context::Side;
use super::events::table;
use super::narrator::Narrator;
use super::string_ids as id;
use super::NarrationRequest;

/// The requests for an Attract event, or `None` for any other event.
pub(super) fn requests(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let base = narrator.context();
    let request = match *event {
        BattleEvent::Infatuated { by_player, .. } => table(
            id::PKMNFELLINLOVE,
            base.user_vs_target(Side::user(by_player)),
        ),
        BattleEvent::ObliviousPreventsAttraction { by_player, .. } => table(
            id::PKMNPREVENTSROMANCEWITH,
            base.user_vs_target(Side::user(by_player))
                .with_ability(Side::user(by_player).other(), AbilityId::OBLIVIOUS),
        ),
        BattleEvent::InfatuatedByCuteCharm { by_player, .. } => table(
            id::PKMNSXINFATUATEDY,
            base.user_vs_target(Side::user(by_player))
                .with_ability(Side::user(by_player).other(), AbilityId::CUTE_CHARM),
        ),
        BattleEvent::InLove { by_player } => {
            let infatuated = Side::user(by_player);
            let mut context = base.user_vs_target(infatuated);
            context.scripting = Some(infatuated.other());
            table(id::PKMNINLOVE, context)
        }
        BattleEvent::ImmobilizedByLove { by_player, .. } => table(
            id::PKMNIMMOBILIZEDBYLOVE,
            base.user_vs_target(Side::user(by_player)),
        ),
        _ => return None,
    };
    Some(vec![request])
}
