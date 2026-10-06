//! Shared fixtures for the `import_rom` tests: temp dirs, source ROMs, fake
//! packs, and the descriptor-pressure harness.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rom_import::fixture::RomFixture;
use rom_import::ImportedPack;

/// A pack of `bytes` the injected importer hands back, standing in for a
/// real one.
pub(super) fn fake_pack(bytes: &[u8]) -> ImportedPack {
    ImportedPack::new(7, bytes.to_vec())
}

/// A temporary directory that removes itself, so a failing test cannot
/// leave a 16 MiB fixture behind.
pub(crate) struct TempDir {
    pub(crate) path: PathBuf,
}

impl TempDir {
    /// A fresh, empty directory under the OS temporary directory.
    pub(crate) fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-import-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary directory");
        Self { path }
    }

    /// A path inside this directory.
    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Every file directly inside `dir`, sorted, as plain names.
pub(super) fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| entry.expect("a readable entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The path of the one entry inside `dir` that [`Dest::temp_name`]'s
/// prefix names -- the temporary file the import is currently building,
/// found by content rather than threaded through as a parameter, since the
/// name itself is generated inside `import_to_with` and never handed back
/// on a path that panics part-way through the import.
pub(super) fn find_temp_path(dir: &Path) -> PathBuf {
    fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| entry.expect("a readable entry").path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&format!("{}.", super::TEMP_PREFIX)))
        })
        .expect("the temporary file the import just created")
}

/// A stand-in for the file the player passes to `--import-rom`.
///
/// It has to be a real, openable file even where the importer is injected:
/// the import opens the source once up front and hands that handle on, so
/// there is no longer such a thing as a source path nothing opens. It keeps
/// its own directory, so a test's assertions about the *destination*
/// directory's contents are unaffected by it.
pub(super) struct SourceRom {
    /// The directory holding it, removed when this is dropped.
    _dir: TempDir,
    /// The path to hand to the import.
    path: PathBuf,
}

impl SourceRom {
    /// A source file whose bytes are nobody's business but the importer's.
    pub(super) fn new(label: &str) -> Self {
        let dir = TempDir::new(label);
        let path = dir.join("emerald.gba");
        fs::write(&path, b"stand-in for the player's cartridge").expect("the source writes");
        Self { _dir: dir, path }
    }

    /// The path to hand to the import.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// Write a synthetic Emerald-shaped ROM image and return its path.
///
/// The image has a valid cartridge header, so it gets past every structural
/// check and is rejected on identity alone: its SHA-1 matches no shipped
/// profile, which is exactly what a player pointing the flag at the wrong
/// file would hit.
pub(super) fn write_fixture_rom(dir: &TempDir) -> PathBuf {
    let path = dir.join("fixture.gba");
    fs::write(&path, RomFixture::new().emerald_header().finish()).expect("the fixture writes");
    path
}

/// Set in the child process [`run_under_descriptor_pressure`] spawns.
#[cfg(unix)]
const DESCRIPTOR_PRESSURE_CHILD: &str = "POKEEMERALD_IMPORT_DESCRIPTOR_PRESSURE_CHILD";

/// Whether this process is the child [`run_under_descriptor_pressure`]
/// spawned, where the test body runs under its lowered limit.
#[cfg(unix)]
pub(crate) fn in_descriptor_pressure_child() -> bool {
    std::env::var_os(DESCRIPTOR_PRESSURE_CHILD).is_some()
}

/// Reruns the test at `test_path` alone in a child process whose soft
/// `RLIMIT_NOFILE` is 36, and fails unless it passes there.
#[cfg(unix)]
pub(crate) fn run_under_descriptor_pressure(test_path: &str) {
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(r#"ulimit -Sn 36 && exec "$0" "$1" --exact --nocapture"#)
        .arg(&exe)
        .arg(test_path)
        .env(DESCRIPTOR_PRESSURE_CHILD, "1")
        .output()
        .expect("the child test process must be spawnable");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1 passed"),
        "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Fills this process's descriptor table, then frees exactly `free` slots,
/// so a test's headroom does not depend on how many descriptors the
/// harness happens to hold. The pressure lasts while the result is held.
#[cfg(unix)]
pub(crate) fn fill_descriptor_table_leaving(free: usize) -> Vec<fs::File> {
    let mut fillers = Vec::new();
    let exhausted = loop {
        match fs::File::open("/dev/null") {
            Ok(file) => fillers.push(file),
            Err(err) => break err,
        }
    };
    assert_eq!(
        rustix::io::Errno::from_io_error(&exhausted),
        Some(rustix::io::Errno::MFILE),
        "{exhausted:?}"
    );
    fillers.truncate(fillers.len() - free);
    fillers
}
