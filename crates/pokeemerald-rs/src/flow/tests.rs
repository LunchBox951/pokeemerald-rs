//! Shared fixtures for the `flow` test modules: button states, the scratch
//! save file and the scratch-file symlink-safety helpers, the entryless
//! scratch pack, and the fade-wait drivers.
//!
//! The scene-transition tests themselves live in per-concern siblings
//! (`overworld_load_failure_tests`, `title_advance_tests`,
//! `main_menu_confirm_tests`, `main_menu_decision_tests`,
//! `main_menu_input_tests`, `save_isolation_tests`, `intro_handoff_tests`,
//! `title_clock_tests`). The I-6 save/exit/reload round trip lives in
//! [`super::save_continue_round_trip_tests`] -- see that module's own docs.

use super::{advance_scene, AppScene, MainMenuState};
use crate::game_save::SaveSlot;
use crate::main_menu::MainMenuType;
use platform::{ButtonState, Buttons, Frame};

/// Generously above the 22-call trace [`rendering::NormalPaletteFade`]'s own
/// tests pin (`crates/rendering/src/palette_fade.rs`), so a regression that
/// never drives the fade to [`rendering::PaletteFadeStatus::Done`] fails
/// [`drive_through_fade_wait`] loudly instead of hanging the test.
const MAX_FADE_WAIT_FRAMES: usize = 40;

/// Drives a fade-wait `scene` with no input until it leaves, returning the
/// destination, the frames presented while waiting, and the destination's
/// first frame.
///
/// # Panics
///
/// If `scene` is not already waiting, or the wait does not reach
/// [`rendering::PaletteFadeStatus::Done`] within [`MAX_FADE_WAIT_FRAMES`].
pub(super) fn drive_through_fade_wait(
    mut scene: AppScene,
    save_slot: &mut SaveSlot,
    pack_source: &crate::pack_source::PackSource,
) -> (AppScene, Vec<Box<Frame>>, Box<Frame>) {
    let mut wait_frames = Vec::new();
    for _ in 0..MAX_FADE_WAIT_FRAMES {
        assert!(
            matches!(
                scene,
                AppScene::TitleFadeWait(_) | AppScene::MainMenuFadeWait(_)
            ),
            "drive_through_fade_wait called on a scene that was not already waiting"
        );
        let (next, frame) =
            advance_scene(scene, ButtonState::new(), save_slot, pack_source.clone());
        if matches!(
            next,
            AppScene::TitleFadeWait(_) | AppScene::MainMenuFadeWait(_)
        ) {
            wait_frames.push(frame);
            scene = next;
        } else {
            return (next, wait_frames, frame);
        }
    }
    panic!("fade wait did not reach `Done` within {MAX_FADE_WAIT_FRAMES} frames");
}

/// Steps the fresh boot's menu setup after the title fade's done frame
/// (`scene`, the [`AppScene::MainMenuFadeIn`] handed off on it) with no input
/// until the menu is ready to read input: the white fade-in, then the save
/// check (F), the battery check, the windows, and the highlight (F+3), the last
/// frame before the first input poll (`InitMainMenu(FALSE)`,
/// `main_menu.c:583-608`). Returns the menu and that frame.
/// `boot_setup_frames_tests` pins the frame counts.
///
/// # Panics
///
/// If `scene` is not the setup handoff or the menu is not reached within
/// [`MAX_FADE_WAIT_FRAMES`].
pub(super) fn drive_through_menu_setup(
    mut scene: AppScene,
    save_slot: &mut SaveSlot,
    pack_source: &crate::pack_source::PackSource,
) -> (AppScene, Box<Frame>) {
    assert!(
        matches!(scene, AppScene::MainMenuFadeIn(_)),
        "the title fade must hand off to the menu's fade-in and setup frames"
    );
    for _ in 0..MAX_FADE_WAIT_FRAMES {
        let (next, frame) =
            advance_scene(scene, ButtonState::new(), save_slot, pack_source.clone());
        if matches!(next, AppScene::MainMenu(_)) {
            return (next, frame);
        }
        scene = next;
    }
    panic!("the menu did not take input within {MAX_FADE_WAIT_FRAMES} frames");
}

pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// A scratch save file for one test, removed on drop (including on
/// unwind). Every test gets its own path: `SaveSlot` is passed by
/// reference precisely so no test has to touch the process-wide
/// environment that decides the real one.
pub(super) struct TempSave {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl TempSave {
    /// A save in a directory of its own.
    ///
    /// [`engine::save::SaveFile::lock`] takes one lock per save *directory*,
    /// so scratch saves sharing one directory would serialise on a single
    /// lock -- and a test that removed it would strip the exclusion the
    /// others were relying on.
    pub(super) fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-flow-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("scratch directory must be creatable");
        let path = dir.join("pokeemerald.sav");
        Self { dir, path }
    }

