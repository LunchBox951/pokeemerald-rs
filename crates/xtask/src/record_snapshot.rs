//! Deterministic RGB and metadata capture for named headless scenes.
//!
//! A capture publishes an immutable generation containing `<scene>.rgb` and
//! `<scene>.meta`, then atomically replaces `<scene>.generation` to make both
//! payloads visible together. Clean captures are reproducible for the same
//! commit, retained asset pack, scene, and implied inputs. Dirty captures only
//! identify their commit with a `-dirty` suffix.
//!
//! The FNV-1a hashes locate changes in the RGB payload and retained pack. Only
//! exact byte comparison establishes equality.

use std::fmt;
use std::path::{Path, PathBuf};

use assets::pack::{AssetPack, PackError};
use pokeemerald_rs::main_menu::{MainMenuScene, MainMenuSceneError, MainMenuType};
use pokeemerald_rs::title::{TitleScene, TitleSceneError};

use crate::Scene;

mod staging;

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 160;

/// Fixed title frame with the "Press Start" banner visible.
///
/// Any fixed frame is deterministic. A visible banner also makes the capture
/// sensitive to banner regressions.
const TITLE_FRAME_INDEX: u32 = 16;

const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;
const CHANNEL_MASK: u32 = 0xFF;

/// Failure to load, compose, or publish a snapshot.
#[derive(Debug)]
pub enum RecordSnapshotError {
    /// Loading or decoding the asset pack failed.
    Pack(PackError),
    /// Constructing the requested scene from the pack failed.
    Scene(String),
    /// Creating or publishing the capture failed at the given path.
    Write(PathBuf, String),
}

