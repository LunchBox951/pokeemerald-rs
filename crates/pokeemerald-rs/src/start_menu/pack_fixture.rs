//! Synthetic start-menu pack construction and scratch-file lifetime.

/// A unique scratch path for a synthetic pack fixture.
pub(super) fn synthetic_pack_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// Writes `bytes` to `path` and removes it on drop, so a failed assertion
/// still cleans up the scratch file.
pub(super) struct TempPackFile {
    path: std::path::PathBuf,
}

impl TempPackFile {
    pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Self {
        use std::io::Write as _;
        // `create_new` (O_EXCL) never follows a symlink at `path`. A stale
        // occupant is unlinked (the link itself, never its target) and retried.
        let mut file = None;
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(f) => {
                    file = Some(f);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::fs::remove_file(path).expect("a stale scratch entry is removable");
                }
                Err(e) => panic!("the scratch directory is writable: {e}"),
            }
        }
        file.expect("the scratch path is exclusively creatable")
            .write_all(bytes)
            .expect("the scratch pack is writable");
        Self {
            path: path.to_path_buf(),
        }
    }
}

impl Drop for TempPackFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The smallest pack [`StartMenuChrome::from_pack`] accepts, carrying two
/// distinguishable selectable standard frames -- frame 0 (green) and frame 5
/// (red) -- plus the fixed message box and the normal font sheet, so which
/// frame a menu drew is readable from a single border pixel.
pub(super) fn synthetic_start_menu_pack_bytes() -> Vec<u8> {
    use crate::pack_test_support::{image_entry, palette_entry, palette_entry_with_color};
    use rendering::Bgr555;

    const FRAME_SIDE: u32 = 24;
    const FRAME_BIT_DEPTH: u8 = 4;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    const FRAME_BORDER_PALETTE_INDEX: u8 = 1;
    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FONT_BIT_DEPTH: u8 = 2;

    crate::pack_test_support::pack_bytes(vec![
        image_entry(
            "text-window/image/1",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/1",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(0, 31, 0),
        ),
        image_entry(
            "text-window/image/6",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/6",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(31, 0, 0),
        ),
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ])
}

#[cfg(unix)]
#[test]
fn scratch_pack_writer_does_not_follow_a_planted_symlink() {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-symlink-probe-start-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let victim = dir.join("victim");
    std::fs::write(&victim, b"precious").unwrap();
    let path = dir.join("scratch.pack");
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let after = std::fs::read(&victim).unwrap();
    drop(fixture);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        after, b"precious",
        "the writer followed the symlink and clobbered its target"
    );
}
