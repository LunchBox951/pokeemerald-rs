//! `cargo xtask record-snapshot` (F-3, V-4, issue #226): a deterministic
//! frame capture of one named scene, plus the metadata a human reviewer
//! needs to bless it.
//!
//! # Why this exists
//!
//! V-4 requires visual snapshots "blessed via review, not hash alone" —
//! every presentation slice up to issue #216 (main menu) has had to fall
//! back on synthetic-pack pixel assertions plus an `#[ignore]`d real-pack
//! determinism check, with no artifact an operator could actually look at
//! (see PR #223's "V-4 note"). This subcommand is that artifact.
//!
//! # What it drives
//!
//! [`Scene`] (defined in `crate` — the crate root, not here — because
//! `parse`/`Command::RecordSnapshot` must route it even when this whole
//! module is compiled out, mirroring how [`crate::Suite`] lives outside
//! the feature-gated [`crate::e2e`]) names one of the real headless scenes
//! `pokeemerald-rs` already exposes: the title screen
//! (`pokeemerald_rs::title`) and the no-save main menu (`pokeemerald_rs::main_menu`,
//! issue #216) in each of its two selectable states. Composition is
//! reached through each scene's `from_pack` constructor, not its
//! `load_default` convenience — [`run`] loads the pack itself via
//! [`assets::pack::AssetPack::load`] so tests can point it at a scratch
//! synthetic pack instead of the one real, developer-local pack path
//! (mirrors `pokeemerald_rs::main_menu::tests`' own synthetic-pack style).
//!
//! No scene here takes a scripted input sequence yet — that is the
//! `scenario` subcommand's job (still `NotImplemented`, module docs of
//! `crate`). [`Scene::MainMenuOption`] records the one `DPAD_DOWN` press
//! its selection state implies as its `inputs` metadata (below); every
//! other scene's `inputs` list is empty, honestly reflecting a static
//! boot-state capture rather than a replayed session.
//!
//! # Capture format — one generation pointer, two payload files
//!
//! A PNG encoder would be a new Cargo dependency (`minimal-deps` forbids
//! that without owner approval), so [`run`] writes raw pixel bytes instead:
//!
//! - `<scene>.generation` — the atomically replaced commit point naming one
//!   complete generation directory.
//! - `<generation>/<scene>.rgb` — [`SCREEN_WIDTH`] x [`SCREEN_HEIGHT`]
//!   pixels, row-major, 3 bytes per pixel (R, G, B) — the composed frame's
//!   `platform::Frame` `0x00RRGGBB` `u32`s split into their three
//!   channel bytes (never naming `platform::Frame` directly, mirroring
//!   `crate::e2e`'s own "stay on the `pokeemerald-rs` dependency, not
//!   `platform`" reasoning).
//! - `<generation>/<scene>.meta` — plain UTF-8 text, one `key: value` line
//!   per field, in a fixed order (see [`render_meta`]): `scene`, `width`, `height`,
//!   `pixel_format`, `inputs`, `rgb_hash`, `pack_hash`, `git_sha`.
//!
//! Deliberately two payload files rather than one framed container: a
//! reviewer (or `diff`/`cmp`) can compare pixel payloads byte-for-byte without parsing
//! anything. `rgb_hash` — [`fnv1a64`] of the `.rgb` file's own bytes — is
//! the value `docs/snapshots.md`'s blessing workflow actually tracks per
//! scene, independent of `pack_hash` (provenance: which pack state produced
//! it) and `git_sha` (which changes every commit even when the pixels do
//! not).
//!
//! # Determinism
//!
//! In a clean worktree, both payload files' bytes are a pure function of the
//! loaded pack bytes and selected [`Scene`], plus the ambient git SHA — no
//! timestamp, RNG, or wall-clock read. A dirty capture records only a generic
//! `-dirty` marker and is not promised to be reproducible from the named
//! commit. A complete versioned pair is staged first and becomes visible only
//! when its single generation pointer is atomically replaced, so interruption
//! before that commit point preserves the previous visible pair
//! ([`run_with_paths`]). `rgb_hash`/`pack_hash` are both
//! [`fnv1a64`] (a small, owned, `std`-only hash — not a `sha2` dependency
//! this module has no cryptographic need for); they are change-detection
//! signals, not proof of byte identity. `pack_hash` covers the exact retained
//! buffer used to compose the scene, not a second read of its path, and not
//! just the entries this scene reads: two packs that
//! decode identically for one scene but differ elsewhere must still be told
//! apart, since a future bug in an unrelated extractor could otherwise go
//! unnoticed by this capture's own hash.

