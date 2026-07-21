//! Rendering subsystem (S-2): 240x160 tile / sprite / layer renderer.
//!
//! Slice 1 (issue #50, merged) stood up the crate's headless-testable core:
//! an owned [`Framebuffer`], faithful GBA BGR555 -> RGB888 palette
//! conversion ([`Bgr555::to_rgb888`]), 4bpp/8bpp indexed tile decoding
//! ([`Tileset`]), and a single regular (non-affine) background tile layer
//! compositor ([`BgLayer`]).
//!
//! Slice 2 (issue #64) adds an OAM-equivalent sprite layer ([`OamEntry`],
//! [`SpriteLayer`]), wrapping regular-BG scroll offsets
//! ([`BgLayer::composite_scrolled`]), and the cross-layer priority
//! compositor ([`compose_frame`]) that orders up to four BG layers plus
//! sprites the way the GBA PPU does.
//!
//! Affine transforms (BG or sprite), windows (`WIN0`/`WIN1`/`OBJWIN`), alpha
//! blending/brightness effects, and mosaic are out of scope for both slices;
//! wiring this crate into `platform`'s presentation surface is a future
//! integration issue `(constitution-vs-roadmap)`.
//!
//! `std`-only, no FFI, no dependency on `platform` `(minimal-deps, no-ffi)`.
//! Behaviour is transcribed from `pokeemerald/src/palette.c`,
//! `pokeemerald/src/bg.c`, and `pokeemerald/src/sprite.c` — verified against
//! `mgba`'s software renderer as the hardware-behaviour reference — never
//! copied verbatim `(no-verbatim, behavioral-fidelity)`.

pub mod bg;
pub mod compositor;
pub mod error;
pub mod framebuffer;
pub mod oam;
pub mod palette;
pub mod sprite;
pub mod tile;
pub mod tilemap;

pub use bg::BgLayer;
pub use compositor::{compose_frame, BgSlot};
pub use error::RenderError;
pub use framebuffer::Framebuffer;
pub use oam::{obj_dimensions, OamEntry, ObjShape};
pub use palette::{Bgr555, Palette, Rgb888};
pub use sprite::{SpriteLayer, SpritePixel};
pub use tile::{BitDepth, Tile, Tileset};
pub use tilemap::{ScreenEntry, Tilemap};
