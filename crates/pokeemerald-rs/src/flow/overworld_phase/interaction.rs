//! A-press interaction lookup ([`OverworldPhase::interaction_tokens_this_frame`])
//! and its [`InteractionOutcome`].

use engine::overworld::{select_interaction_target, Direction, InteractionTarget};
use platform::{ButtonState, Buttons};

use crate::overworld::npc_scripts;

use super::OverworldPhase;

/// The save block holds no secret bases yet, so no entrance is ever already taken.
const NO_SAVED_SECRET_BASES: &[assets::SecretBaseId] = &[];

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
        let target = select_interaction_target(
            &self.player,
            runtime,
            &self.save1.event_data,
            NO_SAVED_SECRET_BASES,
        )?;
        Some(self.find_interaction_outcome(target))
    }

    /// The lookup half of [`OverworldPhase::interaction_tokens_this_frame`]:
    /// decide what an A press does with `target`, the first of the object,
    /// background-event, then metatile stages to select anything
    /// ([`select_interaction_target`]).
    ///
    /// Route 103's rival object event (issue #248,
    /// `super::route103_rival_trigger::is_rival_trigger`) is checked
    /// *before* the ordinary dialog lookup: its own `script`,
    /// `"Route103_EventScript_Rival"`, is deliberately not one
    /// [`npc_scripts::script_text`] recognizes (that module's own bounded
    /// table), so without this branch it would simply open no dialog on A,
    /// a silent gap rather than the trainer battle it should start. Every
    /// other object event's script is unaffected. Mom's script
    /// (`OBJ_EVENT_GFX_MOM`'s) additionally carries its `faceplayer` turn
    /// ([`FacePlayer`]); no other script does.
    ///
    /// Every selected target consumes the frame, as a script start does
    /// upstream (`field_control_avatar.c:172`); a script this port doesn't
    /// model has nothing to render.
    fn find_interaction_outcome(&self, target: InteractionTarget) -> InteractionOutcome {
        match target {
            InteractionTarget::Object(object) => {
                if super::route103_rival_trigger::is_rival_trigger(self.map_id, object.script) {
                    InteractionOutcome::RivalBattle
                } else {
                    npc_scripts::script_text(object.script).map_or(
                        InteractionOutcome::Unmodelled(target),
                        |tokens| {
                            InteractionOutcome::Dialog(ScriptDialog {
                                tokens,
                                face_player: npc_scripts::script_faces_player(object.script)
                                    .then_some(FacePlayer {
                                        local_id: object.local_id,
                                        direction: object.player_facing.opposite(),
                                    }),
                            })
                        },
                    )
                }
            }
            InteractionTarget::Background(_) | InteractionTarget::Metatile(_) => {
                InteractionOutcome::Unmodelled(target)
            }
        }
    }
}

/// The `faceplayer` half of a supported script: turn the selected live object event toward the
/// player, as `ObjectEventFaceOppositeDirection(selected, GetPlayerFacingDirection())` does
/// (`scrcmd.c:1153-1158`, `event_object_movement.c:4975-4978`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FacePlayer {
    /// The selected object's id within its map (`gSelectedObjectEvent`).
    local_id: u8,
    /// The direction the object ends up facing: opposite the player's facing.
    direction: Direction,
}

/// A supported script's message, with the object mutation that precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScriptDialog {
    /// The token stream the [`crate::overworld::NpcDialog`] opens with.
    tokens: Vec<engine::text::Token>,
    /// `Some` when the script runs `faceplayer` before its message box.
    face_player: Option<FacePlayer>,
}

impl OverworldPhase {
    /// Runs a script's `faceplayer`, returning the dialog's tokens.
    ///
    /// Called as the interaction claims the frame, ahead of the dialog opening and of anything
    /// reading the object's facing afterwards. Turns the live entry only: the authored template
    /// is never written, and the object keeps its tile.
    pub(super) fn start_script_dialog(&mut self, dialog: ScriptDialog) -> Vec<engine::text::Token> {
        if let Some(FacePlayer {
            local_id,
            direction,
        }) = dialog.face_player
        {
            if let Some(entry) = self.object_events.get_mut(local_id) {
                entry.state_mut().face(direction);
            }
        }
        dialog.tokens
    }
}

/// What a same-frame A-press interaction ([`OverworldPhase::interaction_tokens_this_frame`])
/// should do -- a dialog box (the ordinary NPC case,
/// [`npc_scripts::script_text`]), the Route 103 rival battle (issue #248),
/// or nothing observable for a selected target this port doesn't launch
/// yet -- never more than one. Not a dialog itself: a
/// trainer battle is not a message box, so it needs its own outcome rather
/// than being squeezed into [`Vec<engine::text::Token>`]'s shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InteractionOutcome {
    /// Open an [`crate::overworld::NpcDialog`] with this script's message, after the
    /// script's `faceplayer`, if it has one.
    Dialog(ScriptDialog),
    /// Start the Route 103 rival battle
    /// ([`OverworldPhase::begin_route103_rival_battle`]).
    RivalBattle,
    /// A selected target this port has no launcher for: an object whose
    /// script [`npc_scripts::script_text`] doesn't recognize, a sign or
    /// hidden item, or a script-backed metatile. Upstream still consumes the
    /// frame ([`OverworldPhase::find_interaction_outcome`]'s own doc
    /// comment), but this port has nothing to render for it.
    Unmodelled(InteractionTarget),
}
