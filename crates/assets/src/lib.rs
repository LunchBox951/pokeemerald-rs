//! Assets subsystem (S-4): extraction pipeline + typed access.
//!
//! Exposes extracted upstream game data as idiomatic, owned Rust types
//! `(oop-boundaries)`. The first extracted table is the battle
//! type-effectiveness chart (see [`type_chart`]); more data follows as the
//! crate fills out.

pub mod error;
pub mod type_chart;

pub use error::AssetError;
pub use type_chart::{Effectiveness, Type, TypeChart};