impl fmt::Display for RecordSnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "record-snapshot: {err}"),
            Self::Scene(msg) => write!(f, "record-snapshot: scene failed to build: {msg}"),
            Self::Write(path, msg) => {
                write!(
                    f,
                    "record-snapshot: writing {} failed: {msg}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for RecordSnapshotError {}

impl From<PackError> for RecordSnapshotError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<TitleSceneError> for RecordSnapshotError {
    fn from(err: TitleSceneError) -> Self {
        Self::Scene(err.to_string())
    }
}

impl From<MainMenuSceneError> for RecordSnapshotError {
    fn from(err: MainMenuSceneError) -> Self {
        Self::Scene(err.to_string())
    }
}

/// Paths and provenance for a published capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Published `<generation>/<scene>.rgb` path.
    pub rgb_path: PathBuf,
    /// Published `<generation>/<scene>.meta` path.
    pub meta_path: PathBuf,
    /// RGB payload length in bytes.
    pub payload_len: usize,
    /// Lowercase FNV-1a hash of the RGB payload.
    pub rgb_hash: String,
    /// Lowercase FNV-1a hash of the retained pack bytes.
    pub pack_hash: String,
    /// Source commit with an optional `-dirty` suffix, or `"unknown"`.
    pub git_sha: String,
}

/// Capture `scene` from the default local pack into `<repo root>/snapshots/`.
///
/// # Errors
///
/// See [`run_with_paths`].
pub fn run(scene: Scene) -> Result<Report, RecordSnapshotError> {
    let repo_root = crate::extract::repo_root();
    let (pack_path, output_dir) = default_paths(&repo_root);
    run_with_paths(scene, &pack_path, &output_dir)
}

fn default_paths(repo_root: &Path) -> (PathBuf, PathBuf) {
    (
        repo_root.join(crate::extract::OUTPUT_RELATIVE_PATH),
        repo_root.join("snapshots"),
    )
}

/// Capture `scene` from `pack_path` into a generation under `output_dir`.
///
/// # Errors
///
/// Returns [`RecordSnapshotError::Pack`] for an invalid pack,
/// [`RecordSnapshotError::Scene`] when scene construction fails, and
/// [`RecordSnapshotError::Write`] when generation publication fails.
pub fn run_with_paths(
    scene: Scene,
    pack_path: &Path,
    output_dir: &Path,
) -> Result<Report, RecordSnapshotError> {
    let pack = AssetPack::load(pack_path)?;
    capture_loaded(scene, &pack, output_dir, || Ok(()))
}

fn capture_loaded<F>(
    scene: Scene,
    pack: &AssetPack,
    output_dir: &Path,
    after_rgb_staged: F,
) -> Result<Report, RecordSnapshotError>
where
    F: FnOnce() -> Result<(), RecordSnapshotError>,
{
    let (rgb_bytes, inputs) = compose(scene, pack)?;
    let rgb_hash = format!("{:016x}", fnv1a64(&rgb_bytes));
    let pack_hash = format!("{:016x}", fnv1a64(pack.bytes()));
    let git_sha = git_sha(&crate::extract::repo_root()).unwrap_or_else(|| "unknown".to_owned());

    std::fs::create_dir_all(output_dir)
        .map_err(|e| RecordSnapshotError::Write(output_dir.to_path_buf(), e.to_string()))?;

    let meta = render_meta(scene, &inputs, &rgb_hash, &pack_hash, &git_sha);
    let (rgb_path, meta_path) = publish_generation(
        scene,
        output_dir,
        &rgb_bytes,
        meta.as_bytes(),
        after_rgb_staged,
    )?;

    Ok(Report {
        rgb_path,
        meta_path,
        payload_len: rgb_bytes.len(),
        rgb_hash,
        pack_hash,
        git_sha,
    })
}

fn publish_generation<F>(
    scene: Scene,
    output_dir: &Path,
    rgb_bytes: &[u8],
    meta_bytes: &[u8],
    after_rgb_staged: F,
) -> Result<(PathBuf, PathBuf), RecordSnapshotError>
where
    F: FnOnce() -> Result<(), RecordSnapshotError>,
{
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);
    let (generation, staged_dir, generation_dir, mut staged_dir_claim) = loop {
        let generation = format!(
            "{}.generation-{}-{}",
            scene.name(),
            std::process::id(),
            NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
        );
        let staged_dir = output_dir.join(format!(".{generation}.staged"));
        let generation_dir = output_dir.join(&generation);
        // Cheap early skip only; `std::fs::create_dir`'s exclusivity is the actual guard.
        if generation_dir.exists() {
            continue;
        }
        if let Some(claim) = create_and_claim_staged_dir(&staged_dir, claim_staged_dir)? {
            break (generation, staged_dir, generation_dir, claim);
        }
    };
    let pointer_path = output_dir.join(format!("{}.generation", scene.name()));
    let staged_rgb = staged_dir.join(format!("{}.rgb", scene.name()));
    let staged_meta = staged_dir.join(format!("{}.meta", scene.name()));

    let mut renamed = false;
    let result = (|| {
        std::fs::write(&staged_rgb, rgb_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_rgb.clone(), e.to_string()))?;
        after_rgb_staged()?;
        std::fs::write(&staged_meta, meta_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_meta.clone(), e.to_string()))?;
        // The promoting rename frees the staging name, so the claim can no
        // longer answer for it past this point regardless of whether the
        // rename itself goes on to succeed; on Windows the hold has to be
        // given up first, since it denies the rename the same as anything
        // else.
        staged_dir_claim.release_hold();
        std::fs::rename(&staged_dir, &generation_dir)
            .map_err(|e| RecordSnapshotError::Write(generation_dir.clone(), e.to_string()))?;
        renamed = true;
        // See `staging` for the guard this stage-then-publish pair provides.
        let staged_pointer = stage_pointer(&pointer_path, format!("{generation}\n").as_bytes())
            .map_err(|e| RecordSnapshotError::Write(pointer_path.clone(), e.to_string()))?;
        staged_pointer
            .publish(&pointer_path)
            .map_err(|e| RecordSnapshotError::Write(pointer_path.clone(), e.to_string()))?;
        Ok((
            generation_dir.join(format!("{}.rgb", scene.name())),
            generation_dir.join(format!("{}.meta", scene.name())),
        ))
    })();

    // Neither staging name is removed once it may no longer be exclusively
    // ours: `generation_dir`'s was never claimed, and `staged_dir`'s claim
    // is only trusted while `!renamed` -- the rename attempt gives up the
    // claim's hold before it runs, whether or not it goes on to succeed.
    if result.is_err() && !renamed {
        result.map_err(|source| clean_up_staged_dir(&staged_dir, staged_dir_claim, source))
    } else {
        result
    }
}

