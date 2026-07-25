//! `e2e --suite smoke` (F-3, V-1; extended for I-2, issue #109): a headless
//! boot-shell smoke run.
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
//! `App::new_headless` always composes the I-1 synthetic scene (never the
//! real title screen, see `pokeemerald_rs::app`'s module docs), so the above
//! is unchanged from before I-2 -- exactly the "WITHOUT a pack (CI), the
//! current synthetic-scene smoke behavior stays exactly as is" requirement
//! (issue #109). [`check_title_screen`] is the I-2 addition: when a local
//! asset pack *is* present (`cargo xtask extract` has been run), it
//! additionally loads the real title screen
//! (`pokeemerald_rs::title::load_default`) and asserts, at two fixed frame
//! indices (0 and 37, issue #116's OBJ sprites / alpha blend / cloud scroll
//! slice), that: composing the same frame index twice is pixel-identical
//! (deterministic), each composed frame is non-blank, and frame 0 differs
//! from frame 37 (the logo shine sweep and "Press Start" blink cadence have
//! both visibly moved on by then -- see `pokeemerald_rs::title`'s module
//! docs) -- entirely separately from `App`, so it neither depends on nor
//! perturbs the headless `App` run above. Without a local pack, this step is
//! a no-op.
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
    /// A local asset pack is present, but loading/decoding the real title
    /// screen from it failed for a reason other than "no pack" (that case
    /// is not an error, see [`check_title_screen`]) -- carries
    /// `pokeemerald_rs::title::TitleSceneError`'s rendered message.
    TitleSceneFailed(String),
    /// A local asset pack is present and the real title screen loaded, but
    /// composing the same frame index twice produced two different frames --
    /// `compose` must be a pure function of its frame index and
    /// already-decoded data (I-2, issues #109 and #116). Carries the
    /// offending frame index.
    TitleFrameNotDeterministic(u32),
    /// A local asset pack is present and the real title screen composed
    /// deterministically, but a frame was blank (every pixel black) -- the
    /// decoded BG layers/sprites, or the compositor, produced nothing.
    /// Carries the offending frame index.
    TitleFrameBlank(u32),
    /// A local asset pack is present and both checked frames composed
    /// deterministically and non-blank, but frame 0 and frame 37 were
    /// pixel-identical -- the animated OBJ sprites (logo shine sweep,
    /// "Press Start" blink) should have visibly moved on by frame 37 (I-2,
    /// issue #116).
    TitleFramesNotAnimated,
}

impl fmt::Display for E2eError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedStop(frame) => {
                write!(f, "boot shell reported an unexpected stop at frame {frame}")
            }
            Self::Step(msg) => write!(f, "boot shell step failed: {msg}"),
            Self::BlankFrame => write!(f, "composed boot scene frame was blank (all black)"),
            Self::TitleSceneFailed(msg) => write!(f, "title screen failed to load: {msg}"),
            Self::TitleFrameNotDeterministic(frame) => {
                write!(
                    f,
                    "composing title screen frame {frame} twice produced different frames"
                )
            }
            Self::TitleFrameBlank(frame) => {
                write!(
                    f,
                    "composed title screen frame {frame} was blank (all black)"
                )
            }
            Self::TitleFramesNotAnimated => write!(
                f,
                "title screen frame 0 and frame 37 were pixel-identical -- expected the OBJ sprites to have animated by then"
            ),
        }
    }
}

impl std::error::Error for E2eError {}

/// Run the `smoke` suite: boot the headless shell and drive it for
/// [`SMOKE_FRAMES`] frames, asserting a clean boot (see the module docs),
/// then [`check_title_screen`] (a no-op without a local pack).
///
/// # Errors
///
/// Returns [`E2eError`] if the shell reports an unexpected early stop, a
/// step errors, the final composed frame is blank, or (with a local pack
/// present) the real title screen fails to load, composes
/// non-deterministically, or composes blank.
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

    check_title_screen()
}

/// The I-2 smoke addition (issue #109, strengthened for issue #116): with a
/// local asset pack present, load the real title screen and, at frame
/// indices 0 and 37, assert the composed frame is non-blank and
/// deterministic across two `compose_frame` calls at that same index, then
/// assert the two frames differ from each other (module docs); without a
/// pack, do nothing.
///
/// Deliberately independent of `App`/`App::new_headless` above -- it loads
/// `pokeemerald_rs::title::load_default` directly, so this check can never
/// perturb (or depend on) the synthetic-scene headless run.
///
/// # Errors
///
/// [`E2eError::TitleSceneFailed`] if a pack is present but fails to load or
/// decode for any reason other than "no pack" (that case returns `Ok(())`,
/// not an error -- see [`pokeemerald_rs::title::TitleSceneError::is_pack_missing`]);
/// [`E2eError::TitleFrameNotDeterministic`] or [`E2eError::TitleFrameBlank`]
/// if a pack is present and loads, but composing frame 0 or frame 37 fails
/// either check; [`E2eError::TitleFramesNotAnimated`] if both frames pass
/// but are pixel-identical to each other.
fn check_title_screen() -> Result<(), E2eError> {
    let scene = match pokeemerald_rs::title::load_default() {
        Ok(scene) => scene,
        Err(err) if err.is_pack_missing() => return Ok(()),
        Err(err) => return Err(E2eError::TitleSceneFailed(err.to_string())),
    };

    // Composed frames are `Box<platform::Frame>` -- deliberately never named
    // here (module docs: this suite's only workspace dependency stays
    // `pokeemerald-rs`, not `platform`), so each frame index's determinism
    // check is inlined rather than factored into a helper with an explicit
    // return type.
    let frame0_a = scene.compose_frame(0);
    let frame0_b = scene.compose_frame(0);
    if frame0_a != frame0_b {
        return Err(E2eError::TitleFrameNotDeterministic(0));
    }
    if frame0_a.iter().all(|&pixel| pixel == 0) {
        return Err(E2eError::TitleFrameBlank(0));
    }

    let frame37_a = scene.compose_frame(37);
    let frame37_b = scene.compose_frame(37);
    if frame37_a != frame37_b {
        return Err(E2eError::TitleFrameNotDeterministic(37));
    }
    if frame37_a.iter().all(|&pixel| pixel == 0) {
        return Err(E2eError::TitleFrameBlank(37));
    }

    if frame0_a == frame37_a {
        return Err(E2eError::TitleFramesNotAnimated);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_title_screen, run_smoke};

    #[test]
    fn smoke_suite_boots_cleanly_headless() {
        run_smoke().expect("headless smoke run should boot cleanly");
    }

    #[test]
    fn title_screen_check_is_a_no_op_or_succeeds() {
        // Whether or not this environment has run `cargo xtask extract`,
        // the check must never fail: no pack -> `Ok(())` (module docs); a
        // real local pack -> genuinely valid, non-blank, deterministic,
        // animated frames at indices 0 and 37 (see `pokeemerald_rs::title::tests`'
        // `real_pack_composes_non_blank_deterministic_title_frames` for the
        // dedicated, always-runnable-with-`--ignored` version of this same
        // assertion).
        check_title_screen().expect("title screen check should never fail in a clean checkout");
    }
}
