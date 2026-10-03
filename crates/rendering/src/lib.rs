//! Headless 240x160 GBA tile and sprite rendering.
//!
//! [`compose_frame`] composites [`BgLayer`], [`AffineBgLayer`], and
//! [`SpriteLayer`] inputs into an owned [`Framebuffer`].
//! [`compose_frame_with_effects`] adds windows, color special effects, and
//! mosaic, configured through [`FrameEffects`]; [`compose_frame`] is the same
//! call with default effects.
//!
//! Layer ordering, sprite admission, and palette fades are specified in the
//! module documentation ([`compositor`], [`sprite`], [`palette_fade`]).
//!
//! Uses only `std`, with no FFI and no dependency on `platform`
//! `(minimal-deps, no-ffi)`.

pub mod affine;
pub mod bg;
pub mod bg_affine;
pub mod compositor;
pub mod effects;
pub mod error;
pub mod framebuffer;
pub mod mosaic;
pub mod oam;
mod oam_budget;
pub mod palette;
pub mod palette_fade;
pub mod sprite;
mod sprite_affine;
pub mod tile;
pub mod tilemap;
pub mod window;

pub use affine::AffineMatrix;
pub use bg::BgLayer;
pub use bg_affine::{AffineBgLayer, AffineTilemap, Overflow};
pub use compositor::{
    compose_frame, compose_frame_with_effects, BgSlot, FrameEffects, PaletteColorTransform,
    PaletteStage,
};
pub use effects::{
    alpha_blend, brighten, darken, ColorEffect, EffectsConfig, LayerKind, LayerTargets,
};
pub use error::RenderError;
pub use framebuffer::Framebuffer;
pub use mosaic::{MosaicConfig, MosaicSize};
pub use oam::{obj_dimensions, AffineMode, OamEntry, ObjMode, ObjShape};
pub use palette::{Bgr555, Palette, Rgb888};
pub use palette_fade::{NormalPaletteFade, PaletteFadeStatus, PaletteFadeTarget};
pub use sprite::{SpriteLayer, SpritePixel};
pub use tile::{BitDepth, Tile, Tileset};
pub use tilemap::{ScreenEntry, Tilemap};
pub use window::{WindowConfig, WindowLayerEnable, WindowRange, WindowRect};