/// Creates `staged_dir` exclusively and claims it through `claim`, the pair
/// that together make the directory this publish's to remove later (see
/// [`StagedDirClaim`]). `Ok(None)` means the name was already taken and the
/// caller should move on to the next one.
///
/// A claim that fails after the create leaves `staged_dir` exactly where it
/// is. `create_dir`'s success proves the name was this call's only at the
/// instant it returned, and the claim is what would have carried that proof
/// forward; without it, the name may already answer to a directory another
/// writer put there once it carried this one off, and removing by that name
/// would take the replacement. The directory is named in the reported error
/// instead, since nothing will come back for it.
fn create_and_claim_staged_dir(
    staged_dir: &Path,
    claim: impl FnOnce(&Path) -> std::io::Result<StagedDirClaim>,
) -> Result<Option<StagedDirClaim>, RecordSnapshotError> {
    match std::fs::create_dir(staged_dir) {
        Ok(()) => claim(staged_dir).map(Some).map_err(|error| {
            RecordSnapshotError::Write(
                staged_dir.to_path_buf(),
                format!(
                    "{error}; the staging directory {} was left where it is, since claiming it is what would have proved it still this capture's to remove",
                    staged_dir.display()
                ),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(RecordSnapshotError::Write(
            staged_dir.to_path_buf(),
            error.to_string(),
        )),
    }
}

/// Binds cleanup to the directory object this call created, not to whatever
/// its name may later answer to (see [`claim_staged_dir`]). On Unix the
/// held descriptor is what makes the recorded `(dev, ino)` trustworthy: a
/// deleted-but-still-open inode cannot be handed to a new directory, so a
/// re-stat that still matches cannot be a reused number fooling the check.
///
/// Removal is bound to that object rather than to the name: on Unix
/// [`remove_staged_dir`] renames the verified directory to a private
/// sibling only that cleanup can be naming, reads `(dev, ino)` again
/// there, and removes that name, so a directory put at the staging name
/// after the check is never reached (the `windows` variant below denies
/// that replacement outright through its hold). What stays open is the
/// claim itself: `create_dir` returns no handle, so the open that claims
/// the directory is a second, non-atomic step on every platform this
/// module supports, and neither `std` nor POSIX offers an atomic
/// create-and-open for directories (unlike a file's `create_new`).
#[cfg(unix)]
struct StagedDirClaim {
    dev: u64,
    ino: u64,
    _hold: Option<std::fs::File>,
}

/// As [`StagedDirClaim`] above. Windows has no by-handle identity to read
/// back on demand (unlike Unix's `(dev, ino)`), so the hold is the proof
/// instead: opened with no sharing at all, it denies any other opener a
/// rename or delete of the directory it names for as long as it stays
/// open, so nothing needs to be re-read to know the name still means the
/// same object -- closing the second gap Unix has, since there is no
/// separate re-verify step to race against the removal that follows it.
/// [`StagedDirClaim::release_hold`] gives the hold up right before the
/// promoting rename, which needs exactly the access the hold denies; past
/// that release, ownership can no longer be confirmed at all, and
/// `remove_staged_dir` drops the hold the same way immediately before its
/// own `remove_dir_all`, so that release is this platform's only gap.
#[cfg(windows)]
struct StagedDirClaim {
    hold: Option<std::fs::File>,
}

/// As [`StagedDirClaim`] above, for targets with neither an inode nor a
/// sharing hold to lean on; ownership can never be confirmed here.
#[cfg(not(any(unix, windows)))]
struct StagedDirClaim;

impl StagedDirClaim {
    /// Gives up the hold the promoting rename needs (Windows only; see
    /// [`StagedDirClaim`]). A no-op everywhere else.
    #[cfg(unix)]
    #[expect(
        clippy::unused_self,
        reason = "one signature for every platform; only Windows has a hold to give up"
    )]
    fn release_hold(&mut self) {}

    #[cfg(windows)]
    fn release_hold(&mut self) {
        self.hold.take();
    }

    #[cfg(not(any(unix, windows)))]
    #[expect(
        clippy::unused_self,
        reason = "one signature for every platform; only Windows has a hold to give up"
    )]
    fn release_hold(&mut self) {}
}

