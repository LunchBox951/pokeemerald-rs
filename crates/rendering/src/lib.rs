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
//! Slice 3 (issue #98) adds affine (rotation/scaling) support: the shared
//! [`AffineMatrix`] parameter type, an affine BG tile layer
//! ([`AffineTilemap`], [`AffineBgLayer`]), and affine (plus double-size)
//! sprite sampling on [`SpriteLayer`] via [`OamEntry::with_affine`] and
//! [`SpriteLayer::with_affine_matrices`]. Both slot into
//! [`compose_frame`]/[`BgSlot`] without changing their signatures
//! ([`BgSlot::new_affine`] adds the affine BG entry point).
//!
//! Slice 4 (issue #99) completes the deferred effect group: hardware
//! windows (`WIN0`/`WIN1`/`OBJWIN`/`WINOUT`, [`window`]), color special
//! effects (alpha blend, brighten, darken, [`effects`]), and mosaic
//! ([`mosaic`]), all wired into [`compositor::compose_frame_with_effects`]
//! via the [`compositor::FrameEffects`] parameter struct.
//! [`compose_frame`]'s own signature is unchanged — it delegates to
//! [`compositor::compose_frame_with_effects`] with
//! [`compositor::FrameEffects::default`], which reproduces pre-slice-4
//! output byte-for-byte.
//!
//! Wiring this crate into `platform`'s presentation surface is a future
//! integration issue `(constitution-vs-roadmap)`.
//!
//! `std`-only, no FFI, no dependency on `platform` `(minimal-deps, no-ffi)`.
//! Behaviour is transcribed from `pokeemerald/src/palette.c`,
//! `pokeemerald/src/bg.c`, and `pokeemerald/src/sprite.c` — verified against
//! `mgba`'s software renderer as the hardware-behaviour reference — never
//! copied verbatim `(no-verbatim, behavioral-fidelity)`.

pub mod affine;
pub mod bg;
pub mod bg_affine;
pub mod compositor;
pub mod effects;
pub mod error;
pub mod framebuffer;
pub mod mosaic;
pub mod oam;
pub mod palette;
pub mod sprite;
mod sprite_affine;
pub mod tile;
pub mod tilemap;
pub mod window;

pub use affine::AffineMatrix;
pub use bg::BgLayer;
pub use bg_affine::{AffineBgLayer, AffineTilemap, Overflow};
pub use compositor::{compose_frame, compose_frame_with_effects, BgSlot, FrameEffects};
pub use effects::{
    alpha_blend, brighten, darken, ColorEffect, EffectsConfig, LayerKind, LayerTargets,
};
pub use error::RenderError;
pub use framebuffer::Framebuffer;
pub use mosaic::{MosaicConfig, MosaicSize};
pub use oam::{obj_dimensions, AffineMode, OamEntry, ObjMode, ObjShape};
pub use palette::{Bgr555, Palette, Rgb888};
pub use sprite::{SpriteLayer, SpritePixel};
pub use tile::{BitDepth, Tile, Tileset};
pub use tilemap::{ScreenEntry, Tilemap};
pub use window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};
