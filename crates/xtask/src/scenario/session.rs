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
use std::sync::atomic::{AtomicU64, Ordering};

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
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "pokeemerald-scenario-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                // A directory left by an earlier process with a recycled id.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
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
