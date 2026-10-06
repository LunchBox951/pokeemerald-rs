//! NPC A-press interaction lookup ([`OverworldPhase::interaction_tokens_this_frame`])
//! and its [`InteractionOutcome`].

use engine::overworld::facing_object_event;
use platform::{ButtonState, Buttons};

use crate::overworld::npc_scripts;

use super::OverworldPhase;

impl OverworldPhase {
    /// The token stream a [`crate::overworld::NpcDialog`] should open with this frame, or
    /// `None` -- [`OverworldPhase::step`]'s whole A-press decision, in one
    /// place.
    ///
    /// Two gates, in upstream's own order:
    ///
    /// 1. **The player must be between steps.** `FieldGetPlayerInput` only
    ///    ever sets `input->pressedAButton` while
    ///    `gPlayerAvatar.tileTransitionState` is `T_TILE_CENTER` or
    ///    `T_NOT_MOVING` (`pokeemerald/src/field_control_avatar.c:95-107`)
    ///    -- an A press *during* a tile crossing is discarded outright,
    ///    never queued -- and `ProcessPlayerFieldInput` reaches
    ///    `TryStartInteractionScript` only through that flag (`:172`). This
    ///    port's counterpart to that transition state is
    ///    [`PlayerState::in_transit`], the same gate
    ///    [`OverworldPhase::step`]'s warp check already uses for
    ///    `tookStep`.
    /// 2. **The A press must be a fresh edge** (`newKeys`, not `heldKeys`).
    ///
    /// Called from [`OverworldPhase::step`] before that frame's movement is
    /// applied (contract and citations in that method's "NPC dialog routing"
    /// section). `self.player.in_transit()` is frame-exact with upstream:
    /// `UpdatePlayerAvatarTransitionState` runs before `FieldGetPlayerInput`
    /// (`overworld.c:1442-1454`), so A is first accepted the frame after a
    /// walk's last movement pixel.
    ///
    /// `&self` (not `&mut self`): [`OverworldPhase::step`] calls this while
    /// `runtime` still borrows `self.scene`, and acting on the outcome
    /// itself (which does need `&mut self`) happens afterward, once that
    /// borrow has ended.
    pub(super) fn interaction_tokens_this_frame(
        &self,
        buttons: ButtonState,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<InteractionOutcome> {
        if self.player.in_transit() || !buttons.is_newly_pressed(Buttons::A) {
            return None;
        }
        self.find_interaction_outcome(runtime)
    }

    /// The lookup half of [`OverworldPhase::interaction_tokens_this_frame`]:
    /// find the object event `self.player` currently faces
    /// ([`facing_object_event`]) and decide what an A press on it does.
    ///
    /// Route 103's rival object event (issue #248,
    /// `super::route103_rival_trigger::is_rival_trigger`) is checked
    /// *before* the ordinary dialog lookup: its own `script`,
    /// `"Route103_EventScript_Rival"`, is deliberately not one
    /// [`npc_scripts::script_text`] recognizes (that module's own bounded
    /// table), so without this branch it would simply open no dialog on A,
    /// a silent gap rather than the trainer battle it should start. Every
    /// other object event's script is unaffected -- Mom's own dialog path
    /// (`OBJ_EVENT_GFX_MOM`'s script) is byte-identical to before this
    /// method grew a second arm.
    ///
    /// Below that, only `"0x0"` (upstream's `NULL`-script sentinel) means no
    /// interaction; any other unrecognized script still consumes the frame
    /// upstream (`field_control_avatar.c:172`).
    fn find_interaction_outcome(
        &self,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<InteractionOutcome> {
        let object = facing_object_event(&self.player, runtime, &self.save1.event_data)?;
        if super::route103_rival_trigger::is_rival_trigger(self.map_id, object.script) {
            return Some(InteractionOutcome::RivalBattle);
        }
        if object.script == "0x0" {
            return None;
        }
        Some(
            npc_scripts::script_text(object.script)
                .map_or(InteractionOutcome::Unmodelled, InteractionOutcome::Dialog),
        )
    }
}

/// What a same-frame A-press interaction ([`OverworldPhase::interaction_tokens_this_frame`])
/// should do -- a dialog box (the ordinary NPC case,
/// [`npc_scripts::script_text`]), the Route 103 rival battle (issue #248),
/// or nothing observable for a real script this port doesn't model yet --
/// never more than one. Not a dialog itself: a
/// trainer battle is not a message box, so it needs its own outcome rather
/// than being squeezed into [`Vec<engine::text::Token>`]'s shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InteractionOutcome {
    /// Open an [`crate::overworld::NpcDialog`] with this token stream.
    Dialog(Vec<engine::text::Token>),
    /// Start the Route 103 rival battle
    /// ([`OverworldPhase::begin_route103_rival_battle`]).
    RivalBattle,
    /// A real, non-`"0x0"` script [`npc_scripts::script_text`] doesn't
    /// recognize: upstream still consumes the frame (`find_interaction_outcome`'s
    /// own doc comment), but this port has nothing to render for it.
    Unmodelled,
}
