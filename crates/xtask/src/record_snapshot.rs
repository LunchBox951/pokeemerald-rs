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
//! # Capture format — two files, no new dependency
//!
//! A PNG encoder would be a new Cargo dependency (`minimal-deps` forbids
//! that without owner approval), so [`run`] writes raw pixel bytes instead:
//!
//! - `<scene>.rgb` — [`SCREEN_WIDTH`] x [`SCREEN_HEIGHT`] pixels, row-major,
//!   3 bytes per pixel (R, G, B) — the composed frame's
//!   `platform::Frame` `0x00RRGGBB` `u32`s split into their three
//!   channel bytes (never naming `platform::Frame` directly, mirroring
//!   `crate::e2e`'s own "stay on the `pokeemerald-rs` dependency, not
//!   `platform`" reasoning).
//! - `<scene>.meta` — plain UTF-8 text, one `key: value` line per field, in
//!   a fixed order (see [`render_meta`]): `scene`, `width`, `height`,
//!   `pixel_format`, `inputs`, `rgb_hash`, `pack_hash`, `git_sha`.
//!
//! Deliberately two files rather than one framed container: a reviewer (or
//! `diff`/`cmp`) can compare pixel payloads byte-for-byte without parsing
//! anything. `rgb_hash` — [`fnv1a64`] of the `.rgb` file's own bytes — is
//! the value `docs/snapshots.md`'s blessing workflow actually tracks per
//! scene, independent of `pack_hash` (provenance: which pack state produced
//! it) and `git_sha` (which changes every commit even when the pixels do
//! not).
//!
//! # Determinism
//!
//! Both files' bytes are a pure function of the pack's own bytes and the
//! selected [`Scene`], plus the ambient git SHA (with a `-dirty` marker when
//! the worktree has uncommitted changes — [`git_sha`]) — no timestamp, no
//! RNG, no wall-clock read anywhere in this module. The `.rgb`/`.meta` pair
//! is staged through `.tmp` siblings and renamed into place, so a partial
//! failure never publishes a `.rgb` the blessing workflow would pair with a
//! stale `.meta` ([`run_with_paths`]). `rgb_hash`/`pack_hash` are both
//! [`fnv1a64`] (a small, owned, `std`-only hash — not a `sha2` dependency
//! this module has no cryptographic need for); `pack_hash` covers the whole
//! pack file's bytes, not just the entries this scene reads: two packs that
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

