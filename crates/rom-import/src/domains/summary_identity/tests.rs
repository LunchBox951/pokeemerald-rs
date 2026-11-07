//! Summary-identity reader tests over a synthetic ROM.
//!
//! The fixture plants the nature pointer table with its strings, a 213-row
//! `gRegionMapEntries` at its 8-byte stride with filler where the geometry
//! sits, the three special strings, and the species conversion array. It reads
//! them back through `assets`' own decoder. The failure tests break one root,
//! pointer, stream or value at a time.

use assets::pack::{
    NatureId, SummaryIdentityError, SummaryIdentitySection, SummaryIdentityTable,
    MAPSEC_AQUA_HIDEOUT_OLD, NATURE_COUNT, REGION_MAP_SECTION_COUNT, SPECIES_TO_NATIONAL_COUNT,
};
use assets::SpeciesId;
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{
    decode_summary_identity_payload, write, PACK_ID, REGION_NAME_OFFSET, REGION_ROW_BYTES,
};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{Roots, SummaryIdentityRoots};

/// Where the fixture plants each table.
const NATURES: usize = 0x1000;
const NATURE_STRINGS: usize = 0x1400;
const REGIONS: usize = 0x2000;
const REGION_STRINGS: usize = 0x4000;
const SPECIALS: usize = 0x6000;
const CONVERSIONS: usize = 0x7000;
/// Strings sit one per 8-byte cell.
const CELL: usize = 8;
/// Filler for bytes the schema must never carry: the section geometry.
const FILLER: u8 = 0xEE;
/// The section whose name is a bare `EOS`.
const EMPTY_SECTION: usize = 87;

fn small(off: usize) -> u32 {
    u32::try_from(off).expect("a fixture offset fits in u32")
}

fn byte(index: usize) -> u8 {
    u8::try_from(index).expect("a fixture index fits in u8")
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + small(off))
}

fn nature(id: usize) -> Vec<u8> {
    vec![0x01 + byte(id), 0xFF]
}

fn section(id: usize) -> Vec<u8> {
    if id == EMPTY_SECTION {
        vec![0xFF]
    } else {
        vec![0x01 + byte(id % 200), 0x01 + byte(id / 200), 0xFF]
    }
}

/// The dex identity `species` converts to: distinct, and inside the old-Unown range.
fn national(species: usize) -> u16 {
    u16::try_from(species % 412).unwrap()
}

const FERRY: [u8; 3] = [0x31, 0x32, 0xFF];
const SECRET_BASE: [u8; 3] = [0x33, 0x34, 0xFF];
const HIDEOUT: [u8; 3] = [0x35, 0x36, 0xFF];

fn fixture() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for id in 0..NATURE_COUNT {
        rom = rom
            .write_ptr(NATURES + id * 4, at(NATURE_STRINGS + id * CELL))
            .write(NATURE_STRINGS + id * CELL, &nature(id));
    }
    for id in 0..REGION_MAP_SECTION_COUNT {
        // x, y, width, height, then the name pointer.
        let row = REGIONS + id * REGION_ROW_BYTES;
        rom = rom
            .write(row, &[FILLER; REGION_NAME_OFFSET])
            .write_ptr(row + REGION_NAME_OFFSET, at(REGION_STRINGS + id * CELL))
            .write(REGION_STRINGS + id * CELL, &section(id));
    }
    rom = rom
        .write(SPECIALS, &FERRY)
        .write(SPECIALS + CELL, &SECRET_BASE)
        .write(SPECIALS + 2 * CELL, &HIDEOUT);
    for species in 1..=SPECIES_TO_NATIONAL_COUNT {
        rom = rom.write(
            CONVERSIONS + (species - 1) * 2,
            &national(species).to_le_bytes(),
        );
    }
    rom
}

