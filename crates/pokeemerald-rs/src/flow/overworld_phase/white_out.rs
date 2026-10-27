//! The loss transition for [`OverworldPhase`]: heal the whole party, halve the
//! player's money, and relocate the player to the saved heal location.
//! Upstream's `DoWhiteOut` (`pokeemerald/src/overworld.c:358-366`) does the
//! same after `IsPlayerDefeated` (`src/battle_setup.c:994-1010`: lost or drew)
//! routes a wild or trainer battle to `CB2_WhiteOut`.
//!
//! # Steps
//!
//! [`OverworldPhase::white_out`] runs, in order:
//!
//! 1. Money becomes `money / 2` with plain unsigned division (`SetMoney`,
//!    `overworld.c:361`).
//! 2. [`OverworldPhase::heal_whole_party_and_reselect_lead`] restores HP, PP
//!    and status of every stored party member and re-selects the active
//!    battler (`HealPlayerParty`, `src/script_pokemon_util.c:30-59`).
//! 3. [`engine::event_data::EventData::clear_white_out_state`] clears the
//!    persistent field-mode flags and pending abnormal weather
//!    (`Overworld_ResetStateAfterWhiteOut`, `overworld.c:399-413`). It runs
//!    before the destination is resolved, so an unresolvable destination does
//!    not skip it.
//! 4. [`OverworldPhase::warp_to_saved_location`] moves the player to
//!    `save1.last_heal_location` (`SetWarpDestinationToLastHealLocation` and
//!    `WarpIntoMap`, `overworld.c:626-668`). The whole saved `WarpData` is the
//!    destination, warp id included: a valid warp id names that warp event's
//!    position, otherwise the stored `x`/`y` apply when both are
//!    non-negative, otherwise the map's center.
//!
//! Not modelled: `EventScript_WhiteOut`'s Elite Four reset and Mr. Briney
//! relocation (this port has neither), the "You blacked out!" message, the
//! 120-frame fade, and `ResetInitialPlayerAvatarState`.
//!
//! # Callers
//!
//! The wild-battle driver
//! ([`super::wild_battle::OverworldPhase::advance_wild_battle_frame`]) and the
//! Route 103 rival driver
//! ([`super::route103_rival_trigger::OverworldPhase::advance_route103_rival_battle_frame`])
//! both call [`OverworldPhase::white_out`] the moment the battle reports
//! `PlayerLost`, before control returns to [`super::OverworldPhase::step`].
//! No frame therefore exists with a fainted party and a controllable player,
//! so neither driver screens for a fainted lead.
//!
//! The scripted Route 101 first battle never whites out. It calls
//! [`OverworldPhase::heal_whole_party_and_reselect_lead`] directly, without
//! the money loss or the heal-location warp.
//!
//! # RNG
//!
//! The transition draws no random numbers.

use battle::Dex;

use super::{saved_map_id, OverworldPhase};

impl OverworldPhase {
    /// Runs the loss transition described in the module docs. A
    /// `last_heal_location` that names no known map (only a hand-edited save
    /// produces one) logs and leaves the player where the battle ended, after
    /// money, party and field state are updated; upstream has no such case.
    pub(super) fn white_out(&mut self) {
        eprintln!(
            "white-out: the player lost -- healing the party, halving money, and warping to \
             the last heal location"
        );

        self.save1.money /= 2;

        self.heal_whole_party_and_reselect_lead("white-out");

        // Runs even when the warp below cannot resolve.
        self.save1.event_data.clear_white_out_state();

        let heal_location = self.save1.last_heal_location;
        let Some(map) = saved_map_id(heal_location) else {
            eprintln!(
                "white-out: last heal location {heal_location:?} does not name a known map -- \
                 staying put"
            );
            return;
        };
        self.warp_to_saved_location(map, heal_location);
    }

