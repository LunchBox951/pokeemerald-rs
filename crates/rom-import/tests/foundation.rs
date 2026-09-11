//! The crate's public surface, exercised the way a domain reader will use
//! it: build a synthetic ROM, wrap it, walk a pointer, unpack a compressed
//! blob, select a profile.

use rom_import::fixture::{profile_for, RomFixture};
use rom_import::{
    lz77_decompress_at, select_profile, select_profile_with, GbaPtr, ImportError, Rom, ROM_BASE,
    ROM_SIZE,
};

/// A ROM with a pointer at `0x1000` aimed at an LZ77 stream at `0x2000`
/// that unpacks to `ABCABC`.
fn rom_with_a_compressed_blob() -> Rom {
    let target = GbaPtr::new(ROM_BASE + 0x2000).expect("inside the cartridge window");
    let stream = [
        0x10,
        0x06,
        0x00,
        0x00,        // type 0x10, 6 bytes out
        0b0001_0000, // three literals, then a back-reference
        b'A',
        b'B',
        b'C',
        0x00,
        0x02,
    ];
    let bytes = RomFixture::new()
        .emerald_header()
        .write_ptr(0x1000, target)
        .write(0x2000, &stream)
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

#[test]
fn a_reader_walks_a_pointer_to_a_compressed_blob() {
    let rom = rom_with_a_compressed_blob();
    assert_eq!(rom.bytes().len(), ROM_SIZE);

    let reader = rom.reader();
    let ptr = reader.ptr(0x1000).expect("a valid cartridge pointer");
    assert_eq!(ptr.offset(), 0x2000);
    assert_eq!(
        lz77_decompress_at(&reader, ptr.offset(), Some(6)).expect("a valid stream"),
        b"ABCABC"
    );
}

#[test]
fn a_synthetic_rom_is_never_a_shipped_revision() {
    let rom = rom_with_a_compressed_blob();
    let err = select_profile(&rom).expect_err("a fixture is not a real ROM");
    assert!(matches!(err, ImportError::UnsupportedRevision { .. }));
    assert!(err
        .to_string()
        .contains("f3ae088181bf583e55daf962a92bb46f4f1d07b7"));
}

#[test]
fn a_profile_built_from_the_fixture_selects_it() {
    let rom = rom_with_a_compressed_blob();
    let profile = profile_for(rom.bytes());
    let selected = select_profile_with(&rom, std::slice::from_ref(&profile)).expect("a match");
    assert_eq!(selected.name, "test fixture");
    assert_eq!(selected.sha1, rom.digest());
}

/// A fresh directory under the OS temporary directory, unique to this
/// process and this call, removed on drop.
///
/// A fixed shared path would let two `cargo test` processes (two worktrees,
/// or a workspace run alongside a targeted run) race: one process's cleanup
/// could unlink the other's fixture mid-import. Naming is settled by
/// `std::fs::create_dir`'s exclusivity, not by any counter shared across
/// calls: each attempt tries a candidate derived from the label, this
/// process's id, and the attempt number, and only an `AlreadyExists` moves
/// to the next candidate.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let pid = std::process::id();
        for attempt in 0..1_000u32 {
            let path =
                std::env::temp_dir().join(format!("rom-import-foundation-{label}-{pid}-{attempt}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => panic!("a writable temp dir: {err}"),
            }
        }
        panic!("no unused temp directory name for {label} under pid {pid}");
    }

    fn join(&self, name: &str) -> std::path::PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn import_fails_closed_on_an_unsupported_rom() {
    let dir = TempDir::new("test");
    let rom_path = dir.join("fixture.gba");
    let out_path = dir.join("assets.pack");
    std::fs::write(&rom_path, rom_with_a_compressed_blob().bytes()).expect("a writable temp file");

    // The fixture is not the supported revision, so the import stops at
    // profile selection and nothing is written.
    let err = rom_import::import(&rom_path, &out_path).expect_err("a fixture is not a real ROM");
    assert!(matches!(err, ImportError::UnsupportedRevision { .. }));
    assert!(!out_path.exists(), "import must never write a pack");
}
