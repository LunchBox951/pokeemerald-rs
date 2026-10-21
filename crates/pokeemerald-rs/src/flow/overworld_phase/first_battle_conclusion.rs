//! The Route 101 first battle's conclusion: the state changes upstream's
//! `Route101_EventScript_BirchsBag` makes after the scripted fight, which
//! [`OverworldPhase::conclude_first_battle`] applies on every reported
//! [`battle::BattleOutcome`] (`PlayerWon`, `PlayerLost`, `WildFled`).
//!
//! # Outcome-independent
//!
//! Upstream's `CB2_EndFirstBattle` (`pokeemerald/src/battle_setup.c:950-954`)
//! returns to the field with no `IsPlayerDefeated` branch, so the bag
//! script's tail runs whether the fight was won, lost, or fled
//! (`pokeemerald/data/maps/Route101/scripts.inc:218-245`):
//!
//! ```text
//! special HealPlayerParty
//! setflag FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE
//! clearflag FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH
//! setflag FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG
//! setvar VAR_BIRCH_LAB_STATE, 2
//! setvar VAR_ROUTE101_STATE, 3
//! (map-name popup and bedroom-hide lines omitted)
//! warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5
//! ```
//!
//! `PlayerRan` is never reported because the first-battle driver never
//! chooses Run.
//!
//! # Aborted battles
//!
//! An aborted battle reports no outcome and does not conclude. Aborting is
//! port-only; upstream would Struggle and end normally. An abort can leave
//! only an alive lead: damage that faints the lead reports `PlayerLost`.
//!
//! # State written
//!
//! In order:
//!
//! 1. The whole party is healed through
//!    [`super::white_out::OverworldPhase::heal_whole_party_and_reselect_lead`].
//!    It is not routed through [`super::white_out::OverworldPhase::white_out`]
//!    because upstream's `HealPlayerParty` here has no money halving and no
//!    heal-location warp.
//! 2. Four hide flags are written. Three are the script's own lines
//!    (`scripts.inc:233-235`, in that order). The fourth,
//!    `FLAG_HIDE_ROUTE_101_ZIGZAGOON`, is what upstream's earlier
//!    `removeobject LOCALID_ROUTE101_ZIGZAGOON` (`:224`) persists as
//!    (`RemoveObjectEventByLocalIdAndMap` -> `FlagSet`,
//!    `src/event_object_movement.c:1389-1397`); it is written first.
//!    [`assets::MapEventsTable`] templates carry these flag names, and
//!    `engine::overworld::object_event` drops a template whose flag is set
//!    from rendering and collision. [`crate::new_game`] starts a save with
//!    the lab-Birch flag set, so clearing it places Birch in the lab.
//! 3. `VAR_STARTER_MON` is set to the encoding of
//!    [`new_game::PROVISIONAL_STARTER_SPECIES`] through
//!    [`PlayerStarter::var_value`]. Upstream writes it before the fight in
//!    `CB2_GiveStarter` (`battle_setup.c:921`); the port has no starter
//!    choice to hook that write to, so it happens here with the same value.
//! 4. `VAR_BIRCH_LAB_STATE` is set to [`BIRCH_LAB_STATE_STARTER_GIVEN`].
//! 5. `VAR_ROUTE101_STATE` is set to [`ROUTE101_STATE_CONCLUDED`], one past
//!    `super::first_battle_trigger`'s `TRIGGER_CONSUMED_STATE`.
//! 6. The player warps to Birch's lab at explicit coordinates through
//!    [`OverworldPhase::warp_to_position`]. The two-argument `warp` form is
//!    a coordinate pair, not a warp id (`pokeemerald/asm/macros/event.inc`,
//!    `formatwarp`).
//!
//! Not modelled: the starter-select UI and starter handout, the cutscene
//! movement and messages, `FLAG_SYS_POKEMON_GET`, `FLAG_RESCUED_BIRCH`,
//! `FLAG_HIDE_MAP_NAME_POPUP`, and the gender-conditional bedroom-hide calls.
//! Nothing the port models reads those flags.
//!
//! # No fainted lead survives
//!
//! The heal runs in the frame the battle ends, so no fainted lead reaches
//! the overworld after a first battle. The wild-encounter and Route 103
//! rival entry points do not screen for a fainted lead and rely on this.
//!
//! # RNG
//!
//! The conclusion draws nothing from the RNG stream.

