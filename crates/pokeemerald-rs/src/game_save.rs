//! The save-file drivers this crate's game flow calls (I-6, issue #214):
//! boot-time load and in-session write, over
//! [`engine::save::SaveFile`]/[`engine::save::SaveStore`].
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the two upstream
//! entry points that bracket a whole play session:
//!
//! | upstream | here |
//! |----------|------|
//! | `Save_ResetSaveCounters` + `LoadGameSave(SAVE_NORMAL)` (`pokeemerald/src/intro.c:1153-1156`, `src/save.c:871-898`) | [`SaveSlot::load`] |
//! | `TrySavingData(SAVE_NORMAL)` into `HandleSavingData` (`pokeemerald/src/save.c:707-783`) | [`SaveSlot::store`] |
//!
//! Everything about *save contents* — the sector footer format, the
//! checksums, the two-slot rotation, and the corrupt-slot decision table —
//! already lives in `engine::save::store` (issues #94/#117/#130/#138) and is
//! reused verbatim here, never re-derived `(oop-boundaries)`.
//!
//! # `gSaveFileStatus`, and how "no file" maps onto "no flash"
//!
//! Upstream keeps the boot load's verdict in the global `gSaveFileStatus`
//! (`src/save.c:89`), which the main menu then branches on
//! (`Task_MainMenuCheckSaveFile`, `src/main_menu.c:641-670`). This port has
//! no globals `(oop-boundaries)`, so the verdict travels as a value —
//! [`SavedGame::status`], from [`SaveSlot::load`] into
//! [`crate::flow::AppScene::MainMenu`].
//!
//! [`SaveFileStatus`] adds one variant to
//! [`engine::save::SaveStatus`]: [`SaveFileStatus::NoFlash`], upstream's
//! `SAVE_STATUS_NO_FLASH`. Upstream returns it when `gFlashMemoryPresent !=
//! TRUE` (`src/save.c:875-879`) — the cartridge's save chip could not be
//! identified at all, so nothing can be read *or* written. Its file-system
//! counterpart is exactly the same situation: the save path could not be
//! resolved, or the file that is there is unreadable or not a save image.
//! Upstream's own handling of that case is what this port then inherits for
//! free — `HAS_NO_SAVED_GAME`, i.e. `NEW GAME`/`OPTION` only
//! (`src/main_menu.c:666-669`) — rather than a fabricated policy of this
//! port's own.
//!
//! **A `NoFlash` boot never overwrites the file it failed to read.** That is
//! upstream's rule too (`TrySavingData` refuses outright when
//! `gFlashMemoryPresent != TRUE`, `src/save.c:765-771`), and it is the
//! difference between an unreadable save the player can still recover by
//! hand and one this port silently destroys on the next save.
//!
//! # Who calls the writer
//!
//! The player does, through the field start menu: `START` -> `SAVE` ->
//! upstream's confirm/overwrite chain (`src/start_menu.c`'s
//! `sSaveDialogCallback` sequence, ported as [`crate::start_menu`], I-6
//! issue #232). The three entry points below are that flow's two
//! `TrySavingData` arms plus the guard the one unprompted write needs; this
//! module never decides *whether* to save, only how.
//!
//! Issue #214's `flow::save_on_exit` stand-in — an automatic write on
//! application shutdown, a deliberate deviation from upstream, which writes
//! nothing when the lid closes — is gone with that flow.
//!
//! # Deferred, and honestly so
//!
//! - **`DoSaveFailedScreen`.** Upstream shows a dedicated "save failed"
//!   screen and retries against `gDamagedSaveSectors`. There is no such
//!   screen here; a failed write is reported to the caller as a
//!   [`engine::save::SaveFileError`], logged, and surfaced to the player as
//!   the start menu's own `gText_SaveError` message.
//! - **The save-file error message windows.** `SAVE_STATUS_CORRUPT` and
//!   `SAVE_STATUS_ERROR` each open a main-menu message window first
//!   (`gText_SaveFileErased` / `gText_SaveFileCorrupted`,
//!   `src/main_menu.c:649-659`). The *menu type* each resolves to is
//!   modelled ([`SaveFileStatus::menu_shows_continue`]); the windows are
//!   not.
//! - **`CopyPartyAndObjectsToSave`/`FromSave`'s object-event half.**
//!   Upstream syncs all of `gObjectEvents` into the save block around every
//!   save/load (`src/load_save.c:196-206`). This port models one field of
//!   one entry — the player's facing
//!   ([`engine::save::SavedObjectEvent`]) — which is the only part a
//!   continue observably restores here. The party half is fully modelled
//!   since issue #232 ([`crate::party`]).

