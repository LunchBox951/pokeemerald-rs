//! Boot-time save loading and in-session persistence.
//!
//! `engine::save` owns the flash-image format; this module owns the session rules:
//! boot classification, save lineage, overwrite consent, and stale-writer refusal.
//!
//! An unreadable image latches upstream's `SAVE_STATUS_NO_FLASH`, so [`SaveSlot`]
//! disables saving for the session like `TrySavingData` (`save.c:765-771, 871-879`).

use engine::save::{
    BaseSnapshot, SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveFileGuard, SaveStatus,
    SaveStore, StorageSource,
};

/// The save status used to choose the boot menu — upstream `gSaveFileStatus`'s
/// `SAVE_STATUS_*` values (`pokeemerald/include/save.h:34-38`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFileStatus {
    /// `SAVE_STATUS_EMPTY` — nothing has ever been saved.
    Empty,
    /// `SAVE_STATUS_OK` — an intact save was loaded.
    Ok,
    /// `SAVE_STATUS_CORRUPT` — no intact slot survives.
    Corrupt,
    /// `SAVE_STATUS_ERROR` — one intact slot was loaded; the other holds
    /// damage.
    Error,
    /// `SAVE_STATUS_NO_FLASH` — the save medium itself is unusable.
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

    /// Whether the main menu offers `CONTINUE` for this status. `Error` joins
    /// `Ok`: one slot loaded intact (`pokeemerald/src/main_menu.c:654-660`).
    pub(crate) const fn menu_shows_continue(self) -> bool {
        match self {
            Self::Ok | Self::Error => true,
            Self::Empty | Self::Corrupt | Self::NoFlash => false,
        }
    }

    /// Whether boot re-clears `SaveBlock2` and re-defaults its options after
    /// this verdict: `CB2_InitCopyrightScreenAfterBootup` calls
    /// `Sav2_ClearSetDefault` for `SAVE_STATUS_EMPTY` and
    /// `SAVE_STATUS_CORRUPT` only (`pokeemerald/src/intro.c:1154-1156`), so
    /// `Error`'s one intact slot keeps its recovered options.
    pub(crate) const fn boot_clears_save_block2(self) -> bool {
        match self {
            Self::Empty | Self::Corrupt => true,
            Self::Ok | Self::Error | Self::NoFlash => false,
        }
    }
}

/// Blocks recovered during boot and the status that determines whether they
/// are usable.
///
/// The blocks are always populated, like `gSaveBlock1Ptr`/`gSaveBlock2Ptr`; callers
/// gate on [`SaveFileStatus::menu_shows_continue`], never on the blocks' plausibility.
#[derive(Debug)]
pub(crate) struct SavedGame {
    /// `gSaveFileStatus`.
    pub(crate) status: SaveFileStatus,
    /// The recovered `SaveBlock1`.
    pub(crate) block1: SaveBlock1,
    /// The recovered `SaveBlock2`.
    pub(crate) block2: SaveBlock2,
}

impl SavedGame {
    /// Fresh default blocks under `status` -- there is no image to recover
    /// them from, whether because the medium was never consulted
    /// ([`SaveSlot::none`]'s `Empty`) or is unusable
    /// ([`SaveSlot::disabled`]/a read failure's `NoFlash`).
    fn defaulted(status: SaveFileStatus) -> Self {
        Self {
            status,
            block1: SaveBlock1::default(),
            block2: SaveBlock2::default(),
        }
    }
}

/// The result of a write that can be refused without an I/O error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreOutcome {
    /// The blocks were rotated in and persisted.
    Written,
    /// A continuable save appeared before the player consented to replacing
    /// it. The file is unchanged.
    RefusedExistingSave,
    /// Another process persisted newer progress after this session loaded.
    /// The file is unchanged.
    RefusedStaleSession,
    /// The disk holds this session's generation number over a different
    /// base that donor loss does not explain, so it may be another save
    /// swapped in after load. The file is unchanged.
    RefusedConflictingSave,
}

/// The source of unmodelled bytes retained when serializing a save.
///
/// Unmodelled bytes live in the disk image, so a new-game write clears that base
/// on every attempt while a continued session carries its loaded base forward.
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

/// What a continued session does with a reload's base before writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseReconcile {
    /// Write over the reloaded base unchanged.
    Keep,
    /// Put the reload back on the session's cached base.
    Restore,
    /// Refuse the write: the base differs for a reason not proven safe.
    Refuse,
}

