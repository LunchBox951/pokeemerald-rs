//! The media one scenario run owns: a private copy of the source pack and a
//! durable save file in a scratch directory of its own, so a scenario never
//! reads the checkout's pack in place, an installed pack, or a player's
//! save, and a restart reopens the very same pack and save (`main_menu.c:1067`
//! continues from the medium; `overworld.c:1705-1737`).
//!
//! Selection is explicit and fail-closed: a pack that cannot be prepared or a
//! save that cannot be opened is a [`ScenarioError::Setup`], never a run
//! against some other source. Nothing here touches the process environment.

use std::path::{Path, PathBuf};

use pokeemerald_rs::App;

use super::ScenarioError;

const PACK_FILE_NAME: &str = "pokeemerald.pack";
const SAVE_FILE_NAME: &str = "pokeemerald.sav";

/// Where a scenario's pack comes from. Either way the run reads a private
/// copy, never the source in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PackSelection {
    /// Copy an already-built pack file (the checkout-extracted pack for
    /// [`Self::checkout`]).
    Prepared(PathBuf),
    /// Import the supported ROM at this path into a fresh pack.
    ImportedRom(PathBuf),
}

impl PackSelection {
    /// The checkout's own extracted pack, `cargo xtask extract`'s output.
    pub(super) fn checkout() -> Self {
        Self::Prepared(assets::AssetPack::repo_pack_path())
    }

    fn prepare(&self, destination: &Path) -> Result<(), String> {
        match self {
            Self::Prepared(source) => std::fs::copy(source, destination)
                .map(drop)
                .map_err(|error| format!("source pack {}: {error}", source.display())),
            Self::ImportedRom(rom) => rom_import::import(rom, destination)
                .map(drop)
                .map_err(|error| format!("ROM import of {}: {error}", rom.display())),
        }
    }
}

/// A directory created exclusively for one session and removed with it.
#[derive(Debug)]
struct Scratch(PathBuf);

impl Scratch {
    fn create() -> std::io::Result<Self> {
        // The exclusive `create_dir` is the only arbiter of uniqueness: the
        // first caller to claim a name owns it, so a counter needs no state
        // beyond this loop.
        let mut n: u64 = 0;
        loop {
            let path = std::env::temp_dir()
                .join(format!("pokeemerald-scenario-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                // Taken by another session in this process, or left by an
                // earlier process with a recycled id.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One scenario's owned pack and save media.
#[derive(Debug)]
pub(super) struct Session {
    pack: PathBuf,
    save: PathBuf,
    _scratch: Scratch,
}

impl Session {
    /// Prepare the media for `selection`.
    ///
    /// # Errors
    ///
    /// [`ScenarioError::Setup`] if the scratch directory cannot be created
    /// or the pack cannot be prepared.
    pub(super) fn new(selection: &PackSelection) -> Result<Self, ScenarioError> {
        let scratch = Scratch::create()
            .map_err(|error| ScenarioError::Setup(format!("scratch directory: {error}")))?;
        let pack = scratch.0.join(PACK_FILE_NAME);
        selection.prepare(&pack).map_err(ScenarioError::Setup)?;
        let save = scratch.0.join("save").join(SAVE_FILE_NAME);
        Ok(Self {
            pack,
            save,
            _scratch: scratch,
        })
    }

    /// Construct the production [`App`] on this session's pack and save.
    /// Dropping it and calling this again restarts onto the same save.
    ///
    /// # Errors
    ///
    /// [`ScenarioError::Start`] if the pack does not load or the save
    /// medium cannot be opened.
    pub(super) fn start(&self) -> Result<App, ScenarioError> {
        App::new_headless_real_at(&self.pack, &self.save)
            .map_err(|error| ScenarioError::Start(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pokeemerald-no-such-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_missing_source_pack_is_a_setup_error() {
        let error = Session::new(&PackSelection::Prepared(missing("pack")))
            .expect_err("no source pack must never prepare a session");
        assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
    }

    #[test]
    fn a_missing_rom_is_a_setup_error() {
        let error = Session::new(&PackSelection::ImportedRom(missing("rom")))
            .expect_err("no ROM must never prepare a session");
        assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
    }

    #[test]
    fn a_prepared_pack_is_copied_into_an_owned_scratch_directory() {
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-source-{}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let first = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        let second = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert_ne!(first.pack, source);
        assert_ne!(first.pack, second.pack);
        assert_ne!(first.save, second.save);
        assert_eq!(std::fs::read(&first.pack).unwrap(), b"not a real pack");
        let scratch = first.pack.parent().unwrap().to_path_buf();
        drop(first);
        assert!(
            !scratch.exists(),
            "the scratch directory dies with its session"
        );
    }

    #[test]
    fn a_session_that_cannot_load_its_pack_reports_start_failure_not_success() {
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-garbage-{}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let session = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        std::fs::remove_file(&source).unwrap();
        let Err(error) = session.start() else {
            panic!("a garbage pack must not boot");
        };
        assert!(matches!(error, ScenarioError::Start(_)), "got {error:?}");
        assert!(
            !session.save.exists(),
            "a failed boot must not leave a save behind"
        );
    }
}

#[cfg(test)]
mod restart_tests {
    use pokeemerald_rs::main_menu::MainMenuItem;
    use pokeemerald_rs::{AppButtons, AppState};

    use super::*;

    const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

    fn press(app: &mut App, buttons: AppButtons) {
        app.set_headless_buttons(buttons).unwrap();
        assert!(app.step().unwrap());
        app.set_headless_buttons(AppButtons::NONE).unwrap();
        assert!(app.step().unwrap());
    }

    #[test]
    #[ignore = "needs a local pack produced by `cargo xtask extract`"]
    fn real_pack_restart_continues_the_game_the_first_boot_saved() {
        let _pack = crate::extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = Session::new(&PackSelection::checkout()).expect("owned session");

        let mut first = session.start().expect("first boot");
        // Drive the production new-game path to the first overworld frame.
        for frame in super::super::boot_to_first_fight::frames() {
            first.set_headless_buttons(frame.buttons).unwrap();
            assert!(first.step().unwrap());
            if first.state() == AppState::Overworld {
                break;
            }
        }
        assert_eq!(first.state(), AppState::Overworld);
        for _ in 0..60 {
            first.set_headless_buttons(AppButtons::NONE).unwrap();
            assert!(first.step().unwrap());
        }
        // START -> SAVE (BAG, player, SAVE) -> YES, then confirm messages.
        press(&mut first, AppButtons::START);
        press(&mut first, AppButtons::DOWN);
        press(&mut first, AppButtons::DOWN);
        let mut frames = 0;
        while !session.save.exists() {
            assert!(frames < SAVE_FLOW_FRAME_BUDGET, "save never written");
            press(&mut first, AppButtons::A);
            frames += 2;
        }
        drop(first);

        let mut restarted = session.start().expect("restart onto the same save");
        assert_eq!(restarted.state(), AppState::Title);
        restarted.set_headless_buttons(AppButtons::START).unwrap();
        assert!(restarted.step().unwrap());
        for _ in 0..=super::super::FADE_WAIT_FRAMES {
            restarted.set_headless_buttons(AppButtons::NONE).unwrap();
            assert!(restarted.step().unwrap());
        }
        assert_eq!(
            restarted.state(),
            AppState::MainMenu(MainMenuItem::Continue),
            "the restarted session must offer CONTINUE from the first session's save"
        );
    }
}