use engine::save::{
    BaseSnapshot, SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStatus, SaveStore,
};

/// The boot load's verdict — upstream `gSaveFileStatus`'s `SAVE_STATUS_*`
/// values (`pokeemerald/include/save.h:34-38`).
///
/// [`engine::save::SaveStatus`] covers the four a real flash read can
/// produce; this adds `SAVE_STATUS_NO_FLASH`, which upstream reaches without
/// reading anything at all (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFileStatus {
    /// `SAVE_STATUS_EMPTY` — nothing has ever been saved.
    Empty,
    /// `SAVE_STATUS_OK` — an intact save was loaded.
    Ok,
    /// `SAVE_STATUS_CORRUPT` — no intact slot survives.
    Corrupt,
    /// `SAVE_STATUS_ERROR` — one slot is intact and was loaded; the other
    /// has at least one damaged sector.
    Error,
    /// `SAVE_STATUS_NO_FLASH` — the save medium itself is unusable (module
    /// docs).
    NoFlash,
}

impl SaveFileStatus {
    /// The [`engine::save::SaveStatus`] a store load produced, widened to
    /// this enum.
    const fn from_store(status: SaveStatus) -> Self {
        match status {
            SaveStatus::Empty => Self::Empty,
            SaveStatus::Ok => Self::Ok,
            SaveStatus::Corrupt => Self::Corrupt,
            SaveStatus::Error => Self::Error,
        }
    }

    /// Whether the main menu offers `CONTINUE` for this status — upstream
    /// `Task_MainMenuCheckSaveFile`'s `tMenuType` assignment
    /// (`pokeemerald/src/main_menu.c:641-670`).
    ///
    /// `SAVE_STATUS_OK` (`:643-648`) and `SAVE_STATUS_ERROR` (`:654-660`)
    /// both select `HAS_SAVED_GAME`: an errored load still copied a fully
    /// intact slot's data through (`engine::save::SaveStatus::Error`'s own
    /// docs), so continuing from it is correct, not a gamble. `CORRUPT`
    /// (`:649-653`), `EMPTY` (`:661-665`), and `NO_FLASH` (`:666-670`) all
    /// select `HAS_NO_SAVED_GAME`.
    pub(crate) const fn menu_shows_continue(self) -> bool {
        match self {
            Self::Ok | Self::Error => true,
            Self::Empty | Self::Corrupt | Self::NoFlash => false,
        }
    }
}

/// The result of the boot load: the verdict plus the blocks it recovered.
///
/// The blocks are always present, even for [`SaveFileStatus::Empty`] or
/// [`SaveFileStatus::Corrupt`] — that mirrors upstream, where
/// `gSaveBlock1Ptr`/`gSaveBlock2Ptr` always point at *something*
/// (`CopySaveSlotData` runs unconditionally after `GetSaveValidStatus`) and
/// `Sav2_ClearSetDefault` merely zeroes them when the status says the data
/// is unusable (`src/intro.c:1155-1156`). Callers must gate on
/// [`SaveFileStatus::menu_shows_continue`] rather than on the blocks looking
/// plausible.
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
    /// The "medium unusable" outcome: `SAVE_STATUS_NO_FLASH` with the zeroed
    /// blocks `Sav2_ClearSetDefault` would leave behind.
    fn no_flash() -> Self {
        Self {
            status: SaveFileStatus::NoFlash,
            block1: SaveBlock1::default(),
            block2: SaveBlock2::default(),
        }
    }
}

