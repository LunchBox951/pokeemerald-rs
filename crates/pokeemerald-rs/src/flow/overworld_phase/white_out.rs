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
//!      full PP, cleared status for *every* occupied party member, not just
//!      [`OverworldPhase::party_lead`]. This port never decodes any slot
//!      but the selected one into a live battler, so
//!      [`OverworldPhase::white_out`] heals that one slot through
//!      [`battle::BattlePokemon::heal`] and every other occupied slot
//!      through a decode/heal/merge round trip on its own saved bytes
//!      ([`crate::party::from_save_pokemon`]/
//!      [`crate::party::merge_into_save_pokemon`]). `battle` models no
//!      non-volatile status and no EV-raised maximum, so this transition
//!      completes each slot's heal on its retained backing record directly:
//!      clearing its status word and restoring its `hp` to its own
//!      `max_hp`; the next merge/save therefore cannot restore the
//!      pre-white-out status or file a healed slot as damaged. With the
//!      whole party healed, an earlier slot the continue-time scan
//!      (`SetBattlePartyIds`) skipped as fainted may now be the first
//!      usable one, so [`crate::party::select_active_battler`] is re-run
//!      against the healed records afterward, exactly as a fresh battle's
//!      own re-scan upstream would.
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
//! could never re-roll a wild encounter (`crate::flow::wild_encounter::lead_can_fight`,
//! since removed) and could never re-enter the Route 103 rival fight
//! (`battle::BattleError::FaintedBattler`, an emergent consequence of
//! [`battle::Battle::new`]/`new_trainer` refusing a fainted battler, not a
//! bespoke guard -- `route103_rival_trigger`'s own former module docs
//! recorded the resulting dead end). Neither is reachable through a wild or
//! trainer loss any more: this method runs the instant such a loss is
//! reported, before the driver ever returns control to
//! [`super::OverworldPhase::step`], so no frame exists in which the party is
//! fainted and the player can act. One loss path used to be exempt -- the
//! Route 101 scripted first battle, whose `CB2_EndFirstBattle` has no
//! `IsPlayerDefeated` branch and so never whites out -- which is why
//! `lead_can_fight` survived past this issue, narrowed to that residual
//! state. Issue #251's `first_battle_conclusion` closes it too (its own
//! heal is not routed through *this* method -- see that module's docs for
//! why: no money halving, no heal-location warp, just
//! `Route101_EventScript_BirchsBag`'s own narrower `HealPlayerParty` +
//! warp-to-lab), which is why both fail-closed guards this section used to
//! name are gone rather than merely narrowed further.
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
    /// Every other occupied saved slot is healed too (module docs), and
    /// [`crate::party::select_active_battler`] is re-run against the
    /// healed records once that is done: a slot that was fainted (and so
    /// skipped) when continue last selected an active battler may now be
    /// the first usable one, exactly as upstream's own `SetBattlePartyIds`
    /// would find at the next battle. The outgoing lead's own record is
    /// merged before this re-scan can run, not just healed in place, so a
    /// reselection never drops the session's own PP heal, EVs, or
    /// experience gained on it (they would otherwise live only on
    /// [`OverworldPhase::party_lead`], which the reselection may replace).
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

        let dex = Dex::new();
        let stored_count =
            usize::from(self.save1.player_party_count).min(self.save1.player_party.len());

        // HealPlayerParty() -- the slot continue selected
        // ([`OverworldPhase::party_lead_slot`]) heals through its live
        // battler; every other occupied slot heals below, through its own
        // saved bytes.
        if let Some(lead) = self.party_lead.as_mut() {
            let slot = self.party_lead_slot;
            // `MON_DATA_STATUS`/`MON_DATA_HP`: cleared and maxed unconditionally, even
            // if the live battler's own heal below fails -- upstream's per-mon effect
            // has no failure mode of its own to skip these two plain fields for
            // (`script_pokemon_util.c:39-42,52-57`), matching the dormant loop's own
            // fallback below on a decode it cannot fully process either.
            self.save1.player_party[slot].status = 0;
            // `MON_DATA_HP`: the heal fills the battler to the model's 0-EV
            // maximum, but the retained record's maximum may carry an EV
            // contribution above it. Upstream restores to MAX_HP
            // (`script_pokemon_util.c:39-42`), so complete that here too --
            // otherwise the next merge files a fully healed lead as damaged.
            self.save1.player_party[slot].hp = self.save1.player_party[slot].max_hp;
            match lead.heal(&dex) {
                Ok(()) => {
                    // The record's hp no longer matches what the (healed) battler
                    // will report, so re-measure the load offset the merge adds
                    // back onto it -- [`crate::party::hp_hidden_by_load`], fed the
                    // record *as just healed* (`hp == max_hp`), the same function
                    // `copy_party_and_objects_from_save` uses at load. That -- not
                    // `lead.stats().max_hp` -- matters: this record may have
                    // levelled up since the load that first measured its offset
                    // (a battle won, then lost, on the way to this white-out), and
                    // `lead.stats().max_hp` is the `0`-EV floor at the battler's
                    // *current* level, while `self.save1.player_party[slot].max_hp`
                    // here is still the retained maximum at the record's own
                    // (unchanged) `level` byte -- comparing the two would measure
                    // a gap between mismatched levels. `hp_hidden_by_load` instead
                    // floors at `stored.level`, matching what
                    // `merge_into_save_pokemon`'s recompute branch rebases against
                    // (`base.level`, that same still-unchanged byte) on the next
                    // save (issue #384's round-3 review: an EV-trained lead that
                    // levelled up and then whited out filed damaged, because the
                    // gap `F(mon.level()) - F(base.level)` this mismatch drops is
                    // real HP the next merge could never recover).
                    self.lead_hp_hidden_by_load =
                        crate::party::hp_hidden_by_load(&dex, &self.save1.player_party[slot], lead);
                    // Persisted now, not deferred to the next ordinary SAVE's merge
                    // (`copy_party_and_objects_to_save`): the re-scan below may
                    // hand `party_lead`/`party_lead_slot` to a different slot
                    // before any SAVE happens, and that merge only ever targets
                    // whichever slot is *currently* selected. Filing the fully
                    // healed PP into this slot's own record here is what keeps it
                    // from being silently dropped if that reselection moves on.
                    self.save1.player_party[slot] = crate::party::merge_into_save_pokemon(
                        &dex,
                        lead,
                        &self.save1.player_party[slot],
                        &mut self.lead_hp_hidden_by_load,
                    );
                }
                Err(error) => {
                    eprintln!(
                        "white-out: couldn't fully heal the party lead's PP ({error}) -- HP and \
                         status still cleared"
                    );
                }
            }
        }

        // Every other occupied slot: this port has no live battler for it
        // to heal (it was never sent out this session), so its saved
        // record is healed directly instead -- status and HP as plain
        // fields, and PP through a decode/heal/merge round trip on the
        // record's own bytes (`crate::party::from_save_pokemon`/
        // `crate::party::merge_into_save_pokemon`), the same primitives
        // continue-load and an ordinary SAVE already use elsewhere in this
        // crate. A record this port cannot decode still gets its plaintext
        // HP and status cleared -- upstream's own `HealPlayerParty` has no
        // decode step of its own to fail either.
        for (slot, record) in self.save1.player_party[..stored_count]
            .iter_mut()
            .enumerate()
        {
            if self.party_lead.is_some() && slot == self.party_lead_slot {
                continue;
            }
            record.status = 0;
            record.hp = record.max_hp;
            match crate::party::from_save_pokemon(&dex, record) {
                Ok(mut dormant) => match dormant.heal(&dex) {
                    Ok(()) => {
                        let mut hidden = crate::party::hp_hidden_by_load(&dex, record, &dormant);
                        let merged = crate::party::merge_into_save_pokemon(
                            &dex,
                            &dormant,
                            record,
                            &mut hidden,
                        );
                        *record = merged;
                    }
                    Err(error) => {
                        eprintln!(
                            "white-out: slot {slot} couldn't fully heal its PP ({error}) -- HP \
                             and status still cleared"
                        );
                    }
                },
                Err(error) => {
                    eprintln!(
                        "white-out: slot {slot} {error} -- HP and status still cleared, PP left \
                         as saved"
                    );
                }
            }
        }

        // SetBattlePartyIds (`crate::party::select_active_battler`'s own
        // docs) re-scans from slot 0 at the start of every battle upstream;
        // with the whole party just healed above, an earlier slot the
        // continue-time scan skipped as fainted may now be the first
        // usable one, so this port's own cached selection is re-run here
        // too, not only on the next continue load.
        if stored_count > 0 {
            match crate::party::select_active_battler(
                &dex,
                &self.save1.player_party[..stored_count],
            ) {
                Ok((slot, mon)) => {
                    if slot != self.party_lead_slot || self.party_lead.is_none() {
                        self.lead_hp_hidden_by_load = crate::party::hp_hidden_by_load(
                            &dex,
                            &self.save1.player_party[slot],
                            &mon,
                        );
                        self.party_lead = Some(mon);
                        self.party_lead_slot = slot;
                        self.undecodable_lead_retained = false;
                    }
                }
                Err(err) => {
                    eprintln!("white-out: {err} -- keeping the previously selected slot");
                }
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

#[cfg(test)]
mod tests {
    use assets::MapId;
    use engine::save::{Coords16, WarpData};

    use crate::flow::save_continue_tests::{new_game_phase, save_from_the_start_menu};
    use crate::flow::tests::TempSave;

    use super::OverworldPhase;

    const ROUTE_101_STATE: u16 = 0x4060;
    const FLAG_TEMP_12: u16 = 0x12;

    #[test]
    fn white_out_clears_stored_status_before_an_immediate_save() {
        const STORED_STATUS: u32 = 0x40;

        let mut phase = new_game_phase();
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
        let lead = crate::new_game::provisional_starter().with_original_trainer_id(trainer_id);
        let mut stored = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
        stored.status = STORED_STATUS;
        phase.save1.player_party_count = 1;
        phase.save1.player_party[0] = stored;
        phase.party_lead = Some(lead);
        assert_ne!(
            phase.save1.player_party[0].status, 0,
            "setup: lead is statused"
        );

        phase.white_out();

        let temp = TempSave::new("white-out-clears-stored-status");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        assert_eq!(slot.load().block1.player_party[0].status, 0);
    }

    /// The HP half of the same completion: an EV-trained record's maximum
    /// sits above the model's, and `heal` can only fill the battler to the
    /// model's own full. The white-out restores the record's `hp` to its
    /// retained `max_hp`, as upstream's `HealPlayerParty` does, so the
    /// next save files the healed lead at full rather than damaged.
    #[test]
    fn white_out_restores_the_stored_hp_to_the_retained_maximum() {
        const EV_HP_BONUS: u16 = 7;

        let mut phase = new_game_phase();
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
        let lead = crate::new_game::provisional_starter().with_original_trainer_id(trainer_id);
        let mut stored = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
        stored.max_hp += EV_HP_BONUS;
        stored.hp = 1;
        phase.save1.player_party_count = 1;
        phase.save1.player_party[0] = stored;
        phase.party_lead = Some(lead);

        phase.white_out();

        let temp = TempSave::new("white-out-restores-stored-hp");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load().block1.player_party[0];
        assert_eq!(saved.hp, saved.max_hp, "a white-out heal files full");
        assert_eq!(
            saved.max_hp, stored.max_hp,
            "the EV-raised maximum is retained"
        );
    }

    /// Issue #384's round-3 review: a level-up between load and a
    /// subsequent loss must not cost the lead's retained EV points. The
    /// previous fix re-measured [`OverworldPhase::white_out`]'s own offset
    /// against `lead.stats().max_hp` -- the `0`-EV floor at the battler's
    /// *current* level -- while [`crate::party::merge_into_save_pokemon`]'s
    /// recompute branch rebases that same offset against a floor taken at
    /// `base.level`, the record's own (still pre-level-up) stored byte. A
    /// level-up before the white-out left those two floors measured at
    /// different levels, and the gap between them -- real, EV-derived HP --
    /// went missing from the very next save. Reproduced through the real
    /// flow this port drives a white-out through, not by calling the merge
    /// directly, because the defect lives in what
    /// [`OverworldPhase::white_out`] itself writes into
    /// [`OverworldPhase::lead_hp_hidden_by_load`] before any merge runs.
    #[test]
    fn white_out_after_a_level_up_files_the_healed_lead_at_full_not_damaged() {
        let dex = battle::Dex::new();
        let mut phase = new_game_phase();
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);

        // A real hp-EV investment: the recompute branch this test exercises
        // reads the record's own retained EV substructure bytes, so they
        // have to be genuine, not just a bump to `stored.max_hp` the way
        // `white_out_restores_the_stored_hp_to_the_retained_maximum` above
        // gets away with for its untouched-level fixture. Carried by the
        // lead itself too (`with_evs`), matching what a real
        // `party::from_save_pokemon` decode would seed it with -- a lead
        // whose own EVs disagree with its paired record's bytes is a pair
        // `from_saved`'s own continue path can never produce.
        let evs = battle::Evs {
            hp: 252,
            ..battle::Evs::default()
        };
        let lead = crate::new_game::provisional_starter()
            .with_original_trainer_id(trainer_id)
            .with_evs(evs);
        let species = dex
            .species(lead.species())
            .expect("the starter is in the dex");

        let ev_aware_at_level_5 = battle::compute_stats_with_evs(
            lead.species(),
            species,
            lead.level(),
            lead.nature(),
            lead.ivs(),
            evs,
        );

        // `to_save_pokemon` writes the lead's own EVs through, so the
        // record's `evs_and_condition` bytes already agree with `evs`
        // without a manual poke.
        let mut stored = crate::party::to_save_pokemon(&dex, &lead);
        stored.max_hp = u16::try_from(ev_aware_at_level_5.max_hp).unwrap();
        stored.hp = stored.max_hp;

        phase.save1.player_party_count = 1;
        phase.save1.player_party[0] = stored;
        phase.party_lead = Some(lead);

        // A battle won this session raises the level, exactly as
        // `BattlePokemon::apply_experience` would; `reconcile_saved_experience`
        // exercises the identical stats-preserving level-raise machinery
        // (`BattlePokemon::raise_level_to_experience`) without needing a
        // full battle fixture here.
        let level_13 = assets::experience_for_level(species.growth_rate, 13)
            .expect("level 13 is on every growth curve");
        let leveled_lead = phase.party_lead.as_mut().expect("setup: a lead is present");
        leveled_lead.reconcile_saved_experience(level_13);
        assert_eq!(leveled_lead.level(), 13, "fixture sanity: the level moved");
        assert_eq!(
            leveled_lead.current_hp(),
            leveled_lead.stats().max_hp,
            "fixture sanity: the mon is still at its own (0-EV) full \
             through the level-up, unfainted, so the white-out below heals \
             a live lead -- battle's own live cache stays 0-EV regardless \
             of the retained EVs with_evs seeded in (battle's module docs)"
        );

        phase.white_out();

        let temp = TempSave::new("white-out-after-level-up");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load().block1.player_party[0];

        let final_lead = phase.party_lead.as_ref().expect("still present");
        let ev_aware_at_level_13 = battle::compute_stats_with_evs(
            final_lead.species(),
            species,
            13,
            final_lead.nature(),
            final_lead.ivs(),
            evs,
        );
        assert_eq!(
            saved.max_hp,
            u16::try_from(ev_aware_at_level_13.max_hp).unwrap(),
            "fixture sanity: the merge recomputed the level-13 EV-aware block"
        );
        assert_eq!(
            saved.hp, saved.max_hp,
            "a white-out heal after a level-up must file the lead at full \
             under the new maximum -- not damaged by the gap between the \
             old and new floors the mismatched-level offset lost"
        );
    }

    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_explicit_coordinate_warp_rejects_out_of_bounds_position_atomically() {
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        let destination = MapId("MAP_ROUTE101");
        let header = assets::MapHeaderTable::new()
            .header(destination)
            .expect("Route 101 must resolve in the generated map-header table");
        phase.save1.last_heal_location = WarpData {
            map_group: i8::try_from(header.group).unwrap(),
            map_num: i8::try_from(header.num).unwrap(),
            warp_id: -1,
            x: i16::MAX,
            y: i16::MAX,
        };
        phase
            .save1
            .event_data
            .flag_set(FLAG_TEMP_12)
            .expect("FLAG_TEMP_12 is an ordinary flag id");

        let player_before = phase.player;
        let map_before = phase.map_id;
        let location_before = phase.save1.location;
        let position_before = phase.save1.pos;
        assert_eq!(phase.save1.event_data.var_get(ROUTE_101_STATE), Ok(0));

        phase.white_out();

        assert_eq!(phase.player, player_before);
        assert_eq!(phase.map_id, map_before);
        assert_eq!(phase.save1.location, location_before);
        assert_eq!(phase.save1.pos, position_before);
        assert_eq!(
            phase.save1.event_data.flag_get(FLAG_TEMP_12),
            Ok(true),
            "a rejected warp must not commit its scratch temp-field-data clear"
        );
        assert_eq!(
            phase.save1.event_data.var_get(ROUTE_101_STATE),
            Ok(0),
            "a rejected warp must not commit Route 101's entry transition"
        );
    }

    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_immediate_post_white_out_save_persists_home_map_and_position() {
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        let heal_location = phase.save1.last_heal_location;

        phase.white_out();

        assert_eq!(phase.save1.location, heal_location, "setup: warp completed");
        assert_eq!(
            phase.save1.pos,
            Coords16 {
                x: heal_location.x,
                y: heal_location.y,
            },
            "the live save blocks must be coherent before another overworld step"
        );

        let temp = TempSave::new("immediate-post-white-out");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load();

        assert!(saved.status.menu_shows_continue());
        assert_eq!(saved.block1.location, heal_location);
        assert_eq!(
            saved.block1.pos,
            Coords16 {
                x: heal_location.x,
                y: heal_location.y,
            }
        );
    }

    /// Issue #379: the relocated player's elevation must already be the
    /// heal-location tile's own real elevation the instant [`white_out`]
    /// returns, before any step ever runs -- not the `ELEVATION_TRANSITION`
    /// wildcard [`OverworldPhase::warp_to_position`] used to hardcode.
    /// [`crate::new_game::default_last_heal_location`]'s male default names
    /// `(4, 2)` on the default player's house 2F, which is elevation `3` in
    /// the real bundled layout -- confirmed by this test rather than merely
    /// asserted, since a fixture that stopped matching the real tile would
    /// otherwise pass vacuously. Both
    /// [`engine::overworld::PlayerState::elevation`] and
    /// [`engine::overworld::PlayerState::previous_elevation`] must read `3`:
    /// [`engine::overworld::PlayerState::new`]'s own doc comment records
    /// that a freshly placed player starts both fields equal.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_white_out_relocation_adopts_the_heal_locations_elevation_before_any_movement() {
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        let heal_location = phase.save1.last_heal_location;
        assert_eq!(
            (heal_location.x, heal_location.y),
            (4, 2),
            "setup: the default save's own male heal-location default"
        );

        phase.white_out();

        assert_eq!(
            phase.player.elevation(),
            3,
            "the heal-location tile's own real elevation, not the transition wildcard"
        );
        assert_eq!(
            phase.player.previous_elevation(),
            3,
            "a freshly placed player's previous elevation starts equal to its current one"
        );
    }
}