    /// Restores full HP, full PP and cleared status for every occupied party
    /// slot, then re-selects the active battler so a previously fainted
    /// earlier slot can become lead. A slot whose PP cannot be restored (the
    /// record does not decode) still gets HP and status restored. `context`
    /// labels the log lines.
    pub(super) fn heal_whole_party_and_reselect_lead(&mut self, context: &str) {
        let dex = Dex::new();
        let stored_count =
            usize::from(self.save1.player_party_count).min(self.save1.player_party.len());

        // The owner heals the lead and merges it back first so the re-scan
        // keeps its session state; a lead beyond a nonzero stored count is
        // merged without healing, and a zero count still heals slot 0.
        if let Err(error) = self.heal_whole_lead(&dex) {
            eprintln!(
                "{context}: couldn't fully heal the party lead's PP ({error}) -- HP and status \
                 still cleared"
            );
        }

        // Non-lead slots: HP and status live in plaintext record fields and
        // are restored even when the record cannot be decoded for PP.
        for slot in 0..stored_count {
            if self.owns_lead_slot(slot) {
                continue;
            }
            let record = &mut self.save1.player_party[slot];
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
                            "{context}: slot {slot} couldn't fully heal its PP ({error}) -- HP \
                             and status still cleared"
                        );
                    }
                },
                Err(error) => {
                    eprintln!(
                        "{context}: slot {slot} {error} -- HP and status still cleared, PP left \
                         as saved"
                    );
                }
            }
        }

        // An earlier fainted slot may now be the first usable one. Upstream's
        // `SetBattlePartyIds` scans every party slot, not only the stored
        // count (`pokeemerald/src/battle_controllers.c:591-606`).
        self.reselect_lead_from_save(&dex, context);
    }
}

#[cfg(test)]
mod tests {
    use assets::MapId;
    use engine::save::{Coords16, WarpData};

    use crate::flow::save_continue_support::{new_game_phase, save_from_the_start_menu};
    use crate::flow::tests::TempSave;

    use super::OverworldPhase;

    const ROUTE_101_STATE: u16 = 0x4060;
    const FLAG_TEMP_12: u16 = 0x12;

    /// A white-out clears the persistent field-mode flags and ends flagged
    /// abnormal weather (`Overworld_ResetStateAfterWhiteOut`,
    /// `pokeemerald/src/overworld.c:399-413`), and the saved file shows it.
    /// Each id is the upstream `include/constants/{flags,vars}.h` value.
    /// The heal location (group `-1`) deliberately fails to resolve, which
    /// needs no asset pack and proves the reset does not depend on the warp.
    #[test]
    fn white_out_clears_the_persistent_field_and_weather_state_even_without_a_warp_destination() {
        const FLAG_SYS_USE_FLASH: u16 = 0x888;
        const FLAG_SYS_USE_STRENGTH: u16 = 0x889;
        const FLAG_SYS_CYCLING_ROAD: u16 = 0x88B;
        const FLAG_SYS_SAFARI_MODE: u16 = 0x88C;
        const FLAG_SYS_CRUISE_MODE: u16 = 0x88D;
        const VAR_ABNORMAL_WEATHER_LOCATION: u16 = 0x4037;
        const VAR_SHOULD_END_ABNORMAL_WEATHER: u16 = 0x4039;
        const UNRELATED_FLAG: u16 = 0x100;

        let mut phase = new_game_phase();
        for flag in [
            FLAG_SYS_USE_FLASH,
            FLAG_SYS_USE_STRENGTH,
            FLAG_SYS_CYCLING_ROAD,
            FLAG_SYS_SAFARI_MODE,
            FLAG_SYS_CRUISE_MODE,
            UNRELATED_FLAG,
        ] {
            phase.save1.event_data.flag_set(flag).unwrap();
        }
        phase
            .save1
            .event_data
            .var_set(VAR_SHOULD_END_ABNORMAL_WEATHER, 1)
            .unwrap();
        phase
            .save1
            .event_data
            .var_set(VAR_ABNORMAL_WEATHER_LOCATION, 3)
            .unwrap();
        // `saved_map_id` rejects a negative group, so the warp never runs.
        phase.save1.last_heal_location.map_group = -1;

        phase.white_out();

        let temp = TempSave::new("white-out-clears-field-state");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load().block1.event_data;
        for flag in [
            FLAG_SYS_USE_FLASH,
            FLAG_SYS_USE_STRENGTH,
            FLAG_SYS_CYCLING_ROAD,
            FLAG_SYS_SAFARI_MODE,
            FLAG_SYS_CRUISE_MODE,
        ] {
            assert_eq!(saved.flag_get(flag), Ok(false), "flag {flag:#06x} survived");
        }
        assert_eq!(saved.var_get(VAR_SHOULD_END_ABNORMAL_WEATHER), Ok(0));
        assert_eq!(saved.var_get(VAR_ABNORMAL_WEATHER_LOCATION), Ok(0));
        assert_eq!(
            saved.flag_get(UNRELATED_FLAG),
            Ok(true),
            "a flag outside the named set must survive the reset"
        );
    }