/// Opens `path` (a freshly created `staged_dir`) and records the identity
/// that will prove ownership of it at cleanup time.
#[cfg(unix)]
fn claim_staged_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    use std::os::unix::fs::MetadataExt as _;

    match open_directory_hold(path) {
        Ok(hold) => {
            let meta = hold.metadata()?;
            Ok(StagedDirClaim {
                dev: meta.dev(),
                ino: meta.ino(),
                _hold: Some(hold),
            })
        }
        // A directory that allows creation and search but not reading refuses
        // the only open this target has; its identity still names it, without
        // the inode pin a held descriptor adds.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let meta = std::fs::symlink_metadata(path)?;
            Ok(StagedDirClaim {
                dev: meta.dev(),
                ino: meta.ino(),
                _hold: None,
            })
        }
        Err(error) => Err(error),
    }
}

/// Opens a directory for identity and holding alone: `O_PATH`, which asks no
/// read permission of a directory that allows only creation and search, on
/// the Linux targets whose `fcntl.h` shares the generic value for it.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv64"
    )
))]
fn open_directory_hold(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    const O_PATH: i32 = 0o10_000_000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_PATH)
        .open(path)
}

/// As above where `std` offers only a read open of a directory; XNU refuses
/// `O_EXEC` on one with `EISDIR`, so Apple targets take this arm too.
#[cfg(all(
    unix,
    not(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "x86_64",
            target_arch = "x86",
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "riscv64"
        )
    ))
))]
fn open_directory_hold(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

/// As [`claim_staged_dir`] above, for the sharing hold (see
/// [`StagedDirClaim`]). `FILE_FLAG_BACKUP_SEMANTICS` is the documented
/// `CreateFileW` flag that lets a directory be opened at all; `share_mode`
/// `0` is what then denies every other opener, matching
/// `staging::create_new_exclusive`'s use of the same flag for a file.
#[cfg(windows)]
fn claim_staged_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let hold = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .share_mode(0)
        .open(path)?;
    Ok(StagedDirClaim { hold: Some(hold) })
}

/// As [`claim_staged_dir`] above, for targets with no ownership proof to
/// record (see [`StagedDirClaim`]).
#[cfg(not(any(unix, windows)))]
fn claim_staged_dir(_path: &Path) -> std::io::Result<StagedDirClaim> {
    Ok(StagedDirClaim)
}

/// Removes `staged_dir` if `claim` still names the very directory this
/// publish created there, folding a cleanup failure into `source` rather
/// than discarding it. A directory found replaced is left alone -- it
/// belongs to whoever holds it now -- in silence when the check itself saw
/// the replacement, and named in the error when it arrived after.
fn clean_up_staged_dir(
    staged_dir: &Path,
    claim: StagedDirClaim,
    source: RecordSnapshotError,
) -> RecordSnapshotError {
    match remove_staged_dir(staged_dir, claim) {
        Ok(()) | Err(None) => source,
        Err(Some(cleanup_error)) => fold_cleanup_error(source, staged_dir, Some(cleanup_error)),
    }
}

/// Appends `cleanup_error`, if any, to `source` rather than discarding it;
/// `source` alone otherwise.
fn fold_cleanup_error(
    source: RecordSnapshotError,
    path: &Path,
    cleanup_error: Option<std::io::Error>,
) -> RecordSnapshotError {
    let Some(cleanup_error) = cleanup_error else {
        return source;
    };
    RecordSnapshotError::Write(
        path.to_path_buf(),
        format!(
            "{source}; additionally failed to remove the abandoned staging directory {}: {cleanup_error}",
            path.display()
        ),
    )
}

