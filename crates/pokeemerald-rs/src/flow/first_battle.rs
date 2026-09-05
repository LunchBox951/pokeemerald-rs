//! Builds and drives Route 101's scripted first battle.
//!
//! [`start_first_battle`] creates Emerald's fixed level-2 Zigzagoon through
//! the `CreateMon` personality and IV path. It then consumes the unmodelled
//! held-item draw before battle initialization, preserving upstream's
//! frame-free shared RNG order. Only frame-free draws align: the headless
//! driver has no vblank interleaving, while `VBlankCB_Battle` advances the
//! generator once per battle vblank (`battle_main.c:2085-2089`).
//!
//! [`advance_first_battle`] uses the first move slot because first battles
//! forbid running. It writes the player lead back after a terminal or failed
//! turn. Upstream's `CB2_EndFirstBattle` returns directly to the field even
//! after a loss (`battle_setup.c:950-954`), so a fainted lead is valid here.

use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, PlayerAction};
use engine::rng::Rng;

use super::battle_finalize::finalize_battle_turn;
use super::move_learn::settle_move_learn_prompts;
use super::wild_encounter::SharedRng;

const HEADLESS_PLAYER_MOVE_SLOT: usize = 0;

/// Emerald's scripted first-battle opponent.
pub const FIRST_BATTLE_OPPONENT_SPECIES: assets::SpeciesId = assets::SpeciesId(288);

/// The scripted first-battle opponent's level.
pub const FIRST_BATTLE_OPPONENT_LEVEL: u8 = 2;

fn consume_unmodelled_held_item_draw(rng: &mut Rng) {
    // `SetWildMonHeldItem` runs here upstream (`pokemon.c:6678`). Held items
    // are not modelled, but consuming its draw preserves shared RNG order.
    let _ = rng.next_u16();
}

/// Builds Emerald's scripted first-battle opponent and starts the battle.
///
/// `player_trainer_id` is the save owner's id, not the party lead's --
/// upstream's `CreateMon(&gEnemyParty[0], SPECIES_ZIGZAGOON, ..., OT_ID_PLAYER_ID, 0)`
/// (`src/battle_controllers.c:70`) stores `gSaveBlock2Ptr->playerTrainerId`.
///
/// # Errors
///
/// Returns opponent-construction or battle-initialization errors.
pub fn start_first_battle(
    player_lead: BattlePokemon,
    player_trainer_id: u32,
    rng: &mut Rng,
) -> Result<Battle, BattleError> {
    let dex = Dex::new();
    let opponent_moves =
        battle::initial_moveset(FIRST_BATTLE_OPPONENT_SPECIES, FIRST_BATTLE_OPPONENT_LEVEL);
    let opponent = battle::build_pokemon_with_random_personality(
        &dex,
        FIRST_BATTLE_OPPONENT_SPECIES,
        FIRST_BATTLE_OPPONENT_LEVEL,
        opponent_moves,
        &mut SharedRng::new(rng),
    )?
    .with_original_trainer_id(player_trainer_id);
    consume_unmodelled_held_item_draw(rng);

    let is_scripted_first_battle = true;
    Battle::new(
        dex,
        player_lead,
        opponent,
        is_scripted_first_battle,
        &mut SharedRng::new(rng),
    )
}

/// Advances the first battle by one headless turn.
///
/// Returns `None` when the slot is empty, the battle remains active, or the
/// turn fails. A terminal or failed turn empties the slot and writes back the
/// player lead with battle-only stat stages cleared. A failed turn has no
/// [`BattleOutcome`], so callers must inspect the slot to distinguish it from
/// an active battle.
pub fn advance_first_battle(
    battle_slot: &mut Option<Battle>,
    player_lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let player_action = PlayerAction::UseMove(HEADLESS_PLAYER_MOVE_SLOT);
    let turn_failed = match battle.take_turn(player_action, &mut SharedRng::new(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("first battle: turn failed ({error:?}) -- ending the encounter");
            true
        }
    };
    let _ = settle_move_learn_prompts(battle);
    finalize_battle_turn(battle_slot, turn_failed, player_lead)
}

#[cfg(test)]
mod tests;
