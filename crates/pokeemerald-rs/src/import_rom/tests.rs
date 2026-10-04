//! Unit tests for the `--import-rom` write path.
//!
//! No real ROM is involved anywhere. The importer is injected where the
//! test is about *publishing* the pack, and `rom_import::fixture` supplies
//! a synthetic cartridge image where the test is about the real wiring.

mod error_rendering;
mod failed_publication;
mod fixture_wiring;
mod hostile_paths;
mod publishing;
mod support;
mod temporary_naming;

use std::fs;

use super::{import_to_with, TEMP_PREFIX};
pub(super) use support::TempDir;
use support::{fake_pack, SourceRom};
#[cfg(unix)]
pub(super) use support::{
    fill_descriptor_table_leaving, in_descriptor_pressure_child, run_under_descriptor_pressure,
};

#[cfg(unix)]
#[test]
fn a_deep_destination_under_descriptor_pressure_still_publishes() {
    // Pinning is sized against the soft limit alone, so a process already
    // holding most of its table can finish creation with none left.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::tests::a_deep_destination_under_descriptor_pressure_still_publishes",
        );
        return;
    }
    let dir = TempDir::new("descriptor-pressure");
    let mut level = dir.join("d");
    for _ in 1..32 {
        level = level.join("d");
    }
    let pack_path = level.join("pokeemerald.pack");
    let source = SourceRom::new("descriptor-pressure-src");
    let fillers = fill_descriptor_table_leaving(4);
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    });
    drop(fillers);
    outcome.expect("releasable pins must not starve the destination and temporary file");
    assert_eq!(
        fs::read(&pack_path).expect("the pack was published"),
        b"pack bytes"
    );
}
