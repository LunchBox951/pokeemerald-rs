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
//! [`crate::flow::first_battle`] (issue #221) is the sibling driver for the
//! scripted `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon fight, kept separate rather
//! than folded in here: "always attempt to run" is not just a different
//! choice for that battle, it is an *illegal* one
//! ([`battle::BattleError::RunForbidden`]), so the two drivers cannot share
//! one action policy.
//!
//! # The white-out, modelled (issue #261)
//!
//! Upstream never leaves the overworld holding a fainted party. Losing a wild
//! battle sets `gBattleOutcome` to a defeat, `CB2_EndWildBattle`
//! (`src/battle_setup.c:602-616`) routes to `CB2_WhiteOut`
//! (`src/overworld.c:1550-1570`) instead of `CB2_ReturnToField`, and
//! `DoWhiteOut` (`:358-366`) runs the white-out script, halves the player's
//! money, **heals the whole party** (`HealPlayerParty`), and warps to the
//! last heal location.
//! [`crate::flow::overworld_phase::white_out::OverworldPhase::white_out`]
//! now reproduces exactly that — see its own module docs for the full
//! upstream citation — called by
//! [`crate::flow::overworld_phase::wild_battle::OverworldPhase::advance_wild_battle_frame`]
//! the instant [`advance_wild_battle`] reports
//! [`battle::BattleOutcome::PlayerLost`], after that call has already
//! written the fainted lead back into `party_lead`. So the fainted state
//! this doc comment used to describe as unmodelled now exists for at most
//! the one frame between the battle ending and the white-out running, never
//! long enough for another step to observe it.
//!
//! **Formerly a fail-closed guard, now retired.** Before this issue, this
//! module instead refused the encounter roll outright while the lead was
//! fainted (`lead_can_fight`, since removed) — a state upstream itself can
//! never reach, since the white-out above already heals the party before
//! the player's next step. That guard was the *documented interim posture*
//! for the gap this section used to describe as unmodelled; now that the
//! real white-out runs, the guard's premise (a fainted lead can persist into
//! a later step) is false in production, so no guard is left to write. See
//! `wild_encounter::tests::a_lost_battle_now_heals_the_party_and_halves_money`
//! (plus its pack-gated companion
//! `wild_encounter::tests::real_pack_a_lost_wild_battle_warps_home_to_the_default_heal_location`)
//! for the behavioural tests that replace the old
//! `a_lost_battle_leaves_a_fainted_lead_and_no_later_grass_step_draws`
//! regression, and their own doc comments for why.
//!
//! # The unmodelled gate ahead of all this
//!
//! Route 101's grass is fenced off at the start of a real playthrough by
//! `Route101_EventScript_...`'s coord events and `VAR_ROUTE101_STATE` (the
//! "wait, don't go out — it's unsafe" gate), and the player has no Pokémon
//! until the rescue chain's `CB2_GiveStarter` hands one over. This port has
//! no script engine, so neither exists: the encounter roll here is reachable
//! the moment the player walks into grass, and the starter handout is stood
//! in for by [`crate::new_game::provisional_starter`] — a deterministic
//! level-5 Treecko assigned at
//! [`crate::flow::overworld_phase::OverworldPhase::load_default`] (issue
//! #207 review), so a fresh game really fights the encounter it rolls. A
//! phase whose party lead is `None` (a bare test phase, or a driver that
//! clears it) still logs the encounter and starts no battle. The script-gate
//! gap and the stood-in handout are recorded on the ledger rather than
//! papered over.

use battle::{
    Battle, BattleError, BattleOutcome, BattlePokemon, BattleRng, Dex, PlayerAction, StatStages,
};
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
///
/// The type stays inside `crate::flow` — it is `pub(super)`, not `pub`, so
/// no crate-external caller can hold one — and [`SharedRng::new`] is
/// `pub(super)` for the same reach, so [`crate::flow::first_battle`] (issue
/// #221) can borrow the same adapter for its own construction/driver pair
/// without a second newtype duplicating this one's `BattleRng` impl.
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
/// `field_event_fired` is the precedence one: everything upstream runs
/// *above* the arrow poll returns `TRUE` out of `ProcessPlayerFieldInput`
/// when it fires — `CheckStandardWildEncounter` at `:162` when a wild
/// Pokémon appears, and `TryStartStepBasedScript` at `:155-161` when a coord
/// event does (`TryStartCoordEventScript`, `:485-486`; this port's own is
/// the Route 101 first-battle trigger, issue #231) — so the arrow poll two
/// lines below never runs on such a frame. Its caller
/// (`crate::flow::overworld_phase::OverworldPhase::step`) computes that
/// single "something already claimed this frame" value once and hands the
/// same one to [`field_input_consumed`].
///
/// Like [`roll_eligible_landing`]'s `preempting` arm, the `field_event_fired`
/// half is presently unreachable rather than merely untaken: for an
/// encounter, the arrow poll reads the tile the player already stands on,
/// which on the drain frame is the same tile the roll read, and no metatile
/// behavior is both an arrow-warp id and a land-encounter id; for the coord
/// trigger, Route 101 declares no `warp_events` at all
/// (`data/maps/Route101/map.json`), so no arrow warp can exist on the one
/// map the trigger is wired to (pinned by
/// `crate::flow::overworld_phase::tests::route_101_has_no_warp_events_so_the_trigger_can_never_race_one`).
/// It encodes upstream's ordering all the same, so a future slice that
/// widens either id set — or gives Route 101 a warp — inherits the right
/// precedence instead of a silent double-fire.
pub(super) const fn arrow_poll_open(in_transit: bool, field_event_fired: bool) -> bool {
    !in_transit && !field_event_fired
}