/// What [`SaveSlot::store_unless_foreign_save`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreOutcome {
    /// The blocks were rotated in and persisted.
    Written,
    /// The write was refused: a continuable save this session did not load
    /// is on disk, and overwriting it needs the consent prompt upstream
    /// gates on `gDifferentSaveFile` (`src/main_menu.c`), which this port
    /// cannot show yet. The file was left byte-untouched.
    RefusedExistingSave,
    /// The write was refused: the save on disk has rotated past the state
    /// this session loaded at boot (its `gSaveCounter` no longer matches),
    /// i.e. another process saved in between, and writing would overwrite
    /// that newer progress with this session's stale copy (issue #214
    /// review). Upstream has no counterpart -- one cartridge, one game --
    /// so refusal is this port's own conservative policy. The file was
    /// left byte-untouched.
    RefusedStaleSession,
}

/// Whether counter `b` sits strictly *ahead* of counter `a` on the save
/// counter's wrapping number line -- serial-number arithmetic: `b` is ahead
/// iff stepping forward from `a` reaches `b` in fewer than half the counter
/// space. The session-vs-disk drift checks in [`SaveSlot::store`] need this
/// rather than [`counter_b_is_newer`]'s adjacent-pair rule: that rule is a
/// faithful model of upstream's `GetSaveValidStatus`, which only ever
/// compares the two on-disk slots -- generations exactly one apart -- while
/// an arbitrary number of another process's saves can intervene between
/// this session's load and its exit write, including runs that straddle the
/// `u32` wrap (`MAX -> 0 -> 1`, which the adjacent rule mis-orders as *not*
/// newer than `MAX`) (#230 review round five). Kept here, not in
/// `engine::save`: the slot resolver must keep upstream's exact rule
/// `(behavioral-fidelity)`; this drift question has no upstream counterpart.
const fn counter_is_ahead(a: u32, b: u32) -> bool {
    b != a && b.wrapping_sub(a) < u32::MAX / 2
}

/// This session's save medium: the one file the game loads from at boot and
/// writes back to.
///
/// Owned and passed explicitly (into [`crate::flow::advance_scene`], out of
/// [`crate::app::App`]) rather than resolved at each use site
/// `(oop-boundaries)`. That is not just tidiness: it is what lets the
/// headless round-trip test point a whole game flow at a scratch file
/// without touching process-global environment state, which parallel test
/// threads share.
///
/// `None` inside is upstream's `gFlashMemoryPresent != TRUE`: the save path
/// could not be resolved at all, so this session can neither load nor save
/// (module docs' `NoFlash` rule).
#[derive(Debug)]
pub(crate) struct SaveSlot {
    file: Option<SaveFile>,
    /// The `gSaveCounter` the image held when this session last read or
    /// wrote it -- the session's save *identity*. `None` until
    /// [`SaveSlot::load`] establishes it (or when the medium is unusable).
    /// [`SaveSlot::store`]'s stale-session check compares the disk's
    /// current counter against this before overwriting anything.
    session_counter: Option<u32>,
    /// The serialization base this session actually booted from --
    /// [`engine::save::SaveStore::base_snapshot`] of [`SaveSlot::load`]'s
    /// store, refreshed after every successful write. When a write-time
    /// read falls *behind* the session identity (the newest slot was
    /// damaged and [`engine::save::SaveStore::load`] adopted the older
    /// intact slot), the healing write restores this base first, so the
    /// deferred state written forward is the session's own lineage -- not
    /// a silent rollback to the older save's play time/options/Pokédex
    /// (#230 review).
    session_bases: Option<BaseSnapshot>,
    /// `gSaveFileStatus` (`pokeemerald/src/save.c:89`): the verdict the
    /// *boot* load reached, latched for the whole session so the
    /// start-menu save flow can branch on it exactly as
    /// `SaveConfirmInputCallback` does (`src/start_menu.c:1008-1023`).
    /// `None` until [`SaveSlot::load`] has run -- upstream's own global is
    /// zero-initialized, i.e. `SAVE_STATUS_EMPTY`, until then, which is
    /// what [`SaveSlot::boot_status`] reports.
    session_status: Option<SaveFileStatus>,
}

