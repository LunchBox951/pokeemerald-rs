//! Connects an eligible overworld landing to a completed wild battle.
//!
//! # RNG stream
//!
//! [`SharedRng`] lends the overworld-owned generator to battle code, keeping
//! encounter selection, opponent construction, battle initialization, and turns on one
//! stream. [`start_wild_battle`] consumes those construction draws in order, including the
//! held-item draw whose result is not modelled.
//! Only the frame-free draw order matches upstream: `VBlankCB_Battle`
//! (`battle_main.c:2085-2089`) advances the generator once per battle vblank, while this
//! headless driver has no battle-vblank interleaving. The callback remains pending in the
//! ledger.
//!
//! # Headless action policy
//!
//! With no battle menu, [`advance_wild_battle`] attempts to run every turn. Scripted and
//! trainer battles use separate drivers because they require different legal actions. A
//! rolled encounter without a party lead is logged and dropped before construction, so it
//! spends no battle-construction draws.
//!
//! # Wild-battle loss
//!
//! A terminal battle is written back before [`advance_wild_battle`] returns its outcome. Its
//! same-frame caller sends [`BattleOutcome::PlayerLost`] through the overworld white-out,
//! which heals the lead before another field frame can observe it.
//!
//! # Lead-health eligibility
//!
//! Encounter eligibility needs no lead-health gate. Wild losses white out in their terminal
//! frame, the scripted first-battle conclusion heals the lead after every outcome, and save
//! loading heals the single-member Route 101 rescue state that can otherwise contain a
//! fainted lead.

use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, BattleRng, Dex, PlayerAction};
use engine::overworld::metatile_behavior::MB_NORMAL;
use engine::overworld::warp::WarpTrigger;
use engine::overworld::wild_encounter::{WildEncounter, WildEncounterState};
use engine::overworld::{MapRuntime, TilePos};
use engine::rng::Rng;

use super::battle_finalize::finalize_battle_turn;
use super::move_learn::settle_move_learn_prompts;

pub(super) struct SharedRng<'a>(&'a mut Rng);

impl<'a> SharedRng<'a> {
    pub(super) fn new(rng: &'a mut Rng) -> Self {
        Self(rng)
    }
}

impl BattleRng for SharedRng<'_> {
    fn next_u16(&mut self) -> u16 {
        self.0.next_u16()
    }

    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
}

pub(super) fn roll_for_step(
    state: &mut WildEncounterState,
    rng: &mut Rng,
    map: assets::MapId,
    runtime: &MapRuntime<'_>,
    eligible_landing: Option<TilePos>,
) -> Option<WildEncounter> {
    let (x, y) = eligible_landing?;
    let metatile_behavior = runtime.metatile_behavior(x, y).unwrap_or(MB_NORMAL);
    let encounter_header = assets::WildEncounterTable::new().get_by_map(map);
    state.check_standard_wild_encounter(metatile_behavior, encounter_header, rng)
}

/// The completed step `CheckStandardWildEncounter` (`:162`) is allowed to
/// roll on: one the door-shaped warp above it (`TryStartStepBasedScript`,
/// `:155-161`) has not already claimed by returning TRUE.
///
/// `TryArrowWarp` is deliberately not an input here. It sits at `:164-168`,
/// *below* the roll, so an arrow poll that is still open on a landing frame
/// can never suppress the roll -- it is the roll that suppresses the poll
/// (`arrow_poll_open`).
pub(super) fn roll_eligible_landing(
    landed: Option<TilePos>,
    door_warp: Option<WarpTrigger>,
) -> Option<TilePos> {
    landed.filter(|_| door_warp.is_none())
}

/// Whether `ProcessPlayerFieldInput` reaches `TryArrowWarp` (`:164-168`)
/// this frame: the player must be between steps -- upstream sets
/// `input->heldDirection` only at `T_TILE_CENTER`/`T_NOT_MOVING`
/// (`:95-112`) -- and nothing above `:164` may have claimed the frame
/// already.
pub(super) const fn arrow_poll_open(in_transit: bool, landing_claimed: bool) -> bool {
    !in_transit && !landing_claimed
}

/// Whether an earlier `ProcessPlayerFieldInput` step already claimed this frame; any
/// present warp counts, even `Unsupported` (`field_control_avatar.c:688-749`, `:817-820`, `:172`).
pub(super) const fn field_input_consumed(
    field_event_fired: bool,
    warp_trigger: Option<WarpTrigger>,
) -> bool {
    field_event_fired || warp_trigger.is_some()
}

/// Screens every possible land encounter before the map is allowed to roll.
///
/// Upstream's `CreateWildMon` (`wild_encounter.c:379`) cannot reject an unsupported
/// moveset. Rejecting one after this port rolled it would consume unmatched RNG draws, so
/// one unsupported entry disables encounter draws and bookkeeping for the whole map.
pub(super) fn map_wild_table_fightable(map: assets::MapId) -> bool {
    let Some(land_encounters) = assets::WildEncounterTable::new()
        .get_by_map(map)
        .and_then(|header| header.land)
    else {
        return true;
    };

    let dex = Dex::new();
    for wild_slot in &land_encounters.mons {
        for level in wild_slot.min_level..=wild_slot.max_level {
            if let Err(error) = battle::ensure_wild_startable(&dex, wild_slot.species, level) {
                eprintln!(
                    "wild encounter: {}'s land table can roll species {:?} at level {level}, \
                     which the battle engine can't fight yet ({error:?}) -- \
                     no encounters will roll on this map",
                    map.name(),
                    wild_slot.species,
                );
                return false;
            }
        }
    }
    true
}

fn consume_unmodelled_wild_held_item_draw(rng: &mut Rng) {
    // Upstream selects the wild mon's held item between opponent construction and battle
    // initialization (`SetWildMonHeldItem`, `pokemon.c:6678`). Held items are not modelled,
    // but consuming its draw preserves the frame-free shared RNG sequence.
    let _ = rng.next_u16();
}

/// `player_trainer_id` is the save owner's id, not the party lead's --
/// upstream's `CreateWildMon` -> `CreateMonWithNature` ->
/// `CreateMon(..., OT_ID_PLAYER_ID, 0)` (`pokemon.c:2305`) stores
/// `gSaveBlock2Ptr->playerTrainerId`.
pub(super) fn start_wild_battle(
    player_lead: BattlePokemon,
    encounter: WildEncounter,
    player_trainer_id: u32,
    rng: &mut Rng,
) -> Result<Battle, BattleError> {
    let dex = Dex::new();
    let wild_moves = battle::initial_moveset(encounter.species, encounter.level);
    let wild_opponent = battle::build_wild_pokemon(
        &dex,
        encounter.species,
        encounter.level,
        wild_moves,
        &mut SharedRng::new(rng),
    )?
    .with_original_trainer_id(player_trainer_id);
    consume_unmodelled_wild_held_item_draw(rng);

    let is_scripted_first_battle = false;
    Battle::new(
        dex,
        player_lead,
        wild_opponent,
        is_scripted_first_battle,
        &mut SharedRng::new(rng),
    )
}

pub(super) fn advance_wild_battle(
    battle_slot: &mut Option<Battle>,
    player_lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = battle_slot.as_mut()?;
    let player_action = PlayerAction::Run;
    let turn_failed = match battle.take_turn(player_action, &mut SharedRng::new(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("wild battle: turn failed ({error:?}) -- ending the encounter");
            true
        }
    };
    let _ = settle_move_learn_prompts(battle);
    finalize_battle_turn(battle_slot, turn_failed, player_lead)
}

#[cfg(test)]
mod tests;
