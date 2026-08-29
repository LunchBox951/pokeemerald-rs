//! Headless end-to-end smoke verification.
//!
//! [`run_smoke`] drives the production [`App::step`] path through its null
//! platform backend. When a default asset pack exists, the suite also verifies
//! the pack-backed title and overworld renderers. A missing pack skips those
//! two checks so the synthetic boot check remains available in clean CI.

use std::fmt;

use pokeemerald_rs::App;

const BOOT_FRAME_COUNT: u32 = 30;
const BLACK_PIXEL: u32 = 0;
const INITIAL_TITLE_FRAME: u32 = 0;
const TITLE_ANIMATION_PROBE_FRAME: u32 = 20;
const INITIAL_OVERWORLD_TICK: u32 = 0;
const SECOND_OVERWORLD_DETERMINISM_TICK: u32 = 17;
const SMOKE_PLAYER_TILE: (i32, i32) = (5, 5);
const SMOKE_PLAYER_GROUND_ELEVATION: u8 = 3;
const NATIVE_FRAME_WIDTH: usize = 240;
const AVATAR_LEFT: usize = 112;
const AVATAR_TOP: usize = 64;
const AVATAR_WIDTH: usize = 16;
const AVATAR_HEIGHT: usize = 32;
const MIN_DISTINCT_MAP_COLORS: usize = 4;

/// Why `e2e --suite smoke` failed.
#[derive(Debug)]
pub enum E2eError {
    /// The headless application stopped at the contained frame index.
    UnexpectedStop(u32),
    /// A headless application step failed with the contained message.
    Step(String),
    /// The headless boot application produced an all-black frame.
    BlankFrame,
    /// The title scene failed to load from an existing pack.
    TitleSceneFailed(String),
    /// Repeated title composition differed at the contained frame index.
    TitleFrameNotDeterministic(u32),
    /// The title scene produced an all-black frame at the contained index.
    TitleFrameBlank(u32),
    /// The title scene did not change between its two animation probes.
    TitleFramesNotAnimated,
    /// The default overworld scene failed to load from an existing pack.
    OverworldSceneFailed(String),
    /// Repeated overworld composition differed for the same state.
    OverworldFrameNotDeterministic,
    /// The overworld scene produced an all-black frame.
    OverworldFrameBlank,
    /// The overworld scene did not contain enough map detail outside the avatar.
    OverworldFrameLacksMapDetail,
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
                "title screen frame {INITIAL_TITLE_FRAME} and frame {TITLE_ANIMATION_PROBE_FRAME} were pixel-identical -- expected the animation to have moved on by then"
            ),
            Self::OverworldSceneFailed(msg) => {
                write!(f, "default overworld room failed to load: {msg}")
            }
            Self::OverworldFrameNotDeterministic => write!(
                f,
                "composing the default overworld room's frame twice produced different frames"
            ),
            Self::OverworldFrameBlank => write!(
                f,
                "composed default overworld room frame was blank (all black)"
            ),
            Self::OverworldFrameLacksMapDetail => write!(
                f,
                "composed default overworld room frame lacked map detail outside the avatar"
            ),
        }
    }
}

impl std::error::Error for E2eError {}

/// Run the bounded headless boot, title, and overworld smoke checks.
///
/// # Errors
///
/// Returns [`E2eError`] when a production path fails or a visual probe is
/// blank, static, or non-deterministic.
pub fn run_smoke() -> Result<(), E2eError> {
    let mut app = App::new_headless();

    for frame in 0..BOOT_FRAME_COUNT {
        let keep_going = app.step().map_err(|err| E2eError::Step(err.to_string()))?;
        if !keep_going {
            return Err(E2eError::UnexpectedStop(frame));
        }
    }

    if is_blank(app.frame()) {
        return Err(E2eError::BlankFrame);
    }

    check_title_screen()?;
    check_overworld_scene()
}

