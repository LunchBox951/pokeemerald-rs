//! The white-out transition upstream runs on a battle loss (issue #261,
//! I-4/I-5): heal the whole party, halve the player's money, and warp home
//! to the last heal location -- `CB2_WhiteOut`'s `DoWhiteOut`
//! (`pokeemerald/src/overworld.c:358-366`).
//!
//! # Upstream's chain, precisely
//!
//! `CB2_EndWildBattle` (`pokeemerald/src/battle_setup.c:602-616`) and
//! `CB2_EndTrainerBattle` (`:1327-1349`, its own white-out branch at
//! `:1333-1338`) both route a defeat (`IsPlayerDefeated`, `:994-1010`:
//! `B_OUTCOME_LOST` or `B_OUTCOME_DREW`) to `SetMainCallback2(CB2_WhiteOut)`
//! instead of `CB2_ReturnToField`/`CB2_ReturnToFieldContinueScriptPlayMapMusic`.
//! `CB2_WhiteOut` (`src/overworld.c:1550-1570`) is a 120-frame fade, at the
//! end of which it calls, in order:
//!
//! 1. `DoWhiteOut` (`:358-366`):
//!    - `RunScriptImmediately(EventScript_WhiteOut)` --
//!      `EventScript_WhiteOut` (`data/event_scripts.s:585-588`) resets the
//!      Elite Four and moves Mr. Briney to whichever spot `VAR_BRINEY_LOCATION`
//!      names. **Not modelled**: this port has no Elite Four state and no
//!      Mr. Briney (both un-bundled content), and no message box for the
//!      "You blacked out!" text this script's caller
//!      (`BattleMainCB2`/message layer, not shown here) would otherwise
//!      display -- recorded on the `EventScript_WhiteOut` ledger entry.
//!    - `SetMoney(&gSaveBlock1Ptr->money, GetMoney(&gSaveBlock1Ptr->money) /
//!      2)` (`:361`) -- plain unsigned integer division, no rounding, no
//!      floor beyond what `/` already does. [`OverworldPhase::white_out`]
//!      reproduces exactly this: `self.save1.money /= 2`.
//!    - `HealPlayerParty()` (`src/script_pokemon_util.c:30-59`) -- full HP,
//!      full PP, cleared status for every party member.
//!      [`battle::BattlePokemon::heal`] is the per-mon effect; this port
//!      models one party slot ([`OverworldPhase::party_lead`]), so healing
//!      it is the whole of this port's `HealPlayerParty`.
//!    - `Overworld_ResetStateAfterWhiteOut` (`:399-...`, private upstream)
//!      -- clears field-effect/avatar transition state this port has no
//!      counterpart for (cycling road, Safari Zone, etc. flags this port
//!      does not model -- same unmodelled set `Overworld_ResetStateAfterFly`'s
//!      sibling functions already leave alone elsewhere in this port).
//!    - `SetWarpDestinationToLastHealLocation` (`:665-668`) --
//!      `sWarpDestination = gSaveBlock1Ptr->lastHealLocation`.
//!    - `WarpIntoMap` (`:626-631`) -- `ApplyCurrentWarp` (copies
//!      `sWarpDestination` into `gSaveBlock1Ptr->location` verbatim),
//!      `LoadCurrentMapData`, `SetPlayerCoordsFromWarp` (`:603-624`, the
//!      `WARP_ID_NONE` branch: use the destination's raw `x`/`y`).
//!      [`OverworldPhase::warp_to_position`] is this port's counterpart --
//!      see that method's own doc comment for the elevation and
//!      `save1.location` shape this produces.
//! 2. `ResetInitialPlayerAvatarState`, `ScriptContext_Init`,
//!    `UnlockPlayerFieldControls`, `FieldCB_WarpExitFadeFromBlack`,
//!    `DoMapLoadLoop` -- the ordinary map-load/control-handoff machinery
//!    every warp already goes through in this port
//!    ([`OverworldPhase::warp_to_position`] itself), so nothing extra is
//!    needed here.
//!
//! # Where this port calls it, and why both drivers share one method
//!
//! [`crate::flow::wild_encounter`]'s wild-battle driver
//! ([`super::wild_battle::OverworldPhase::advance_wild_battle_frame`]) and
//! [`crate::flow::route103_rival`]'s trainer-battle driver
//! ([`super::route103_rival_trigger::OverworldPhase::advance_route103_rival_battle_frame`])
//! both reach `IsPlayerDefeated`'s branch upstream -- a wild loss through
//! `CB2_EndWildBattle`, a trainer loss through `CB2_EndTrainerBattle` -- so
//! both need the identical three-step transition. [`OverworldPhase::white_out`]
//! is that one shared method rather than a duplicated one per driver
//! `(oop-boundaries)`, and it is also the reusable home issue #251's future
//! `special HealPlayerParty` script-command dispatch (a Pokémon Center
//! visit, not a loss) will call into for its own heal half --
//! [`battle::BattlePokemon::heal`] is written on the owned type both this
//! module and that future one need, not folded into this method, for
//! exactly that reason.
//!
//! # What this retires
//!
//! Before this issue, this port modelled *none* of the above and instead
//! failed closed at the one place the gap was RNG-observable: a fainted lead
//! could never re-roll a wild encounter
//! (`crate::flow::wild_encounter::lead_can_fight`, now removed) and could
//! never re-enter the Route 103 rival fight
//! (`battle::BattleError::FaintedBattler`, an emergent consequence of
//! [`battle::Battle::new`]/`new_trainer` refusing a fainted battler, not a
//! bespoke guard -- `route103_rival_trigger`'s own former module docs
//! recorded the resulting dead end). Both are unreachable now: this method
//! runs the instant a loss is reported, before the driver ever returns
//! control to [`super::OverworldPhase::step`], so no frame exists in which
//! the party is fainted and the player can act.
//!
//! # RNG stream
//!
//! Draws nothing. `DoWhiteOut`'s own calls are all plain state mutation --
//! no `Random()` call appears anywhere in the chain above.

