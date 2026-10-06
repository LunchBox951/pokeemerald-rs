//! Shared fixtures for the title test modules: synthetic images and scratch pack files.

use std::mem::size_of;

pub(super) const RED_BGR555_LE: [u8; 2] = 0x001F_u16.to_le_bytes();
pub(super) const GREEN_BGR555_LE: [u8; 2] = 0x03E0_u16.to_le_bytes();
const PALETTE_ENTRY_KIND_TAG: u8 = 1;

pub(super) fn tiled_image(
    width: usize,
    height: usize,
    tile_value: impl Fn(usize, usize) -> u8,
) -> Vec<u8> {
    let mut pixels = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = tile_value(x / 8, y / 8);
        }
    }
    pixels
}

/// Writes `bytes` to `path` and removes it on drop, so a failed assertion
/// or panic still cleans up the scratch file.
pub(super) struct TempPackFile {
    pub(super) path: std::path::PathBuf,
}

impl TempPackFile {
    fn write(path: &std::path::Path, bytes: &[u8]) -> Self {
        std::fs::write(path, bytes).expect("the scratch directory is writable");
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

pub(super) fn write_synthetic_palette_pack(entries: &[(&str, &[u8])]) -> TempPackFile {
    let header_size = assets::pack::MAGIC.len() + size_of::<u32>() + size_of::<u32>();
    let directory_size: usize = entries
        .iter()
        .map(|(id, _)| {
            size_of::<u16>()
                + id.len()
                + size_of::<u8>()
                + size_of::<u64>()
                + size_of::<u64>()
                + size_of::<u16>()
        })
        .sum();
    let mut payload_offset = header_size + directory_size;

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (entry_id, colors) in entries {
        let color_count = u16::try_from(colors.len() / 2).unwrap();
        out.extend_from_slice(&u16::try_from(entry_id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(entry_id.as_bytes());
        out.push(PALETTE_ENTRY_KIND_TAG);
        out.extend_from_slice(&(payload_offset as u64).to_le_bytes());
        out.extend_from_slice(&(colors.len() as u64).to_le_bytes());
        out.extend_from_slice(&color_count.to_le_bytes());
        payload_offset += colors.len();
    }
    for (_, colors) in entries {
        out.extend_from_slice(colors);
    }

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-title-test-palette-{}-{}.pack",
        std::process::id(),
        out.len()
    ));
    TempPackFile::write(&path, &out)
}

#[test]
fn temp_pack_file_removes_the_scratch_file_while_a_panic_unwinds() {
    let observed = std::cell::RefCell::new(std::path::PathBuf::new());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture =
            write_synthetic_palette_pack(&[("title/palette/pokemon_logo", &RED_BGR555_LE)]);
        *observed.borrow_mut() = fixture.path.clone();
        assert!(
            fixture.path.exists(),
            "the scratch pack is written up front"
        );
        panic!("simulated assertion failure inside the test body");
    }));
    assert!(result.is_err(), "the simulated failure must unwind");
    let path = observed.borrow();
    assert!(
        !path.exists(),
        "the Drop guard must remove {} during unwinding",
        path.display()
    );
}
