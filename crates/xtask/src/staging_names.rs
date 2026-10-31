//! The unpredictable staging-name policy the pack-extraction and
//! snapshot-pointer publishers share (`extract::publish` and
//! `record_snapshot`). `gen_rom_profile`'s profile publisher keeps
//! its own clock-and-salt temporary beside the profile: it makes one
//! exclusive create and treats a collision as an error, so it walks no
//! candidates.

use std::path::{Path, PathBuf};

/// Width of the staging suffix's hex digits: wide enough that guessing a
/// value ahead of a run is impractical, matching the unpredictable-name
/// approach [`engine::save::file::staging`](../../engine/src/save/file/staging.rs)
/// takes for the identical problem.
const HEX_DIGITS: usize = 10;

/// How many candidate names one publish tries before giving up. The caller's
/// exclusive create makes each attempt exclusive, so this only bounds retries
/// against a genuine collision -- another run racing to stage at the same
/// moment -- not against a planted symlink, which an exclusive create refuses
/// outright regardless of how many names are offered.
const WALK_ATTEMPTS: usize = 256;

/// The unpredictable sibling names one publish of `target` walks, in the
/// order tried.
pub(crate) fn candidates(target: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let mask = value_mask();
    let mut value = unique_value();
    std::iter::repeat_with(move || {
        let candidate = path_with_value(target, value);
        value = value.wrapping_add(1) & mask;
        candidate
    })
    .take(WALK_ATTEMPTS)
}

/// Renders the sibling name for one candidate `value`, `.tmp.<hex>` suffixed
/// onto `target`'s own name.
pub(crate) fn path_with_value(target: &Path, value: u64) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(format!(".tmp.{value:0HEX_DIGITS$x}"));
    PathBuf::from(name)
}

/// `std`-only entropy folded into one value that fits [`HEX_DIGITS`]: process
/// id, clock nanoseconds, and a fresh `RandomState` key, which alone already
/// differs between two calls at the same nanosecond. The caller's exclusive
/// create is what keeps two stagings from colliding; this value only makes
/// the name unguessable ahead of time.
fn unique_value() -> u64 {
    use std::hash::{BuildHasher, Hasher};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(nanos);
    hasher.finish() & value_mask()
}

/// Every value [`HEX_DIGITS`] hex digits can render, and no other.
fn value_mask() -> u64 {
    (1_u64 << (4 * HEX_DIGITS)) - 1
}

#[cfg(test)]
mod tests {
    use super::{candidates, path_with_value, HEX_DIGITS, WALK_ATTEMPTS};
    use std::path::Path;

    #[test]
    fn a_walk_offers_distinct_fixed_width_siblings_up_to_the_attempt_limit() {
        let target = Path::new("out").join("pokeemerald.pack");
        let walk: Vec<_> = candidates(&target).collect();

        assert_eq!(walk.len(), WALK_ATTEMPTS);
        let unique: std::collections::HashSet<_> = walk.iter().collect();
        assert_eq!(unique.len(), WALK_ATTEMPTS, "no name may repeat in a walk");
        for candidate in &walk {
            let name = candidate.to_string_lossy();
            let suffix = name
                .strip_prefix(&*target.to_string_lossy())
                .and_then(|rest| rest.strip_prefix(".tmp."))
                .unwrap_or_else(|| panic!("not a sibling of the target: {name}"));
            assert_eq!(suffix.len(), HEX_DIGITS);
            assert!(suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn a_value_renders_zero_padded_beside_the_target() {
        assert_eq!(
            path_with_value(Path::new("a.pack"), 0xAB),
            Path::new("a.pack.tmp.00000000ab")
        );
    }
}
