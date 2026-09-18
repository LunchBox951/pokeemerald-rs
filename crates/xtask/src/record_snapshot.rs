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
    let (generation, staged_dir, generation_dir) = loop {
        let generation = format!(
            "{}.generation-{}-{}",
            scene.name(),
            std::process::id(),
            NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
        );
        let staged_dir = output_dir.join(format!(".{generation}.staged"));
        let generation_dir = output_dir.join(&generation);
        // Cheap early skip only; [`refuse_a_taken_generation_name`] is the actual guard.
        if generation_dir.exists() {
            continue;
        }
        match std::fs::create_dir(&staged_dir) {
            Ok(()) => break (generation, staged_dir, generation_dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(RecordSnapshotError::Write(staged_dir, error.to_string()));
            }
        }
    };
    let pointer_path = output_dir.join(format!("{}.generation", scene.name()));
    let staged_rgb = staged_dir.join(format!("{}.rgb", scene.name()));
    let staged_meta = staged_dir.join(format!("{}.meta", scene.name()));
    let generation_rgb = generation_dir.join(format!("{}.rgb", scene.name()));
    let generation_meta = generation_dir.join(format!("{}.meta", scene.name()));

    // No payload is ever written through `generation_dir`: both reach that
    // name inside `staged_dir`, in the rename below. `mkdir` hands back no
    // descriptor, so a directory created at the public name and reopened by
    // it could not be told from a competitor's substitute.
    let mut generation_claim: Option<GenerationDirClaim> = None;
    let result = (|| {
        std::fs::write(&staged_rgb, rgb_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_rgb.clone(), e.to_string()))?;
        after_rgb_staged()?;
        std::fs::write(&staged_meta, meta_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_meta.clone(), e.to_string()))?;
        let staged_hold = hold_staged_dir(&staged_dir)
            .map_err(|e| RecordSnapshotError::Write(staged_dir.clone(), e.to_string()))?;
        refuse_a_taken_generation_name(&generation_dir)?;
        std::fs::rename(&staged_dir, &generation_dir)
            .map_err(|e| RecordSnapshotError::Write(generation_dir.clone(), e.to_string()))?;
        let claim = claim_generation_dir(&generation_dir, staged_hold).map_err(|e| {
            RecordSnapshotError::Write(
                generation_dir.clone(),
                format!("the promoted generation is left in place, unremovable by this call because its ownership could not be proven: {e}"),
            )
        })?;
        let claim = generation_claim.insert(claim);
        // See `staging` for the guard this stage-then-publish pair provides.
        let staged_pointer = stage_pointer(&pointer_path, format!("{generation}\n").as_bytes())
            .map_err(|e| RecordSnapshotError::Write(pointer_path.clone(), e.to_string()))?;
        require_generation_claim(claim, &generation_dir)?;
        staged_pointer
            .publish(&pointer_path)
            .map_err(|e| RecordSnapshotError::Write(pointer_path.clone(), e.to_string()))?;
        Ok((generation_rgb, generation_meta))
    })();

    result.map_err(|source| {
        clean_up_generation(&staged_dir, &generation_dir, generation_claim, source)
    })
}

/// Refuses `generation_dir` outright when anything at all already answers
/// to that name -- a directory, a file, or a symlink -- so a competing
/// writer's entry is reported rather than promoted over.
///
/// POSIX `rename(2)` replaces an empty destination directory, so the bare
/// rename that promotes `staged_dir` cannot be trusted to keep a competing
/// writer's directory. This check runs as tightly before that rename as
/// `std` allows, which leaves one irreducible gap: a directory created at
/// the name between the two calls is still replaced. Closing it needs a
/// rename that refuses an existing destination in the same syscall --
/// Linux `renameat2(RENAME_NOREPLACE)`, Apple `renameatx_np(RENAME_EXCL)`
/// -- which `std` does not expose.
fn refuse_a_taken_generation_name(generation_dir: &Path) -> Result<(), RecordSnapshotError> {
    match std::fs::symlink_metadata(generation_dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(RecordSnapshotError::Write(
            generation_dir.to_path_buf(),
            "a competing writer already owns this generation name".to_owned(),
        )),
        Err(error) => Err(RecordSnapshotError::Write(
            generation_dir.to_path_buf(),
            format!("the generation name could not be read before promotion: {error}"),
        )),
    }
}