use battle::Dex;

use super::{saved_map_id, OverworldPhase};

impl OverworldPhase {
    /// `DoWhiteOut` (module docs): heal the party, halve the money, warp
    /// home. The one call both battle drivers make on
    /// [`battle::BattleOutcome::PlayerLost`], after the battle has already
    /// written the fainted lead back into [`OverworldPhase::party_lead`] --
    /// so this always has a real mon to heal in production (a bare test
    /// phase with no lead is the only `None` case, and is a no-op there,
    /// same as [`OverworldPhase::begin_wild_battle`]'s own defensive `None`
    /// arm).
    ///
    /// A `last_heal_location` that cannot be resolved to a known map -- in
    /// practice only a hand-edited save: even
    /// [`crate::new_game::default_last_heal_location`]'s `Other`-gender
    /// zero default resolves, since group 0/num 0 is a real
    /// generated-table entry -- still heals and halves money,
    /// but logs and leaves the player exactly where the battle ended
    /// ([`OverworldPhase::warp_to`]'s own "leaves the player exactly where
    /// they stood" failure contract). Upstream has no such failure mode
    /// (`GetHealLocation`'s `NULL` return would itself be reached only by
    /// data corruption), so this is this port's own honest fallback for a
    /// state the fixed default ([`crate::new_game::init_save_blocks`])
    /// never actually produces.
    pub(super) fn white_out(&mut self) {
        eprintln!(
            "white-out: the player lost -- healing the party, halving money, and warping to \
             the last heal location (DoWhiteOut, issue #261)"
        );

        // SetMoney(&gSaveBlock1Ptr->money, GetMoney(&gSaveBlock1Ptr->money) / 2);
        self.save1.money /= 2;

        // HealPlayerParty() -- this port's one modeled party slot.
        if let Some(lead) = self.party_lead.as_mut() {
            if let Err(error) = lead.heal(&Dex::new()) {
                eprintln!("white-out: couldn't heal the party lead ({error}) -- left as-is");
            }
        }

        // SetWarpDestinationToLastHealLocation() + WarpIntoMap().
        let heal_location = self.save1.last_heal_location;
        let Some(map) = saved_map_id(heal_location) else {
            eprintln!(
                "white-out: last heal location {heal_location:?} does not name a known map -- \
                 staying put"
            );
            return;
        };
        self.warp_to_position(map, heal_location.x, heal_location.y);
    }
}

// No sibling `tests` module here -- matching this file's own trigger
// siblings (`first_battle_trigger`, `route103_rival_trigger`, `wild_battle`):
// none of them own a dedicated test file either. `Self::white_out`'s
// behaviour is exercised end-to-end from each battle driver's own outcome
// tests instead -- `crate::flow::wild_encounter::tests` for the wild path,
// `super::route103_rival_tests` for the Route 103 rival path -- since a
// white-out is only ever reached through one of those two losses in
// production.
