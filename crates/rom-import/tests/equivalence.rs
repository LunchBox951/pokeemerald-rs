//! The equivalence gate: the ROM backend's pack against the checkout's.
//!
//! Discussion #71's policy A and policy C promise the same pack from the
//! same game. Nothing else in the tree can check that promise: the two
//! backends read completely different inputs, and only a real ROM and a
//! real `cargo xtask extract` pack together prove they agree.
//!
//! So this test is `#[ignore]`d and needs both:
//!
//! ```text
//! POKEEMERALD_ROM=/path/to/pokeemerald.gba cargo test -p rom-import -- --ignored
//! ```
//!
//! The checkout side is read from `cargo xtask extract`'s own fixed
//! destination, `<repo root>/assets-pack/pokeemerald.pack`, and never
//! through runtime pack resolution — see [`checkout_pack_path`].
//!
//! It skips with a printed reason when either is missing, rather than
//! failing: a contributor without a ROM must still be able to run
//! `--ignored` `(gated-by-default)`.
//!
//! # What it compares
//!
//! Whole directories, not a hand-listed set of ids. The ROM backend's pack
//! is *partial* until every domain lands, so the comparison is scoped two
//! ways:
//!
//! - Every id the ROM backend wrote must exist in the checkout pack, with
//!   the same kind, the same metadata, and the same payload bytes.
//! - Every checkout id under a prefix in [`COMPLETE_PREFIXES`] must exist in
//!   the ROM backend's pack, so a domain that silently drops a root fails
//!   here instead of shipping a pack with a hole in it. A domain slice adds
//!   its prefix when it lands.
//!
//! [`REVIEWED_DIFFERENCES`] is the escape hatch for an id the two backends
//! genuinely cannot agree on. It is empty, and the intent is that it stays
//! empty: `title/palette/pokemon_logo` was the one candidate, and it was
//! resolved by making both backends honour upstream's own `-num_colors`
//! cut rather than by writing the difference down here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use assets::Song;
use pack_format::{parse_directory, DirectoryEntry, EntryKind, OUTPUT_RELATIVE_PATH};

/// The environment variable naming the ROM to import.
const ROM_ENV: &str = "POKEEMERALD_ROM";

/// Id prefixes the ROM backend claims to cover completely.
///
/// A checkout id under one of these with no ROM counterpart is a failure.
/// Anything outside them is a domain that has not landed yet, and is
/// counted but not compared.
const COMPLETE_PREFIXES: &[&str] = &[
    "title/",
    "interface/",
    "tileset/",
    "layout/",
    "sprite/",
    "font/",
    "text-window/",
    "audio/",
];

/// Ids the two backends are known and accepted to disagree on, each with
/// the reason it was signed off.
///
/// Empty. Every entry here is a promise not kept, so adding one needs a
/// reason that survives review, not a convenient way to make this test
/// pass `(test-ratchet)`.
const REVIEWED_DIFFERENCES: &[(&str, &str)] = &[];

#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_rom_backend_matches_the_checkout_pack() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }

    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing {} failed: {err}", rom_path.display()));
    let checkout_bytes = std::fs::read(&checkout_path)
        .unwrap_or_else(|err| panic!("reading {} failed: {err}", checkout_path.display()));

    let imported = index(&rom_bytes, "the imported pack");
    let checkout = index(&checkout_bytes, "the checkout pack");

    let reviewed: BTreeMap<&str, &str> = REVIEWED_DIFFERENCES.iter().copied().collect();
    let mut differences = Vec::new();

    for (id, entry) in &imported {
        if let Some(reason) = reviewed.get(id.as_str()) {
            eprintln!("reviewed difference: {id}: {reason}");
            continue;
        }
        let Some(theirs) = checkout.get(id) else {
            differences.push(format!("{id}: the checkout pack has no such entry"));
            continue;
        };
        if let Some(difference) = compare(id, &rom_bytes, entry, &checkout_bytes, theirs) {
            differences.push(difference);
        }
    }

    for id in checkout.keys() {
        let claimed = COMPLETE_PREFIXES
            .iter()
            .any(|prefix| id.starts_with(prefix));
        if claimed && !imported.contains_key(id) && !reviewed.contains_key(id.as_str()) {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    for difference in &differences {
        eprintln!("difference: {difference}");
    }
    eprintln!(
        "compared {} imported entries against {} checkout entries; {} difference(s)",
        imported.len(),
        checkout.len(),
        differences.len()
    );
    assert!(
        differences.is_empty(),
        "{} entry difference(s) between the ROM backend and the checkout pack",
        differences.len()
    );
}

/// Where `cargo xtask extract` writes: `<repo root>/`
/// [`OUTPUT_RELATIVE_PATH`], the same fixed destination
/// `xtask::extract::run` computes from its own manifest directory.
///
/// Deliberately not [`pack_format::default_pack_path`]. That resolver
/// answers "where does a *running game* find its pack", and its first two
/// rungs — `$POKEEMERALD_PACK` and the OS user-data directory — are exactly
/// the two destinations `--import-rom` writes to. Resolving through it
/// would let this gate compare a fresh ROM import against an earlier ROM
/// import and pass without the checkout extractor being involved at all,
/// and a typo in the override would skip the gate even with a valid
/// checkout pack on disk `(test-ratchet)`.
fn checkout_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/rom-import is always two levels under the repo root")
        .join(OUTPUT_RELATIVE_PATH)
}

