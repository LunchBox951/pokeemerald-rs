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
//! # The unmodelled gate *behind* all this: losing
//!
//! Upstream never leaves the overworld holding a fainted party. Losing a wild
//! battle sets `gBattleOutcome` to a defeat, `CB2_EndWildBattle`
//! (`src/battle_setup.c:602-616`) routes to `CB2_WhiteOut`
//! (`src/overworld.c:1550-1570`) instead of `CB2_ReturnToField`, and
//! `DoWhiteOut` (`:358-366`) runs the white-out script, halves the player's
//! money, **heals the whole party** (`HealPlayerParty`), and warps to the last
//! heal location. None of that is modelled here `(no script engine, no money,
//! no heal locations, no warp-to-Pokémon-Center)`, so
//! [`advance_wild_battle`] writes the player's mon back unconditionally —
//! fainted included — and the player is left standing in the grass where
//! upstream would already be waking up in a Pokémon Center.
//!
//! Rather than invent a white-out, this port **fails closed** at the only
//! place the gap is RNG-observable: [`lead_can_fight`] refuses the encounter
//! roll while the lead mon is fainted, so a grass step in that state draws
//! nothing at all instead of spending a wild-mon construction and
//! `Battle::new`'s turn-number draw on a battle that can only error out. That
//! is a state upstream cannot reach, so suppressing it costs no fidelity and
//! keeps the stream where a real playthrough would have it. The white-out
//! itself is enumerated as NOT-modelled on this slice's ledger entry for
//! `src/battle_setup.c#BattleSetup_StartWildBattle`.
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
use engine::overworld::warp::WarpTrigger;
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

/// Which completed step, if any, [`roll_for_step`] is allowed to see this
/// frame — upstream's `ProcessPlayerFieldInput` precedence
/// (`field_control_avatar.c:155-172`), as a value.
///
/// Upstream reaches `CheckStandardWildEncounter` (`:162`) only by falling
/// *through* the step-based script/warp block at `:155-161`; a warp there
/// returns `TRUE` out of `ProcessPlayerFieldInput` and the encounter check
/// never runs. That matters even though a door tile can never itself roll an
/// encounter (no behavior is both `IsWarpMetatileBehavior` and
/// `MetatileBehavior_IsLandWildEncounter`): `CheckStandardWildEncounter`
/// *always* writes `sPrevMetatileBehavior` and consumes an immunity step
/// (`:668-686`), so letting it see a warp frame would corrupt the very
/// bookkeeping the next real grass step draws against.
///
/// `preempting` is this port's own #194 addition — an arrow warp that fired
/// *before* movement, consuming the frame the way upstream's pre-`PlayerStep`
/// `ProcessPlayerFieldInput` does. It cannot coincide with a `landed` step
/// (a preempted frame requires the player to have been at rest, and
/// `pending_landing` is empty at rest — see `advance_or_skip_for_preempt`'s
/// own `debug_assert`), so that arm is defensive rather than reachable; it is
/// spelled out anyway so the precedence stays correct if a future slice makes
/// the two coincide.
pub(super) fn roll_eligible_landing(
    landed: Option<TilePos>,
    preempting: Option<WarpTrigger>,
    door_warp: Option<WarpTrigger>,
) -> Option<TilePos> {
    if preempting.is_some() || door_warp.is_some() {
        return None;
    }
    landed
}

/// Whether the every-frame `TryArrowWarp` poll (`field_control_avatar.c:164-168`)
/// is still open this frame.
///
/// Two gates, and they are not the same gate. `in_transit` is this port's
/// counterpart to the `T_TILE_CENTER`/`T_NOT_MOVING` test that sets
/// `input->heldDirection` upstream in the first place (`:95-112`).
/// `encounter_fired` is the precedence one: `CheckStandardWildEncounter` at
/// `:162` returns `TRUE` when a wild Pokémon appears, and
/// `ProcessPlayerFieldInput` returns with it — so the arrow poll two lines
/// below never runs on the frame an encounter fires.
///
/// Like [`roll_eligible_landing`]'s `preempting` arm, the `encounter_fired`
/// half is presently unreachable rather than merely untaken: the arrow poll
/// reads the tile the player already stands on, which on the drain frame is
/// the same tile the roll read, and no metatile behavior is both an
/// arrow-warp id and a land-encounter id. It encodes upstream's ordering all
/// the same, so a future slice that widens either id set inherits the right
/// precedence instead of a silent double-fire.
pub(super) const fn arrow_poll_open(in_transit: bool, encounter_fired: bool) -> bool {
    !in_transit && !encounter_fired
}

