//! Rendering subsystem (S-2): 240x160 tile / sprite / layer renderer.
//!
//! Headless, testable core: an owned [`Framebuffer`], faithful GBA
//! BGR555 -> RGB888 palette conversion ([`Bgr555::to_rgb888`]), and
//! 4bpp/8bpp indexed tile decoding ([`Tileset`]). Regular ([`BgLayer`])
//! and affine/rotation-scaling ([`AffineBgLayer`]) background tile
//! layers, plus an OAM-equivalent sprite layer ([`SpriteLayer`],
//! [`OamEntry`]) with affine and double-size sampling, all composite
//! through [`compose_frame`], which orders up to four BG layers plus
//! sprites the way the GBA PPU does.
//!
//! [`compositor::compose_frame_with_effects`] extends composition with
//! the full hardware effect group: windows (`WIN0`/`WIN1`/`OBJWIN`/
//! `WINOUT`, [`window`]), color special effects (alpha blend, brighten,
//! darken, [`effects`]), and mosaic ([`mosaic`]), all controlled by the
//! [`compositor::FrameEffects`] parameter struct. [`compose_frame`]
//! delegates to it with [`compositor::FrameEffects::default`].
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
