//! Turn-engine integration tests (S-6 housekeeping, issue #209), split by
//! behavior family while exercising only the public `battle` API. Shared
//! scripted-RNG and deterministic-mon fixtures remain in `common/mod.rs`.

mod common;

#[path = "turn_engine/confusion.rs"]
mod confusion;
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
#[path = "turn_engine/paralysis.rs"]
mod paralysis;
#[path = "turn_engine/participant_admission.rs"]
mod participant_admission;
#[path = "turn_engine/pipelines.rs"]
mod pipelines;
#[path = "turn_engine/poison_player_reserve_admission.rs"]
mod poison_player_reserve_admission;
#[path = "turn_engine/poison_residual_ordering.rs"]
mod poison_residual_ordering;
#[path = "turn_engine/poison_status_stats.rs"]
mod poison_status_stats;
#[path = "turn_engine/poison_support.rs"]
mod poison_support;
#[path = "turn_engine/poison_synchronize.rs"]
mod poison_synchronize;
#[path = "turn_engine/poison_trainer_aftermath.rs"]
mod poison_trainer_aftermath;
#[path = "turn_engine/pressure.rs"]
mod pressure;
#[path = "turn_engine/shed_skin.rs"]
mod shed_skin;
#[path = "turn_engine/stat_changes.rs"]
mod stat_changes;
#[path = "turn_engine/status_ability_matrix.rs"]
mod status_ability_matrix;
#[path = "turn_engine/trainer_battle.rs"]
mod trainer_battle;
#[path = "turn_engine/turn_order.rs"]
mod turn_order;
