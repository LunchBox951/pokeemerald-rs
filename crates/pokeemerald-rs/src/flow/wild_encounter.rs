//! The overworld → battle handoff for a wild encounter (I-4, issue #169).
//!
//! `engine::overworld::wild_encounter` owns the *roll* — the engine-domain
//! decision that a wild Pokémon appears, at which species and level. This
//! module owns the *handoff* `(oop-boundaries)`: turning that
//! [`WildEncounter`] into a battle-ready [`BattlePokemon`] and a
//! [`battle::Battle`], which needs both the `engine` and the `battle` crate
//! and therefore belongs in the integration layer that already depends on
//! each.
//!
//! # One RNG stream across the boundary
//!
//! Upstream draws everything from one global generator, so the wild mon's
//! nature/personality/IV rolls (`CreateWildMon`, `wild_encounter.c:379`) read
//! the values immediately after the slot/level rolls that chose it, and
//! `BattleStartClearSetData`'s `gRandomTurnNumber` reads the one after those
//! `(behavioral-fidelity)`. Neither `engine` nor `battle` depends on the
//! other, so each names its own view of that stream —
//! [`engine::rng::RandomSource`] and [`battle::BattleRng`]. [`SharedRng`]
//! is the adapter that lets one owned [`engine::rng::Rng`] satisfy both, so
//! the handoff never splits the sequence.
//!
//! # What "runs a battle" means at this slice
//!
//! There is no battle scene: no transition animation, no battle backdrop,
//! no action menu, no party/bag/switch options, and no trainer AI (`I-5`).
//! [`advance_wild_battle`] therefore drives the encounter headlessly, one
//! [`battle::Battle::take_turn`] per frame, with the player attempting to
//! **run** every turn — the one action a driver can choose without inventing
//! a decision the absent UI would have asked the player for. Every *rule* it
//! exercises is the real engine: turn order, the wild mon's own move pick,
//! `TryRunFromBattle`'s escape odds, damage, and fainting. The stand-in is
//! the choice of action, and only that; it is enumerated as NOT-modelled on
//! this slice's ledger entry for `src/battle_setup.c`.
//!
//! # The unmodelled gate ahead of all this
//!
//! Route 101's grass is fenced off at the start of a real playthrough by
//! `Route101_EventScript_...`'s coord events and `VAR_ROUTE101_STATE` (the
//! "wait, don't go out — it's unsafe" gate), and the player has no Pokémon
//! until Birch's bag script hands one over. This port has no script engine,
//! so neither exists: the encounter roll here is reachable the moment the
//! player walks into grass, and [`crate::flow::overworld_phase`]'s party
//! lead is `None` on a fresh save exactly as upstream's party is — which is
//! why the production path logs the encounter and starts no battle until a
//! party mon exists. Both gaps are recorded on the ledger rather than
//! papered over.

use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, BattleRng, Dex, PlayerAction};
use engine::overworld::wild_encounter::{WildEncounter, WildEncounterState};
use engine::overworld::{MapRuntime, TilePos};
use engine::rng::Rng;

/// One [`engine::rng::Rng`], seen through [`battle::BattleRng`].
///
/// A borrowing newtype rather than an owned generator: the caller keeps the
/// single stream and lends it for the duration of a `battle` call, so a
/// battle draw and an overworld draw can never come from different sequences
/// (module docs). Both traits' `next_u32` compose two `next_u16` draws low
/// half first, so forwarding only `next_u16` is enough to keep them in
/// lockstep — but `next_u32` is forwarded explicitly anyway so a future
/// change to either default cannot silently desynchronise them.
struct SharedRng<'a>(&'a mut Rng);

impl BattleRng for SharedRng<'_> {
    fn next_u16(&mut self) -> u16 {
        self.0.next_u16()
    }

    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
}