/// Reconciles a reload at `disk` with a continued session at `session`.
///
/// The disk falling back past the session's generation restores, as before.
/// Equal counters over an identical base -- every ordinary save -- keep it.
/// Equal counters over a different base restore only on `donor_lost`
/// ([`donor_was_lost`]); anything else is refused, because equal counters
/// prove neither common lineage nor equal contents, and a restore would put
/// this session's storage into both slots within two saves.
const fn reconcile_base(
    disk: u32,
    session: u32,
    same_base: bool,
    donor_lost: bool,
) -> BaseReconcile {
    if counter_is_ahead(disk, session) {
        BaseReconcile::Restore
    } else if disk != session || same_base {
        BaseReconcile::Keep
    } else if donor_lost {
        BaseReconcile::Restore
    } else {
        BaseReconcile::Refuse
    }
}

/// Whether an equal-counter base mismatch is the loss of the storage donor
/// this session merged at boot, rather than a different save.
///
/// All three must hold: the session's load merged a legacy donor's storage
/// (`session_merged_donor`); the reload still adopts a five-sector legacy head
/// (`disk_source`), whose storage is never its own; and that head's block
/// payloads are byte-identical to the session's (`same_blocks`). Then only
/// donor-supplied storage differs, and the session's copy is the one it loaded.
/// Store-level provenance of which generation donated is not needed: the
/// head's identity and its legacy shape are the evidence.
const fn donor_was_lost(
    session_merged_donor: bool,
    disk_source: StorageSource,
    same_blocks: bool,
) -> bool {
    session_merged_donor && disk_source.is_legacy_head() && same_blocks
}

/// This session's save medium: the one file boot loads from and writes back to.
#[derive(Debug)]
pub(crate) struct SaveSlot {
    file: Option<SaveFile>,
    session_counter: Option<u32>,
    session_base: Option<BaseSnapshot>,
    /// Whether `session_base`'s storage was merged from a legacy donor at
    /// boot and no write has since replaced it ([`donor_was_lost`]).
    session_merged_donor: bool,
    session_status: Option<SaveFileStatus>,
    /// The status [`Self::load`] reports (and defaults blocks under) when
    /// `file` is `None` -- distinguishes a medium that is unusable
    /// ([`Self::disabled`]'s `NoFlash`, upstream's missing-flash-chip
    /// verdict, `save.c:871-879`) from one that simply never consults a save
    /// at all ([`Self::none`]'s `Empty`, upstream's nothing-ever-saved
    /// verdict). A read failure always latches `NoFlash` regardless of this
    /// field -- see [`Self::load`].
    absent_status: SaveFileStatus,
}

impl SaveSlot {
    /// A deliberately unavailable medium: loads as `NoFlash`, never writes --
    /// upstream's own missing-flash-chip verdict. Use [`Self::none`] instead
    /// for a medium that is absent by design rather than by failure, e.g. a
    /// scripted scenario's boot.
    pub(crate) const fn disabled() -> Self {
        Self {
            file: None,
            session_counter: None,
            session_base: None,
            session_merged_donor: false,
            session_status: None,
            absent_status: SaveFileStatus::NoFlash,
        }
    }