/// The ROM to import, if the environment names one.
fn rom_path() -> Option<PathBuf> {
    let value = std::env::var_os(ROM_ENV)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// Parse a pack's directory into an id-keyed map.
fn index(bytes: &[u8], what: &str) -> BTreeMap<String, DirectoryEntry> {
    parse_directory(bytes)
        .unwrap_or_else(|err| panic!("{what} is not a well-formed pack: {err}"))
        .into_iter()
        .map(|entry| (entry.id.clone(), entry))
        .collect()
}

/// Compare one entry across both packs, or `None` if they match.
fn compare(
    id: &str,
    ours_bytes: &[u8],
    ours: &DirectoryEntry,
    theirs_bytes: &[u8],
    theirs: &DirectoryEntry,
) -> Option<String> {
    if ours.kind != theirs.kind {
        return Some(format!(
            "{id}: {} from the ROM, {} from the checkout",
            describe(ours.kind),
            describe(theirs.kind)
        ));
    }
    let ours_payload = &ours_bytes[ours.offset..ours.offset + ours.length];
    let theirs_payload = &theirs_bytes[theirs.offset..theirs.offset + theirs.length];
    if ours_payload == theirs_payload {
        return None;
    }
    if id.starts_with("audio/song/") {
        if let Some(report) = compare_songs(id, ours_payload, theirs_payload) {
            return Some(report);
        }
    }
    let first = ours_payload
        .iter()
        .zip(theirs_payload)
        .position(|(a, b)| a != b);
    Some(match first {
        Some(at) => format!(
            "{id}: payloads differ at byte {at} ({:#04x} from the ROM, {:#04x} from the \
             checkout); lengths {} and {}",
            ours_payload[at],
            theirs_payload[at],
            ours_payload.len(),
            theirs_payload.len()
        ),
        None => format!(
            "{id}: one payload is a prefix of the other; lengths {} and {}",
            ours_payload.len(),
            theirs_payload.len()
        ),
    })
}

/// Name the first event the two backends disagree on, if both payloads
/// decode as songs. A byte offset into an event stream is hard to read; a
/// track and event index with the two events side by side is not.
fn compare_songs(id: &str, ours: &[u8], theirs: &[u8]) -> Option<String> {
    let (ours, theirs) = (Song::decode(ours).ok()?, Song::decode(theirs).ok()?);
    if ours.tracks().len() != theirs.tracks().len() {
        return Some(format!(
            "{id}: {} tracks from the ROM, {} from the checkout",
            ours.tracks().len(),
            theirs.tracks().len()
        ));
    }
    for (track, (a, b)) in ours.tracks().iter().zip(theirs.tracks()).enumerate() {
        let first = a.iter().zip(b).position(|(x, y)| x != y);
        let at = match first {
            Some(at) => at,
            None if a.len() == b.len() => continue,
            None => a.len().min(b.len()),
        };
        return Some(format!(
            "{id}: track {track} event {at}: {:?} from the ROM, {:?} from the checkout              (track lengths {} and {})",
            a.get(at),
            b.get(at),
            a.len(),
            b.len()
        ));
    }
    Some(format!("{id}: header metadata differs"))
}

/// One entry's kind and metadata, on one line.
fn describe(kind: EntryKind) -> String {
    match kind {
        EntryKind::Image {
            width,
            height,
            bit_depth,
        } => format!("image {width}x{height} at {bit_depth}bpp"),
        EntryKind::Palette { color_count } => format!("palette of {color_count}"),
        EntryKind::Raw => "raw".to_owned(),
    }
}
