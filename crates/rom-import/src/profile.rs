//! Revision profiles: which ROM this is, and where its data lives.
//!
//! A profile is selected by whole-file SHA-1 and by nothing else. No
//! heuristic, no signature scan, no "close enough" fallback. Every address a
//! later slice adds to [`Roots`] is authoritative for exactly one build of
//! the game, and running those addresses against a ROM that merely looks
//! similar produces plausible garbage rather than a clean failure. The header
//! game code and version are checked afterwards, as corroboration, never as a
//! way in.

use crate::error::ImportError;
use crate::rom::Rom;
use crate::sha1::Digest;

/// The authoritative ROM addresses a profile carries.
///
/// Empty in this slice. Later slices fill it in as domain readers land, one
/// field per root the importer needs to walk: the species table, the
/// tileset and sprite tables, the map group index, the text bank pointers,
/// and so on. Each field is a [`GbaPtr`](crate::GbaPtr) or a ROM offset
/// taken from the one supported build, never something discovered at run
/// time.
///
/// The struct is `#[non_exhaustive]` so adding roots stays a non-breaking
/// change for anything outside this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Roots {}

impl Roots {
    /// A profile with no roots recorded yet.
    pub const NONE: Self = Self {};
}

/// One known ROM build, and where its data lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionProfile {
    /// A human-readable name, used in error messages.
    pub name: &'static str,
    /// The whole-file SHA-1 that identifies this build.
    pub sha1: Digest,
    /// The header game code this build must carry.
    pub game_code: [u8; 4],
    /// The header version byte this build must carry.
    pub version: u8,
    /// The addresses the importer reads from this build.
    pub roots: Roots,
}

/// Pokemon Emerald, US, revision 0. The only build the importer supports.
///
/// Other regions and revisions move data around, so each would need its own
/// profile with its own verified addresses. None exist yet, and none are
/// planned before the US build imports end to end.
pub const EMERALD_US_REV0: RevisionProfile = RevisionProfile {
    name: "Pokemon Emerald (US) rev 0",
    sha1: Digest::from_hex("f3ae088181bf583e55daf962a92bb46f4f1d07b7"),
    game_code: *b"BPEE",
    version: 0,
    roots: Roots::NONE,
};

/// Every profile the importer ships with.
pub const KNOWN_PROFILES: &[RevisionProfile] = &[EMERALD_US_REV0];

/// Select the profile for `rom` from `profiles`.
///
/// Matching is by whole-file SHA-1 alone. The winning profile's game code
/// and version are then checked against the ROM's header, which catches a
/// mistyped profile entry rather than a mistyped ROM.
///
/// # Errors
///
/// [`ImportError::UnsupportedRevision`] if no profile's hash matches;
/// [`ImportError::ProfileHeaderMismatch`] if one does but its header fields
/// disagree.
pub fn select_with<'p>(
    rom: &Rom,
    profiles: &'p [RevisionProfile],
) -> Result<&'p RevisionProfile, ImportError> {
    let digest = rom.digest();
    let profile = profiles
        .iter()
        .find(|candidate| candidate.sha1 == digest)
        .ok_or(ImportError::UnsupportedRevision {
            sha1: digest,
            game_code: rom.header().game_code,
            version: rom.header().version,
        })?;

    if profile.game_code != rom.header().game_code || profile.version != rom.header().version {
        return Err(ImportError::ProfileHeaderMismatch {
            profile: profile.name,
        });
    }
    Ok(profile)
}

/// Select the profile for `rom` from [`KNOWN_PROFILES`].
///
/// # Errors
///
/// As [`select_with`].
pub fn select(rom: &Rom) -> Result<&'static RevisionProfile, ImportError> {
    select_with(rom, KNOWN_PROFILES)
}

#[cfg(test)]
mod tests {
    use super::{select, select_with, RevisionProfile, Roots, EMERALD_US_REV0, KNOWN_PROFILES};
    use crate::error::ImportError;
    use crate::fixture::{shared_emerald_rom, shared_fixture_profile};

    #[test]
    fn the_shipped_profile_names_the_supported_rom() {
        assert_eq!(EMERALD_US_REV0.game_code, *b"BPEE");
        assert_eq!(EMERALD_US_REV0.version, 0);
        assert_eq!(
            EMERALD_US_REV0.sha1.to_string(),
            "f3ae088181bf583e55daf962a92bb46f4f1d07b7"
        );
        assert_eq!(EMERALD_US_REV0.roots, Roots::NONE);
        assert_eq!(Roots::default(), Roots::NONE);
        assert_eq!(KNOWN_PROFILES.len(), 1);
    }

    #[test]
    fn a_hash_that_matches_nothing_is_unsupported() {
        // The fixture is a synthetic image, so it can never match a shipped
        // profile however convincing its header is.
        let err = select(shared_emerald_rom()).unwrap_err();
        match err {
            ImportError::UnsupportedRevision {
                sha1,
                game_code,
                version,
            } => {
                assert_eq!(sha1, shared_emerald_rom().digest());
                assert_eq!(game_code, *b"BPEE");
                assert_eq!(version, 0);
            }
            other => panic!("expected UnsupportedRevision, got {other}"),
        }
        assert!(select(shared_emerald_rom())
            .unwrap_err()
            .to_string()
            .contains("f3ae088181bf583e55daf962a92bb46f4f1d07b7"));
    }

    #[test]
    fn a_matching_hash_selects_the_profile() {
        let profile = shared_fixture_profile();
        let selected = select_with(shared_emerald_rom(), std::slice::from_ref(profile)).unwrap();
        assert_eq!(selected, profile);
    }

    #[test]
    fn a_profile_whose_header_disagrees_is_rejected() {
        // Same hash, wrong version: the profile table is what is wrong here,
        // not the ROM.
        let mut wrong = *shared_fixture_profile();
        wrong.version = 9;
        wrong.name = "wrong version";
        let err = select_with(shared_emerald_rom(), &[wrong]).unwrap_err();
        assert!(matches!(
            err,
            ImportError::ProfileHeaderMismatch {
                profile: "wrong version"
            }
        ));

        let mut wrong = *shared_fixture_profile();
        wrong.game_code = *b"BPRE";
        wrong.name = "wrong game code";
        assert!(matches!(
            select_with(shared_emerald_rom(), &[wrong]).unwrap_err(),
            ImportError::ProfileHeaderMismatch {
                profile: "wrong game code"
            }
        ));
    }

    #[test]
    fn an_empty_profile_table_selects_nothing() {
        let profiles: &[RevisionProfile] = &[];
        assert!(matches!(
            select_with(shared_emerald_rom(), profiles),
            Err(ImportError::UnsupportedRevision { .. })
        ));
    }
}