/// Removes `staged_dir` when `claim` still proves ownership of it. `Err(None)`
/// means it does not (definitely replaced, or already gone) and nothing was
/// touched; `Err(Some(_))` means ownership could not be confirmed, or another
/// writer took the name after the check (see [`remove_verified_staged_dir`]),
/// or the removal itself failed, and must be reported rather than swallowed.
#[cfg(unix)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "one signature for every platform; the Windows implementation must own `claim` to drop its hold before removing"
)]
fn remove_staged_dir(
    staged_dir: &Path,
    claim: StagedDirClaim,
) -> Result<(), Option<std::io::Error>> {
    remove_staged_dir_after_check(staged_dir, &claim, || {})
}

/// [`remove_staged_dir`] with `after_check` run between the ownership read
/// and the removal, so a test can install a competitor in exactly that window.
#[cfg(unix)]
fn remove_staged_dir_after_check(
    staged_dir: &Path,
    claim: &StagedDirClaim,
    after_check: impl FnOnce(),
) -> Result<(), Option<std::io::Error>> {
    use std::os::unix::fs::MetadataExt as _;

    match std::fs::symlink_metadata(staged_dir) {
        Ok(found)
            if found.file_type().is_dir()
                && (claim.dev, claim.ino) == (found.dev(), found.ino()) =>
        {
            after_check();
            remove_verified_staged_dir(staged_dir, claim)
        }
        Ok(_) => Err(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(None),
        Err(error) => Err(Some(error)),
    }
}

/// Removes the directory [`remove_staged_dir`]'s check just matched, bound to
/// that directory rather than to the name it was found under: the name is
/// renamed to a private sibling ([`private_cleanup_path`]) that no competing
/// writer can name, `claim`'s identity is read again there, and only that
/// private name is removed. A writer that takes the staging name over between
/// the check and the rename therefore loses nothing -- what the rename moved
/// is its directory, not this one's, so it fails the second read and goes back
/// (see [`report_foreign_staged_dir`]) instead of being removed.
///
/// The private name still stands between that second read and the removal,
/// and no writer following this module's naming ever derives it; anchoring
/// the removal to the descriptor instead would take `unlinkat`, which `std`
/// does not expose.
#[cfg(unix)]
fn remove_verified_staged_dir(
    staged_dir: &Path,
    claim: &StagedDirClaim,
) -> Result<(), Option<std::io::Error>> {
    use std::os::unix::fs::MetadataExt as _;

    let private = private_cleanup_path(staged_dir, claim);
    match std::fs::rename(staged_dir, &private) {
        Ok(()) => {}
        // The name answered to nothing by the time the rename ran, so this
        // publish's directory had already been carried off; whatever comes
        // back to that name is another writer's.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Some(std::io::Error::other(format!(
                "the staging directory {} was carried off before cleanup could take hold of it, so it was left in place",
                staged_dir.display()
            ))));
        }
        Err(error) => return Err(Some(error)),
    }
    match std::fs::symlink_metadata(&private) {
        Ok(found)
            if found.file_type().is_dir()
                && (claim.dev, claim.ino) == (found.dev(), found.ino()) =>
        {
            std::fs::remove_dir_all(&private).map_err(|error| {
                Some(std::io::Error::new(
                    error.kind(),
                    format!(
                        "removing the staging directory failed after it was moved to {}, where it was left: {error}",
                        private.display()
                    ),
                ))
            })
        }
        _ => Err(Some(report_foreign_staged_dir(staged_dir, &private))),
    }
}

/// Puts back what [`remove_verified_staged_dir`]'s rename turned out to have
/// moved -- a competing writer's directory, which took the staging name
/// between the ownership check and that rename -- and says where it ended up.
#[cfg(unix)]
fn report_foreign_staged_dir(staged_dir: &Path, private: &Path) -> std::io::Error {
    if restore_foreign_staged_dir(staged_dir, private) {
        return std::io::Error::other(format!(
            "another writer's directory stood at {} by the time cleanup took hold of it, so it was left in place",
            staged_dir.display()
        ));
    }
    std::io::Error::other(format!(
        "another writer's directory stood at {} by the time cleanup took hold of it, and that name was taken again before it could go back, so it was left in place at {}",
        staged_dir.display(),
        private.display()
    ))
}

