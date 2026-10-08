//! Shared fixtures for the title test modules: synthetic images and scratch pack files.

use crate::pack_test_support::pack_bytes;

pub(super) const RED_BGR555: u16 = 0x001F;
pub(super) const GREEN_BGR555: u16 = 0x03E0;

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

pub(super) fn write_synthetic_palette_pack(entries: &[(&str, &[u16])]) -> TempPackFile {
    let out = pack_bytes(
        entries
            .iter()
            .map(|(id, colors)| pack_format::palette_entry((*id).into(), colors).unwrap())
            .collect(),
    );
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-title-test-palette-{}-{}.pack",
        std::process::id(),
        out.len()
    ));
    TempPackFile::write(&path, &out)
}

#[test]
fn synthetic_palette_pack_matches_the_pack_writer_serialization() {
    let fixture = write_synthetic_palette_pack(&[
        ("title/palette/z_second", &[GREEN_BGR555]),
        ("title/palette/a_first", &[RED_BGR555]),
    ]);
    let expected = pack_bytes(vec![
        pack_format::palette_entry("title/palette/z_second".into(), &[GREEN_BGR555]).unwrap(),
        pack_format::palette_entry("title/palette/a_first".into(), &[RED_BGR555]).unwrap(),
    ]);
    assert_eq!(std::fs::read(&fixture.path).unwrap(), expected);
}

#[test]
fn temp_pack_file_removes_the_scratch_file_while_a_panic_unwinds() {
    let observed = std::cell::RefCell::new(std::path::PathBuf::new());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture =
            write_synthetic_palette_pack(&[("title/palette/pokemon_logo", &[RED_BGR555])]);
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