use std::fmt;
use std::path::{Path, PathBuf};

use assets::pack::{AssetPack, PackError};
use pokeemerald_rs::main_menu::{MainMenuScene, MainMenuSceneError};
use pokeemerald_rs::title::{TitleScene, TitleSceneError};

use crate::Scene;

/// The GBA's native frame resolution — mirrors `platform::GBA_WIDTH`, kept
/// as a local literal rather than a `platform` dependency (module docs).
const SCREEN_WIDTH: usize = 240;
/// See [`SCREEN_WIDTH`].
const SCREEN_HEIGHT: usize = 160;

/// The title screen's frame index this scene captures. Frame 16 sits in
/// the *visible* half of the "Press Start" blink cadence
/// (`pokeemerald_rs::title`'s `press_start_visible`: visible for frames
/// 15–30 of every 32-frame period, invisible outside it) — frame 0 does
/// not, and a capture with the banner hidden could never catch a banner
/// regression: the blessing workflow would hash-match a title screen whose
/// "Press Start" text was broken outright. Any fixed index is equally
/// deterministic (every animated quantity is a pure function of the frame
/// number), so the one that maximises what the capture can witness wins.
const TITLE_FRAME_INDEX: u32 = 16;

/// Why [`run`]/[`run_with_paths`] failed.
///
/// Concrete per-module enum `(oop-boundaries)`; no `anyhow`.
#[derive(Debug)]
pub enum RecordSnapshotError {
    /// Loading the pack failed — most commonly [`PackError::NotFound`]
    /// (no local pack; run `cargo xtask extract` first).
    Pack(PackError),
    /// The pack loaded, but building the requested scene out of it failed.
    /// Carries the scene error's rendered message (`TitleSceneError` or
    /// `MainMenuSceneError` — folded to a `String` here so this enum does
    /// not need a variant per scene type).
    Scene(String),
    /// Creating the output directory, or writing the `.rgb`/`.meta` file,
    /// failed. Carries the path and the underlying error's rendered
    /// message.
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

/// A completed capture: where its two files landed, and the metadata
/// written alongside the pixels — returned so `crate::dispatch` can print
/// a summary without re-reading either file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The written `<scene>.rgb` path.
    pub rgb_path: PathBuf,
    /// The written `<scene>.meta` path.
    pub meta_path: PathBuf,
    /// The captured payload's length in bytes (`SCREEN_WIDTH * SCREEN_HEIGHT * 3`).
    pub payload_len: usize,
    /// [`fnv1a64`] of the written `.rgb` file's own bytes, lowercase hex —
    /// the value the blessing workflow (`docs/snapshots.md`) tracks per
    /// scene.
    pub rgb_hash: String,
    /// [`fnv1a64`] of the whole pack file's bytes, lowercase hex.
    pub pack_hash: String,
    /// The current `git rev-parse HEAD`, suffixed `-dirty` when the
    /// worktree has uncommitted changes (see [`git_sha`]), or `"unknown"`
    /// if it could not be determined (module docs: never a stand-in for
    /// wall-clock data, just a best-effort provenance string).
    pub git_sha: String,
}

/// Capture [`scene`](Scene) against the default local pack
/// (`crate::extract::repo_root().join(crate::extract::OUTPUT_RELATIVE_PATH)`)
/// and write it under `<repo root>/snapshots/`.
///
/// # Errors
///
/// See [`run_with_paths`].
pub fn run(scene: Scene) -> Result<Report, RecordSnapshotError> {
    let repo_root = crate::extract::repo_root();
    let (pack_path, output_dir) = default_paths(&repo_root);
    run_with_paths(scene, &pack_path, &output_dir)
}