use engine::event_data::EventData;

use crate::flow::route103_rival::PlayerStarter;
use crate::new_game;

use super::OverworldPhase;

/// `VAR_BIRCH_LAB_STATE` (`include/constants/vars.h:152`). The constants in
/// this file are independent transcriptions, not shared with other modules.
const VAR_BIRCH_LAB_STATE: u16 = 0x4084;

/// The value the script writes to [`VAR_BIRCH_LAB_STATE`] (`scripts.inc:236`).
const BIRCH_LAB_STATE_STARTER_GIVEN: u16 = 2;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`).
const VAR_STARTER_MON: u16 = 0x4023;

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`).
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// The value the script writes to [`VAR_ROUTE101_STATE`] (`scripts.inc:237`).
const ROUTE101_STATE_CONCLUDED: u16 = 3;

/// `FLAG_HIDE_ROUTE_101_ZIGZAGOON` (`include/constants/flags.h:801`), the
/// hide flag of the rescue Zigzagoon template (local id 4 in the generated
/// Route 101 events).
const FLAG_HIDE_ROUTE_101_ZIGZAGOON: u16 = 0x2EE;

/// `FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE`
/// (`include/constants/flags.h:769`), the hide flag of the rescue-battle
/// Birch template (local id 2).
const FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE: u16 = 0x2D0;

/// `FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH`
/// (`include/constants/flags.h:770`), the hide flag of the lab's Birch.
const FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH: u16 = 0x2D1;

/// `FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG` (`include/constants/flags.h:749`),
/// the hide flag of the starters bag object.
const FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG: u16 = 0x2BC;

const BIRCHS_LAB: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB");

/// Arrival tile `(x, y)` in [`BIRCHS_LAB`] (`scripts.inc:242`).
const BIRCHS_LAB_ARRIVAL: (i16, i16) = (6, 5);

impl OverworldPhase {
    /// Applies the post-battle state changes described in the module docs.
    /// Called for every reported outcome, never for an aborted battle.
    pub(super) fn conclude_first_battle(&mut self) {
        eprintln!(
            "first battle: concluded -- healing the party, writing the Birch's-bag vars, and \
             warping to the lab"
        );

        self.heal_whole_party_and_reselect_lead("first battle");

        Self::write_flag(
            &mut self.save1.event_data,
            FLAG_HIDE_ROUTE_101_ZIGZAGOON,
            true,
        );
        Self::write_flag(
            &mut self.save1.event_data,
            FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE,
            true,
        );
        Self::write_flag(
            &mut self.save1.event_data,
            FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH,
            false,
        );
        Self::write_flag(
            &mut self.save1.event_data,
            FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG,
            true,
        );

        match PlayerStarter::from_species(new_game::PROVISIONAL_STARTER_SPECIES) {
            Some(starter) => Self::write_var(
                &mut self.save1.event_data,
                VAR_STARTER_MON,
                starter.var_value(),
            ),
            // Logged rather than defaulting to Treecko's encoding.
            None => eprintln!(
                "first battle: the provisional starter species has no VAR_STARTER_MON \
                 mapping -- leaving the var unwritten"
            ),
        }

        Self::write_var(
            &mut self.save1.event_data,
            VAR_BIRCH_LAB_STATE,
            BIRCH_LAB_STATE_STARTER_GIVEN,
        );

        Self::write_var(
            &mut self.save1.event_data,
            VAR_ROUTE101_STATE,
            ROUTE101_STATE_CONCLUDED,
        );

        let (x, y) = BIRCHS_LAB_ARRIVAL;
        self.warp_to_position(BIRCHS_LAB, x, y);
    }

    /// Writes a var, logging instead of failing. The ids used here are fixed
    /// in-range constants, so the error arm is not expected to run.
    fn write_var(event_data: &mut EventData, id: u16, value: u16) {
        if let Err(error) = event_data.var_set(id, value) {
            eprintln!("first battle: couldn't write var {id:#06x} ({error}) -- left as-is");
        }
    }

    fn write_flag(event_data: &mut EventData, id: u16, set: bool) {
        let result = if set {
            event_data.flag_set(id)
        } else {
            event_data.flag_clear(id)
        };
        if let Err(error) = result {
            eprintln!("first battle: couldn't write flag {id:#06x} ({error}) -- left as-is");
        }
    }
}
