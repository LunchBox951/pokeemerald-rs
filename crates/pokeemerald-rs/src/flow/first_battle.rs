//! The scripted `BATTLE_TYPE_FIRST_BATTLE` construction and its own headless
//! driver (issue #221, ladders to S-6/I-4).
//!
//! Upstream reaches this battle through `CB2_StartFirstBattle`
//! (`pokeemerald/src/battle_setup.c:930`-`:945`), which sets
//! `gBattleTypeFlags = BATTLE_TYPE_FIRST_BATTLE` and hands off to
//! `CB2_InitBattle`. From there `BattleMainCB2` (`src/battle_main.c:684`)
//! calls `SetUpBattleVarsAndBirchZigzagoon`
//! (`src/battle_controllers.c:42`-`:76`), whose `BATTLE_TYPE_FIRST_BATTLE`
//! branch (`:67`-`:72`) builds the enemy party's sole member:
//!
//! ```text
//! ZeroEnemyPartyMons();
//! CreateMon(&gEnemyParty[0], SPECIES_ZIGZAGOON, 2, USE_RANDOM_IVS, 0, 0, OT_ID_PLAYER_ID, 0);
//! i = 0;
//! SetMonData(&gEnemyParty[0], MON_DATA_HELD_ITEM, &i);
//! ```
//!
//! Emerald's scripted opponent is a level-2 Zigzagoon (species 288), not
//! R/S's Poochyena — [`FIRST_BATTLE_OPPONENT_SPECIES`]/
//! [`FIRST_BATTLE_OPPONENT_LEVEL`] name that pair rather than leaving it a
//! magic literal, the same way `new_game::PROVISIONAL_STARTER_SPECIES`
//! names the starter's. The held-item zeroing has no counterpart to model:
//! [`battle::BattlePokemon`] carries no held-item field at all, so "no
//! item" is already this crate's only representable state.
//!
//! [`start_first_battle`] is that construction — [`battle::Battle::new`]
//! called with `first_battle = true` — minus everything upstream does
//! around it that this port does not model yet (script engine, battle
//! transition, presentation), the same scope
//! [`crate::flow::wild_encounter::start_wild_battle`] keeps for an ordinary
//! encounter. [`advance_first_battle`] is this event's own per-turn driver,
//! deliberately **not** shared with
//! [`crate::flow::wild_encounter::advance_wild_battle`]: that driver always
//! attempts to run, and a `first_battle` forbids running outright
//! ([`battle::BattleError::RunForbidden`], issue #187) — reusing it here
//! would turn the very first turn into an instantly-rejected, dropped
//! battle instead of a played one. [`advance_first_battle`] always chooses
//! the first move slot instead — a deterministic stand-in for the player's
//! actual pick, the same *kind* of choice-of-action stand-in
//! [`crate::flow::wild_encounter`]'s module docs describe for the ordinary
//! driver, just aimed at a legal action for this battle type. Every rule
//! beyond that choice — crit suppression, the wild opponent's
//! `AI_FirstBattle` move choice and flee threshold, damage, fainting — is
//! the real `battle::Battle` from issue #187.
//!
//! # What this slice does not wire up
//!
//! Upstream reaches [`start_first_battle`]'s upstream counterpart through
//! the Route 101 rescue-chain narrative gate: May/Brendan's Zigzagoon
//! cornering the player, the `VAR_ROUTE101_STATE` coord events, the
//! starter-choose UI and `CB2_GiveStarter`'s `ScriptGiveMon` immediately
//! before it, and `CB2_StartFirstBattle`'s `B_TRANSITION_BLUR`. None of
//! that exists here — there is no script engine to run it — so this module
//! is a construction/driver pair a future script-engine hookup can call,
//! exercised directly by this module's own tests today rather than through
//! [`crate::flow::overworld_phase::OverworldPhase`]. That gap is recorded on
//! the `src/battle_setup.c` ledger entry rather than papered over; issue
//! #221 scoped this slice to exactly this "smaller deterministic
//! construction path" instead.
//!
//! # RNG stream
//!
//! Off the same single shared stream as every other `crate::flow` handoff
//! (`crate::flow::wild_encounter`'s module docs): the enemy's personality
//! and IVs ([`battle::build_pokemon_with_random_personality`] — *not*
//! [`battle::build_wild_pokemon`]'s nature-forced path; see that function's
//! docs for exactly why `CreateMon`'s draw order differs from
//! `CreateWildMon`'s), then [`battle::Battle::new`]'s turn-number draw (and
//! its conditional speed-tie draw). That is upstream's own order too:
//! `SetUpBattleVarsAndBirchZigzagoon` (`:684`) runs before `BeginBattleIntro`
//! reaches `BattleStartClearSetData`'s `gRandomTurnNumber = Random()`
//! (`battle_main.c:3140`, `:3019` calls it) — construction draws first, the
//! turn-number draw after, exactly as [`start_first_battle`] orders them.

