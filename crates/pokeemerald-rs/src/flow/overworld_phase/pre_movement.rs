//! This frame's pre-movement field-input decisions
//! ([`OverworldPhase::resolve_pre_movement_field_input`]): the branches
//! upstream's `ProcessPlayerFieldInput` weighs before `PlayerStep`, in its
//! own order, as [`OverworldPhase::step`] sequences them.

use engine::overworld::{trigger_arrow_warp, Direction, MapRuntime, WarpTrigger};
use platform::ButtonState;

use crate::flow::wild_encounter;
use crate::start_menu::StartMenu;

use super::interaction::InteractionOutcome;
use super::OverworldPhase;

/// This frame's pre-movement field-input decisions.
pub(super) struct PreMovementFieldInput {
    pub(super) arrow_trigger: Option<WarpTrigger>,
    /// The pre-movement animated-door check: [`super::animated_door`]
    /// against the tile the player *faces*, not one they stand on -- see
    /// [`OverworldPhase::step`]'s "Warp timing" section.
    pub(super) animated_door_trigger: Option<WarpTrigger>,
    pub(super) interaction: Option<InteractionOutcome>,
    /// The menu a fresh `START` press built, if
    /// [`OverworldPhase::start_menu_may_open`] allowed it.
    pub(super) start_menu: Option<StartMenu>,
}

impl OverworldPhase {
    /// This call's pre-movement field-input decisions: the frame-start
    /// stance ([`Self::step`]'s "Warp timing" section), the arrow-warp
    /// poll, the same-frame interaction, the animated-door poll, and a fresh `START` press's menu, already built if it claims
    /// the frame -- `TryArrowWarp` (`field_control_avatar.c:164-168`),
    /// `TryStartInteractionScript` (`:172`), `TryDoorWarp` (`:170-178`) and
    /// `pressedStartButton` (`:182`), in that order, each skipped once one
    /// above it has claimed the frame.
    ///
    /// `landing_claimed` is [`Self::step`]'s answer for the three branches
    /// that sit *above* all four upstream -- the completed step's coordinate
    /// event and door-shaped warp (`TryStartStepBasedScript`, `:155-161`)
    /// and its encounter roll (`CheckStandardWildEncounter`, `:162`) -- each
    /// of which returns TRUE out of `ProcessPlayerFieldInput` before `:164`
    /// is ever reached. The completed-step work runs at the start of the call
    /// after the walk animation drains, so a ready landing and an at-rest
    /// field poll land on the *same* call and this gate is load-bearing.
    ///
    /// A forced tile closes the `T_TILE_CENTER` half of that same gate
    /// (`:95`), holding the four branches for the landing call alone
    /// ([`engine::overworld::PlayerState::field_input_suppressed`]).
    pub(super) fn resolve_pre_movement_field_input(
        &self,
        buttons: ButtonState,
        direction: Option<Direction>,
        runtime: &MapRuntime<'_>,
        landing_claimed: bool,
    ) -> PreMovementFieldInput {
        let facing = self.player.facing();
        let position = self.player.position();
        // `PlayerGetElevation()`'s retained `previousElevation`, not the
        // collision above -- every lookup below queries this (`field_player_avatar.c:1192-1195`).
        let previous_elevation = self.player.previous_elevation();
        // Upstream only sets `input->heldDirection`/`input->pressedAButton`
        // /`input->pressedStartButton` at all while `tileTransitionState` is
        // `T_TILE_CENTER` or `T_NOT_MOVING` (`:95-112`), and reaches none of
        // the four branches below on a frame the completed step already
        // claimed -- one predicate for both halves of that, plus the
        // forced-movement tile that closes the first of those two states
        // (method doc).
        let poll_open = wild_encounter::arrow_poll_open(self.player.in_transit(), landing_claimed)
            && !self.player.field_input_suppressed();
        let arrow_direction = direction.filter(|held| *held == facing);

        // Gated on transit only, so it fires inside the turn lock (`field_player_avatar.c:901-929`).
        let arrow_trigger = poll_open
            .then_some(arrow_direction)
            .flatten()
            .and_then(|d| {
                let (x, y) = position;
                trigger_arrow_warp(runtime, x, y, previous_elevation, d)
            });

        // NPC interaction, resolved before `advance_or_skip_for_preempt`
        // can turn or step the player (contract: `step`'s "NPC dialog
        // routing" section). Skipped when `arrow_trigger` already fired, as
        // `TryArrowWarp` returns ahead of the interaction check, and while
        // forced movement is armed, as `FieldGetPlayerInput` never reaches
        // `TryStartInteractionScript` either (method doc). The tokens
        // belong to the pre-warp map, so a same-frame warp drops them
        // rather than opening the departed map's dialog on the destination.
        let interaction =
            (!landing_claimed && arrow_trigger.is_none() && !self.player.field_input_suppressed())
                .then(|| self.interaction_tokens_this_frame(buttons, runtime))
                .flatten();

        // Upstream reaches `TryDoorWarp` only after the arrow check and the
        // A-button interaction lookup have both fallen through
        // (`field_control_avatar.c:164-178`), same as its early `return
        // TRUE`s ahead of it.
        let animated_door_trigger = (poll_open && arrow_trigger.is_none() && interaction.is_none())
            .then_some(arrow_direction)
            .flatten()
            .and_then(|d| super::animated_door::trigger(runtime, position, previous_elevation, d));

        let start_menu = self
            .start_menu_may_open(
                buttons,
                landing_claimed
                    || arrow_trigger.is_some()
                    || interaction.is_some()
                    || animated_door_trigger.is_some(),
            )
            .then(|| self.build_start_menu())
            .flatten();

        PreMovementFieldInput {
            arrow_trigger,
            animated_door_trigger,
            interaction,
            start_menu,
        }
    }
}