/// Whether this frame's field input has already been consumed by the time
/// `ProcessPlayerFieldInput` reaches `TryStartInteractionScript` (`:172`).
///
/// A resolved warp and a fired encounter consume it identically upstream —
/// both return `TRUE` from `ProcessPlayerFieldInput` before `:172` is reached
/// (`:155-161` and `:162` respectively) — so a same-frame A press finds
/// nothing to talk to. [`WarpTrigger::Unsupported`] deliberately does *not*
/// count: this port could not resolve that warp, nothing happened, and
/// swallowing the player's A press on top of that would compound one
/// unmodelled destination into a second missing behaviour.
pub(super) const fn field_input_consumed(
    encounter_fired: bool,
    warp_trigger: Option<WarpTrigger>,
) -> bool {
    encounter_fired || matches!(warp_trigger, Some(WarpTrigger::Resolved { .. }))
}

/// Whether an encounter may be rolled at all with `lead` as the party's lead
/// mon — the fail-closed half of the unmodelled white-out (module docs).
///
/// `None` (a fresh save with no party at all) is **allowed**: that is exactly
/// upstream's own state before Birch's bag, where the roll happens for real
/// and only the battle is missing, so the stream stays upstream's and
/// [`crate::flow::overworld_phase::OverworldPhase::begin_wild_battle`] logs
/// the encounter it cannot fight.
///
/// A *fainted* lead here means a fainted-**only** party, a state upstream
/// cannot be in: losing routes through `CB2_WhiteOut`, which heals the party
/// before the player ever takes another step (module docs). (Upstream's
/// `gPlayerParty[0]` can be fainted with a *live* party behind it and rolls
/// normally — `IsWildLevelAllowedByRepel`, `wild_encounter.c:878-887`, skips
/// zero-HP mons rather than bailing — but this port models a one-mon party,
/// so lead-fainted and party-fainted coincide; a future multi-slot party
/// slice must revisit this guard.) Rolling here would draw a wild mon's five
/// nature/personality/IV values plus `Battle::new`'s `gRandomTurnNumber` on
/// every grass step, only to reject the battle afterwards — repeatable draws
/// with no upstream counterpart. So the roll is refused before it draws
/// anything, and the refusal is logged rather than silent.
pub(super) fn lead_can_fight(lead: Option<&BattlePokemon>) -> bool {
    if lead.is_some_and(BattlePokemon::is_fainted) {
        eprintln!(
            "wild encounter: the lead mon has fainted -- no roll until it is healed \
             (upstream would have whited out; see flow::wild_encounter's module docs)"
        );
        return false;
    }
    true
}

/// Build the wild mon a [`WildEncounter`] describes and start a
/// [`battle::Battle`] around it — `CreateWildMon` (`wild_encounter.c:379`)
/// followed by `BattleSetup_StartWildBattle` (`src/battle_setup.c:389-395`,
/// whose non-Safari arm is `DoStandardWildBattle`, `:402-419`), minus the
/// battle *transition* the latter schedules (`CreateBattleStartTask` `:414`
/// and its `Task_BattleStart`, `:351-376`).
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
/// That write-back is unconditional, [`BattleOutcome::PlayerLost`] included,
/// so a lost battle really does leave a fainted mon in `lead` — upstream
/// instead white-outs and heals the party, which this port does not model
/// (module docs). [`lead_can_fight`] is the guard that keeps that state from
/// becoming RNG-observable.
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