/// Whether this frame's field input has already been consumed by the time
/// `ProcessPlayerFieldInput` reaches `TryStartInteractionScript` (`:172`).
///
/// A resolved warp and a fired field event ([`arrow_poll_open`]'s own
/// `field_event_fired`: a wild encounter, or a coord-event script such as
/// the Route 101 first-battle trigger) consume it identically upstream —
/// both return `TRUE` from `ProcessPlayerFieldInput` before `:172` is reached
/// (`:155-161` and `:162` respectively) — so a same-frame A press finds
/// nothing to talk to. [`WarpTrigger::Unsupported`] deliberately does *not*
/// count: this port could not resolve that warp, nothing happened, and
/// swallowing the player's A press on top of that would compound one
/// unmodelled destination into a second missing behaviour.
///
/// The coord-event half of `field_event_fired` is unreachable here too, and
/// for a data reason rather than a logic one: none of Route 101's six object
/// events stands adjacent to either rescue tile, so no A press can find an
/// interaction to discard on the frame the trigger fires (pinned by
/// `crate::flow::overworld_phase::tests::no_route_101_object_event_stands_beside_the_rescue_trigger_tiles`).
pub(super) const fn field_input_consumed(
    field_event_fired: bool,
    warp_trigger: Option<WarpTrigger>,
) -> bool {
    field_event_fired || matches!(warp_trigger, Some(WarpTrigger::Resolved { .. }))
}

/// Whether every wild mon `map`'s land table can roll is one the battle
/// engine can actually fight — the pre-flight over
/// [`battle::ensure_wild_startable`] that decides, **before any draw ever
/// happens**, whether encounter rolls are enabled on this map at all (issue
/// #207 review).
///
/// Upstream never rejects a wild battle, so a moveset this engine cannot
/// execute yet (Route 102's level-3 Seedot knows Bide and Harden, neither in
/// the damaging nor the stat-lowering pipeline) has no stream-faithful
/// failure mode once the roll has happened: `CreateWildMon`'s five draws are
/// already spent by the time [`battle::Battle::new`] screens the moveset,
/// and dropping the battle there leaves draws with no upstream counterpart.
/// So the whole table is screened *up front*: on a map that fails, a grass
/// step draws nothing and moves no encounter bookkeeping, and the refusal is
/// logged once here (at the map transition that computes it) rather than
/// per step -- the same no-draw-at-all shape the former `lead_can_fight`
/// guard used for its own now-retired reason (module docs, "The white-out,
/// modelled").
///
/// A map with no wild header, or no land table, is trivially fightable —
/// there is nothing to roll, and [`roll_for_step`] already resolves that to
/// a no-op. The gate widens automatically as the turn engine gains move
/// support, and the ratchet tests pinning today's boundary (Route 101 in,
/// Route 102 out) must be updated alongside it.
pub(super) fn map_wild_table_fightable(map: assets::MapId) -> bool {
    let Some(land) = assets::WildEncounterTable::new()
        .get_by_map(map)
        .and_then(|header| header.land)
    else {
        return true;
    };
    let dex = Dex::new();
    for slot in &land.mons {
        for level in slot.min_level..=slot.max_level {
            if let Err(error) = battle::ensure_wild_startable(&dex, slot.species, level) {
                eprintln!(
                    "wild encounter: {}'s land table can roll species {:?} at level {level}, \
                     which the battle engine can't fight yet ({error:?}) -- \
                     no encounters will roll on this map",
                    map.name(),
                    slot.species,
                );
                return false;
            }
        }
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
/// The order is upstream's; the *count* is one short of it. Between those
/// two, `CB2_InitBattleInternal` calls `SetWildMonHeldItem`
/// (`src/battle_main.c:700`), whose `u16 rnd = Random() % 100`
/// (`src/pokemon.c:6682`) is gated only on `!(gBattleTypeFlags & (LEGENDARY |
/// TRAINER | PYRAMID | PIKE))` — and `DoStandardWildBattle` sets
/// `gBattleTypeFlags = 0` (`src/battle_setup.c:408`), so the gate passes and
/// upstream draws it. This port models no held items at all
/// ([`battle::BattlePokemon`] has no such field), so that draw produces
/// nothing here and is simply absent: from the turn-number draw onward this
/// handoff has consumed one fewer value than upstream would have. The gap is
/// pre-existing and uniform — [`crate::flow::first_battle`]'s scripted fight
/// skips the identical draw for the identical reason (its module docs' "RNG
/// stream" section), and it is enumerated as NOT-modelled on the
/// `src/battle_setup.c#CB2_StartFirstBattle` ledger entry.
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
        &mut SharedRng::new(rng),
    )?;
    // `false`: this handoff is #169's ongoing Route 101 grass encounter, not
    // the scripted intro Zigzagoon fight -- see issue #187's module docs on
    // `battle::Battle` for exactly what `BATTLE_TYPE_FIRST_BATTLE` changes.
    // That one-time narrative event's own construction and headless driver
    // now live in `crate::flow::first_battle` (issue #221) -- its own
    // `gBattleTypeFlags` assignment and scripted opponent
    // (`SetUpBattleVarsAndBirchZigzagoon`, `src/battle_controllers.c:67`-`:72`)
    // are not this function's concern, and this Route 101 handoff is
    // unchanged by that module's addition. [`advance_wild_battle`]'s "always
    // try to run" driver policy depends on running staying legal here, which
    // `first_battle = true` would break -- exactly why
    // `crate::flow::first_battle` needs its own driver rather than reusing
    // this one.
    Battle::new(dex, player_lead, wild, false, &mut SharedRng::new(rng))
}

