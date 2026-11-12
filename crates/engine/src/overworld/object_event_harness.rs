//! Test harness for the object-event scheduler: build a collection from authored-shaped
//! templates, run frames against a scripted field RNG, and read the result back.
//!
//! Later movement types extend it by scripting [`ScriptedRng`] draws and ticking [`Rig`].

use assets::{MovementType, ObjectEvent, TrainerType};

use crate::event_data::EventData;
use crate::rng::RandomSource;

use super::object_event::{FrameContext, ObjectEventCollection};
use super::player::TilePos;

/// A field RNG that replays fixed draws and panics once they run out, so a movement type that
/// draws when it must not fails the test at the draw.
pub(crate) struct ScriptedRng {
    values: Vec<u16>,
    draws: usize,
}

impl ScriptedRng {
    pub(crate) fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            draws: 0,
        }
    }

    /// A script with no draws: any draw panics.
    pub(crate) fn exhausted() -> Self {
        Self::new([])
    }

    pub(crate) const fn draws(&self) -> usize {
        self.draws
    }

    /// Whether the whole script is still unconsumed.
    pub(crate) fn is_untouched(&self) -> bool {
        self.draws == 0
    }
}

impl RandomSource for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .values
            .get(self.draws)
            .copied()
            .unwrap_or_else(|| panic!("ScriptedRng exhausted after {} draws", self.draws));
        self.draws += 1;
        value
    }
}

/// A template with the fields the scheduler reads; the hide flag defaults to never hidden.
pub(crate) fn template(
    local_id: u8,
    x: i16,
    y: i16,
    movement_type: MovementType,
    flag: &'static str,
) -> ObjectEvent {
    ObjectEvent {
        local_id,
        graphics_id: "OBJ_EVENT_GFX_MOM",
        x,
        y,
        elevation: 3,
        movement_type,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: None,
        flag,
    }
}

pub(crate) fn leaked(events: Vec<ObjectEvent>) -> &'static [ObjectEvent] {
    Box::leak(events.into_boxed_slice())
}

/// Runs the map-load admission pass over `collection` with the player at `player` and `event_data`
/// as the flags: every in-window entry whose hide flag is clear becomes active.
pub(crate) fn admit(
    collection: &mut ObjectEventCollection,
    player: TilePos,
    event_data: &EventData,
) {
    collection.run_frame(
        &FrameContext {
            player_position: player,
            event_data,
            frozen: false,
            player_dashing: false,
        },
        &mut ScriptedRng::exhausted(),
    );
}

/// Seeds `templates` and admits every one in view of `player` with no flag set.
pub(crate) fn admitted(
    templates: &'static [ObjectEvent],
    player: TilePos,
) -> ObjectEventCollection {
    let mut collection = ObjectEventCollection::from_templates(templates);
    admit(&mut collection, player, &EventData::new());
    collection
}

/// One collection, its flags, the player's tile, a freeze switch, and a field RNG.
pub(crate) struct Rig {
    pub(crate) collection: ObjectEventCollection,
    pub(crate) event_data: EventData,
    pub(crate) player: TilePos,
    pub(crate) frozen: bool,
    pub(crate) dashing: bool,
    pub(crate) rng: ScriptedRng,
}

impl Rig {
    /// Seeds `templates` with the player at `player` and an RNG that fails on any draw.
    pub(crate) fn new(templates: Vec<ObjectEvent>, player: TilePos) -> Self {
        Self {
            collection: ObjectEventCollection::from_templates(leaked(templates)),
            event_data: EventData::new(),
            player,
            frozen: false,
            dashing: false,
            rng: ScriptedRng::exhausted(),
        }
    }

    /// Runs `frames` overworld frames at the rig's current player tile and freeze state.
    pub(crate) fn tick(&mut self, frames: usize) {
        for _ in 0..frames {
            self.collection.run_frame(
                &FrameContext {
                    player_position: self.player,
                    event_data: &self.event_data,
                    frozen: self.frozen,
                    player_dashing: self.dashing,
                },
                &mut self.rng,
            );
        }
    }
}
