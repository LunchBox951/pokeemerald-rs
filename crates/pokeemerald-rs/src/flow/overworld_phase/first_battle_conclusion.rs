//! `Route101_EventScript_BirchsBag`'s post-battle tail (issue #251, ladders
//! to I-4): the heal, the three var writes, and the warp to Birch's lab
//! upstream runs the instant the Route 101 scripted first battle ends -- on
//! **either** outcome.
//!
//! # The upstream chain: the fight is inside the bag script, not after it
//!
//! `crate::flow::first_battle`'s own module docs already trace the front
//! half of this chain; this module is the back half,
//! `Route101_EventScript_BirchsBag`'s own tail
//! (`pokeemerald/data/maps/Route101/scripts.inc:218-245`):
//!
//! ```text
//! special ChooseStarter          @ starter-select UI -- not modelled
//! applymovement LOCALID_ROUTE101_BIRCH, Route101_Movement_BirchApproachPlayer
//! waitmovement 0
//! msgbox Route101_Text_YouSavedMe, MSGBOX_DEFAULT
//! special HealPlayerParty
//! setflag FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE
//! clearflag FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_BIRCH
//! setflag FLAG_HIDE_ROUTE_101_BIRCH_STARTERS_BAG
//! setvar VAR_BIRCH_LAB_STATE, 2
//! setvar VAR_ROUTE101_STATE, 3
//! clearflag FLAG_HIDE_MAP_NAME_POPUP
//! checkplayergender
//! call_if_eq VAR_RESULT, MALE, Route101_EventScript_HideMayInBedroom
//! call_if_eq VAR_RESULT, FEMALE, Route101_EventScript_HideBrendanInBedroom
//! warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5
//! ```
//!
//! `special ChooseStarter` (`pokeemerald/src/battle_setup.c:911-915`) does
//! not run this script's own remainder at all: it hands the whole callback
//! chain off to `CB2_ChooseStarter` and stashes `CB2_GiveStarter` as
//! `gMain.savedCallback`. `CB2_GiveStarter` (`:917-929`) writes
//! `VAR_STARTER_MON`, hands over the chosen starter (`ScriptGiveMon`), and
//! sets `SetMainCallback2(CB2_StartFirstBattle)` -- the actual
//! `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon fight this port's own trigger
//! already starts directly (issue #231, since there is no `ChooseStarter`
//! UI to hang it off of). Once the fight ends, `CB2_EndFirstBattle`
//! (`battle_setup.c:950-954`) is unconditional --
//! `SetMainCallback2(CB2_ReturnToFieldContinueScriptPlayMapMusic)`, no
//! `IsPlayerDefeated` branch anywhere in the chain
//! (`crate::flow::first_battle`'s own module docs cite this precisely) --
//! so control returns to the *original* script exactly where `special
//! ChooseStarter` left it: the `applymovement`/`msgbox`/`HealPlayerParty`/
//! var-writes/`warp` tail above, **regardless of whether the fight was won,
//! lost, or the Zigzagoon fled**.
//!
//! This is why the heal, the var writes, and the warp all belong here,
//! gated on nothing but "the battle produced a real outcome" -- not on
//! *which* outcome. [`OverworldPhase::conclude_first_battle`] runs for
//! every [`battle::BattleOutcome`]
//! [`crate::flow::first_battle::advance_first_battle`] ever reports
//! (`PlayerWon`, `PlayerLost`, `WildFled`; `PlayerRan` is unreachable --
//! the driver never chooses Run, `RunForbidden`) -- never on an *aborted*
//! battle (`super::first_battle_trigger`'s own "When the var advances"
//! section: an abort reports no outcome at all; it is a port-only
//! construct -- upstream would Struggle and end normally -- so there is no
//! upstream state for a conclusion to model, and the abort can only leave
//! an alive (if damaged) lead, never a fainted one: any damage that faints
//! the lead sets a `PlayerLost` outcome and returns Ok, so a fainted lead is
//! always a reported outcome, never an abort).
//!
//! # What's modelled, narrowly
//!
//! - **`special HealPlayerParty`** (`src/script_pokemon_util.c:30-59`) --
//!   [`battle::BattlePokemon::heal`] on [`OverworldPhase::party_lead`], the
//!   same primitive [`super::white_out::OverworldPhase::white_out`] already
//!   calls for the wild/trainer white-out (that module's own docs name this
//!   exact reuse as the reason `heal` lives on the owned type rather than
//!   folded into either call site, `oop-boundaries`). **Not** routed
//!   through `white_out` itself: upstream's `HealPlayerParty` here has no
//!   accompanying `SetMoney(.../2)` or warp-to-heal-location -- this is the
//!   bag script's own heal, not a white-out, and halving the player's money
//!   for winning (or merely surviving) a story-mandated fight would be a
//!   fidelity bug, not a shortcut.
//! - **`setvar VAR_BIRCH_LAB_STATE, 2`** -- [`VAR_BIRCH_LAB_STATE`],
//!   transcribed from `include/constants/vars.h:152`.
//! - **The real `VAR_STARTER_MON` write** -- `include/constants/vars.h:53`,
//!   whose own comment names the encoding [`crate::flow::route103_rival::PlayerStarter::var_value`]
//!   reproduces (`0`=Treecko, `1`=Torchic, `2`=Mudkip). Upstream writes it
//!   *before* the fight, in `CB2_GiveStarter` (`battle_setup.c:921`,
//!   `*GetVarPointer(VAR_STARTER_MON) = gSpecialVar_Result`) -- this port
//!   writes it here, after, because the trigger already starts the fight
//!   directly with no `ChooseStarter` step to hook the write into (module
//!   docs above). The *value* is identical either way:
//!   [`crate::new_game::PROVISIONAL_STARTER_SPECIES`] is fixed for both the
//!   pre-fight and post-fight timing, so no player-visible difference
//!   exists yet, and nothing reads `VAR_STARTER_MON` before this method
//!   runs. A future starter-choice slice must move this write back to real
//!   choose-time.
//! - **`setvar VAR_ROUTE101_STATE, 3`** -- [`ROUTE101_STATE_CONCLUDED`],
//!   the chain's own terminal value: `super::first_battle_trigger`'s
//!   `TRIGGER_CONSUMED_STATE` (`2`) already covers the mid-cutscene write,
//!   this is the final one, and it retires `VAR_ROUTE101_STATE` for the
//!   rest of a playthrough -- nothing upstream ever reads it past `3`.
//! - **`warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5`** --
//!   [`OverworldPhase::warp_to_position`], the same explicit-coordinate warp
//!   [`super::white_out::OverworldPhase::white_out`] uses. The two-argument
//!   `warp` form (`pokeemerald/asm/macros/event.inc`'s `formatwarp`: two
//!   bare arguments are a coordinate pair with `WARP_ID_NONE`, not a warp
//!   id) is exactly `warp_to_position`'s shape, not
//!   [`OverworldPhase::warp_to`]'s. The lab's own layout/tileset
//!   (`LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB`, `gTileset_Lab`) are
//!   already bundled -- `crates/assets/src/map_headers.rs` and
//!   `map_layouts.rs` both carry the map, and
//!   `crate::overworld::resolve_tileset_pack_name` already maps
//!   `"gTileset_Lab"` to the pack's own `"lab"` tileset -- so this warp
//!   lands on a real, walkable room rather than an unmapped destination;
//!   `first_battle_conclusion_tests::real_pack_a_lost_first_battle_still_heals_and_reaches_the_lab`
//!   is the standing proof.
//!
//! # What's deliberately deferred
//!
//! Recorded on the `data/maps/Route101#Route101_EventScript_BirchsBag`
//! ledger entry rather than silently dropped: the `special ChooseStarter`
//! UI itself and `CB2_GiveStarter`'s `ScriptGiveMon` handout (no starter
//! choice exists yet -- `crate::new_game::PROVISIONAL_STARTER_SPECIES`
//! remains the whole port's stand-in), the `fadescreen`/`applymovement`/
//! `msgbox` cutscene dressing (no script engine, the same gap
//! `super::first_battle_trigger`'s own module docs record for the rescue
//! cutscene), `FLAG_SYS_POKEMON_GET`/`FLAG_RESCUED_BIRCH` and the four
//! other `setflag`/`clearflag` lines (nothing this port models reads any of
//! them: no Pokédex, no rendered Birch NPC in the lab, no bag object event
//! to hide), and the gender-conditional bedroom-hide calls (the sibling's
//! bedroom scene exists, but nothing routes a fresh save through it a
//! second time post-rescue for this to matter yet).
//!
//! # What this retires
//!
//! Before this issue, a lost Route 101 first battle was the one loss path
//! issue #261's white-out could not cover (`CB2_EndFirstBattle` has no
//! `IsPlayerDefeated` branch, so it never routes to `CB2_WhiteOut`) --
//! `crate::flow::wild_encounter::lead_can_fight` and
//! `super::route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`'s
//! own fainted-lead screen existed solely to fail closed on the fainted
//! lead that residual state left standing in the overworld (issue #261
//! review; both modules' own former docs named this exact exemption). This
//! module's heal runs on *every* outcome, in the same frame the battle
//! ends -- so that state can no longer exist past the frame
//! [`OverworldPhase::conclude_first_battle`] runs, and both guards were
//! left with no reachable input to refuse. Removed rather than left inert
//! (`crate::flow::wild_encounter`'s and `super::route103_rival_trigger`'s
//! own updated module docs), with their behavioural pins replaced by
//! strictly *stronger* ones proving the state itself is unreachable, not
//! merely guarded (`(test-ratchet)`; see
//! `crate::flow::wild_encounter::tests::a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted`
//! and
//! `super::route103_rival_tests::a_lost_route_101_first_battle_still_lets_the_healed_lead_fight_the_rival`
//! for the replacement tests and their own doc comments for exactly what
//! they retire).
//!
//! # RNG stream
//!
//! Draws nothing: [`battle::BattlePokemon::heal`], [`EventData::var_set`],
//! and [`OverworldPhase::warp_to_position`] are all plain state mutation --
//! matching [`super::white_out::OverworldPhase::white_out`]'s own identical
//! claim, for the same reason.