/// Confirms `generation_dir` still names the directory `claim` was made
/// for, refusing rather than following a symlink or any other replacement
/// found at that name.
fn require_generation_claim(
    claim: &GenerationDirClaim,
    generation_dir: &Path,
) -> Result<(), RecordSnapshotError> {
    match claim.still_holds(generation_dir) {
        Ok(true) => Ok(()),
        Ok(false) => Err(RecordSnapshotError::Write(
            generation_dir.to_path_buf(),
            "a competing writer replaced generation_dir after this call claimed it".to_owned(),
        )),
        Err(error) => Err(RecordSnapshotError::Write(
            generation_dir.to_path_buf(),
            format!("generation_dir's ownership could not be confirmed: {error}"),
        )),
    }
}

/// Cleans up after a failed publish: `staged_dir` is always attempted
/// (ownership of that name is [`staging`]'s concern, not this function's),
/// while `generation_dir` is only removed once `claim` still confirms this
/// call owns it -- a directory a competing writer took back in the
/// meantime is left exactly as found, not guessed at. Any cleanup failure
/// is folded into `source` rather than discarded, an ownership read that
/// could not be completed included: it leaves the generation on disk just
/// as a failed removal would.
///
/// A successful promotion leaves no `staged_dir` behind, so only its
/// absence is passed over silently.
fn clean_up_generation(
    staged_dir: &Path,
    generation_dir: &Path,
    mut generation_claim: Option<GenerationDirClaim>,
    source: RecordSnapshotError,
) -> RecordSnapshotError {
    let staged_cleanup_error = std::fs::remove_dir_all(staged_dir)
        .err()
        .filter(|error| error.kind() != std::io::ErrorKind::NotFound);
    let error = fold_cleanup_error(source, staged_dir, staged_cleanup_error);
    let Some(claim) = &mut generation_claim else {
        return error;
    };
    match claim.still_holds(generation_dir) {
        Ok(true) => {}
        Ok(false) => return error,
        Err(read_error) => {
            return RecordSnapshotError::Write(
                generation_dir.to_path_buf(),
                format!(
                    "{error}; additionally left {} behind: its ownership could not be confirmed: {read_error}",
                    generation_dir.display()
                ),
            );
        }
    }
    // The hold itself would deny this removal on Windows; give it up first
    // (see `GenerationDirClaim`). A no-op on Unix.
    claim.release_hold();
    let generation_cleanup_error = std::fs::remove_dir_all(generation_dir).err();
    fold_cleanup_error(error, generation_dir, generation_cleanup_error)
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
            "{source}; additionally failed to remove {}: {cleanup_error}",
            path.display()
        ),
    )
}

/// The identity a directory is recognized by once it has been renamed to
/// another name (see [`hold_staged_dir`] and [`claim_generation_dir`]).
#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirIdentity {
    dev: u64,
    ino: u64,
}

/// A descriptor on `staged_dir` taken before the promoting rename and kept
/// open across it, with the identity read through it.
///
/// The open descriptor is what makes that identity conclusive: an inode
/// with a live descriptor on it cannot be freed and handed to a directory
/// a competitor creates later, so an entry that matches after the rename
/// is this call's own directory rather than a reused number.
#[cfg(unix)]
struct StagedDirHold {
    identity: DirIdentity,
    hold: std::fs::File,
}

/// As [`StagedDirHold`] above, for targets whose directories carry no
/// identity a descriptor held across the rename could pin.
#[cfg(not(unix))]
struct StagedDirHold;

/// Opens `path` and reads the identity it will still be recognizable by
/// after the promoting rename has given it a different name.
#[cfg(unix)]
fn hold_staged_dir(path: &Path) -> std::io::Result<StagedDirHold> {
    use std::os::unix::fs::MetadataExt as _;

    let hold = std::fs::File::open(path)?;
    let found = hold.metadata()?;
    if !found.file_type().is_dir() {
        return Err(std::io::Error::other(
            "the staging name stopped denoting a directory before promotion",
        ));
    }
    Ok(StagedDirHold {
        identity: DirIdentity {
            dev: found.dev(),
            ino: found.ino(),
        },
        hold,
    })
}

/// As [`hold_staged_dir`] above, for targets with nothing to hold.
#[cfg(not(unix))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for every platform; only the unix arm opens anything that could fail"
)]
fn hold_staged_dir(_path: &Path) -> std::io::Result<StagedDirHold> {
    Ok(StagedDirHold)
}

/// Binds the pointer publication and the failure cleanup to the directory
/// object this call promoted, not merely to the name it now answers to (see
/// [`require_generation_claim`] and [`clean_up_generation`]).
///
/// On Unix it is [`StagedDirHold`]'s descriptor, carried over here, that
/// keeps the recorded [`DirIdentity`] conclusive for as long as the claim
/// lives.
#[cfg(unix)]
struct GenerationDirClaim {
    identity: DirIdentity,
    _hold: std::fs::File,
}