/// The title screen's frame index this scene captures: the very first
/// composed frame (`compose_frame(0)`) — deterministic and free of any
/// "which frame did the blink cadence land on" ambiguity, unlike a later
/// frame (`crate::e2e`'s own frame-20 check exists precisely because later
/// frames *do* differ from frame 0).
const TITLE_FRAME_INDEX: u32 = 0;

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
    /// Reading the pack's own bytes (for [`fnv1a64`] hashing) failed after
    /// [`AssetPack::load`] already succeeded — practically unreachable
    /// (the file was just read), but a real I/O race is possible. Carries
    /// the path and the underlying error's rendered message.
    ReadPack(PathBuf, String),
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
            Self::ReadPack(path, msg) => {
                write!(
                    f,
                    "record-snapshot: reading pack {} failed: {msg}",
                    path.display()
                )
            }
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
    let pack_path = repo_root.join(crate::extract::OUTPUT_RELATIVE_PATH);
    let output_dir = repo_root.join("snapshots");
    run_with_paths(scene, &pack_path, &output_dir)
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
/// [`RecordSnapshotError::ReadPack`] if the pack's bytes cannot be re-read
/// for hashing; [`RecordSnapshotError::Write`] if `output_dir` cannot be
/// created or either output file cannot be written.
pub fn run_with_paths(
    scene: Scene,
    pack_path: &Path,
    output_dir: &Path,
) -> Result<Report, RecordSnapshotError> {
    let pack = AssetPack::load(pack_path)?;
    let (rgb_bytes, inputs) = compose(scene, &pack)?;
    let rgb_hash = format!("{:016x}", fnv1a64(&rgb_bytes));

    let pack_bytes = std::fs::read(pack_path)
        .map_err(|e| RecordSnapshotError::ReadPack(pack_path.to_path_buf(), e.to_string()))?;
    let pack_hash = format!("{:016x}", fnv1a64(&pack_bytes));
    let git_sha = git_sha(&crate::extract::repo_root()).unwrap_or_else(|| "unknown".to_owned());

    std::fs::create_dir_all(output_dir)
        .map_err(|e| RecordSnapshotError::Write(output_dir.to_path_buf(), e.to_string()))?;

    let rgb_path = output_dir.join(format!("{}.rgb", scene.name()));
    let meta_path = output_dir.join(format!("{}.meta", scene.name()));
    let meta = render_meta(scene, &inputs, &rgb_hash, &pack_hash, &git_sha);

    // Write both files to `.tmp` siblings first and only then rename them
    // into place, so a capture that fails part-way can never leave a
    // *valid-looking* half-capture behind. The blessing workflow
    // (`docs/snapshots.md`) reads the pair — a `.rgb` whose companion
    // `.meta` write failed would otherwise sit next to the *previous*
    // run's stale `.meta`, and an operator diffing hashes would be
    // blessing pixels against metadata that never described them. Both
    // temporaries are written before either rename, and the `.meta`
    // rename lands last, so the window in which `.rgb` is new and `.meta`
    // is stale is one `rename(2)` wide rather than one whole frame
    // composition + file write wide. (Same-directory renames are atomic
    // on every platform this project targets; `write_then_rename` cleans
    // up its own temporary on failure.)
    let rgb_tmp = tmp_sibling(&rgb_path);
    let meta_tmp = tmp_sibling(&meta_path);
    let staged = write_temporaries(&rgb_tmp, &rgb_bytes, &meta_tmp, meta.as_bytes());
    if let Err(err) = staged {
        let _ = std::fs::remove_file(&rgb_tmp);
        let _ = std::fs::remove_file(&meta_tmp);
        return Err(err);
    }
    rename_into_place(&rgb_tmp, &rgb_path)?;
    rename_into_place(&meta_tmp, &meta_path)?;

    Ok(Report {
        rgb_path,
        meta_path,
        payload_len: rgb_bytes.len(),
        rgb_hash,
        pack_hash,
        git_sha,
    })
}

/// The staging path [`run_with_paths`] writes before renaming into place:
/// `path` with `.tmp` appended (`main-menu-new-game.rgb.tmp`), so the
/// temporary is a sibling in the same directory — a cross-directory rename
/// is not guaranteed atomic (it may not even stay on one filesystem),
/// which would defeat the whole point.
///
/// The suffix is appended to the *whole* file name rather than replacing
/// the extension: `<scene>.tmp` for both files would have the two writes
/// racing each other over one path.
fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Write both staging files (see [`run_with_paths`]'s atomicity note).
/// Split out so its caller can clean up *both* temporaries on the first
/// failure with one error path.
fn write_temporaries(
    rgb_tmp: &Path,
    rgb_bytes: &[u8],
    meta_tmp: &Path,
    meta_bytes: &[u8],
) -> Result<(), RecordSnapshotError> {
    std::fs::write(rgb_tmp, rgb_bytes)
        .map_err(|e| RecordSnapshotError::Write(rgb_tmp.to_path_buf(), e.to_string()))?;
    std::fs::write(meta_tmp, meta_bytes)
        .map_err(|e| RecordSnapshotError::Write(meta_tmp.to_path_buf(), e.to_string()))
}

/// Move a staged temporary onto its final path, reporting the *final* path
/// in any error (that is the path the caller asked for and the one
/// `docs/snapshots.md` names).
fn rename_into_place(tmp: &Path, final_path: &Path) -> Result<(), RecordSnapshotError> {
    std::fs::rename(tmp, final_path).map_err(|e| {
        let _ = std::fs::remove_file(tmp);
        RecordSnapshotError::Write(final_path.to_path_buf(), e.to_string())
    })
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
/// `-dirty` appended when the worktree has uncommitted changes. `None` on
/// any failure (git missing from `PATH`, `repo_root` not a git checkout,
/// non-UTF8 output) — [`run_with_paths`] falls back to the literal string
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
    // A failed/unavailable `status` must not lose the SHA we already have:
    // fall back to reporting it clean rather than to `"unknown"`. `git`
    // just answered `rev-parse`, so this is close to unreachable.
    let dirty = git_output(repo_root, &["status", "--porcelain"])
        .is_some_and(|status| !status.trim().is_empty());
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
