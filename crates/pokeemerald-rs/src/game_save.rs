//! Boot-time save loading and in-session persistence.
//!
//! `engine::save` owns the flash-image format, validation, and slot rotation.
//! This module owns the session-level rules: boot classification, save
//! lineage, overwrite consent, and protection against concurrent stale writes.
//!
//! An unreadable image maps to upstream's `SAVE_STATUS_NO_FLASH`, not
//! `SAVE_STATUS_CORRUPT`. `TrySavingData` refuses to write after that status
//! (`pokeemerald/src/save.c:765-771, 871-879`), so [`SaveSlot`] disables saving
//! for the rest of the session instead of risking an unreadable file.

use engine::save::{
    BaseSnapshot, SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStatus, SaveStore,
};

/// The save status used to choose the boot menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFileStatus {
    Empty,
    Ok,
    Corrupt,
    Error,
    NoFlash,
}

impl SaveFileStatus {
    const fn from_store(status: SaveStatus) -> Self {
        match status {
            SaveStatus::Empty => Self::Empty,
            SaveStatus::Ok => Self::Ok,
            SaveStatus::Corrupt => Self::Corrupt,
            SaveStatus::Error => Self::Error,
        }
    }

    /// Whether the main menu offers `CONTINUE` for this status.
    pub(crate) const fn menu_shows_continue(self) -> bool {
        match self {
            Self::Ok | Self::Error => true,
            Self::Empty | Self::Corrupt | Self::NoFlash => false,
        }
    }
}

/// Blocks recovered during boot and the status that determines whether they
/// are usable.
#[derive(Debug)]
pub(crate) struct SavedGame {
    pub(crate) status: SaveFileStatus,
    pub(crate) block1: SaveBlock1,
    pub(crate) block2: SaveBlock2,
}

impl SavedGame {
    fn no_flash() -> Self {
        Self {
            status: SaveFileStatus::NoFlash,
            block1: SaveBlock1::default(),
            block2: SaveBlock2::default(),
        }
    }
}

/// The result of a write that can be refused without an I/O error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreOutcome {
    Written,
    /// A continuable save appeared before the player consented to replacing
    /// it. The file is unchanged.
    RefusedExistingSave,
    /// Another process persisted newer progress after this session loaded.
    /// The file is unchanged.
    RefusedStaleSession,
}

/// The source of unmodelled bytes retained when serializing a save.
///
/// The port retains unmodelled bytes in the disk image rather than live RAM.
/// New-game writes must therefore clear that base on every attempt, while a
/// continued session carries its loaded base forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveLineage {
    /// The session started a new adventure and retains no previous save data.
    NewGame,
    /// The session continued the loaded adventure and retains its deferred
    /// bytes.
    Continued,
}

impl SaveLineage {
    const fn clears_base(self) -> bool {
        match self {
            Self::NewGame => true,
            Self::Continued => false,
        }
    }
}

const SERIAL_COUNTER_HALF_RANGE: u32 = 1 << (u32::BITS - 1);

/// Whether `candidate` is unambiguously ahead of `baseline` in serial-number
/// arithmetic. Equal and exactly antipodal counters are not ordered.
const fn counter_is_ahead(baseline: u32, candidate: u32) -> bool {
    candidate != baseline && candidate.wrapping_sub(baseline) < SERIAL_COUNTER_HALF_RANGE
}

#[derive(Debug)]
pub(crate) struct SaveSlot {
    file: Option<SaveFile>,
    session_counter: Option<u32>,
    session_base: Option<BaseSnapshot>,
    session_status: Option<SaveFileStatus>,
}

impl SaveSlot {
    pub(crate) const fn disabled() -> Self {
        Self {
            file: None,
            session_counter: None,
            session_base: None,
            session_status: None,
        }
    }

