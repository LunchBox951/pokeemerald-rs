//! Emerald's game-state and field-behavior primitives.
//!
//! Use this crate for deterministic randomness ([`rng`]), field-script
//! execution ([`script`]), flags and variables ([`event_data`]), text and
//! dialogue presentation ([`text`]), save data and file persistence
//! ([`save`]), and overworld rules ([`overworld`]).
//!
//! Typed game assets come from `assets`; shared data-directory resolution
//! comes from `pack-format`. Callers supply script hosts, input, and frame
//! timing. Display backends, audio playback, and application flow remain
//! outside this crate.

pub mod event_data;
pub mod overworld;
pub mod rng;
pub mod save;
pub mod script;
pub mod text;
