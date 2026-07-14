//! Rendering subsystem (S-2): 240x160 tile / sprite / layer renderer.
//!
//! This slice (issue #50) stands up the crate's headless-testable core: an
//! owned [`Framebuffer`], faithful GBA BGR555 -> RGB888 palette conversion
//! ([`Bgr555::to_rgb888`]), 4bpp/8bpp indexed tile decoding ([`Tileset`]),
//! and a single regular (non-affine, non-scrolling) background tile layer
//! compositor ([`BgLayer`]). Sprites/OAM, multi-layer priority, BG
//! scrolling/affine transforms, and windows/blending effects are follow-on
//! S-2 slices; wiring this crate into `platform`'s presentation surface is a
//! future integration issue `(constitution-vs-roadmap)`.
//!
//! `std`-only, no FFI, no dependency on `platform` `(minimal-deps, no-ffi)`.
//! Behaviour is transcribed from `pokeemerald/src/palette.c`,
//! `pokeemerald/src/bg.c`, and `pokeemerald/src/gpu_regs.c` — verified
//! against `mgba`'s software renderer as the hardware-behaviour reference —
//! never copied verbatim `(no-verbatim, behavioral-fidelity)`.

pub mod bg;
pub mod error;
pub mod framebuffer;
pub mod palette;
pub mod tile;

pub use bg::{BgLayer, ScreenEntry, Tilemap};
pub use error::RenderError;
pub use framebuffer::Framebuffer;
pub use palette::{Bgr555, Palette, Rgb888};
pub use tile::{BitDepth, Tile, Tileset};
