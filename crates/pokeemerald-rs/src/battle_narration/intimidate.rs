//! Intimidate's entry messages.
//!
//! Both are printed by `BattleScript_IntimidateActivates`
//! (`data/battle_scripts_1.s:4024`-`:4048`) with the Intimidate holder as the
//! scripting battler and the opposing battler as `gBattlerTarget`.
//! `Cmd_trygetintimidatetarget` sets that scripting battler and prepares the
//! holder's ability in buffer 0 (`src/battle_script_commands.c:9567`-`:9574`).

use assets::AbilityId;
use battle::BattleEvent;

use super::context::{BattleBuffer, BufferOp, NarrationContext, Side};
use super::events::table;
use super::narrator::Narrator;
use super::string_ids as id;
use super::NarrationRequest;

/// The requests for an Intimidate event, or `None` for any other event.
pub(super) fn requests(narrator: &Narrator, event: &BattleEvent) -> Option<Vec<NarrationRequest>> {
    let request = match *event {
        BattleEvent::Intimidated { by_player, .. } => {
            let holder = Side::user(by_player);
            table(id::PKMNCUTSATTACKWITH, holder_context(narrator, holder))
        }
        BattleEvent::IntimidatePrevented { by_player, ability } => {
            let holder = Side::user(by_player);
            let context = holder_context(narrator, holder)
                .with_ability(holder.other(), ability)
                .with_buffer(
                    0,
                    BattleBuffer::Program(vec![BufferOp::Ability(AbilityId::INTIMIDATE)]),
                );
            table(id::PREVENTEDFROMWORKING, context)
        }
        _ => return None,
    };
    Some(vec![request])
}

/// The holder as the scripting battler and the other side as its target.
fn holder_context(narrator: &Narrator, holder: Side) -> NarrationContext {
    let mut context = narrator
        .context()
        .user_vs_target(holder)
        .with_ability(holder, AbilityId::INTIMIDATE);
    context.scripting = Some(holder);
    context
}