/// Puts the directory under `private` back at `staged_dir`, reporting whether
/// it got there. `create_dir` is what makes that name free rather than merely
/// observed free: it refuses a name a third writer has taken, and holds it
/// against one arriving next, so the `rename` that follows replaces this
/// placeholder alone -- a bare "does anything answer to it" check would leave
/// `rename` free to replace that writer's own directory.
#[cfg(unix)]
fn restore_foreign_staged_dir(staged_dir: &Path, private: &Path) -> bool {
    if std::fs::create_dir(staged_dir).is_err() {
        return false;
    }
    if std::fs::rename(private, staged_dir).is_ok() {
        return true;
    }
    // `remove_dir` takes the placeholder back out and refuses anything else.
    let _ = std::fs::remove_dir(staged_dir);
    false
}

/// A sibling name for `staged_dir` that only this cleanup can be naming: the
/// staging name plus this process's id, the claimed directory's inode, and the
/// wall clock. Two cleanups at once hold two live directories, which cannot
/// share an inode number; two in sequence cannot share a clock reading.
#[cfg(unix)]
fn private_cleanup_path(staged_dir: &Path, claim: &StagedDirClaim) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut name = staged_dir.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".cleanup-{}-{}-{nanos}",
        std::process::id(),
        claim.ino
    ));
    staged_dir.with_file_name(name)
}

/// As [`remove_staged_dir`] above: on Windows there is no path to re-verify,
/// since `claim`'s hold is itself the proof (see [`StagedDirClaim`]) -- for
/// as long as it stayed held, no rename or delete of `staged_dir` could
/// have succeeded, so the name still means the directory this publish
/// created. A claim whose hold is already gone cannot prove that, and is
/// reported rather than guessed at.
#[cfg(windows)]
fn remove_staged_dir(
    staged_dir: &Path,
    mut claim: StagedDirClaim,
) -> Result<(), Option<std::io::Error>> {
    let Some(hold) = claim.hold.take() else {
        return Err(Some(std::io::Error::other(
            "the staging directory's sharing hold was already released for the promoting rename, so its ownership can no longer be confirmed",
        )));
    };
    // The hold itself denies the removal below; give it up first.
    drop(hold);
    std::fs::remove_dir_all(staged_dir).map_err(Some)
}

/// As [`remove_staged_dir`] above, for targets with no ownership proof to
/// check (see [`StagedDirClaim`]): cleanup can never be confirmed safe, so
/// it never runs.
#[cfg(not(any(unix, windows)))]
fn remove_staged_dir(
    staged_dir: &Path,
    _claim: StagedDirClaim,
) -> Result<(), Option<std::io::Error>> {
    Err(Some(std::io::Error::other(format!(
        "this target has no way to confirm ownership of the staging directory {}, so it was left in place",
        staged_dir.display()
    ))))
}

/// Hex width of the pointer staging suffix, matching [`crate::extract`]'s
/// unpredictable staging names for the identical risk (F-3).
const POINTER_STAGING_HEX_DIGITS: usize = 10;

/// How many unpredictable pointer staging names one publish tries before
/// giving up on a genuine collision; a planted name is refused outright.
const POINTER_STAGING_ATTEMPTS: usize = 256;

/// Stages the pointer at an unguessable sibling of `pointer_path`, retrying
/// through [`pointer_staging_candidates`] on a genuine name collision.
fn stage_pointer(pointer_path: &Path, bytes: &[u8]) -> std::io::Result<staging::StagedFile> {
    stage_pointer_with_candidates(bytes, pointer_staging_candidates(pointer_path))
}

/// Stages `bytes` at the first of `candidates` that `staging::stage` finds
/// free, reporting the last collision once they have all turned out taken.
fn stage_pointer_with_candidates(
    bytes: &[u8],
    candidates: impl IntoIterator<Item = PathBuf>,
) -> std::io::Result<staging::StagedFile> {
    let mut last_collision = None;
    for candidate in candidates {
        match staging::stage(&candidate, bytes) {
            Ok(staged) => return Ok(staged),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last_collision = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_collision.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exhausted pointer staging attempts",
        )
    }))
}

