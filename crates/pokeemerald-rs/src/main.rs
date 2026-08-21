//! `pokeemerald-rs` binary entry point (I-1 slice 1: boot shell; I-2, issue
//! #109: real title screen).
//!
//! Loads the real title screen from the local asset pack (`cargo xtask
//! extract`), opens the game window, and drives the frame loop at the GBA's
//! real ~59.73 Hz cadence -- see [`pokeemerald_rs::App`] for the owned shell
//! `main` delegates to (`main` stays thin `(oop-boundaries)`) and the
//! library crate root (`src/lib.rs`) for why `App` lives there rather than
//! here.
//!
//! No pack extracted yet? `App::new` fails with a clean diagnostic (naming
//! the exact `./init.sh`/`cargo xtask extract` commands to run) printed by
//! this `main`, not a panic -- see [`pokeemerald_rs::app::AppError`].
//!
//! The reduced intro cinematic, new-game flow, and the title -> main menu
//! -> intro -> overworld scene transitions run inside
//! `pokeemerald_rs::flow`'s `advance_scene`; battles run *inside* the
//! overworld phase (`pokeemerald_rs::flow`'s `overworld_phase` module,
//! which owns the overworld scene). The save medium is owned by
//! [`pokeemerald_rs::App`] and threaded through every `advance_scene`
//! call; `App` also holds the title music's audio device, driven
//! separately after each scene step.
//!
//! **Manual-run only**: CI is headless, so this binary never runs in a
//! test -- only the library crate's headless glue is unit-tested, plus
//! `xtask`'s `e2e --suite smoke` run, which drives `App::new_headless`
//! in-process (F-3, V-1) --
//! that constructor always uses the I-1 synthetic scene, never the real
//! title screen (see `pokeemerald_rs::app`'s module docs). Verify the real
//! windowed shell locally with:
//!
//! ```sh
//! cargo xtask extract   # once, after ./init.sh
//! cargo run --release -p pokeemerald-rs
//! ```
//!
//! which opens a window presenting the real title screen at ~59.73 Hz.
//! Close the window or press Escape to exit cleanly.

use std::process::ExitCode;

use pokeemerald_rs::App;

fn main() -> ExitCode {
    match App::new("pokeemerald-rs").and_then(|mut app| app.run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pokeemerald-rs: {err}");
            ExitCode::FAILURE
        }
    }
}