/// Derive [`run`]'s two repository-relative paths without reading or writing
/// either location.
fn default_paths(repo_root: &Path) -> (PathBuf, PathBuf) {
    (
        repo_root.join(crate::extract::OUTPUT_RELATIVE_PATH),
        repo_root.join("snapshots"),
    )
}

/// [`run`], parameterized over the pack and output paths — split out so
/// tests can point at a scratch synthetic pack and a scratch output
/// directory instead of the one real pack path / the repo's own
/// `snapshots/` directory (mirrors `crate::extract::extract_to` being
/// split out from `crate::extract::run` for the identical reason).
///
/// # Errors
///
/// [`RecordSnapshotError::Pack`] if `pack_path` does not exist or is not a
/// well-formed pack; [`RecordSnapshotError::Scene`] if the pack loads but
/// lacks (or misdecodes) the entries `scene` needs;
/// [`RecordSnapshotError::Write`] if `output_dir` cannot be created or the
/// generation cannot be staged or committed.
pub fn run_with_paths(
    scene: Scene,
    pack_path: &Path,
    output_dir: &Path,
) -> Result<Report, RecordSnapshotError> {
    let pack = AssetPack::load(pack_path)?;
    capture_loaded(scene, &pack, output_dir, || Ok(()))
}

/// Compose and publish from one already-loaded pack. The hook runs after the
/// RGB payload is staged but before metadata or the generation pointer; tests
/// use it to prove that failure at that boundary cannot change visibility.
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

/// Stage an entire generation and publish it by atomically replacing one
/// pointer. Versioned directories are never modified after publication, so a
/// reader that resolves the pointer sees two files from one generation.
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
    let (generation, staged_dir, generation_dir, pointer_tmp) = loop {
        let generation = format!(
            "{}.generation-{}-{}",
            scene.name(),
            std::process::id(),
            NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
        );
        let staged_dir = output_dir.join(format!(".{generation}.staged"));
        let generation_dir = output_dir.join(&generation);
        let pointer_tmp = output_dir.join(format!(".{generation}.pointer"));
        if generation_dir.exists() || pointer_tmp.exists() {
            continue;
        }
        match std::fs::create_dir(&staged_dir) {
            Ok(()) => break (generation, staged_dir, generation_dir, pointer_tmp),
            // Fall through to the next iteration (a bare `continue` here is
            // `clippy::needless_continue`).
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(RecordSnapshotError::Write(staged_dir, error.to_string()));
            }
        }
    };
    let pointer_path = output_dir.join(format!("{}.generation", scene.name()));
    let staged_rgb = staged_dir.join(format!("{}.rgb", scene.name()));
    let staged_meta = staged_dir.join(format!("{}.meta", scene.name()));

    let result = (|| {
        std::fs::write(&staged_rgb, rgb_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_rgb.clone(), e.to_string()))?;
        after_rgb_staged()?;
        std::fs::write(&staged_meta, meta_bytes)
            .map_err(|e| RecordSnapshotError::Write(staged_meta.clone(), e.to_string()))?;
        std::fs::rename(&staged_dir, &generation_dir)
            .map_err(|e| RecordSnapshotError::Write(generation_dir.clone(), e.to_string()))?;
        std::fs::write(&pointer_tmp, format!("{generation}\n"))
            .map_err(|e| RecordSnapshotError::Write(pointer_tmp.clone(), e.to_string()))?;
        std::fs::rename(&pointer_tmp, &pointer_path)
            .map_err(|e| RecordSnapshotError::Write(pointer_path.clone(), e.to_string()))?;
        Ok((
            generation_dir.join(format!("{}.rgb", scene.name())),
            generation_dir.join(format!("{}.meta", scene.name())),
        ))
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&pointer_tmp);
        let _ = std::fs::remove_dir_all(&staged_dir);
        let _ = std::fs::remove_dir_all(&generation_dir);
    }
    result
}

