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
//! # Deferred, and honestly so
//!
//! - **The start-menu SAVE flow.** Upstream's in-game save is a
//!   `START` -> `SAVE` -> confirmation-dialog chain
//!   (`src/start_menu.c`'s `StartMenu_Save`/`SaveStartCallback` task
//!   sequence), none of which is built. [`crate::flow::save_on_exit`] is the
//!   headless-safe stand-in this slice ships instead; see its own docs.
//! - **`DoSaveFailedScreen`.** Upstream shows a dedicated "save failed"
//!   screen and retries against `gDamagedSaveSectors`. There is no such
//!   screen here; a failed write is reported to the caller as a
//!   [`engine::save::SaveFileError`] and logged.
//! - **The save-file error message windows.** `SAVE_STATUS_CORRUPT` and
//!   `SAVE_STATUS_ERROR` each open a main-menu message window first
//!   (`gText_SaveFileErased` / `gText_SaveFileCorrupted`,
//!   `src/main_menu.c:649-659`). The *menu type* each resolves to is
//!   modelled ([`SaveFileStatus::menu_shows_continue`]); the windows are
//!   not.
//! - **`CopyPartyAndObjectsToSave`/`FromSave`.** Upstream syncs
//!   `gPlayerParty` and `gObjectEvents` into the save block around every
//!   save/load (`src/save.c:663-672`). This port has no `gObjectEvents`
//!   model, and no encoder between `battle::BattlePokemon` and
//!   [`engine::save::Pokemon`]'s encrypted substructures — see
//!   [`crate::flow::overworld_phase::OverworldPhase::continue_saved_game`]
//!   for what that costs on the party side.

use engine::save::{SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStatus, SaveStore};

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
            Ok(file) => Self { file: Some(file) },
            Err(err) => {
                eprintln!("save: {err} -- this session cannot load or save");
                Self { file: None }
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
        Self { file: Some(file) }
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
    pub(crate) fn load(&self) -> SavedGame {
        let Some(file) = &self.file else {
            return SavedGame::no_flash();
        };
        let mut store = match file.read() {
            Ok(Some(store)) => store,
            // No file yet: erased flash. `SaveStore::new`'s all-`0xFF`
            // buffer is exactly that, and resolves to `SAVE_STATUS_EMPTY`.
            Ok(None) => SaveStore::new(),
            Err(err) => {
                eprintln!("save: {err} -- starting without a saved game");
                return SavedGame::no_flash();
            }
        };
        let outcome = store.load();
        SavedGame {
            status: SaveFileStatus::from_store(outcome.status),
            block1: outcome.block1,
            block2: outcome.block2,
        }
    }

    /// `TrySavingData(SAVE_NORMAL)` into `HandleSavingData`'s `SAVE_NORMAL`
    /// arm (`pokeemerald/src/save.c:735-739, 765-783`): write the whole save
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
    /// at all" case starts from a fresh [`engine::save::SaveStore`].
    ///
    /// # Errors
    ///
    /// [`engine::save::SaveFileError::NoDataDirectory`] if this slot has no
    /// resolved path; otherwise whatever reading back or writing the image
    /// failed with.
    pub(crate) fn store(
        &self,
        block1: &SaveBlock1,
        block2: &SaveBlock2,
    ) -> Result<(), SaveFileError> {
        let file = self.file.as_ref().ok_or(SaveFileError::NoDataDirectory)?;
        let mut store = file.read()?.unwrap_or_else(SaveStore::new);
        // Recovers the counters the image was last written with; the
        // returned blocks are the *previous* save's and are deliberately
        // discarded: `HandleSavingData` overwrites a whole slot from RAM, it
        // never merges.
        drop(store.load());
        store.save(block1, block2);
        file.write(&store)
    }
}

#[cfg(test)]
mod tests;