    #[test]
    fn white_out_clears_stored_status_before_an_immediate_save() {
        const STORED_STATUS: u32 = 0x40;

        let mut phase = new_game_phase();
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
        let lead = crate::new_game::provisional_starter().with_original_trainer_id(trainer_id);
        let mut stored = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
        stored.status = STORED_STATUS;
        phase.save1.player_party_count = 1;
        phase.set_backed_lead_for_test(0, stored, lead);
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

    /// An EV-trained record's maximum sits above the battle model's, so the
    /// battler's `heal` alone would leave the record damaged. The white-out
    /// restores the record's `hp` to its own `max_hp` (`HealPlayerParty`), so
    /// the next save files the lead at full.
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
        phase.set_backed_lead_for_test(0, stored, lead);

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

    /// A level-up between load and a loss must not cost the lead's retained
    /// EV points. The white-out's offset between record and battler maximum
    /// must be measured against the same level floor that
    /// [`crate::party::merge_into_save_pokemon`] rebases against; otherwise
    /// the EV-derived HP gap vanishes from the next save. Driven through
    /// [`OverworldPhase::white_out`] because the lead's
    /// hidden-HP offset is re-measured before any merge runs.
    #[test]
    fn white_out_after_a_level_up_files_the_healed_lead_at_full_not_damaged() {
        let dex = battle::Dex::new();
        let mut phase = new_game_phase();
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);

        // The recompute branch reads the record's EV bytes, so they must be
        // genuine rather than a bumped `max_hp`. The lead carries the same
        // EVs, as a decoded save would.
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

        // `to_save_pokemon` writes the lead's EVs into the record.
        let mut stored = crate::party::to_save_pokemon(&dex, &lead);
        stored.max_hp = u16::try_from(ev_aware_at_level_5.max_hp).unwrap();
        stored.hp = stored.max_hp;

        phase.save1.player_party_count = 1;
        phase.set_backed_lead_for_test(0, stored, lead);

        // Raise the level as a won battle would, through the same
        // stats-preserving level-raise machinery, without a battle fixture.
        let level_13 = assets::experience_for_level(species.growth_rate, 13)
            .expect("level 13 is on every growth curve");
        let leveled_lead = phase.lead_mut_for_test().expect("setup: a lead is present");
        leveled_lead.reconcile_saved_experience(level_13);
        assert_eq!(leveled_lead.level(), 13, "fixture sanity: the level moved");
        assert_eq!(
            leveled_lead.current_hp(),
            leveled_lead.stats().max_hp,
            "fixture sanity: the leveled lead is unfainted at its own (0-EV) \
             full, so the white-out heals a live lead; the battler's stats \
             stay 0-EV regardless of the retained EVs"
        );

        phase.white_out();

        let temp = TempSave::new("white-out-after-level-up");
        let mut slot = temp.slot();
        save_from_the_start_menu(&mut phase, &mut slot);
        let saved = slot.load().block1.player_party[0];

        let final_lead = phase.lead_for_test().expect("still present");
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
             under the new maximum"
        );
    }

    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_explicit_coordinate_warp_rejects_out_of_bounds_position_atomically() {
        let mut phase = OverworldPhase::load(
            crate::pack_source::PackSource::Repo,
            crate::new_game::NewGameOptions::DEFAULT,
            crate::new_game::NewGameIdentity::DEFAULT,
        )
        .expect("run `cargo xtask extract` first");
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
        let mut phase = OverworldPhase::load(
            crate::pack_source::PackSource::Repo,
            crate::new_game::NewGameOptions::DEFAULT,
            crate::new_game::NewGameIdentity::DEFAULT,
        )
        .expect("run `cargo xtask extract` first");
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

    /// The relocated player's elevation is the heal-location tile's real
    /// elevation as soon as [`OverworldPhase::white_out`] returns, not the
    /// `ELEVATION_TRANSITION` wildcard. The male default heal location is the
    /// bed tile of Brendan's house 2F, `x = 4, y = 2` in upstream's
    /// `src/data/heal_locations.json`, whose map layout gives it elevation 3.
    /// The test asserts the coordinates so a fixture drifting off that tile
    /// fails instead of passing vacuously. A freshly placed player
    /// starts with `previous_elevation` equal to `elevation`
    /// ([`engine::overworld::PlayerState::new`]).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_white_out_relocation_adopts_the_heal_locations_elevation_before_any_movement() {
        const BED_X: i16 = 4;
        const BED_Y: i16 = 2;
        const BED_ELEVATION: u8 = 3;

        let mut phase = OverworldPhase::load(
            crate::pack_source::PackSource::Repo,
            crate::new_game::NewGameOptions::DEFAULT,
            crate::new_game::NewGameIdentity::DEFAULT,
        )
        .expect("run `cargo xtask extract` first");
        let heal_location = phase.save1.last_heal_location;
        assert_eq!(
            (heal_location.x, heal_location.y),
            (BED_X, BED_Y),
            "setup: the default save's own male heal-location default"
        );

        phase.white_out();

        assert_eq!(
            phase.player.elevation(),
            BED_ELEVATION,
            "the heal-location tile's own real elevation, not the transition wildcard"
        );
        assert_eq!(
            phase.player.previous_elevation(),
            BED_ELEVATION,
            "a freshly placed player's previous elevation starts equal to its current one"
        );
    }

    /// The white-out warp copies the whole saved heal `WarpData`, warp id
    /// included (`SetWarpDestinationToLastHealLocation`,
    /// `pokeemerald/src/overworld.c:665-668`). `SetPlayerCoordsFromWarp`
    /// (`overworld.c:603-616`) then prefers the named warp event's position
    /// over the stored `(x, y)` whenever the id is valid for the destination
    /// map. An id that names no warp event falls back to the stored pair when
    /// both are non-negative, otherwise to the map's centre.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_white_out_prefers_a_valid_saved_warp_id_over_its_own_stored_coordinates() {
        let mut phase = OverworldPhase::load(
            crate::pack_source::PackSource::Repo,
            crate::new_game::NewGameOptions::DEFAULT,
            crate::new_game::NewGameIdentity::DEFAULT,
        )
        .expect("run `cargo xtask extract` first");
        let home = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
        let header = assets::MapHeaderTable::new()
            .header(home)
            .expect("the default heal location's own map must resolve");
        let warp = assets::MapEventsTable::new()
            .resolve(home)
            .expect("the default heal location's own events must resolve")
            .warp_events[0];
        let stored_bed_position = (
            crate::new_game::DEFAULT_HEAL_LOCATION_X,
            crate::new_game::DEFAULT_HEAL_LOCATION_Y,
        );
        assert_ne!(
            (warp.x, warp.y),
            stored_bed_position,
            "fixture sanity: warp event 0 must not already sit on the stored bed tile, or \
             this test would pass vacuously"
        );

        let heal_location = WarpData {
            map_group: i8::try_from(header.group).unwrap(),
            map_num: i8::try_from(header.num).unwrap(),
            warp_id: 0,
            x: stored_bed_position.0,
            y: stored_bed_position.1,
        };
        phase.save1.last_heal_location = heal_location;

        phase.white_out();

        assert_eq!(
            phase.player.position(),
            (i32::from(warp.x), i32::from(warp.y)),
            "a valid saved warp id must name the landing tile, not the stored coordinates"
        );
        assert_eq!(
            phase.save1.pos,
            Coords16 {
                x: warp.x,
                y: warp.y
            }
        );
        assert_eq!(
            phase.save1.location, heal_location,
            "ApplyCurrentWarp copies the saved heal warp verbatim, warp id included"
        );
        assert_eq!(phase.map_id, home);
    }
}
