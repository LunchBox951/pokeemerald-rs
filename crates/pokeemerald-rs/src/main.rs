//! `pokeemerald-rs` binary entry point (I-1 slice 1: boot shell).
//!
//! Opens the game window, composes a deterministic placeholder scene
//! through the real `rendering` pipeline, and drives the frame loop at the
//! GBA's real ~59.73 Hz cadence -- see [`app::App`] for the owned shell
//! `main` delegates to (`main` stays thin `(oop-boundaries)`) and
//! [`scene::BootScene`] for the placeholder content itself.
//!
//! Title screen, engine/battle wiring, audio, and save are all out of scope
//! for this slice -- see issue #70.
//!
//! **Manual-run only**: CI is headless, so nothing here opens a real window
//! in a test (only the headless glue in [`app`], [`scene`], and [`frame`]
//! is unit-tested). Verify locally with:
//!
//! ```sh
//! cargo run --release -p pokeemerald-rs
//! ```
//!
//! which opens a window presenting the placeholder scene (a checkerboard
//! background plus three overlapping, differently-prioritized sprites) at
//! ~59.73 Hz. Close the window or press Escape to exit cleanly.

mod app;
mod frame;
mod scene;

use std::process::ExitCode;

use app::App;

fn main() -> ExitCode {
    match App::new("pokeemerald-rs").and_then(|mut app| app.run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pokeemerald-rs: {err}");
            ExitCode::FAILURE
        }
    }
}