use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, PlayerAction, StatStages};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// `SPECIES_ZIGZAGOON` (`include/constants/species.h`) — Emerald's scripted
/// first-battle opponent (`pokeemerald/src/battle_controllers.c:69`), not
/// R/S's Poochyena.
pub const FIRST_BATTLE_OPPONENT_SPECIES: assets::SpeciesId = assets::SpeciesId(288);

/// The scripted opponent's fixed level (`battle_controllers.c:69`).
pub const FIRST_BATTLE_OPPONENT_LEVEL: u8 = 2;

/// Build the scripted `BATTLE_TYPE_FIRST_BATTLE` opponent and start the
/// battle around it — `SetUpBattleVarsAndBirchZigzagoon`'s `CreateMon` call
/// (module docs) followed by [`battle::Battle::new`] with `first_battle =
/// true`.
///
/// Draws in upstream's order off the shared stream (module docs, "RNG
/// stream"): the enemy's personality and IVs, then `Battle::new`'s
/// `gRandomTurnNumber` (and its conditional speed-tie draw). The moveset
/// comes from [`battle::initial_moveset`] — `GiveBoxMonInitialMoveset`,
/// which draws nothing — so a level-2 Zigzagoon really knows Tackle and
/// Growl (`crates/battle/tests/turn_engine/first_battle.rs`'s module doc
/// has the full learnset derivation).
///
/// # Errors
///
/// Whatever [`battle::build_pokemon_with_random_personality`] or
/// [`battle::Battle::new`] reports. Unreachable in practice: species/level
/// are fixed, dex-resident constants and `player_lead`'s moveset is the
/// caller's concern, not this function's — pinned by this module's own
/// tests rather than asserted away with an `expect`, so a future dex change
/// that broke either fails a test instead of unwrapping in production.
pub fn start_first_battle(
    player_lead: BattlePokemon,
    rng: &mut Rng,
) -> Result<Battle, BattleError> {
    let dex = Dex::new();
    let moves = battle::initial_moveset(FIRST_BATTLE_OPPONENT_SPECIES, FIRST_BATTLE_OPPONENT_LEVEL);
    let opponent = battle::build_pokemon_with_random_personality(
        &dex,
        FIRST_BATTLE_OPPONENT_SPECIES,
        FIRST_BATTLE_OPPONENT_LEVEL,
        moves,
        &mut SharedRng::new(rng),
    )?;
    Battle::new(dex, player_lead, opponent, true, &mut SharedRng::new(rng))
}

/// Play one turn of the in-progress first battle in `slot`, headlessly
/// (module docs). A no-op if `slot` is empty.
///
/// Mirrors `crate::flow::wild_encounter::advance_wild_battle`'s shape
/// exactly (turn, write-back, neutral stat-stage reset, error-ends-the-
/// battle-too) with one deliberate difference: the action chosen every turn
/// is [`PlayerAction::UseMove`]`(0)`, never [`PlayerAction::Run`], because
/// `first_battle` makes running an instant [`BattleError::RunForbidden`]
/// rather than a legal (if often futile) attempt (module docs). See
/// `advance_wild_battle`'s own doc comment for the write-back and
/// stat-stage-reset rationale — it is identical here.
pub fn advance_first_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let failed = match battle.take_turn(PlayerAction::UseMove(0), &mut SharedRng::new(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("first battle: turn failed ({error:?}) -- ending the encounter");
            true
        }
    };
    let outcome = battle.outcome();
    if !failed && outcome.is_none() {
        return None;
    }
    let mut mon = battle.player().clone();
    *mon.stages_mut() = StatStages::default();
    *lead = Some(mon);
    *slot = None;
    outcome
}

#[cfg(test)]
mod tests;