impl SaveSlot {
    /// The slot at [`engine::save::SaveFile::default_location`].
    ///
    /// An unresolvable path is logged once here and then behaves as
    /// [`SaveFileStatus::NoFlash`] for the rest of the session, the same
    /// "log-or-ignore is fine" policy [`crate::flow`] applies to a
    /// transition's pack load. A game that refuses to start because `$HOME`
    /// is unset is strictly worse than one that offers `NEW GAME`.
    pub(crate) fn default_location() -> Self {
        match SaveFile::default_location() {
            Ok(file) => Self {
                file: Some(file),
                session_counter: None,
                session_bases: None,
                session_status: None,
            },
            Err(err) => {
                eprintln!("save: {err} -- this session cannot load or save");
                Self {
                    file: None,
                    session_counter: None,
                    session_bases: None,
                    session_status: None,
                }
            }
        }
    }

    /// A slot at an explicit `file`.
    ///
    /// Test-only for now: production always resolves the one per-user path
    /// ([`Self::default_location`]). The headless save/continue round-trip
    /// needs a scratch file per test thread, which is the whole reason this
    /// type takes its file by value instead of resolving one internally.
    #[cfg(test)]
    pub(crate) fn at(file: SaveFile) -> Self {
        Self {
            file: Some(file),
            session_counter: None,
            session_bases: None,
            session_status: None,
        }
    }

    /// `gSaveFileStatus` (struct docs): the boot load's verdict, or
    /// [`SaveFileStatus::Empty`] before any load has run -- upstream's
    /// global holds `SAVE_STATUS_EMPTY` (0) then too.
    pub(crate) fn boot_status(&self) -> SaveFileStatus {
        self.session_status.unwrap_or(SaveFileStatus::Empty)
    }

    /// `Save_ResetSaveCounters` + `LoadGameSave(SAVE_NORMAL)`
    /// (`pokeemerald/src/intro.c:1153-1154`): read the image, resolve which
    /// slot is current, and copy it back into blocks.
    ///
    /// Never fails: every failure mode collapses into a [`SavedGame`] whose
    /// status the main menu already knows how to present (module docs). I/O
    /// failures are logged rather than propagated, the same
    /// "log-or-ignore is fine" policy [`crate::flow`] applies to a
    /// transition's pack load: there is no boot-time UI to report them
    /// through, and a game that refuses to start because a stray file is
    /// unreadable is strictly worse than one that offers `NEW GAME`.
    pub(crate) fn load(&mut self) -> SavedGame {
        let Some(file) = &self.file else {
            self.session_status = Some(SaveFileStatus::NoFlash);
            return SavedGame::no_flash();
        };
        let mut store = match file.read() {
            Ok(Some(store)) => store,
            // No file yet: erased flash. `SaveStore::new`'s all-`0xFF`
            // buffer is exactly that, and resolves to `SAVE_STATUS_EMPTY`.
            Ok(None) => SaveStore::new(),
            Err(err) => {
                // Latched for the whole session, exactly as upstream's
                // `gFlashMemoryPresent` stays FALSE once the medium fails
                // to identify: a boot that could not *read* the save must
                // never *write* it later, even if the failure was
                // transient and the medium recovers -- the bytes it would
                // write are a session that never saw the real save (issue
                // #214 review; module docs' `NoFlash` rule).
                eprintln!("save: {err} -- starting without a saved game; saving is disabled for this session");
                self.file = None;
                self.session_status = Some(SaveFileStatus::NoFlash);
                return SavedGame::no_flash();
            }
        };
        let outcome = store.load();
        // The loaded image's counter is this session's save identity: the
        // stale-session check in `store_impl` refuses to overwrite a file
        // that has rotated past it (another process saved in between). The
        // base snapshot is the deferred lineage that identity continued
        // from -- what a corruption-fallback heal writes forward (see
        // `session_bases`' docs).
        self.session_counter = Some(store.save_counter());
        self.session_bases = Some(store.base_snapshot());
        let status = SaveFileStatus::from_store(outcome.status);
        // `gSaveFileStatus` (struct docs): latched here, read later by the
        // start menu's save flow.
        self.session_status = Some(status);
        SavedGame {
            status,
            block1: outcome.block1,
            block2: outcome.block2,
        }
    }