/// Build `scene` out of `pack`, compose its one captured frame, and pack it
/// straight to `.rgb` payload bytes (module docs), plus the list of input
/// names that state implies.
///
/// Converts to bytes inline, in each match arm, rather than returning the
/// composed frame itself: both scenes' `compose_frame` return a
/// `Box<platform::Frame>` (a fixed-size boxed array), and this crate never
/// names `platform::Frame` directly (mirrors `crate::e2e`'s identical
/// choice) — a function signature has no `-> impl Trait` inference to lean
/// on the way a closure would, so the only way to keep that type unnamed
/// here is to never let it escape this function at all.
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
            let menu = MainMenuScene::from_pack(pack)?;
            let frame = menu.compose_frame();
            Ok((frame_to_rgb_bytes(frame.as_slice()), Vec::new()))
        }
        Scene::MainMenuOption => {
            let mut menu = MainMenuScene::from_pack(pack)?;
            menu.move_down();
            let frame = menu.compose_frame();
            Ok((frame_to_rgb_bytes(frame.as_slice()), vec!["DPAD_DOWN"]))
        }
    }
}

/// Pack each `0x00RRGGBB` pixel into 3 bytes (R, G, B), row-major — the
/// `.rgb` file's payload.
fn frame_to_rgb_bytes(frame: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(frame.len() * 3);
    for &pixel in frame {
        bytes.push(u8::try_from((pixel >> 16) & 0xFF).unwrap_or(0));
        bytes.push(u8::try_from((pixel >> 8) & 0xFF).unwrap_or(0));
        bytes.push(u8::try_from(pixel & 0xFF).unwrap_or(0));
    }
    bytes
}

/// Render the `.meta` file's fixed-order `key: value` lines (module docs).
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

/// FNV-1a, 64-bit: a tiny, owned, `std`-only, non-cryptographic hash
/// (module docs explain why this is enough here) over `bytes`.
///
/// The constants are the published FNV-1a 64-bit offset basis and prime —
/// translating published constants is fine (`no-verbatim` only bars
/// transliterating upstream *behaviour* line-for-line; this is a
/// general-purpose public-domain hash algorithm, not `pokeemerald`/`mgba`
/// source).
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

/// Best-effort `git rev-parse HEAD` against `repo_root`, trimmed, with
/// `-dirty` appended when the worktree has uncommitted changes — or when
/// `git status` itself fails, since an unverifiable worktree must not be
/// vouched for as clean. `None` when no SHA is obtainable at all (git
/// missing from `PATH`, `repo_root` not a git checkout, non-UTF8 output) — [`run_with_paths`] falls back to the literal string
/// `"unknown"` rather than propagating an error: a missing SHA should never
/// block a capture a developer is actively trying to record.
///
/// The `-dirty` suffix is load-bearing for the blessing workflow, not
/// cosmetic. A bare SHA claims "these pixels are what commit `abc123`
/// produces", and `docs/snapshots.md`'s table is blessed on exactly that
/// claim — but the overwhelmingly common way to *record* a capture is
/// mid-slice, with edits in the tree. Without the suffix, a capture of
/// uncommitted work is indistinguishable from a capture of the commit it
/// sits on, and a reviewer could bless a hash no one can reproduce from
/// the recorded SHA.
fn git_sha(repo_root: &Path) -> Option<String> {
    let sha = git_output(repo_root, &["rev-parse", "HEAD"])?;
    let sha = sha.trim();
    if sha.is_empty() {
        return None;
    }
    // A failed/unavailable `status` must not lose the SHA we already have,
    // but it must not vouch for cleanliness either: a capture we cannot
    // prove clean is treated as dirty, so a reviewer never blesses a hash
    // the recorded SHA might not reproduce. `git` just answered
    // `rev-parse`, so this is close to unreachable.
    let dirty = git_output(repo_root, &["status", "--porcelain"])
        .is_none_or(|status| !status.trim().is_empty());
    if dirty {
        Some(format!("{sha}-dirty"))
    } else {
        Some(sha.to_owned())
    }
}

/// Run one `git -C <repo_root> <args…>`, returning its stdout as UTF-8.
/// `None` if git is missing, exits non-zero, or emits non-UTF-8 — every
/// caller in [`git_sha`] treats those identically.
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