fn check_title_screen() -> Result<(), E2eError> {
    let scene = match pokeemerald_rs::title::load_default() {
        Ok(scene) => scene,
        Err(err) if err.is_pack_missing() => return Ok(()),
        Err(err) => return Err(E2eError::TitleSceneFailed(err.to_string())),
    };

    let initial_frame = scene.compose_frame(INITIAL_TITLE_FRAME);
    let repeated_initial_frame = scene.compose_frame(INITIAL_TITLE_FRAME);
    if initial_frame != repeated_initial_frame {
        return Err(E2eError::TitleFrameNotDeterministic(INITIAL_TITLE_FRAME));
    }
    if is_blank(initial_frame.as_ref()) {
        return Err(E2eError::TitleFrameBlank(INITIAL_TITLE_FRAME));
    }

    let animated_frame = scene.compose_frame(TITLE_ANIMATION_PROBE_FRAME);
    let repeated_animated_frame = scene.compose_frame(TITLE_ANIMATION_PROBE_FRAME);
    if animated_frame != repeated_animated_frame {
        return Err(E2eError::TitleFrameNotDeterministic(
            TITLE_ANIMATION_PROBE_FRAME,
        ));
    }
    if is_blank(animated_frame.as_ref()) {
        return Err(E2eError::TitleFrameBlank(TITLE_ANIMATION_PROBE_FRAME));
    }

    if initial_frame == animated_frame {
        return Err(E2eError::TitleFramesNotAnimated);
    }

    Ok(())
}

fn check_overworld_scene() -> Result<(), E2eError> {
    let all_event_flags_clear = pokeemerald_rs::overworld::EventData::default();

    let scene = match pokeemerald_rs::overworld::load_default_room(&all_event_flags_clear) {
        Ok(scene) => scene,
        Err(err) if err.is_pack_missing() => return Ok(()),
        Err(err) => return Err(E2eError::OverworldSceneFailed(err.to_string())),
    };

    let player = pokeemerald_rs::overworld::PlayerState::new(
        SMOKE_PLAYER_TILE,
        SMOKE_PLAYER_GROUND_ELEVATION,
        pokeemerald_rs::overworld::Direction::South,
    );
    let initial_frame =
        scene.compose_frame(&player, &all_event_flags_clear, INITIAL_OVERWORLD_TICK);
    let repeated_initial_frame =
        scene.compose_frame(&player, &all_event_flags_clear, INITIAL_OVERWORLD_TICK);
    if initial_frame != repeated_initial_frame {
        return Err(E2eError::OverworldFrameNotDeterministic);
    }
    let later_frame = scene.compose_frame(
        &player,
        &all_event_flags_clear,
        SECOND_OVERWORLD_DETERMINISM_TICK,
    );
    let repeated_later_frame = scene.compose_frame(
        &player,
        &all_event_flags_clear,
        SECOND_OVERWORLD_DETERMINISM_TICK,
    );
    if later_frame != repeated_later_frame {
        return Err(E2eError::OverworldFrameNotDeterministic);
    }
    if is_blank(initial_frame.as_ref()) {
        return Err(E2eError::OverworldFrameBlank);
    }
    if !has_map_detail_outside_avatar(initial_frame.as_ref()) {
        return Err(E2eError::OverworldFrameLacksMapDetail);
    }

    Ok(())
}

fn is_blank(frame: &[u32]) -> bool {
    frame.iter().all(|&pixel| pixel == BLACK_PIXEL)
}

fn has_map_detail_outside_avatar(frame: &[u32]) -> bool {
    let mut distinct_colors = std::collections::BTreeSet::new();
    for (pixel_index, &pixel) in frame.iter().enumerate() {
        let (x, y) = (
            pixel_index % NATIVE_FRAME_WIDTH,
            pixel_index / NATIVE_FRAME_WIDTH,
        );
        let inside_avatar = (AVATAR_LEFT..AVATAR_LEFT + AVATAR_WIDTH).contains(&x)
            && (AVATAR_TOP..AVATAR_TOP + AVATAR_HEIGHT).contains(&y);
        if !inside_avatar {
            distinct_colors.insert(pixel);
        }
    }
    distinct_colors.len() >= MIN_DISTINCT_MAP_COLORS
}

#[cfg(test)]
mod tests {
    use super::{check_overworld_scene, check_title_screen, run_smoke};

    #[test]
    fn smoke_suite_boots_cleanly_headless() {
        run_smoke().expect("headless smoke run should boot cleanly");
    }

    #[test]
    fn title_screen_check_is_a_no_op_or_succeeds() {
        check_title_screen().expect("title screen check should never fail in a clean checkout");
    }

    #[test]
    fn overworld_scene_check_is_a_no_op_or_succeeds() {
        check_overworld_scene()
            .expect("overworld scene check should never fail in a clean checkout");
    }
}
