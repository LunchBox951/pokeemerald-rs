//! `pokeemerald-rs` library surface (I-1 slice 1: boot shell; I-2, issue
//! #109: real title screen): the owned [`App`] type the binary crate's thin
//! `main` (`src/main.rs`) delegates to, plus the modules it is built from.
//!
//! Split into a library plus a thin binary so `xtask`'s headless `e2e
//! --suite smoke` run (F-3, V-1) can construct and drive [`App`] in-process
//! via [`App::new_headless`]/[`App::step`], without spawning the compiled
//! binary `(oop-boundaries)`.
//!
//! [`title`] decodes the real title screen from the local asset pack
//! (`assets::pack`, populated by `cargo xtask extract`); [`App::new`] uses
//! it, falling back to no scene at all (a clean, no-panic diagnostic) when
//! no pack has been extracted yet -- see [`title`]'s module docs.
//!
//! [`overworld`] (I-3, issue #126) composes the map viewport + player OBJ
//! presentation lane over the `engine` overworld runtime (S-5, PR #120) --
//! see its module docs. Not yet wired into [`App`]'s own frame loop (a
//! future integration slice); reachable today via
//! [`overworld::OverworldScene`] directly.

pub mod app;
pub mod frame;
pub mod overworld;
pub mod scene;
pub mod title;

pub use app::App;
