//! Engine subsystem (S-5): overworld, scripts, dialog, save, RNG, menus.
//!
//! Implementation lands incrementally, one concept per module. So far:
//! - [`rng`] — Emerald's deterministic PRNG (issue #22).
//! - [`text`] — the Gen-3 text codec (issue #25), plus
//!   [`text::format`] for number-to-text formatting and placeholder
//!   expansion (issue #55).
//! - [`script`] — the script bytecode interpreter core: context state,
//!   fetch/dispatch loop, call stack, and operand readers (issue #56), plus
//!   [`script::commands`], the first batch of concrete script commands —
//!   control flow, comparison, flags, and vars — bound to [`event_data`]
//!   (issue #69).
//! - [`event_data`] — the event flags/vars store: bit-packed flag storage,
//!   var storage with the `VARS_START` offset, and the temp/special id-space
//!   semantics (issue #65).

pub mod event_data;
pub mod rng;
pub mod script;
pub mod text;
