//! Driving an in-progress wild battle (module split of
//! [`crate::flow::overworld_phase`], issue #210, `oop-boundaries`, issue
//! #169): the per-map fightability screen
//! ([`OverworldPhase::wild_table_fightable`]), the frame-ownership gate
//! [`OverworldPhase::step`] defers to
//! ([`OverworldPhase::advance_wild_battle_frame`]), and turning a fired
//! [`engine::overworld::WildEncounter`] into an in-progress
//! [`battle::Battle`] ([`OverworldPhase::begin_wild_battle`]). The turn
//! engine itself, and the RNG-stream adapter it shares with the overworld,
//! live in [`crate::flow::wild_encounter`]; this module is only the
//! [`OverworldPhase`]-owned glue around it.

use crate::flow::wild_encounter;

use super::OverworldPhase;

impl OverworldPhase {
    /// Whether the current map's land table only rolls wild mons the battle
    /// engine can fight ([`wild_encounter::map_wild_table_fightable`], issue
    /// #207 review). Memoised on [`OverworldPhase::map_id`] itself
    /// ([`OverworldPhase::wild_table_screen`]) because the screen walks the
    /// whole table; a map change invalidates the memo by construction, with
    /// no per-transition update to forget. `false` disables the encounter
    /// roll outright (no draws, no bookkeeping) --
    /// [`wild_encounter::map_wild_table_fightable`]'s own doc comment has
    /// the full reasoning.
    pub(in crate::flow) fn wild_table_fightable(&mut self) -> bool {
        match self.wild_table_screen {
            Some((map, fightable)) if map == self.map_id => fightable,
            _ => {
                let fightable = wild_encounter::map_wild_table_fightable(self.map_id);
                self.wild_table_screen = Some((self.map_id, fightable));
                fightable
            }
        }
    }

    /// Play one frame of an in-progress wild battle, if there is one
    /// (issue #169). Returns whether the battle owned this frame -- `true`
    /// means [`OverworldPhase::step`] must do nothing else,
    /// the way upstream's `CB2_InitBattle` callback owns the frame outright
    /// once `BattleSetup_StartWildBattle` has scheduled it.
    ///
    /// The turn itself, and writing the player's mon back when the battle
    /// ends, are [`wild_encounter::advance_wild_battle`]'s; this is only the
    /// frame-ownership gate and the outcome log.
    pub(super) fn advance_wild_battle_frame(&mut self) -> bool {
        if self.wild_battle.is_none() {
            return false;
        }
        if let Some(outcome) = wild_encounter::advance_wild_battle(
            &mut self.wild_battle,
            &mut self.party_lead,
            &mut self.rng,
        ) {
            eprintln!("wild battle: ended -- {outcome:?}");
            // `CB2_EndWildBattle`'s `IsPlayerDefeated` branch
            // (`src/battle_setup.c:602-616`) -> `CB2_WhiteOut` (issue #261):
            // heal, halve money, warp home. `Self::white_out` (module docs'
            // former "unmodelled gate" section, now retired) is the one
            // shared implementation both this driver and the Route 103
            // rival one call.
            if outcome == battle::BattleOutcome::PlayerLost {
                self.white_out();
            }
        }
        true
    }

    /// Turn a fired [`engine::overworld::WildEncounter`] into an in-progress
    /// [`battle::Battle`] (issue #169) -- upstream's `CreateWildMon` +
    /// `BattleSetup_StartWildBattle` pair, minus the battle transition.
    ///
    /// Nothing starts without a party mon to fight with. Production play
    /// always has one -- [`OverworldPhase::load_default`] assigns
    /// [`crate::new_game::provisional_starter`], the stand-in for the
    /// un-ported Birch-bag handout -- so the `None` arm is the defensive
    /// fallback for a bare [`OverworldPhase::new`] phase
    /// (`crate::flow::wild_encounter`'s module docs). The encounter is
    /// logged and dropped in that case -- the roll itself already happened
    /// and already consumed its draws, so the RNG stream stays where the
    /// roll left it either way.
    ///
    /// A rejected battle (an unknown species, or a wild moveset the turn
    /// engine can't execute) is logged and dropped too, leaving the player
    /// standing in the grass rather than stuck in a half-built battle. The
    /// lead mon is only handed over once the battle is really built, so a
    /// rejection cannot swallow it. Since the per-map table screen
    /// ([`OverworldPhase::wild_table_fightable`]) refuses the roll before
    /// any draw on a map that could produce such a moveset, this arm is
    /// defensive: no table-rolled encounter reaches it today.
    ///
    /// The lead mon reaching here is never *fainted* in production: a loss
    /// heals it before the next step can roll another encounter
    /// ([`Self::white_out`], `crate::flow::wild_encounter`'s module docs
    /// "The white-out, modelled" section) -- the same state upstream itself
    /// cannot reach. Not re-checked here, since a second check could only
    /// ever be dead code against that invariant.
    pub(super) fn begin_wild_battle(
        &mut self,
        encounter: Option<engine::overworld::WildEncounter>,
    ) {
        let Some(encounter) = encounter else {
            return;
        };
        eprintln!(
            "wild encounter: slot {} -- species {:?} at level {}",
            encounter.slot, encounter.species, encounter.level
        );
        let Some(lead) = self.party_lead.clone() else {
            eprintln!("wild encounter: no party mon yet -- no battle to start");
            return;
        };
        match wild_encounter::start_wild_battle(lead, encounter, &mut self.rng) {
            Ok(battle) => {
                self.party_lead = None;
                self.wild_battle = Some(battle);
            }
            Err(error) => eprintln!("wild encounter: can't start a battle ({error:?})"),
        }
    }

    /// [`OverworldPhase::step`]'s single call into whichever of
    /// [`OverworldPhase::begin_first_battle`]/[`OverworldPhase::begin_wild_battle`]
    /// this frame's landing earned (issue #231) -- `encounter` is always
    /// `None` when `first_battle_triggered` (that method's own filter
    /// chain), so exactly one of the two ever actually starts a battle.
    pub(super) fn begin_step_battle(
        &mut self,
        first_battle_triggered: bool,
        encounter: Option<engine::overworld::WildEncounter>,
    ) {
        if first_battle_triggered {
            self.begin_first_battle();
        } else {
            self.begin_wild_battle(encounter);
        }
    }
}