    /// Opens the per-user save location, or disables saving when its path
    /// cannot be resolved.
    pub(crate) fn default_location() -> Self {
        match SaveFile::default_location() {
            Ok(file) => Self {
                file: Some(file),
                session_counter: None,
                session_base: None,
                session_status: None,
            },
            Err(err) => {
                eprintln!("save: {err} -- this session cannot load or save");
                Self::disabled()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn at(file: SaveFile) -> Self {
        Self {
            file: Some(file),
            session_counter: None,
            session_base: None,
            session_status: None,
        }
    }

    /// Returns the boot load's status, or [`SaveFileStatus::Empty`] before the
    /// first load.
    pub(crate) fn boot_status(&self) -> SaveFileStatus {
        self.session_status.unwrap_or(SaveFileStatus::Empty)
    }

    /// Loads the current save for boot.
    ///
    /// Read failures are logged and returned as [`SaveFileStatus::NoFlash`]
    /// because boot has no error-reporting path and must not overwrite a file
    /// it could not read.
    pub(crate) fn load(&mut self) -> SavedGame {
        let Some(file) = &self.file else {
            self.session_status = Some(SaveFileStatus::NoFlash);
            return SavedGame::no_flash();
        };
        let mut store = match file.read() {
            Ok(Some(store)) => store,
            Ok(None) => SaveStore::new(),
            Err(err) => {
                eprintln!("save: {err} -- starting without a saved game; saving is disabled for this session");
                self.file = None;
                self.session_status = Some(SaveFileStatus::NoFlash);
                return SavedGame::no_flash();
            }
        };
        let outcome = store.load();
        self.session_counter = Some(store.save_counter());
        self.session_base = Some(store.base_snapshot());
        let status = SaveFileStatus::from_store(outcome.status);
        self.session_status = Some(status);
        SavedGame {
            status,
            block1: outcome.block1,
            block2: outcome.block2,
        }
    }

    /// Persists the current blocks using this session's [`SaveLineage`].
    ///
    /// The write is refused if the disk contains progress newer than this
    /// session loaded.
    ///
    /// # Errors
    ///
    /// Returns [`SaveFileError::NoDataDirectory`] when saving is disabled, or
    /// the underlying lock, read, or write error.
    pub(crate) fn store(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
        lineage: SaveLineage,
    ) -> Result<StoreOutcome, SaveFileError> {
        self.store_impl(block1, block2, false, lineage)
    }

    /// Persists like [`SaveSlot::store`], but refuses to replace a continuable
    /// save before the player has consented to overwriting it.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`SaveSlot::store`].
    pub(crate) fn store_unless_foreign_save(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
        lineage: SaveLineage,
    ) -> Result<StoreOutcome, SaveFileError> {
        self.store_impl(block1, block2, true, lineage)
    }

    fn store_impl(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
        refuse_foreign: bool,
        lineage: SaveLineage,
    ) -> Result<StoreOutcome, SaveFileError> {
        let clear_base = lineage.clears_base();
        let file = self.file.as_ref().ok_or(SaveFileError::NoDataDirectory)?;
        let _read_modify_write_lock = file.lock()?;
        let mut store = file.read()?.unwrap_or_else(SaveStore::new);
        let disk_status = SaveFileStatus::from_store(store.load().status);
        let disk_counter = store.save_counter();
        if let Some(session_counter) = self.session_counter {
            if counter_is_ahead(session_counter, disk_counter) && disk_status.menu_shows_continue()
            {
                return Ok(StoreOutcome::RefusedStaleSession);
            }
        }
        if !clear_base {
            if let (Some(session_counter), Some(session_base)) =
                (self.session_counter, &self.session_base)
            {
                if counter_is_ahead(disk_counter, session_counter) {
                    store.restore_base(session_base.clone());
                }
            }
        }
        if refuse_foreign && disk_status.menu_shows_continue() {
            return Ok(StoreOutcome::RefusedExistingSave);
        }
        if clear_base {
            store.clear_base();
        }
        store.save(block1, block2);
        file.write(&store)?;
        self.session_counter = Some(store.save_counter());
        self.session_base = Some(store.base_snapshot());
        Ok(StoreOutcome::Written)
    }
}

#[cfg(test)]
mod tests;
