//! Route 101 rescue trigger: stepping onto the rescue coord event starts the first battle directly,
//! standing in for the cutscene and starter selection
//! (`pokeemerald/data/maps/Route101/scripts.inc:19-42`, `pokeemerald/src/battle_setup.c:917-948`).
//! Coord events run before the step's warps and wild encounters
//! (`pokeemerald/src/field_control_avatar.c:155-163,483-495`).

use engine::event_data::EventData;
use engine::overworld::MapRuntime;

use crate::flow::first_battle;
use crate::flow::lead_screen::ensure_lead_can_battle;

use super::{ActiveBattle, OverworldPhase};

const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

const VAR_ROUTE101_STATE: u16 = 0x4060;

const ROUTE101_STATE_GUARD_NAME: &str = "VAR_ROUTE101_STATE";

const TRIGGER_RUN_IMMEDIATELY: &str = "TRIGGER_RUN_IMMEDIATELY";

const RESCUE_NOT_PRIMED_STATE: u16 = 0;

const RESCUE_TRIGGER_READY_STATE: u16 = 1;

const TRIGGER_CONSUMED_STATE: u16 = 2;

const TRIGGER_SCRIPT: &str = "Route101_EventScript_StartBirchRescue";

/// Upstream's on-frame script moves the state from 0 to 1
/// (`pokeemerald/data/maps/Route101/scripts.inc:10-17`).
pub(super) fn sync_route_101_state_on_entry(map: assets::MapId, event_data: &mut EventData) {
    if map != ROUTE_101 {
        return;
    }
    if event_data.var_get(VAR_ROUTE101_STATE) == Ok(RESCUE_NOT_PRIMED_STATE) {
        event_data
            .var_set(VAR_ROUTE101_STATE, RESCUE_TRIGGER_READY_STATE)
            .expect("VAR_ROUTE101_STATE is an ordinary var id");
    }
}

impl OverworldPhase {
    /// The scan ends at the first candidate that yields a script; weather and
    /// immediate-trigger candidates yield none, so the scan continues past them
    /// (`pokeemerald/src/field_control_avatar.c:877-915`). A guard on any other
    /// variable cannot be evaluated here, so it counts as yielding and the
    /// rescue does not fire.
    fn first_battle_trigger_at(
        &self,
        runtime: &MapRuntime<'_>,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> bool {
        if self.map_id != ROUTE_101 {
            return false;
        }
        for event in runtime.coord_events_at(x, y, elevation) {
            let assets::CoordEventKind::Trigger {
                var,
                var_value,
                script,
            } = event.kind
            else {
                continue;
            };
            if var == TRIGGER_RUN_IMMEDIATELY {
                continue;
            }
            let unknown_guard = var != ROUTE101_STATE_GUARD_NAME;
            if unknown_guard {
                return false;
            }
            let guard_fails = self.save1.event_data.var_get(VAR_ROUTE101_STATE) != Ok(var_value);
            if guard_fails {
                continue;
            }
            return script == TRIGGER_SCRIPT;
        }
        false
    }

    pub(super) fn first_battle_trigger_ready(
        &self,
        runtime: &MapRuntime<'_>,
        completed_landing: Option<(i32, i32)>,
    ) -> bool {
        let Some((x, y)) = completed_landing else {
            return false;
        };
        let retained_elevation = self.player.previous_elevation();
        self.first_battle_trigger_at(runtime, x, y, retained_elevation)
    }

    /// Upstream consumes the rescue before the fight (`Route101/scripts.inc:40`);
    /// only a lead the turn engine refuses (an inadmissible battler, or a saved held item
    /// whose effect it does not model) leaves it armed, with nothing drawn.
    pub(super) fn begin_first_battle(&mut self) {
        if let Some(Err(error)) = self.lead_battler().map(ensure_lead_can_battle) {
            eprintln!("first battle: can't start ({error:?}) -- the rescue stays armed");
            return;
        }
        self.first_battle_outcome = None;
        eprintln!(
            "first battle: Route 101 rescue trigger reached -- starting the scripted Zigzagoon \
             battle (issue #231)"
        );
        if let Err(error) = self
            .save1
            .event_data
            .var_set(VAR_ROUTE101_STATE, TRIGGER_CONSUMED_STATE)
        {
            eprintln!(
                "first battle: couldn't advance VAR_ROUTE101_STATE ({error}) -- the trigger \
                 may re-fire"
            );
        }
        let Some(lead) = self.take_lead_battler() else {
            eprintln!("first battle: no party mon yet -- no battle to start");
            return;
        };
        let player_trainer_id = u32::from_le_bytes(self.save2.player_trainer_id);
        // The start consumes its copy; the lent original returns on rejection.
        match first_battle::start_first_battle(lead.clone(), player_trainer_id, &mut self.rng) {
            Ok(battle) => {
                self.active_battle = Some(ActiveBattle::First(battle));
                // The first battle restarts wild-encounter immunity like any battle
                // (`pokeemerald/src/battle_setup.c:941`).
                self.wild.restart_immunity_steps();
            }
            Err(error) => {
                eprintln!("first battle: can't start ({error:?})");
                self.restore_lead_battler(lead);
            }
        }
    }

    pub(super) fn advance_first_battle_frame(
        &mut self,
        battle: battle::Battle,
    ) -> Option<ActiveBattle> {
        let mut slot = Some(battle);
        let mut returned_lead = None;
        let outcome =
            first_battle::advance_first_battle(&mut slot, &mut returned_lead, &mut self.rng);
        if let Some(lead) = returned_lead {
            self.restore_lead_battler(lead);
        }
        if let Some(outcome) = outcome {
            eprintln!("first battle: ended -- {outcome:?}");
            self.first_battle_outcome = Some(outcome);
            self.conclude_first_battle();
        }
        if slot.is_none() {
            self.reinit_field_tileset_animations();
        }
        slot.map(ActiveBattle::First)
    }
}
