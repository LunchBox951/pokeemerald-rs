//! The warp, interaction, battle and `START` menu precedence for a frame's
//! already-decided events ([`OverworldPhase::resolve_step_events`]).

use engine::overworld::WarpTrigger;

use crate::flow::wild_encounter;
use crate::start_menu::StartMenu;

use super::interaction::InteractionOutcome;
use super::OverworldPhase;

impl OverworldPhase {
    /// Commits a built menu and takes the field lock on this frame, as
    /// `ShowStartMenu` reaches `PlayerFreeze` inline (`start_menu.c:581-591`).
    fn commit_start_menu(&mut self, ready: Option<StartMenu>) {
        if let Some(menu) = ready {
            self.take_field_lock();
            // `ShowStartMenu` also runs `StopPlayerAvatar`, which resets
            // movementDirection to facingDirection before any save can copy
            // it (`start_menu.c:581-588`, `field_player_avatar.c:1223-1228`).
            self.player.reset_movement_direction_to_facing();
            self.start_menu = Some(menu);
        }
    }

    /// [`Self::step`]'s warp/interaction/battle precedence, once every input
    /// to it has already been decided against this frame's `runtime` (every
    /// parameter here is an owned value, not a borrow of `runtime`).
    ///
    /// A resolved `warp_trigger` executes first ([`Self::warp_to`]).
    /// Same-frame interaction outcomes act only if neither a warp nor a
    /// `field_event_fired` event (an encounter or the Route 101
    /// first-battle trigger -- [`Self::step`]'s own single definition of
    /// that) already consumed the frame's field input: upstream never
    /// reaches `TryStartInteractionScript` (`:172`) on a frame anything
    /// above it returned TRUE. A [`InteractionOutcome::Dialog`] opens a
    /// message box; a [`InteractionOutcome::RivalBattle`]
    /// (`super::route103_rival_trigger`) starts the Route 103 rival battle
    /// instead -- exactly the same gate, one extra branch. The battle this
    /// frame earned -- if any -- starts next ([`Self::begin_step_battle`]).
    /// Commits the frame's owner in `ProcessPlayerFieldInput` order; an
    /// interaction claiming the frame takes the field lock on that frame.
    pub(super) fn resolve_step_events(
        &mut self,
        warp_trigger: Option<WarpTrigger>,
        encounter: Option<engine::overworld::WildEncounter>,
        field_event_fired: bool,
        first_battle_triggered: bool,
        interaction: Option<InteractionOutcome>,
        start_menu: Option<StartMenu>,
    ) {
        match warp_trigger {
            Some(WarpTrigger::Resolved { map, warp_id }) => self.warp_to(map, warp_id),
            Some(WarpTrigger::Unsupported) => eprintln!(
                "warp: destination at the player's tile can't be resolved by this port \
                 (dynamic map/warp id) -- staying put"
            ),
            None => {}
        }

        let input_consumed = wild_encounter::field_input_consumed(field_event_fired, warp_trigger);
        if input_consumed {
            if interaction.is_some() {
                eprintln!(
                    "npc dialog: discarding a same-frame interaction the warp, the wild \
                     encounter, or the Route 101 first-battle trigger takes precedence over"
                );
            }
        } else {
            if interaction.is_some() {
                self.take_field_lock();
            }
            match interaction {
                Some(InteractionOutcome::Dialog(script)) => self.open_script_dialog(script),
                Some(InteractionOutcome::RivalBattle) => self.begin_route103_rival_battle(),
                // Consumed upstream too (`InteractionOutcome::Unmodelled`'s
                // own doc comment) -- fail closed, nothing to render.
                Some(InteractionOutcome::Unmodelled(target)) => {
                    eprintln!("interaction: no launcher for {target:?} -- frame consumed");
                }
                None => {}
            }
        }

        self.begin_step_battle(first_battle_triggered, encounter);
        self.commit_start_menu(start_menu);
    }
}