    /// `TrySavingData(SAVE_NORMAL)` into `HandleSavingData`'s `SAVE_NORMAL`
    /// arm (`pokeemerald/src/save.c:736-740, 765-783`): write the whole save
    /// slot and persist the resulting image.
    ///
    /// The rotation is upstream's, unchanged: the current image is read back
    /// first and [`engine::save::SaveStore::load`] re-derives `gSaveCounter`
    /// and `gLastWrittenSector` from its footers, so
    /// [`engine::save::SaveStore::save`] alternates slots and advances the
    /// rotation offset exactly as a session that had held those two values
    /// in RAM since boot would (see
    /// [`engine::save::SaveStore::flash_image`] on why the image alone is
    /// enough to recover them).
    ///
    /// An existing file that cannot be read back is *not* silently
    /// overwritten; see the module docs' `NoFlash` rule. Only the "no file
    /// at all" case starts from a fresh [`engine::save::SaveStore`]. A file
    /// whose rotation moved past what this session loaded is refused as
    /// [`StoreOutcome::RefusedStaleSession`] rather than overwritten.
    ///
    /// # Errors
    ///
    /// [`engine::save::SaveFileError::NoDataDirectory`] if this slot has no
    /// usable medium (an unresolvable path, or a boot read failure latched
    /// by [`SaveSlot::load`]); otherwise whatever locking, reading back, or
    /// writing the image failed with.
    pub(crate) fn store(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
    ) -> Result<StoreOutcome, SaveFileError> {
        self.store_impl(block1, block2, false, false)
    }

    /// `TrySavingData(SAVE_OVERWRITE_DIFFERENT_FILE)` into
    /// `HandleSavingData`'s own arm (`pokeemerald/src/save.c:751-760`):
    /// [`SaveSlot::store`], plus the wipe of everything the *previous*
    /// file left behind.
    ///
    /// Upstream reaches this arm only from `SaveDoSaveCallback`'s
    /// `gDifferentSaveFile` branch (`src/start_menu.c:1093-1097`) -- a
    /// session that started a new game and whose player has just answered
    /// YES to `gText_DifferentSaveFile`'s WARNING -- and what it adds over
    /// `SAVE_NORMAL` is erasing the Hall of Fame sectors, so no trace of
    /// the replaced adventure survives. This port has no Hall of Fame
    /// sectors; its counterpart is dropping the *retained payload base*
    /// (`SaveStore::clear_base`), so the deferred bytes the pre-write read
    /// recovered -- the previous trainer's play time, options, Pokédex, and
    /// ciphertext under a discarded encryption key -- do not ride along
    /// into the new file (#230 review).
    ///
    /// # Errors
    ///
    /// The same cases as [`SaveSlot::store`].
    pub(crate) fn store_overwriting_different_file(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
    ) -> Result<StoreOutcome, SaveFileError> {
        self.store_impl(block1, block2, false, true)
    }

    /// [`SaveSlot::store`], except it **refuses** -- file untouched,
    /// [`StoreOutcome::RefusedExistingSave`] -- when the image on disk
    /// holds a continuable save ([`SaveFileStatus::menu_shows_continue`]).
    ///
    /// The one write this port performs with no prompt behind it
    /// (issue #232): upstream's `SaveConfirmInputCallback` skips the
    /// overwrite question entirely when a *new-game* session saves over an
    /// `SAVE_STATUS_EMPTY`/`SAVE_STATUS_CORRUPT` cartridge
    /// (`src/start_menu.c:1008-1019`) -- there is nothing there to ask
    /// about. That reasoning holds on a cartridge and not on a filesystem,
    /// where another instance can have written a real save since this
    /// session booted, so the write is still checked *under the same
    /// inter-process lock* and refused if the image on disk turned out to
    /// be continuable after all. Also clears the retained base, for the
    /// reason [`SaveSlot::store_overwriting_different_file`] documents:
    /// this session never adopted the loaded image's blocks.
    ///
    /// # Errors
    ///
    /// The same cases as [`SaveSlot::store`].
    pub(crate) fn store_unless_foreign_save(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
    ) -> Result<StoreOutcome, SaveFileError> {
        self.store_impl(block1, block2, true, true)
    }