    pub(super) fn slot(&self) -> SaveSlot {
        SaveSlot::at(engine::save::SaveFile::at(self.path.clone()))
    }

    /// The scratch path itself, for the one test that has to damage the
    /// file behind the slot's back.
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempSave {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

/// A save slot pointing at a scratch path with no file at it -- the
/// "never saved" medium every scene-transition test below wants, so none
/// of them can be perturbed by (or perturb) a real local save.
pub(super) fn empty_slot(label: &str) -> (TempSave, SaveSlot) {
    let temp = TempSave::new(label);
    let slot = temp.slot();
    (temp, slot)
}

pub(super) fn held(button: Buttons) -> ButtonState {
    // Two updates: the first makes it newly-pressed, the second makes
    // it merely held (matching a real multi-frame hold).
    let mut state = ButtonState::new();
    state.update(button);
    state.update(button);
    state
}

/// Writes `bytes` to a fresh file at `path`. `create_new` (`O_EXCL`) never
/// follows a symlink at `path`, so a planted link cannot redirect the write
/// onto its target. A stale occupant is unlinked (the entry itself, never
/// what it points to) and the claim retried once.
pub(super) fn write_scratch_exclusive(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    use std::io::Write as _;
    let mut file = None;
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                std::fs::remove_file(path).expect("a stale scratch entry is removable");
            }
            Err(e) => panic!("the scratch directory is writable: {e}"),
        }
    }
    file.expect("the scratch path is exclusively creatable")
        .write_all(bytes.as_ref())
        .expect("the scratch pack is writable");
}

/// Plants a symlink at `path` pointing at a fresh victim file holding a
/// sentinel, runs `write`, and asserts the victim is untouched and the
/// writer replaced the link with a regular file holding `expected`. `write`
/// returns whatever guard keeps its file alive (a fixture whose `Drop`
/// removes it, or the path itself); the guard is dropped only after the
/// replacement is checked, so a writer that silently skips the write fails.
/// The victim is claimed through [`write_scratch_exclusive`], so a link
/// planted at its own predictable name is never followed either. Removes
/// the planted entry and the victim afterwards.
#[cfg(unix)]
pub(super) fn assert_planted_symlink_untouched<G>(
    path: &std::path::Path,
    expected: &[u8],
    write: impl FnOnce() -> G,
) {
    let victim = std::env::temp_dir().join(format!(
        "pokeemerald-rs-symlink-victim-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        path.file_name().unwrap().to_string_lossy()
    ));
    let sentinel = b"victim must survive";
    write_scratch_exclusive(&victim, sentinel);
    let _ = std::fs::remove_file(path);
    std::os::unix::fs::symlink(&victim, path).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(write));
    let replaced = std::fs::symlink_metadata(path).map(|m| m.file_type().is_file());
    let written = std::fs::read(path);
    let completed = outcome.is_ok();
    drop(outcome);
    let after = std::fs::read(&victim).unwrap();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(&victim);
    assert_eq!(
        after, sentinel,
        "a planted symlink's target was overwritten"
    );
    assert!(
        completed,
        "the writer must replace a planted symlink, not panic on it"
    );
    assert!(
        matches!(replaced, Ok(true)),
        "the writer must leave a regular file where the symlink was, got {replaced:?}"
    );
    assert_eq!(
        written.ok().as_deref(),
        Some(expected),
        "the writer must write its bytes to the replaced path"
    );
}

/// A scratch, valid but entryless asset pack -- see
/// `app::tests::subprocess_harness::write_empty_scratch_pack` for the rationale; duplicated
/// because the two boundary tests live in sibling modules with no shared
/// test-only module to hold it.
pub(super) fn scratch_pack_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-flow-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

pub(super) fn write_empty_scratch_pack(label: &str) -> std::path::PathBuf {
    let path = scratch_pack_path(label);
    let bytes = pack_format::PackWriter::new()
        .finish()
        .expect("an entryless pack always serializes");
    write_scratch_exclusive(&path, bytes);
    path
}

pub(super) fn pack_free_menu(menu_type: MainMenuType, save_slot: &mut SaveSlot) -> AppScene {
    AppScene::MainMenu(Box::new(MainMenuState {
        scene: crate::main_menu::synthetic_scene(menu_type),
        saved: save_slot.load(),
    }))
}
