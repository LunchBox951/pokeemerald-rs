//! Selects what an A press in front of the player targets
//! (`GetInteractionScript`, `pokeemerald/src/field_control_avatar.c:240-258`).
//!
//! Selection only: the result names the target and the context its script reads, and nothing here
//! runs a script, locks the field, plays a sound, or opens a UI. The water stage that follows the
//! metatile stage upstream (Surf and Waterfall) is not selected here, and neither is a
//! script an object's saved RAM override replaces.

use assets::{ObjectEvent, SecretBaseId};

use super::background_event::{resolve_background_event, BackgroundEventTarget};
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::is_counter;
use super::metatile_interaction::{classify_metatile_interaction, MetatileInteractionTarget};
use super::object_event::{front_tile, visible_object_event_at, FrontTile};
use super::player::PlayerState;
use crate::event_data::EventData;

/// The object event an A press selected, with the context its script reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectInteraction {
    /// The selected object's id within its map (`gSpecialVar_LastTalked`).
    pub local_id: u8,
    /// The script authored on the object.
    pub script: &'static str,
    /// The player's facing when the button was pressed (`gSpecialVar_Facing`).
    pub player_facing: Direction,
}

/// The first target of the object, background-event, then metatile stages to select something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionTarget {
    /// A visible object in front of the player, or across a counter.
    Object(ObjectInteraction),
    /// A sign, hidden item, or secret-base entrance in front of the player.
    Background(BackgroundEventTarget),
    /// A script-backed metatile in front of the player.
    Metatile(MetatileInteractionTarget),
}

/// Selects the interaction target in front of `player`, or `None` when the press falls through.
///
/// `runtime` must be bound to the map containing `player`. `saved_secret_bases` lists the secret
/// bases the save already holds: facing a secret-base entrance whose base is among them selects no
/// background event, so the metatile stage runs next (`TrySetCurSecretBase` fails,
/// `secret_base.c:681-689`; `field_control_avatar.c:354-361`). Every stage queries the tile in front
/// of the player at the elevation [`front_tile`] derives, and reads the metatile behavior of that
/// front tile, not the tile the player stands on (`field_control_avatar.c:170-172`). An object
/// whose script is null is selected by position but has nothing to run, so selection falls
/// through to the background-event stage without trying a counter's far side.
#[must_use]
pub fn select_interaction_target(
    player: &PlayerState,
    runtime: &MapRuntime<'_>,
    event_data: &EventData,
    saved_secret_bases: &[SecretBaseId],
) -> Option<InteractionTarget> {
    let front = front_tile(player, runtime)?;
    let facing = player.facing();
    let front_behavior = runtime.metatile_behavior(front.x, front.y);

    let front_is_counter = front_behavior.is_some_and(is_counter);
    let object = interacted_object(front, facing, front_is_counter, runtime, event_data);
    if let Some((object, script)) = object.and_then(|object| Some((object, object.script?))) {
        return Some(InteractionTarget::Object(ObjectInteraction {
            local_id: object.local_id,
            script: script.id(),
            player_facing: facing,
        }));
    }

    let background = resolve_background_event(
        runtime.events(),
        front.x,
        front.y,
        front.elevation,
        facing,
        event_data,
    )
    .filter(|target| !is_saved_secret_base(target, saved_secret_bases));
    if let Some(background) = background {
        return Some(InteractionTarget::Background(background));
    }

    let front_cell = runtime.metatile_cell(front.x, front.y)?;
    classify_metatile_interaction(
        front_behavior?,
        facing,
        front.elevation,
        front_cell.elevation,
    )
    .map(InteractionTarget::Metatile)
}

/// Whether `target` is the entrance of a secret base the save already holds.
fn is_saved_secret_base(
    target: &BackgroundEventTarget,
    saved_secret_bases: &[SecretBaseId],
) -> bool {
    matches!(target, BackgroundEventTarget::SecretBase(base) if saved_secret_bases.contains(&base.secret_base_id))
}

/// The object at `front`, or else the one on the far side of a counter
/// (`GetInteractedObjectEventScript`, `field_control_avatar.c:287-313`).
///
/// The far-side lookup happens only when `front_is_counter` and reuses the front tile's elevation.
fn interacted_object<'a>(
    front: FrontTile,
    facing: Direction,
    front_is_counter: bool,
    runtime: &MapRuntime<'a>,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    visible_object_event_at(runtime, front.x, front.y, front.elevation, event_data).or_else(|| {
        if !front_is_counter {
            return None;
        }
        let (dx, dy) = facing.delta();
        let (x, y) = (front.x.checked_add(dx)?, front.y.checked_add(dy)?);
        visible_object_event_at(runtime, x, y, front.elevation, event_data)
    })
}

#[cfg(test)]
mod tests;