use battle::Dex;
use engine::event_data::EventData;

use crate::flow::route103_rival::PlayerStarter;
use crate::new_game;

use super::OverworldPhase;

/// `VAR_BIRCH_LAB_STATE` (`include/constants/vars.h:152`).
const VAR_BIRCH_LAB_STATE: u16 = 0x4084;

/// The value `Route101_EventScript_BirchsBag` itself writes
/// (`scripts.inc:236`, `setvar VAR_BIRCH_LAB_STATE, 2`) -- "the starter has
/// been given," the state a future Birch's-lab slice would gate its own
/// thank-you dialog on.
const BIRCH_LAB_STATE_STARTER_GIVEN: u16 = 2;

/// `VAR_STARTER_MON` (`include/constants/vars.h:53`) -- transcribed
/// independently of any other module's copy, the same "own transcription,
/// not a shared constant" convention
/// `super::route103_rival_trigger::VAR_OBJ_GFX_ID_0`'s own doc comment
/// explains.
const VAR_STARTER_MON: u16 = 0x4023;

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`) -- independently
/// transcribed from `super::first_battle_trigger`'s own (private) copy, the
/// same convention that module's docs explain for `VAR_OBJ_GFX_ID_0`.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// The chain's own terminal value (`scripts.inc:237`, `setvar
/// VAR_ROUTE101_STATE, 3`) -- one past
/// `super::first_battle_trigger::TRIGGER_CONSUMED_STATE`, and the value the
/// var holds for the rest of a playthrough.
const ROUTE101_STATE_CONCLUDED: u16 = 3;