fn family() -> SummaryIdentityRoots {
    SummaryIdentityRoots {
        natures: at(NATURES),
        region_map_entries: at(REGIONS),
        ferry: at(SPECIALS),
        secret_base: at(SPECIALS + CELL),
        hideout: at(SPECIALS + 2 * CELL),
        species_to_national: at(CONVERSIONS),
    }
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

/// What the domain queued: each entry's id, kind, and payload.
type Written = Vec<(String, EntryKind, Vec<u8>)>;

fn written(rom: &Rom, family: SummaryIdentityRoots) -> Result<Written, ImportError> {
    let roots = Roots {
        summary_identity: family,
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    write(rom, &roots, &mut writer)?;
    let bytes = writer.finish().expect("the queued entries are valid");
    let directory = parse_directory(&bytes).expect("the pack parses");
    Ok(directory
        .into_iter()
        .map(|entry| {
            let payload = bytes[entry.offset..entry.offset + entry.length].to_vec();
            (entry.id, entry.kind, payload)
        })
        .collect())
}

fn payload_of(rom: RomFixture, family: SummaryIdentityRoots) -> Result<Vec<u8>, ImportError> {
    decode_summary_identity_payload(&rom_of(rom).reader(), &family)
}

#[test]
fn the_region_row_is_eight_bytes_with_its_name_pointer_in_the_second_word() {
    assert_eq!((REGION_ROW_BYTES, REGION_NAME_OFFSET), (8, 4));
}

#[test]
fn matching_roots_yield_the_canonical_table() {
    let rom = rom_of(fixture());
    let entries = written(&rom, family()).expect("consistent tables");
    let [(id, kind, payload)] = entries.as_slice() else {
        panic!("one entry, got {}", entries.len())
    };
    assert_eq!(id, PACK_ID);
    assert_eq!(*kind, EntryKind::Raw);

    let table = SummaryIdentityTable::decode(payload).expect("canonical bytes");
    for id in 0..NATURE_COUNT {
        assert_eq!(
            table.nature_name(NatureId::new(byte(id)).unwrap()),
            nature(id),
            "nature {id}"
        );
    }
    for id in 0..REGION_MAP_SECTION_COUNT {
        assert_eq!(
            table.region_section_name(u16::try_from(id).unwrap()),
            Some(section(id).as_slice()),
            "section {id}"
        );
    }
    // The empty name keeps its slot and does not shift its neighbours.
    assert_eq!(table.region_section_name(87), Some([0xFF].as_slice()));
    assert_eq!(table.map_name_generic(87), Some(FERRY.as_slice()));
    assert_eq!(
        table.met_location_name(MAPSEC_AQUA_HIDEOUT_OLD),
        Some(HIDEOUT.as_slice())
    );
    // The last rows would land on the wrong bytes at another stride, and the
    // first conversion belongs to species one, not zero.
    assert_eq!(
        table.region_section_name(212),
        Some(section(212).as_slice())
    );
    for species in [1, 2, 251, 252, 411] {
        assert_eq!(
            table
                .species_to_national(SpeciesId(u16::try_from(species).unwrap()))
                .map(assets::pokedex::NationalDexNo::get),
            Some(national(species)),
            "species {species}"
        );
    }
}

#[test]
fn the_payload_reader_and_the_domain_agree() {
    let rom = rom_of(fixture());
    let payload = decode_summary_identity_payload(&rom.reader(), &family()).expect("a table");
    let entries = written(&rom, family()).expect("a table");
    assert_eq!(entries[0].2, payload);
}

#[test]
fn geometry_bytes_never_reach_the_payload() {
    let a = payload_of(fixture(), family()).unwrap();
    let rewritten = fixture().write(REGIONS + 5 * REGION_ROW_BYTES, &[1, 2, 3, 4]);
    assert_eq!(a, payload_of(rewritten, family()).unwrap());
}

#[test]
fn a_profile_without_summary_identity_writes_nothing() {
    let rom = rom_of(fixture());
    let entries = written(&rom, SummaryIdentityRoots::NONE).expect("no roots is no work");
    assert!(entries.is_empty());
}

type Clear = fn(&mut SummaryIdentityRoots);

#[test]
fn a_missing_root_is_refused() {
    let rom = rom_of(fixture());
    let cases: [(&str, Clear); 6] = [
        ("gNatureNamePointers", |r| r.natures = GbaPtr::AT_BASE),
        ("gRegionMapEntries", |r| {
            r.region_map_entries = GbaPtr::AT_BASE;
        }),
        ("gText_Ferry", |r| r.ferry = GbaPtr::AT_BASE),
        ("gText_SecretBase", |r| r.secret_base = GbaPtr::AT_BASE),
        ("gText_Hideout", |r| r.hideout = GbaPtr::AT_BASE),
        ("sSpeciesToNationalPokedexNum", |r| {
            r.species_to_national = GbaPtr::AT_BASE;
        }),
    ];
    for (field, clear) in cases {
        let mut roots = family();
        clear(&mut roots);
        let err = written(&rom, roots).unwrap_err();
        assert!(
            matches!(err, ImportError::StructMismatch { root: PACK_ID, field: f } if f == field),
            "{field}: {err}"
        );
    }
}

#[test]
fn a_table_slot_that_is_not_a_pointer_is_refused() {
    let zero = 0u32.to_le_bytes();
    for at_off in [
        NATURES + 24 * 4,
        REGIONS + 100 * REGION_ROW_BYTES + REGION_NAME_OFFSET,
    ] {
        let rom = rom_of(fixture().write(at_off, &zero));
        let err = written(&rom, family()).unwrap_err();
        assert!(
            matches!(err, ImportError::PointerOutOfRange { at, raw: 0 } if at == at_off),
            "{err}"
        );
    }
}

#[test]
fn tables_running_off_the_image_are_refused() {
    // Only the first row of each table fits before the image ends.
    for (rom, roots) in [
        (
            fixture().write_ptr(ROM_SIZE - REGION_NAME_OFFSET, at(REGION_STRINGS)),
            SummaryIdentityRoots {
                region_map_entries: at(ROM_SIZE - REGION_ROW_BYTES),
                ..family()
            },
        ),
        (
            fixture(),
            SummaryIdentityRoots {
                species_to_national: at(ROM_SIZE - 2),
                ..family()
            },
        ),
        (
            fixture().write_ptr(ROM_SIZE - 4, at(NATURE_STRINGS)),
            SummaryIdentityRoots {
                natures: at(ROM_SIZE - 4),
                ..family()
            },
        ),
    ] {
        let err = written(&rom_of(rom), roots).unwrap_err();
        assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
    }
}

#[test]
fn an_unterminated_stream_is_refused() {
    let last = ROM_SIZE - 3;
    let rom = rom_of(
        fixture()
            .write_ptr(NATURES, at(last))
            .write(last, &[0x01, 0x02, 0x03]),
    );
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(err, ImportError::Truncated { at, .. } if at == last),
        "{err}"
    );
}

#[test]
fn a_malformed_name_names_its_table_and_slot() {
    for (rom, section, id) in [
        (
            fixture().write(NATURE_STRINGS + 5 * CELL, &[0xFC, 0x04, 0xFF]),
            SummaryIdentitySection::Nature,
            5,
        ),
        (
            fixture().write(REGION_STRINGS + 66 * CELL, &[0xFD, 0xFF]),
            SummaryIdentitySection::RegionSection,
            66,
        ),
        (
            fixture().write(SPECIALS + CELL, &[0xFD, 0xFF]),
            SummaryIdentitySection::Special,
            1,
        ),
    ] {
        let err = written(&rom_of(rom), family()).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::SummaryIdentity {
                    id: PACK_ID,
                    source: SummaryIdentityError::MalformedString { section: s, id: i, .. },
                } if s == section && i == id
            ),
            "{section}: {err}"
        );
    }
}

#[test]
fn an_undefined_dex_identity_names_its_species() {
    // The first slot belongs to species one.
    let rom = rom_of(fixture().write(CONVERSIONS, &0x1000u16.to_le_bytes()));
    let err = written(&rom, family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::SummaryIdentity {
                source: SummaryIdentityError::InvalidNationalDexNo {
                    species: 1,
                    national: 0x1000
                },
                ..
            }
        ),
        "{err}"
    );
}
