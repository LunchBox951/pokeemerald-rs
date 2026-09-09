//! Persists a [`SaveStore`] flash image as one host file.
//!
//! This module resolves save paths, performs exact-length reads, and provides
//! locking and atomic writes. Save contents and slot validation remain owned by
//! [`SaveStore`].

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::store::{self, SaveStore};

/// Environment variable containing an explicit save-file path.
pub const SAVE_PATH_ENV: &str = "POKEEMERALD_RS_SAVE";

/// Per-user data subdirectory containing the default save file.
pub const SAVE_DIR_NAME: &str = "pokeemerald-rs";

/// Default save-file name.
pub const SAVE_FILE_NAME: &str = "pokeemerald.sav";

/// File-system or path-resolution failure while accessing a save file.
#[derive(Debug)]
pub enum SaveFileError {
    /// No explicit path or per-user data directory is available.
    NoDataDirectory,
    /// Creating the save file's parent directory failed.
    CreateDirectory {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Reading an existing save file failed.
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Writing the save file failed.
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Creating or locking the sibling lock file failed.
    Lock {
        /// The lock file that could not be created or locked.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// The file length does not match [`store::FLASH_IMAGE_LEN`].
    BadLength {
        /// The file whose length was wrong.
        path: PathBuf,
        /// Required image length.
        expected: usize,
        /// The length actually found.
        got: usize,
    },
}

impl std::fmt::Display for SaveFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDataDirectory => write!(
                f,
                "save file: no per-user data directory in the environment -- \
                 set ${SAVE_PATH_ENV} to choose one explicitly"
            ),
            Self::CreateDirectory { path, source } => {
                write!(f, "save file: creating {} failed: {source}", path.display())
            }
            Self::Read { path, source } => {
                write!(f, "save file: reading {} failed: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(f, "save file: writing {} failed: {source}", path.display())
            }
            Self::Lock { path, source } => {
                write!(f, "save file: locking {} failed: {source}", path.display())
            }
            Self::BadLength {
                path,
                expected,
                got,
            } => write!(
                f,
                "save file: {} is {got} bytes, not a {expected}-byte save image",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SaveFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateDirectory { source, .. }
            | Self::Read { source, .. }
            | Self::Write { source, .. }
            | Self::Lock { source, .. } => Some(source),
            Self::NoDataDirectory | Self::BadLength { .. } => None,
        }
    }
}

/// Host convention used to resolve a per-user data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFamily {
    /// `%APPDATA%`, else `%USERPROFILE%\AppData\Roaming`.
    Windows,
    /// `$HOME/Library/Application Support`.
    MacOs,
    /// The XDG Base Directory Specification: `$XDG_DATA_HOME` when
    /// absolute, else `$HOME/.local/share`.
    Xdg,
}

impl HostFamily {
    /// The family this binary was compiled for.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Xdg
        }
    }
}

/// Resolves `family`'s data directory through `env`, ignoring empty values.
#[must_use]
pub fn data_dir_for(family: HostFamily, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty_path = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match family {
        HostFamily::Windows => non_empty_path("APPDATA").or_else(|| {
            non_empty_path("USERPROFILE").map(|home| home.join("AppData").join("Roaming"))
        }),
        HostFamily::MacOs => {
            non_empty_path("HOME").map(|home| home.join("Library").join("Application Support"))
        }
        HostFamily::Xdg => non_empty_path("XDG_DATA_HOME")
            .filter(|dir| is_absolute_xdg_path(dir.as_os_str()))
            .or_else(|| non_empty_path("HOME").map(|home| home.join(".local").join("share"))),
    }
}

/// Whether `path` is absolute under the XDG Base Directory Specification's
/// POSIX path rules, independently of the platform running this binary.
fn is_absolute_xdg_path(path: &OsStr) -> bool {
    path.as_encoded_bytes().starts_with(b"/")
}

/// Resolves this host's save-file path.
///
/// # Errors
///
/// [`SaveFileError::NoDataDirectory`] if [`SAVE_PATH_ENV`] is unset and no
/// per-user data directory can be derived.
pub fn default_save_path() -> Result<PathBuf, SaveFileError> {
    default_save_path_from(HostFamily::host(), |name: &str| std::env::var_os(name))
}