/// `MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB` -- the `warp` target (module
/// docs).
const BIRCHS_LAB: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB");

/// The `warp`'s own explicit coordinates (`scripts.inc:242`, `warp
/// MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5`).
const BIRCHS_LAB_ARRIVAL: (i16, i16) = (6, 5);

impl OverworldPhase {
    /// `Route101_EventScript_BirchsBag`'s post-battle tail (module docs):
    /// heal the party lead, write the three vars, warp to the lab. The
    /// counterpart
    /// `super::first_battle_trigger::OverworldPhase::advance_first_battle_frame`
    /// calls the instant [`crate::flow::first_battle::advance_first_battle`]
    /// reports **any** real outcome -- never gated on which one (module
    /// docs' "The upstream chain" section explains why: `CB2_EndFirstBattle`
    /// itself never branches on the outcome either).
    ///
    /// Heals defensively -- a `None` party lead (a bare test phase with no
    /// battle actually run through the trigger) is a silent no-op, the same
    /// shape [`super::white_out::OverworldPhase::white_out`] uses, since
    /// production always has a real lead here:
    /// [`crate::flow::first_battle::advance_first_battle`]'s own
    /// write-back contract guarantees one.
    pub(super) fn conclude_first_battle(&mut self) {
        eprintln!(
            "first battle: concluded -- healing the party, writing the Birch's-bag vars, and \
             warping to the lab (Route101_EventScript_BirchsBag, issue #251)"
        );

        // special HealPlayerParty
        if let Some(lead) = self.party_lead.as_mut() {
            if let Err(error) = lead.heal(&Dex::new()) {
                eprintln!("first battle: couldn't heal the party lead ({error}) -- left as-is");
            }
        }

        // *GetVarPointer(VAR_STARTER_MON) = gSpecialVar_Result -- upstream's
        // own value at CB2_GiveStarter time, reproduced here off the
        // provisional starter species (module docs' "What's modelled,
        // narrowly" section explains the timing divergence).
        match PlayerStarter::from_species(new_game::PROVISIONAL_STARTER_SPECIES) {
            Some(starter) => Self::write_var(
                &mut self.save1.event_data,
                VAR_STARTER_MON,
                starter.var_value(),
            ),
            // Unreachable while PROVISIONAL_STARTER_SPECIES is one of the
            // three real starters; logged rather than silently written as
            // Treecko, matching every other None-mapping arm in this diff.
            None => eprintln!(
                "first battle: the provisional starter species has no VAR_STARTER_MON \
                 mapping -- leaving the var unwritten"
            ),
        }

        // setvar VAR_BIRCH_LAB_STATE, 2
        Self::write_var(
            &mut self.save1.event_data,
            VAR_BIRCH_LAB_STATE,
            BIRCH_LAB_STATE_STARTER_GIVEN,
        );

        // setvar VAR_ROUTE101_STATE, 3
        Self::write_var(
            &mut self.save1.event_data,
            VAR_ROUTE101_STATE,
            ROUTE101_STATE_CONCLUDED,
        );

        // warp MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB, 6, 5
        let (x, y) = BIRCHS_LAB_ARRIVAL;
        self.warp_to_position(BIRCHS_LAB, x, y);
    }

    /// A logged, best-effort var write -- [`Self::conclude_first_battle`]'s
    /// own small helper so its three writes don't repeat the same `if let
    /// Err(error) = ...` three times over. Every id it is ever called with
    /// here is a transcribed `include/constants/vars.h` literal well inside
    /// [`engine::event_data`]'s ordinary range, so the error arm is not
    /// reachable in practice -- logged rather than `expect`ed anyway, the
    /// same defensive posture
    /// `super::first_battle_trigger::OverworldPhase::begin_first_battle`'s
    /// own `VAR_ROUTE101_STATE` write already takes.
    fn write_var(event_data: &mut EventData, id: u16, value: u16) {
        if let Err(error) = event_data.var_set(id, value) {
            eprintln!("first battle: couldn't write var {id:#06x} ({error}) -- left as-is");
        }
    }
}
