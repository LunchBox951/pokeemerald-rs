//! `pokeemerald-rs` library surface (I-1 slice 1: boot shell): the owned
//! [`App`] type the binary crate's thin `main` (`src/main.rs`) delegates to,
//! plus the modules it is built from.
//!
//! Split into a library plus a thin binary so `xtask`'s headless `e2e
//! --suite smoke` run (F-3, V-1) can construct and drive [`App`] in-process
//! via [`App::new_headless`]/[`App::step`], without spawning the compiled
//! binary `(oop-boundaries)`.

pub mod app;
pub mod frame;
pub mod scene;

pub use app::App;
