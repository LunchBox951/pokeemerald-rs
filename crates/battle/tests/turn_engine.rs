//! Turn-engine integration tests (S-6 housekeeping, issue #209), split by
//! behavior family while exercising only the public `battle` API. Shared
//! scripted-RNG and deterministic-mon fixtures remain in `common/mod.rs`.

mod common;

#[path = "turn_engine/escape.rs"]
mod escape;
#[path = "turn_engine/first_battle.rs"]
mod first_battle;
#[path = "turn_engine/lifecycle.rs"]
mod lifecycle;
#[path = "turn_engine/move_resolution.rs"]
mod move_resolution;
#[path = "turn_engine/move_selection.rs"]
mod move_selection;
#[path = "turn_engine/stat_changes.rs"]
mod stat_changes;
#[path = "turn_engine/status_conditions.rs"]
mod status_conditions;
#[path = "turn_engine/trainer_battle.rs"]
mod trainer_battle;
#[path = "turn_engine/turn_order.rs"]
mod turn_order;
