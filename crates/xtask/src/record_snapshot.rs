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

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 160;

const TITLE_FRAME_WITH_PRESS_START: u32 = 16;

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

fn compose(
    scene: Scene,
    pack: &AssetPack,
) -> Result<(Vec<u8>, Vec<&'static str>), RecordSnapshotError> {
    match scene {
        Scene::Title => {
            let title = TitleScene::from_pack(pack)?;
            let frame = title.compose_frame(TITLE_FRAME_WITH_PRESS_START);
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
