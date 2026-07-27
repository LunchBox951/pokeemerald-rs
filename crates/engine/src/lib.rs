//! Engine subsystem (S-5): overworld, scripts, dialog, save, RNG, menus.
//!
//! Implementation lands incrementally, one concept per module. So far:
//! - [`rng`] — Emerald's deterministic PRNG (issue #22).
//! - [`text`] — the Gen-3 text codec (issue #25), plus
//!   [`text::format`] for number-to-text formatting and placeholder
//!   expansion (issue #55), [`text::render`] for the frame-driven glyph
//!   renderer, and [`text::window`] for the message-box tile compositor
//!   (issue #124).
//! - [`script`] — the script bytecode interpreter core: context state,
//!   fetch/dispatch loop, call stack, and operand readers (issue #56), plus
//!   [`script::commands`], the first batch of concrete script commands —
//!   control flow, comparison, flags, and vars — bound to [`event_data`]
//!   (issue #69).
//! - [`event_data`] — the event flags/vars store: bit-packed flag storage,
//!   var storage with the `VARS_START` offset, and the temp/special id-space
//!   semantics (issue #65).
//! - [`save`] — upstream-offset-compatible `SaveBlock1`/`SaveBlock2` models
//!   for identity, location/warps, party Pokémon, money, bag contents, and
//!   [`event_data`] flags/vars, plus a checksummed rotating two-slot store
//!   faithful to upstream's flash layout (issues #94/#117).
//! - [`overworld`] — the map runtime ([`overworld::MapRuntime`]: typed map
//!   data binding, connection-edge crossing, object/warp/coord event
//!   lookup) and the on-foot player movement state machine
//!   ([`overworld::PlayerState`]): turn/step semantics, collision against
//!   grid geometry, and warp triggering (issue #108).

pub mod event_data;
pub mod overworld;
pub mod rng;
pub mod save;
pub mod script;
pub mod text;