/// The unpredictable sibling names one pointer publish walks, in the order
/// tried.
fn pointer_staging_candidates(pointer_path: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let mask = pointer_staging_value_mask();
    let mut value = pointer_staging_unique_value();
    std::iter::repeat_with(move || {
        let candidate = pointer_staging_path_with_value(pointer_path, value);
        value = value.wrapping_add(1) & mask;
        candidate
    })
    .take(POINTER_STAGING_ATTEMPTS)
}

/// Renders the sibling name for one candidate `value`, `.tmp.<hex>` suffixed
/// onto `pointer_path`'s own name.
fn pointer_staging_path_with_value(pointer_path: &Path, value: u64) -> PathBuf {
    let mut name = pointer_path.as_os_str().to_os_string();
    name.push(format!(".tmp.{value:0POINTER_STAGING_HEX_DIGITS$x}"));
    PathBuf::from(name)
}

/// `std`-only entropy folded into one value: process id, clock nanoseconds,
/// and a fresh `RandomState` key.
fn pointer_staging_unique_value() -> u64 {
    use std::hash::{BuildHasher, Hasher};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(nanos);
    hasher.finish() & pointer_staging_value_mask()
}

/// Every value [`POINTER_STAGING_HEX_DIGITS`] hex digits can render, and no
/// other.
fn pointer_staging_value_mask() -> u64 {
    (1_u64 << (4 * POINTER_STAGING_HEX_DIGITS)) - 1
}

fn compose(
    scene: Scene,
    pack: &AssetPack,
) -> Result<(Vec<u8>, Vec<&'static str>), RecordSnapshotError> {
    match scene {
        Scene::Title => {
            let title = TitleScene::from_pack(pack)?;
            let frame = title.compose_frame(TITLE_FRAME_INDEX);
            Ok((frame_to_rgb_bytes(frame.as_slice()), Vec::new()))
        }
        Scene::MainMenuNewGame => {
            let menu = MainMenuScene::from_pack(pack, MainMenuType::NoSavedGame)?;
            let frame = menu.compose_frame();
            Ok((frame_to_rgb_bytes(frame.as_slice()), Vec::new()))
        }
        Scene::MainMenuOption => {
            let mut menu = MainMenuScene::from_pack(pack, MainMenuType::NoSavedGame)?;
            menu.move_down();
            let frame = menu.compose_frame();
            Ok((frame_to_rgb_bytes(frame.as_slice()), vec!["DPAD_DOWN"]))
        }
    }
}

fn frame_to_rgb_bytes(frame: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(frame.len() * 3);
    for &pixel in frame {
        bytes.push(u8::try_from((pixel >> RED_SHIFT) & CHANNEL_MASK).unwrap_or(0));
        bytes.push(u8::try_from((pixel >> GREEN_SHIFT) & CHANNEL_MASK).unwrap_or(0));
        bytes.push(u8::try_from(pixel & CHANNEL_MASK).unwrap_or(0));
    }
    bytes
}

fn render_meta(
    scene: Scene,
    inputs: &[&str],
    rgb_hash: &str,
    pack_hash: &str,
    git_sha: &str,
) -> String {
    let inputs = if inputs.is_empty() {
        "none".to_owned()
    } else {
        inputs.join(",")
    };
    format!(
        "scene: {}\nwidth: {SCREEN_WIDTH}\nheight: {SCREEN_HEIGHT}\npixel_format: rgb888\ninputs: {inputs}\nrgb_hash: fnv1a64:{rgb_hash}\npack_hash: fnv1a64:{pack_hash}\ngit_sha: {git_sha}\n",
        scene.name(),
    )
}

/// FNV-1a 64-bit change locator. Exact bytes, not this hash, establish equality.
#[must_use]
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn git_sha(repo_root: &Path) -> Option<String> {
    let sha = git_output(repo_root, &["rev-parse", "HEAD"])?;
    let sha = sha.trim();
    if sha.is_empty() {
        return None;
    }
    let worktree_dirty_or_unknown = git_output(repo_root, &["status", "--porcelain"])
        .is_none_or(|status| !status.trim().is_empty());
    if worktree_dirty_or_unknown {
        Some(format!("{sha}-dirty"))
    } else {
        Some(sha.to_owned())
    }
}

fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests;