    /// A medium deliberately never read or written: loads as `Empty`,
    /// matching a boot where nothing has ever been saved.
    ///
    /// [`App::new_headless_real`](crate::app::App::new_headless_real)'s
    /// contract is "no-save menu, never touches a player's save" -- a fresh
    /// boot, not upstream's missing-flash-chip verdict
    /// ([`Self::disabled`]). Reporting `Empty` rather than `NoFlash` matters
    /// downstream: `Empty` re-defaults `SaveBlock2` like a real never-saved
    /// boot does (`crate::flow::new_game_options_for`,
    /// [`SaveFileStatus::boot_clears_save_block2`]), where `NoFlash` would
    /// instead carry its zero-filled placeholder options into NEW GAME.
    pub(crate) const fn none() -> Self {
        Self {
            file: None,
            session_counter: None,
            session_base: None,
            session_merged_donor: false,
            session_status: None,
            absent_status: SaveFileStatus::Empty,
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
                session_merged_donor: false,
                session_status: None,
                absent_status: SaveFileStatus::NoFlash,
            },
            Err(err) => {
                eprintln!("save: {err} -- this session cannot load or save");
                Self::disabled()
            }
        }
    }

    /// A slot at an explicit `file`, so each test thread gets a scratch save.
    #[cfg(test)]
    pub(crate) fn at(file: SaveFile) -> Self {
        Self {
            file: Some(file),
            session_counter: None,
            session_base: None,
            session_merged_donor: false,
            session_status: None,
            absent_status: SaveFileStatus::NoFlash,
        }
    }

    /// Returns the boot load's status, or [`SaveFileStatus::Empty`] before the
    /// first load.
    pub(crate) fn boot_status(&self) -> SaveFileStatus {
        self.session_status.unwrap_or(SaveFileStatus::Empty)
    }

    /// Loads the current save for boot.
    ///
    /// A `file`-less slot reports and defaults blocks under its own
    /// `absent_status` (`NoFlash` for [`Self::disabled`], `Empty` for
    /// [`Self::none`]). Read failures always log and return
    /// [`SaveFileStatus::NoFlash`] regardless: boot has no error path and
    /// must never overwrite a file it could not read.
    pub(crate) fn load(&mut self) -> SavedGame {
        let Some(file) = &self.file else {
            self.session_status = Some(self.absent_status);
            return SavedGame::defaulted(self.absent_status);
        };
        let mut store = match file.read() {
            Ok(Some(store)) => store,
            Ok(None) => SaveStore::new(),
            Err(err) => {
                eprintln!("save: {err} -- starting without a saved game; saving is disabled for this session");
                self.file = None;
                self.session_status = Some(SaveFileStatus::NoFlash);
                return SavedGame::defaulted(SaveFileStatus::NoFlash);
            }
        };
        let outcome = store.load();
        self.session_counter = Some(store.save_counter());
        self.session_base = Some(store.base_snapshot());
        self.session_merged_donor = outcome.storage_source == StorageSource::LegacyDonor;
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
    /// Each write rereads the file and derives its rotation from that image;
    /// it is refused if the disk holds progress newer than this session loaded.
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
        self.store_under(block1, block2, refuse_foreign, lineage, SaveFile::lock)
    }

    /// As [`SaveSlot::store_impl`], taking the read-modify-write lock through
    /// `lock`, which the whole cycle then runs under.
    fn store_under(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
        refuse_foreign: bool,
        lineage: SaveLineage,
        lock: impl FnOnce(&SaveFile) -> Result<SaveFileGuard, SaveFileError>,
    ) -> Result<StoreOutcome, SaveFileError> {
        let clear_base = lineage.clears_base();
        let file = self.file.as_ref().ok_or(SaveFileError::NoDataDirectory)?;
        let _read_modify_write_lock = lock(file)?;
        let mut store = file.read()?.unwrap_or_else(SaveStore::new);
        let disk_load = store.load();
        let disk_status = SaveFileStatus::from_store(disk_load.status);
        let disk_counter = store.save_counter();
        if let Some(session_counter) = self.session_counter {
            if counter_is_ahead(session_counter, disk_counter) && disk_status.menu_shows_continue()
            {
                return Ok(StoreOutcome::RefusedStaleSession);
            }
        }
        if refuse_foreign && disk_status.menu_shows_continue() {
            return Ok(StoreOutcome::RefusedExistingSave);
        }
        if !clear_base {
            if let (Some(session_counter), Some(session_base)) =
                (self.session_counter, &self.session_base)
            {
                // The reload above fell back past a generation damaged since
                // the session read it (tests::healing_a_damaged_newest_slot_*),
                // or kept the same legacy head but lost the donor that
                // supplied its storage (tests::equal_counter_donor_loss_*);
                // any other equal-counter mismatch is refused
                // (tests::an_equal_counter_replacement_is_refused_*).
                let same_base = store.base_matches(session_base);
                let donor_lost = !same_base
                    && donor_was_lost(
                        self.session_merged_donor,
                        disk_load.storage_source,
                        store.base_blocks_match(session_base),
                    );
                match reconcile_base(disk_counter, session_counter, same_base, donor_lost) {
                    BaseReconcile::Keep => {}
                    BaseReconcile::Restore => store.restore_base(session_base.clone()),
                    BaseReconcile::Refuse => return Ok(StoreOutcome::RefusedConflictingSave),
                }
            }
        }
        if clear_base {
            store.clear_base();
        }
        store.save(block1, block2);
        file.write(&store)?;
        self.session_counter = Some(store.save_counter());
        self.session_base = Some(store.base_snapshot());
        // The write is full-format, so its storage is its own from here on.
        self.session_merged_donor = false;
        Ok(StoreOutcome::Written)
    }
}

#[cfg(test)]
mod tests;