/// As [`GenerationDirClaim`] above. Stable `std` exposes no by-handle
/// identity on Windows (`MetadataExt::file_index` is unstable), so this
/// platform cannot compare the promoted directory against the staged one
/// and the hold proves a narrower thing: opened with no sharing at all, it
/// denies every other opener a rename or delete of whatever directory it
/// opened, from the open onwards. A competitor that takes the name in the
/// window between the promoting rename and that open is therefore not
/// detected here, the one part of this guard Windows does not get.
#[cfg(windows)]
struct GenerationDirClaim {
    hold: Option<std::fs::File>,
}

/// As [`GenerationDirClaim`] above, for targets with neither an inode nor a
/// sharing hold to lean on; ownership can never be confirmed here.
#[cfg(not(any(unix, windows)))]
struct GenerationDirClaim;

impl GenerationDirClaim {
    /// Whether `path` still names the very directory this claim was made
    /// for. A symlink or any other non-matching entry is refused, not
    /// followed.
    #[cfg(unix)]
    fn still_holds(&self, path: &Path) -> std::io::Result<bool> {
        use std::os::unix::fs::MetadataExt as _;

        let found = std::fs::symlink_metadata(path)?;
        let found_identity = DirIdentity {
            dev: found.dev(),
            ino: found.ino(),
        };
        Ok(found.file_type().is_dir() && found_identity == self.identity)
    }

    /// As above: for as long as the hold lives, it denies every other
    /// opener, so the name cannot have come to mean anything else.
    #[cfg(windows)]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "one signature for every platform; only the unix arm can fail to read an identity"
    )]
    fn still_holds(&self, _path: &Path) -> std::io::Result<bool> {
        Ok(self.hold.is_some())
    }

    /// As above, for targets with no ownership proof to check.
    #[cfg(not(any(unix, windows)))]
    #[expect(
        clippy::unnecessary_wraps,
        clippy::unused_self,
        reason = "one signature for every platform; this target can never confirm ownership"
    )]
    fn still_holds(&self, _path: &Path) -> std::io::Result<bool> {
        Ok(false)
    }

    /// Gives up the hold so the directory it names can be removed (Windows
    /// only; see [`GenerationDirClaim`]). A no-op everywhere else.
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

/// Turns `staged`'s descriptor into the claim on the just-promoted `path`,
/// refusing unless the entry now found there is the very directory that
/// descriptor names -- a competing writer can have taken the name back by
/// renaming this call's directory away, and the rename that promoted it
/// says nothing about what the name means afterwards.
///
/// A symlink or any other replacement is refused rather than followed: the
/// check reads `symlink_metadata`, whose answer for a symlink is the link's
/// own and never its target's.
#[cfg(unix)]
fn claim_generation_dir(path: &Path, staged: StagedDirHold) -> std::io::Result<GenerationDirClaim> {
    let claim = GenerationDirClaim {
        identity: staged.identity,
        _hold: staged.hold,
    };
    if !claim.still_holds(path)? {
        return Err(std::io::Error::other(
            "a competing writer took the generation name between this call's promotion and its claim",
        ));
    }
    Ok(claim)
}

/// As [`claim_generation_dir`] above, for the sharing hold (see
/// [`GenerationDirClaim`]). `FILE_FLAG_BACKUP_SEMANTICS` is the documented
/// `CreateFileW` flag that lets a directory be opened at all; `share_mode`
/// `0` is what then denies every other opener, matching
/// `staging::create_new_exclusive`'s use of the same flag for a file.
/// `staged` is unusable here: with no by-handle identity to compare it
/// against, the hold, and not a comparison, is this platform's proof.
#[cfg(windows)]
fn claim_generation_dir(
    path: &Path,
    _staged: StagedDirHold,
) -> std::io::Result<GenerationDirClaim> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let hold = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .share_mode(0)
        .open(path)?;
    Ok(GenerationDirClaim { hold: Some(hold) })
}

/// As [`claim_generation_dir`] above, for targets with no ownership proof
/// to record.
#[cfg(not(any(unix, windows)))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for every platform; this target opens nothing that could fail"
)]
fn claim_generation_dir(
    _path: &Path,
    _staged: StagedDirHold,
) -> std::io::Result<GenerationDirClaim> {
    Ok(GenerationDirClaim)
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
