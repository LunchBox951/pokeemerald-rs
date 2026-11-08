//! Tests for [`super::first_battle_trigger`]: the Route 101 rescue coord-event
//! trigger, its `VAR_ROUTE101_STATE` bookkeeping, and its precedence over the
//! wild encounter roll.
//!
//! # Layout
//!
//! - `support`: the shared Route 101 fixtures.
//! - `trigger`: the var bookkeeping, firing, the scripted battle and its abort.
//! - `connection_entry`: real-pack crossing into Route 101.
//! - `precedence`: the trigger against the wild roll, forced movement, warps and neighbours.
//! - `stacked_events`: fabricated stacked coord-event scans.

mod connection_entry;
mod precedence;
mod stacked_events;
mod support;
mod trigger;