/// Resolves a save-file path from an explicit host family and environment.
///
/// # Errors
///
/// As [`default_save_path`].
pub fn default_save_path_from(
    family: HostFamily,
    env: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SaveFileError> {
    if let Some(explicit) = env(SAVE_PATH_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(explicit));
    }
    data_dir_for(family, env)
        .map(|dir| dir.join(SAVE_DIR_NAME).join(SAVE_FILE_NAME))
        .ok_or(SaveFileError::NoDataDirectory)
}

/// Path-backed persistence for a [`SaveStore`] flash image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    path: PathBuf,
}

impl SaveFile {
    /// A save file at an explicit `path`.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// A save file at [`default_save_path`].
    ///
    /// # Errors
    ///
    /// As [`default_save_path`].
    pub fn default_location() -> Result<Self, SaveFileError> {
        Ok(Self::at(default_save_path()?))
    }

    /// Where this save file lives.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the path currently names a file.
    ///
    /// This advisory check may become stale before a later operation.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Reads an exact-length flash image, or returns `Ok(None)` when absent.
    ///
    /// Call [`SaveStore::load`] on the returned store to reconstruct its
    /// counters and validate its contents.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Read`] for any I/O failure other than "not found";
    /// [`SaveFileError::BadLength`] if the file is not
    /// [`store::FLASH_IMAGE_LEN`] bytes.
    pub fn read(&self) -> Result<Option<SaveStore>, SaveFileError> {
        use std::io::Read as _;

        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(SaveFileError::Read {
                    path: self.path.clone(),
                    source,
                })
            }
        };
        let oversized_image_probe_len = store::FLASH_IMAGE_LEN + 1;
        let mut bytes = Vec::with_capacity(oversized_image_probe_len);
        file.take(oversized_image_probe_len as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| SaveFileError::Read {
                path: self.path.clone(),
                source,
            })?;
        if bytes.len() != store::FLASH_IMAGE_LEN {
            return Err(SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: bytes.len(),
            });
        }
        SaveStore::from_flash_image(&bytes)
            .map(Some)
            .ok_or_else(|| SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: bytes.len(),
            })
    }

    /// Atomically replaces the save file with `store`'s synchronised image.
    ///
    /// The directory holding the save entry -- `.` for a bare relative path --
    /// is best-effort synchronised after the rename; see [`SaveFile::ensure_parent_directory`].
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Write`] if the temporary file could not
    /// be written, synced, or renamed into place, or if something replaced
    /// it between those two steps.
    pub fn write(&self, store: &SaveStore) -> Result<(), SaveFileError> {
        self.write_with(
            store,
            Self::sync_directory_best_effort,
            |bytes| self.stage_shrinking_on_invalid_filename(Self::real_open, bytes),
            |_| {},
        )
    }

    /// As [`SaveFile::write`], synchronising through the given `sync_directory`
    /// and staging through `stage`, rather than always
    /// [`SaveFile::sync_directory_best_effort`] and
    /// [`SaveFile::stage_shrinking_on_invalid_filename`].
    ///
    /// `before_rename` runs on the staged path once it holds the image and
    /// before anything promotes it, which is the only point from which the
    /// window this call has to defend can be occupied on purpose.
    fn write_with(
        &self,
        store: &SaveStore,
        mut sync_directory: impl FnMut(&Path),
        stage: impl FnOnce(&[u8]) -> std::io::Result<StagedSave>,
        before_rename: impl FnOnce(&Path),
    ) -> Result<(), SaveFileError> {
        self.ensure_parent_directory()?;

        let write_error = |source: std::io::Error| SaveFileError::Write {
            path: self.path.clone(),
            source,
        };
        let staged = stage(store.flash_image()).map_err(write_error)?;
        before_rename(&staged.path);
        // Nothing in `std` fuses this check to the rename below, so a
        // replacement landing between the two is still promoted; the
        // exclusive create, the unguessable name, and the handle held open
        // bound that window rather than close it.
        match staged.still_named() {
            Ok(true) => {}
            Ok(false) => {
                return Err(write_error(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "the staged image at {} was replaced before it could be renamed into place",
                        staged.path.display()
                    ),
                )))
            }
            Err(unreadable) => return Err(write_error(staged.remove_after(unreadable))),
        }
        if let Err(source) = std::fs::rename(&staged.path, &self.path) {
            return Err(write_error(staged.remove_after(source)));
        }
        if let Some(containing) = Self::directory_containing(&self.path) {
            sync_directory(containing);
        }
        Ok(())
    }

    /// Candidates one staging rung tries before reporting its namespace
    /// exhausted. The narrowest rung [`Self::stage_shrinking_on_invalid_filename`]
    /// can reach renders a single hex digit, and sixteen names is that
    /// whole namespace: because [`Self::staging_candidates`] steps rather
    /// than redraws, a walk this long tries every one of them, so stale or
    /// concurrently held entries can never report exhaustion while a free
    /// name remains. Wider rungs stop here too, where sixteen distinct
    /// names all landing on taken ones is already out of reach.
    const MAX_STAGING_ATTEMPTS: u32 = 16;

    /// Stages `bytes` under a fresh name from `next_path` on every attempt,
    /// opening each with `open`, retrying a name collision up to
    /// [`Self::MAX_STAGING_ATTEMPTS`] times; returns the file actually
    /// written.
    fn stage(
        mut next_path: impl FnMut() -> PathBuf,
        open: impl Fn(&Path) -> std::io::Result<std::fs::File>,
        bytes: &[u8],
    ) -> std::io::Result<StagedSave> {
        let mut last_collision = None;
        for _ in 0..Self::MAX_STAGING_ATTEMPTS {
            let path = next_path();
            match Self::write_and_sync_with(&open, &path, bytes) {
                Ok(staged) => return Ok(staged),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    last_collision = Some(err);
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_collision.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "exhausted staging attempts",
            )
        }))
    }

    /// As [`Self::stage`] with [`Self::staging_path_with_caps`] as the
    /// candidate generator, except that a candidate the host refuses
    /// outright (`InvalidFilename`) -- rather than merely finding it
    /// already taken (`AlreadyExists`) -- halves the stem cap on a char
    /// boundary (the truncation in [`Self::staging_path_with_caps`] already
    /// respects one) and starts a fresh run of collision retries there, all
    /// the way down to an empty stem. [`Self::MAX_COMPONENT_LEN`] is only
    /// ever a first guess at a limit this module cannot enumerate for every
    /// host or filesystem -- eCryptfs caps a component at 143 bytes, well
    /// under it, and a symlinked temp root can make a host's resolved path
    /// longer than the one this process measures -- so this loop, not that
    /// constant, is what actually keeps the sibling valid.
    ///
    /// An empty stem still carries the fixed-width `.tmp.<hex>` suffix,
    /// which can itself be wider than a directory's remaining room -- a
    /// directory that fit the former, short `.tmp.<pid>` name can be too
    /// tight for this one. Once the stem cannot shrink any further, the hex
    /// component does too, halving [`Self::UNIQUE_COMPONENT_HEX_DIGITS`]
    /// down to a single digit, each retry still drawing fresh randomness.
    /// Propagates only once even that one-digit floor is refused.
    fn stage_shrinking_on_invalid_filename(
        &self,
        open: impl Fn(&Path) -> std::io::Result<std::fs::File>,
        bytes: &[u8],
    ) -> std::io::Result<StagedSave> {
        let mut stem_cap = Self::first_guess_stem_cap();
        let mut hex_digits = Self::UNIQUE_COMPONENT_HEX_DIGITS;
        loop {
            match Self::stage(self.staging_candidates(stem_cap, hex_digits), &open, bytes) {
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename && stem_cap > 0 => {
                    stem_cap /= 2;
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename && hex_digits > 1 => {
                    hex_digits = (hex_digits / 2).max(1);
                }
                result => return result,
            }
        }
    }

    /// Opens `path` exclusively with the real, host `create_new` open --
    /// refusing an existing file, directory, or symlink there instead of
    /// following or truncating it.
    fn real_open(path: &Path) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
    }

    /// As [`Self::write_and_sync`], opening `path` through `open` rather
    /// than always [`Self::real_open`], so a test can substitute a host
    /// that refuses names an injected rule finds too long.
    fn write_and_sync_with(
        open: impl Fn(&Path) -> std::io::Result<std::fs::File>,
        path: &Path,
        bytes: &[u8],
    ) -> std::io::Result<StagedSave> {
        use std::io::Write as _;

        let file = open(path)?;
        let staged = StagedSave {
            path: path.to_path_buf(),
            file,
        };
        let result = (|| {
            let mut writer = std::io::BufWriter::new(&staged.file);
            writer.write_all(bytes)?;
            writer.flush()?;
            staged.file.sync_all()
        })();
        match result {
            Ok(()) => Ok(staged),
            Err(source) => Err(staged.remove_after(source)),
        }
    }

    /// Writes and syncs `bytes` to a freshly, exclusively created `path`,
    /// removing `path` again on any failure once past that open so this
    /// call never deletes an entry a different caller put there. A test
    /// convenience over [`Self::write_and_sync_with`]; production staging
    /// always goes through [`Self::stage_shrinking_on_invalid_filename`].
    /// Gated to its only caller's platform: the re-executed-child technique
    /// that test needs to force a write failure is Linux-specific, so this
    /// is otherwise dead code on every other target.
    #[cfg(all(test, target_os = "linux"))]
    fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<StagedSave> {
        Self::write_and_sync_with(Self::real_open, path, bytes)
    }

    fn sync_directory_best_effort(path: &Path) {
        drop(std::fs::File::open(path).and_then(|directory| directory.sync_all()));
    }

    /// Acquires an advisory inter-process lock for this save path.
    ///
    /// The lock lives on a sibling `.lock` file, not the save file itself:
    /// [`SaveFile::write`] replaces the save's inode by rename, and a lock
    /// on a replaced inode would silently stop excluding anyone who opened
    /// the path afterwards.
    ///
    /// Hold the returned guard across the complete read-modify-write cycle.
    ///
    /// On a first save, creates the missing hierarchy and best-effort
    /// synchronises every ancestor while the lock is held.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Lock`] if the lock file could not be
    /// created or locked.
    pub fn lock(&self) -> Result<SaveFileGuard, SaveFileError> {
        self.lock_with(Self::sync_directory_best_effort)
    }

    /// As [`SaveFile::lock`], synchronising through the given `sync_directory`
    /// rather than always [`SaveFile::sync_directory_best_effort`].
    fn lock_with(&self, sync_directory: impl FnMut(&Path)) -> Result<SaveFileGuard, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;
        let path = self.lock_path();
        let lock_error = |source: std::io::Error| SaveFileError::Lock {
            path: path.clone(),
            source,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(lock_error)?;
        file.lock().map_err(lock_error)?;
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, sync_directory);
            }
        }
        Ok(SaveFileGuard { _lock_file: file })
    }

    /// Creates the save file's parent directory and any missing ancestors;
    /// on a first save, best-effort synchronises every ancestor.
    fn ensure_parent_directory(&self) -> Result<Option<&Path>, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, Self::sync_directory_best_effort);
            }
        }
        Ok(parent)
    }

    /// Creates the save file's parent directory and any missing ancestors,
    /// without synchronising anything.
    fn create_parent_directory(&self) -> Result<Option<&Path>, SaveFileError> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent).map_err(|source| SaveFileError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(parent)
    }

    /// `dir` and every non-empty ancestor above it, outermost first.
    fn ancestor_chain(dir: &Path) -> Vec<PathBuf> {
        let mut levels = Vec::new();
        let mut current = Some(dir);
        while let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) {
            levels.push(path.to_path_buf());
            current = path.parent();
        }
        levels.reverse();
        levels
    }

    /// Best-effort synchronises the containing directory of each level of
    /// `dir`'s ancestor chain, outermost first.
    fn sync_ancestor_chain(dir: &Path, mut sync_directory: impl FnMut(&Path)) {
        for level in Self::ancestor_chain(dir) {
            if let Some(containing) = Self::directory_containing(&level) {
                sync_directory(containing);
            }
        }
    }

    /// The directory holding `level`'s entry: its parent, `.` for a bare
    /// relative name, or `None` for a root, drive prefix, or `.`/`..` level.
    fn directory_containing(level: &Path) -> Option<&Path> {
        use std::path::Component;

        match level.components().next_back() {
            Some(Component::Normal(_)) => Some(
                level
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            ),
            None
            | Some(
                Component::RootDir
                | Component::Prefix(_)
                | Component::CurDir
                | Component::ParentDir,
            ) => None,
        }
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// A collision-resistant sibling staging name -- unguessable to a planted
    /// symlink and unlikely to be shared by a second writer; [`Self::stage`]
    /// retries the rare exact collision, so this needs resistance, not
    /// proof. The save basename is cut to fit the suffix within
    /// [`Self::first_guess_stem_cap`]'s first guess at
    /// [`Self::MAX_COMPONENT_LEN`], which the first staging attempt tries
    /// so the common case succeeds immediately; when a host's real limit
    /// disagrees with that guess, [`Self::stage_shrinking_on_invalid_filename`]
    /// is what actually corrects it, not this function. A test convenience;
    /// production staging walks [`Self::staging_candidates`] instead.
    #[cfg(test)]
    fn staging_path(&self) -> PathBuf {
        self.staging_path_with_caps(
            Self::first_guess_stem_cap(),
            Self::UNIQUE_COMPONENT_HEX_DIGITS,
        )
    }

    /// The candidates one staging rung offers [`Self::stage`], in the
    /// order it tries them: the basename cut to at most `max_stem_len`
    /// bytes -- on a char boundary -- carrying a unique suffix
    /// `hex_digits` wide, drawn fresh for the first candidate and stepped
    /// through the namespace for each one after it.
    ///
    /// Stepped rather than redrawn so the walk never repeats itself. The
    /// narrowest rung the shrink chain reaches holds only sixteen names,
    /// and independent draws over a namespace that small revisit names
    /// already found taken: with fifteen of them held by stale or
    /// concurrent entries, eight draws report exhaustion about three times
    /// in five while a free name sits untried. A walk of
    /// [`Self::MAX_STAGING_ATTEMPTS`] distinct names cannot.
    ///
    /// Never the save path itself. A save whose own basename already has
    /// the `<stem>.tmp.<hex>` shape this renders is one of the names the
    /// walk would otherwise reach -- a save literally named `.tmp.a`, once
    /// the chain has reached an empty stem and a single hex digit, is one
    /// of that rung's sixteen -- and staging there is not staging at all:
    /// with no save yet at the destination `create_new` would succeed on
    /// it, so the image would go down in place, visible while half-written,
    /// and a crash would leave a partial file where the rename is supposed
    /// to publish a whole one. Skipping it costs at most one extra step,
    /// since at most one value in the namespace renders that basename.
    fn staging_candidates(
        &self,
        max_stem_len: usize,
        hex_digits: usize,
    ) -> impl FnMut() -> PathBuf + '_ {
        let mut stem = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if stem.len() > max_stem_len {
            let mut cut = max_stem_len;
            while !stem.is_char_boundary(cut) {
                cut -= 1;
            }
            stem.truncate(cut);
        }
        let mask = Self::unique_value_mask(hex_digits);
        let mut value = Self::unique_value(hex_digits);
        move || loop {
            let candidate = self
                .path
                .with_file_name(format!("{stem}.tmp.{value:0hex_digits$x}"));
            value = value.wrapping_add(1) & mask;
            if !self.aliases_save_path(&candidate) {
                return candidate;
            }
        }
    }

    /// Whether `candidate` names the save path itself on any supported
    /// file system. The candidate's stem is copied from the save's own
    /// basename and its suffix is ASCII, so byte equality under ASCII case
    /// folding is the only alias a case-insensitive volume can add.
    fn aliases_save_path(&self, candidate: &Path) -> bool {
        match (candidate.file_name(), self.path.file_name()) {
            (Some(candidate), Some(save)) => candidate
                .as_encoded_bytes()
                .eq_ignore_ascii_case(save.as_encoded_bytes()),
            _ => candidate == self.path,
        }
    }

    /// The first candidate of a fresh walk under these caps. A test
    /// convenience over [`Self::staging_candidates`]; production staging
    /// hands the whole walk to [`Self::stage`].
    #[cfg(test)]
    fn staging_path_with_caps(&self, max_stem_len: usize, hex_digits: usize) -> PathBuf {
        self.staging_candidates(max_stem_len, hex_digits)()
    }

    /// The stem budget [`Self::staging_path`] tries first: whatever is left
    /// of [`Self::MAX_COMPONENT_LEN`] once the fixed-width suffix is
    /// subtracted. A first guess, not a promise -- see
    /// [`Self::stage_shrinking_on_invalid_filename`].
    fn first_guess_stem_cap() -> usize {
        let suffix_len = ".tmp.".len() + Self::UNIQUE_COMPONENT_HEX_DIGITS;
        Self::MAX_COMPONENT_LEN.saturating_sub(suffix_len)
    }

    /// A first guess at the per-component limit most POSIX and Windows
    /// filesystems share, in bytes -- not a promise every filesystem keeps:
    /// eCryptfs caps a component at 143, well under this. Predicting a
    /// host's real limit precisely is not this module's job;
    /// [`Self::stage_shrinking_on_invalid_filename`] adapts to whatever it
    /// actually is by retrying under a shorter stem when the host refuses
    /// this guess outright.
    const MAX_COMPONENT_LEN: usize = 255;

    /// Width [`Self::unique_value`] is drawn at first, before
    /// [`Self::stage_shrinking_on_invalid_filename`] narrows it under
    /// pressure.
    const UNIQUE_COMPONENT_HEX_DIGITS: usize = 10;

    /// `std`-only entropy folded into one value that fits `width` hex
    /// digits: process id, clock nanoseconds, and a fresh `RandomState`
    /// key, which alone already differs between two calls at the same
    /// nanosecond. `create_new` is what actually keeps two stagings from
    /// colliding on purpose; this width is defence in depth on top of it,
    /// so narrowing it under pressure -- down to a single, still-freshly-
    /// drawn digit -- costs unpredictability, not the exclusivity
    /// guarantee itself.
    fn unique_value(width: usize) -> u64 {
        use std::hash::{BuildHasher, Hasher};

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u128(nanos);
        hasher.finish() & Self::unique_value_mask(width)
    }

    /// Every value `width` hex digits can render, and no other. `width` is
    /// [`Self::UNIQUE_COMPONENT_HEX_DIGITS`] or a halving of it, so it is
    /// never wide enough to overflow the shift.
    fn unique_value_mask(width: usize) -> u64 {
        (1_u64 << (4 * width)) - 1
    }
}