    /// The shared body of the three `TrySavingData` entry points above.
    ///
    /// `refuse_foreign` is the unprompted-write guard
    /// ([`SaveSlot::store_unless_foreign_save`]); `clear_base` drops the
    /// previous file's deferred bytes
    /// ([`SaveSlot::store_overwriting_different_file`]). They are separate
    /// because upstream separates them: only one of its three reachable
    /// save types wipes the old file's leftovers, and none of them refuses
    /// a write the player already consented to.
    fn store_impl(
        &mut self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
        refuse_foreign: bool,
        clear_base: bool,
    ) -> Result<StoreOutcome, SaveFileError> {
        let file = self.file.as_ref().ok_or(SaveFileError::NoDataDirectory)?;
        // Exclusive across processes for the whole read-modify-write
        // cycle, so a concurrent instance can neither interleave its own
        // rotation with ours nor race the staged rename
        // (`SaveFile::lock`'s docs; issue #214 review).
        let _guard = file.lock()?;
        let mut store = file.read()?.unwrap_or_else(SaveStore::new);
        // Recovers the counters the image was last written with; the
        // returned blocks are the *previous* save's and are discarded:
        // `HandleSavingData` overwrites a whole slot from RAM, it never
        // merges. The load's *status* feeds the consent check, and the
        // recovered counter feeds the stale-session check.
        let existing = store.load();
        // The stale-session check (issue #214 review): a counter that
        // moved past the one this session loaded means another process
        // saved since -- overwriting would replace newer persisted
        // progress with this session's stale copy. *Directional* on
        // purpose (#230 review round three): only a counter strictly
        // *newer* than the session's identity is another process's
        // progress. A counter that fell *behind* it is `SaveStore::load`'s
        // corruption fallback -- the newest slot was damaged and the older
        // intact slot's counter was adopted (a continuable `Error`) -- and
        // this session, still holding the valid state, must be allowed to
        // write and heal the image; `!=` would strand it. Also gated on
        // the disk image still being *continuable*: a corrupted or wiped
        // file is not newer progress either. Only armed once `load` has
        // established an identity; checked under the same lock as the
        // write, so the comparison cannot itself race.
        if let Some(loaded) = self.session_counter {
            if counter_is_ahead(loaded, store.save_counter())
                && SaveFileStatus::from_store(existing.status).menu_shows_continue()
            {
                return Ok(StoreOutcome::RefusedStaleSession);
            }
        }
        // The corruption-fallback heal's base (#230 review): when the disk
        // fell *behind* the session identity, `store.load()` above adopted
        // the *older* intact slot -- serialization base included -- so
        // saving now would silently roll deferred state (play time,
        // options, Pokédex) back a generation. The session continued from
        // the boot load's payload; restore that as the base so the heal
        // writes the session's own lineage forward. Moot when the counters
        // match (same base either way) and skipped when `clear_base` is
        // set, since that zeroes the base regardless.
        if !clear_base {
            if let (Some(loaded), Some(bases)) = (self.session_counter, &self.session_bases) {
                if counter_is_ahead(store.save_counter(), loaded) {
                    store.restore_base(bases.clone());
                }
            }
        }
        if refuse_foreign && SaveFileStatus::from_store(existing.status).menu_shows_continue() {
            return Ok(StoreOutcome::RefusedExistingSave);
        }
        if clear_base {
            // A `clear_base` write is a session that never adopted the
            // loaded image's blocks -- the new-game path, which the start
            // menu keys off `gDifferentSaveFile`. Upstream zeroes
            // `gSaveBlock1/2` wholesale before such a session's first save
            // (`Sav2_ClearSetDefault` -> `ClearSav1`/`ClearSav2`), so the
            // deferred bytes the pre-write `load` retained -- the previous
            // trainer's play time, options, Pokédex, ciphertext under a
            // discarded key -- must not ride along into this slot (#230
            // review). A *continued* session keeps the retained base: that
            // is exactly upstream's RAM after `CopySaveSlotData`.
            store.clear_base();
        }
        store.save(block1, block2);
        file.write(&store)?;
        // The write advanced the rotation; future stores in this session
        // must expect the counter *we* just produced -- and the payloads we
        // just wrote are now the session's deferred lineage.
        self.session_counter = Some(store.save_counter());
        self.session_bases = Some(store.base_snapshot());
        Ok(StoreOutcome::Written)
    }
}

#[cfg(test)]
mod tests;