/// Roll for one completed step (upstream's `input->checkStandardWildEncounter`
/// slot in `ProcessPlayerFieldInput`, `field_control_avatar.c:162`).
///
/// Resolves the current map's `gWildMonHeaders` entry
/// (`GetCurrentMapWildMonHeaderId`, keyed here by the map's `MAP_*` name —
/// see `assets::wild_encounters`' module docs on why that, not a
/// group/num pair) and the metatile behavior under the player, then hands
/// both to [`WildEncounterState::check_standard_wild_encounter`].
///
/// `landed` is the tile a step just finished walking onto, or `None` when
/// this frame is not an eligible step at all — no step drained, or a warp
/// already claimed the frame (upstream returns out of
/// `ProcessPlayerFieldInput` before `:162` in that case). `None` rolls
/// nothing and, crucially, **draws nothing**: the caller's gate is upstream's
/// gate, not a filter applied after the fact.
///
/// A tile whose attribute entry can't be decoded is treated as
/// [`engine::overworld::metatile_behavior::MB_NORMAL`] — ordinary ground,
/// which rolls nothing. That fails closed (no encounter) rather than
/// inventing grass, matching the surrounding module's policy for
/// undecodable behaviors.
pub(super) fn roll_for_step(
    state: &mut WildEncounterState,
    rng: &mut Rng,
    map: assets::MapId,
    runtime: &MapRuntime<'_>,
    landed: Option<TilePos>,
) -> Option<WildEncounter> {
    let (x, y) = landed?;
    let behavior = runtime
        .metatile_behavior(x, y)
        .unwrap_or(engine::overworld::metatile_behavior::MB_NORMAL);
    let header = assets::WildEncounterTable::new().get_by_map(map);
    state.check_standard_wild_encounter(behavior, header, rng)
}

/// Build the wild mon a [`WildEncounter`] describes and start a
/// [`battle::Battle`] around it — `CreateWildMon` (`wild_encounter.c:379`)
/// followed by `BattleSetup_StartWildBattle` (`src/battle_setup.c:265`),
/// minus the battle *transition* the latter schedules.
///
/// Draws in upstream's order off the shared stream: the wild mon's
/// nature/personality/IVs ([`battle::build_wild_pokemon`], five draws for a
/// first-try personality), then [`battle::Battle::new`]'s
/// `gRandomTurnNumber` (and its conditional speed-tie draw). The moveset
/// comes from [`battle::initial_moveset`] — upstream's
/// `GiveBoxMonInitialMoveset`, which draws nothing — so a Route 101 Wurmple
/// really knows Tackle and String Shot.
///
/// # Errors
///
/// Whatever [`battle::build_wild_pokemon`] or [`battle::Battle::new`]
/// reports: an unknown species/move, or a wild moveset the turn engine
/// cannot execute. Both validate before drawing, so a rejected encounter
/// leaves the stream where it found it — except that a rejection at
/// `Battle::new` happens *after* the wild mon's own five draws, exactly as
/// upstream's ordering implies.
pub(super) fn start_wild_battle(
    player_lead: BattlePokemon,
    encounter: WildEncounter,
    rng: &mut Rng,
) -> Result<Battle, BattleError> {
    let dex = Dex::new();
    let moves = battle::initial_moveset(encounter.species, encounter.level);
    let wild = battle::build_wild_pokemon(
        &dex,
        encounter.species,
        encounter.level,
        moves,
        &mut SharedRng(rng),
    )?;
    Battle::new(dex, player_lead, wild, &mut SharedRng(rng))
}

/// Play one turn of the in-progress wild battle in `slot`, headlessly
/// (module docs). A no-op if `slot` is empty.
///
/// When the battle ends, `slot` is emptied and the player's mon — damage,
/// PP, and stat stages included — is written back into `lead`, so the
/// overworld keeps the state the battle left it in, the way upstream's
/// `gPlayerParty[0]` persists. Returns the outcome on exactly that frame and
/// `None` on every other.
///
/// A turn that *errors* is not survivable here — there is no action menu to
/// choose differently with — so it ends the battle too: the error is logged,
/// the mon is still written back, and `None` is returned rather than an
/// outcome the engine never reported. Unreachable in practice, since the
/// only action this driver takes is a run attempt and
/// [`battle::Battle::take_turn`] rejects no run.
pub(super) fn advance_wild_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let failed = match battle.take_turn(PlayerAction::Run, &mut SharedRng(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("wild battle: turn failed ({error:?}) -- ending the encounter");
            true
        }
    };
    let outcome = battle.outcome();
    if !failed && outcome.is_none() {
        return None;
    }
    *lead = Some(battle.player().clone());
    *slot = None;
    outcome
}

#[cfg(test)]
mod tests;
