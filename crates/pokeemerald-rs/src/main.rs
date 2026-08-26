//! `pokeemerald-rs` binary entry point (I-1 slice 1: boot shell; I-2, issue
//! #109: real title screen; S-4, issue #122: `--import-rom`).
//!
//! Loads the real title screen from the installed asset pack, opens the
//! game window, and drives the frame loop at the GBA's
//! real ~59.73 Hz cadence -- see [`pokeemerald_rs::App`] for the owned shell
//! `main` delegates to (`main` stays thin `(oop-boundaries)`) and the
//! library crate root (`src/lib.rs`) for why `App` lives there rather than
//! here.
//!
//! [`cli`] parses the one flag this binary takes. `--import-rom <path>`
//! builds the asset pack from the player's own Pokemon Emerald (US) ROM
//! ([`pokeemerald_rs::import_rom`]) and exits without opening a window;
//! `--help` prints [`cli::USAGE`]; anything else plays the game.
//!
//! No pack yet? `App::new` fails with a clean diagnostic (naming both
//! `--import-rom` for players and `./init.sh`/`cargo xtask extract` for
//! developers) printed by
//! this `main`, not a panic -- see [`pokeemerald_rs::app::AppError`].
//!
//! The intro cinematic, new-game flow, engine/battle wiring, audio, and save
//! are all out of scope for this slice -- see issue #70 and #109.
//!
//! **Manual-run only**: CI is headless, so this binary never runs in a
//! test -- only the headless glue in `pokeemerald_rs::app`, `::scene`,
//! `::title`, and `::frame` is unit-tested, plus `xtask`'s `e2e --suite
//! smoke` run, which drives `App::new_headless` in-process (F-3, V-1) --
//! that constructor always uses the I-1 synthetic scene, never the real
//! title screen (see `pokeemerald_rs::app`'s module docs). [`cli`]'s own
//! parse tests do run under `cargo test --workspace`, as this target's
//! unit tests. Verify the real
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

mod cli;

use cli::Command;

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("pokeemerald-rs: {err}");
            return ExitCode::FAILURE;
        }
    };

    match command {
        // Help is what was asked for, so delivering it is success.
        Command::Help => {
            println!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        Command::ImportRom { path } => match pokeemerald_rs::import_rom(&path) {
            Ok(outcome) => {
                println!("{outcome}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("pokeemerald-rs: {err}");
                ExitCode::FAILURE
            }
        },
        Command::Play => match App::new("pokeemerald-rs").and_then(|mut app| app.run()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("pokeemerald-rs: {err}");
                ExitCode::FAILURE
            }
        },
    }
}
