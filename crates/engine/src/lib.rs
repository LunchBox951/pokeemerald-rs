//! Engine subsystem (S-5): overworld, scripts, dialog, save, RNG, menus.
//!
//! Implementation lands incrementally, one concept per module. So far:
//! - [`rng`] — Emerald's deterministic PRNG (issue #22).
//! - [`text`] — the Gen-3 text codec (issue #25).

pub mod rng;
pub mod text;