/// Play one turn of the in-progress wild battle in `slot`, headlessly
/// (module docs). A no-op if `slot` is empty.
///
/// When the battle ends, `slot` is emptied and the player's mon — damage and
/// PP included — is written back into `lead`, so the overworld keeps the
/// state the battle left it in, the way upstream's `gPlayerParty[0]`
/// persists. In-battle stat stages are **reset to neutral** first: upstream
/// keeps them in `gBattleMons[].statStages` only — set to
/// `DEFAULT_STAT_STAGE` when each battler's `gBattleMons` entry is built at
/// battle entry (`BattleIntroDrawTrainersOrMonsSprites`,
/// `battle_main.c:3423-3424`) and again at switch-in
/// (`SwitchInClearSetData`, `:3161`) — and the party's `struct Pokemon` has
/// no stat-stage field at all, so a String Shot taken this battle can never
/// slow the *next* one. Returns the outcome on exactly that frame and
/// `None` on every other.
///
/// That write-back is unconditional, [`BattleOutcome::PlayerLost`] included,
/// so a lost battle really does leave a fainted mon in `lead` for exactly
/// the one frame it takes the caller
/// ([`crate::flow::overworld_phase::wild_battle::OverworldPhase::advance_wild_battle_frame`])
/// to notice the outcome and call
/// [`crate::flow::overworld_phase::white_out::OverworldPhase::white_out`]
/// (module docs, "The white-out, modelled") -- the same frame upstream's own
/// `CB2_EndWildBattle` -> `CB2_WhiteOut` routing takes to reach
/// `HealPlayerParty`.
///
/// A turn that *errors* is not survivable here — there is no action menu to
/// choose differently with — so it ends the battle too: the error is logged,
/// the mon is still written back, and `None` is returned rather than an
/// outcome the engine never reported. The player's Run action is valid, but
/// an unsuccessful escape still gives the wild side a turn; if all of its
/// moves are spent, forced Struggle can reach the currently unsupported move
/// effect and return [`BattleError::UnsupportedMoveEffect`].
pub(super) fn advance_wild_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let failed = match battle.take_turn(PlayerAction::Run, &mut SharedRng::new(rng)) {
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
    let mut mon = battle.player().clone();
    // Stat stages are battle-local upstream (`gBattleMons[].statStages`;
    // the party struct has no such field), so they never survive into the
    // overworld copy.
    *mon.stages_mut() = StatStages::default();
    *lead = Some(mon);
    *slot = None;
    outcome
}

#[cfg(test)]
mod tests;