/// A staged flash image and the handle that wrote it, held open until the
/// image is promoted or abandoned: while the handle lives the file cannot be
/// freed, so nothing that replaces it at the staging path can inherit its
/// identity and pass for it.
#[derive(Debug)]
struct StagedSave {
    path: PathBuf,
    file: std::fs::File,
}

impl StagedSave {
    /// Whether the staging path still names this staged file, rather than a
    /// symlink, directory, or other entry that took its name.
    ///
    /// Where [`is_same_file`] has nothing to compare, refusing everything
    /// that is not a regular file is the whole check. A reading that failed
    /// is neither answer, and surfaces rather than passing for "replaced".
    fn still_named(&self) -> std::io::Result<bool> {
        let found = std::fs::symlink_metadata(&self.path)?;
        Ok(found.file_type().is_file() && is_same_file(&self.file.metadata()?, &found))
    }

    /// Removes this staged file after `source`, folding a cleanup failure
    /// into the returned error rather than swallowing it -- otherwise a
    /// caller who only sees `source` would never learn a staging file was
    /// left behind. An entry that replaced it belongs to whoever put it
    /// there and is left where it is; ownership that could not be read at
    /// all leaves the same file behind, and is reported the same way.
    fn remove_after(&self, source: std::io::Error) -> std::io::Error {
        let left_behind = match self.still_named() {
            Ok(false) => return source,
            // The same accepted bound documented at the rename above
            // (`SaveFile::write_with`) applies to this unlink: nothing in
            // `std` fuses the check above to the removal here, so a peer
            // that lands a replacement in the gap acts with the same
            // directory-write capability that window already accepts.
            Ok(true) => std::fs::remove_file(&self.path).err(),
            Err(unreadable) => Some(unreadable),
        };
        let Some(cleanup_source) = left_behind else {
            return source;
        };
        std::io::Error::new(
            source.kind(),
            format!(
                "{source}; additionally failed to remove the abandoned staging file {}: {cleanup_source}",
                self.path.display()
            ),
        )
    }
}

/// Whether two metadata readings describe the same file system object:
/// device and inode on unix.
///
/// Stable `std` exposes no Windows equivalent -- the file index sits behind
/// the unstable `windows_by_handle` feature -- so off unix this cannot
/// answer, and [`StagedSave::still_named`]'s regular-file test stands alone:
/// a replacement that is itself a regular file goes undetected there.
#[cfg(unix)]
fn is_same_file(staged: &std::fs::Metadata, found: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    (staged.dev(), staged.ino()) == (found.dev(), found.ino())
}

#[cfg(not(unix))]
fn is_same_file(_staged: &std::fs::Metadata, _found: &std::fs::Metadata) -> bool {
    true
}

/// Holds a [`SaveFile::lock`] until dropped.
#[derive(Debug)]
pub struct SaveFileGuard {
    _lock_file: std::fs::File,
}

#[cfg(test)]
mod tests;
