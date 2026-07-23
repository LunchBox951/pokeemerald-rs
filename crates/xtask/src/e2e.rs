//! `e2e --suite smoke` (F-3, V-1): a headless boot-shell smoke run.
//!
//! Drives `pokeemerald_rs::App`'s frame loop entirely in-process, against
//! `platform`'s explicit null window/input backend (see
//! `platform::Platform::new_headless`, reached here through
//! [`pokeemerald_rs::App::new_headless`]), for a small, fixed number of
//! frames, and asserts a clean boot:
//!
//! - every [`pokeemerald_rs::App::step`] call succeeds and reports "keep
//!   going" -- proof the input pump stayed alive and nothing errored;
//! - the scene's composed frame, exactly as handed to
//!   `platform::Platform::present` each step, is non-blank -- proof the
//!   boot scene actually rendered something, not just that the loop ran.
//!
//! No real window, audio device, or timer wait is touched -- `Platform`'s
//! null backend no-ops `wait_for_next_frame` (see its docs) -- so this suite
//! is deterministic and fast enough to run under `cargo test` as well as CI.

use std::fmt;

use pokeemerald_rs::App;

/// The number of frames the smoke suite drives before declaring success.
///
/// Small and fixed: enough to prove the loop runs repeatedly (not just
/// once), far short of anything that would make the suite slow or flaky.
const SMOKE_FRAMES: u32 = 30;

/// Why `e2e --suite smoke` failed.
///
/// Concrete per-crate enum `(oop-boundaries)`; no `anyhow`.
#[derive(Debug)]
pub enum E2eError {
    /// The headless boot shell reported a "stop" (window-close-style)
    /// signal before completing [`SMOKE_FRAMES`] frames. `Platform`'s null
    /// backend never requests this on its own, so seeing it at all is a
    /// bug. Carries the 0-based frame index at which it happened.
    UnexpectedStop(u32),
    /// A step of the boot shell's frame loop (input pump or presentation)
    /// returned an error. Carries the error's rendered message --
    /// `platform::PlatformError` is not named directly here, so `xtask`'s
    /// only workspace dependency for this suite stays `pokeemerald-rs`
    /// itself.
    Step(String),
    /// Every frame ran and reported "keep going", but the composed frame
    /// handed to `Platform::present` was blank (every pixel black) -- the
    /// boot scene, or the presentation path, produced nothing.
    BlankFrame,
}

impl fmt::Display for E2eError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedStop(frame) => {
                write!(f, "boot shell reported an unexpected stop at frame {frame}")
            }
            Self::Step(msg) => write!(f, "boot shell step failed: {msg}"),
            Self::BlankFrame => write!(f, "composed boot scene frame was blank (all black)"),
        }
    }
}

impl std::error::Error for E2eError {}

/// Run the `smoke` suite: boot the headless shell and drive it for
/// [`SMOKE_FRAMES`] frames, asserting a clean boot (see the module docs).
///
/// # Errors
///
/// Returns [`E2eError`] if the shell reports an unexpected early stop, a
/// step errors, or the final composed frame is blank.
pub fn run_smoke() -> Result<(), E2eError> {
    let mut app = App::new_headless();

    for frame in 0..SMOKE_FRAMES {
        let keep_going = app.step().map_err(|err| E2eError::Step(err.to_string()))?;
        if !keep_going {
            return Err(E2eError::UnexpectedStop(frame));
        }
    }

    if app.frame().iter().all(|&pixel| pixel == 0) {
        return Err(E2eError::BlankFrame);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_smoke;

    #[test]
    fn smoke_suite_boots_cleanly_headless() {
        run_smoke().expect("headless smoke run should boot cleanly");
    }
}
