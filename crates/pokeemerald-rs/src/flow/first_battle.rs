//! The scripted `BATTLE_TYPE_FIRST_BATTLE` construction and its own headless
//! driver (issue #221, ladders to S-6/I-4).
//!
//! Upstream reaches this battle through `CB2_StartFirstBattle`
//! (`pokeemerald/src/battle_setup.c:930`-`:945`), which sets
//! `gBattleTypeFlags = BATTLE_TYPE_FIRST_BATTLE` and hands off to
//! `CB2_InitBattle`. From there `CB2_InitBattleInternal`
//! (`src/battle_main.c:684`) calls `SetUpBattleVarsAndBirchZigzagoon`
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
//! # What is wired up around this module, and what still is not
//!
//! Since issue #231, [`crate::flow::overworld_phase::OverworldPhase`] *is*
//! a real production caller: its Route 101 rescue trigger
//! (`flow::overworld_phase::first_battle_trigger`) recognizes the map's
//! own `VAR_ROUTE101_STATE` coord events and drives this construction/
//! driver pair through the per-frame step path. What remains unmodelled is
//! the narrative dressing upstream wraps around that trigger: the wild
//! Zigzagoon cornering Professor Birch, the Birch-bag dialog, the
//! starter-choose UI and `CB2_GiveStarter`'s `ScriptGiveMon` immediately
//! before the fight, and `CB2_StartFirstBattle`'s `B_TRANSITION_BLUR` —
//! there is still no script engine to run those. That remaining gap is
//! recorded on the `src/battle_setup.c` ledger entry rather than papered
//! over; issue #221 scoped the original slice to the construction path,
//! and #231 to the deterministic trigger.
//!
//! # RNG stream
//!
//! Off the same single shared stream as every other `crate::flow` handoff
//! (`crate::flow::wild_encounter`'s module docs): the enemy's personality
//! and IVs ([`battle::build_pokemon_with_random_personality`] — *not*
//! [`battle::build_wild_pokemon`]'s nature-forced path; see that function's
//! docs for exactly why `CreateMon`'s draw order differs from
//! `CreateWildMon`'s), then [`battle::Battle::new`]'s turn-number draw (and
//! its conditional speed-tie draw).
//!
//! The *order* is upstream's own: `SetUpBattleVarsAndBirchZigzagoon`
//! (`battle_main.c:684`) runs before `BeginBattleIntro` reaches
//! `BattleStartClearSetData`'s `gRandomTurnNumber = Random()`
//! (`battle_main.c:3140`, called from `:3021`) — construction draws first,
//! the turn-number draw after, exactly as [`start_first_battle`] orders
//! them.
//!
//! The *count* is one short of upstream's, and knowingly so.
//! `CB2_InitBattleInternal` interposes `SetWildMonHeldItem`
//! (`battle_main.c:700`) between those two, and its `u16 rnd = Random() %
//! 100` (`src/pokemon.c:6682`) is gated only on `!(gBattleTypeFlags &
//! (LEGENDARY | TRAINER | PYRAMID | PIKE))` — a gate
//! `BATTLE_TYPE_FIRST_BATTLE` passes, since `CB2_StartFirstBattle` sets that
//! flag *alone* (`src/battle_setup.c:937`). Upstream therefore spends six
//! draws reaching turn one (personality 2 + IVs 2 + held item 1 + turn
//! number 1) where this port spends five. Held items are unmodelled here
//! outright — [`battle::BattlePokemon`] has no such field, so there is no
//! value for that draw to produce — and the gap is neither new to this
//! battle type nor this slice's to close: an ordinary wild encounter skips
//! exactly the same draw, `DoStandardWildBattle` setting `gBattleTypeFlags =
//! 0` (`src/battle_setup.c:408`) so the same gate passes there too (see
//! [`crate::flow::wild_encounter::start_wild_battle`]). It is enumerated as
//! NOT-modelled on the `src/battle_setup.c#CB2_StartFirstBattle` ledger
//! entry rather than papered over.

use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, PlayerAction, StatStages};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// `SPECIES_ZIGZAGOON` (`include/constants/species.h`) — Emerald's scripted
/// first-battle opponent (`pokeemerald/src/battle_controllers.c:70`), not
/// R/S's Poochyena.
pub const FIRST_BATTLE_OPPONENT_SPECIES: assets::SpeciesId = assets::SpeciesId(288);

/// The scripted opponent's fixed level (`battle_controllers.c:70`).
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
/// Mirrors `crate::flow::wild_encounter::advance_wild_battle`'s shape (turn,
/// write-back, neutral stat-stage reset, error-ends-the-battle-too) with one
/// deliberate difference: the action chosen every turn is
/// [`PlayerAction::UseMove`]`(0)`, never [`PlayerAction::Run`], because
/// `first_battle` makes running an instant [`BattleError::RunForbidden`]
/// rather than a legal (if often futile) attempt (module docs).
///
/// The stat-stage reset is `advance_wild_battle`'s verbatim — stat stages
/// live in `gBattleMons[].statStages` only and never reach the party struct;
/// see that function's doc comment for the citations. The **unconditional**
/// write-back is shaped the same but justified differently: for an ordinary
/// wild battle it is a knowing deviation (upstream white-outs and heals on a
/// loss), while here it is upstream's own behaviour. `CB2_EndFirstBattle`
/// (`pokeemerald/src/battle_setup.c:950`-`:954`) runs
/// `Overworld_ClearSavedMusic` and goes straight to
/// `CB2_ReturnToFieldContinueScriptPlayMapMusic` with no `IsPlayerDefeated`
/// branch at all, unlike `CB2_EndWildBattle` (`:602`-`:616`) — losing the
/// scripted Zigzagoon fight really does leave the player standing on Route
/// 101 with a fainted lead, so writing one back is fidelity, not a gap.
///
/// Choosing a move instead of running has one consequence the Run driver
/// does not: [`PlayerAction::UseMove`] **spends PP**, and this driver
/// persists it (the write-back copies the battle's player mon, PP included).
/// The error arm is therefore genuinely reachable here rather than merely
/// defensive — a lead whose slot 0 has been drained to zero PP fails
/// [`Battle::take_turn`]'s pre-draw validation with
/// [`BattleError::NoPpRemaining`]`(0)`, and a wild side with every slot spent
/// forces Struggle, whose effect this slice cannot execute
/// ([`BattleError::UnsupportedMoveEffect`]). Neither is survivable without an
/// action menu to choose differently with, so either ends the battle: the
/// error is logged, the mon is still written back (drained PP and all),
/// `slot` is emptied, and `None` is returned rather than an outcome the
/// engine never reported. A caller looping until `Some(outcome)` cannot tell
/// that abort from an ordinary ongoing turn by the return value alone and
/// must check `slot` as well — this module's tests pin exactly that contract.
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
