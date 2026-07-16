//! Engine subsystem (S-5): overworld, scripts, dialog, save, RNG, menus.
//!
//! Implementation lands incrementally, one concept per module. So far:
//! - [`rng`] — Emerald's deterministic PRNG (issue #22).
//! - [`text`] — the Gen-3 text codec (issue #25), plus
//!   [`text::format`] for number-to-text formatting and placeholder
//!   expansion (issue #55).
//! - [`script`] — the script bytecode interpreter core: context state,
//!   fetch/dispatch loop, call stack, and operand readers, with no concrete
//!   script commands yet (issue #56).

pub mod rng;
pub mod script;
pub mod text;
