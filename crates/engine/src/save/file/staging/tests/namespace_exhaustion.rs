use std::path::Path;

use super::super::create_new_exclusive;
use super::shared::area;
use crate::save::file::tests::TempDir;
use crate::save::file::SaveFile;
use crate::save::store::FLASH_IMAGE_LEN;

/// At the one-hex-digit floor, occupancy must not report exhaustion while any
/// of the sixteen names is untried.
#[test]
fn a_narrowed_staging_namespace_is_walked_to_its_last_free_name() {
    const FREE_DIGIT: char = 'd';

    let dir = TempDir::new("staging-namespace-exhaustion");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    for digit in "0123456789abcdef".chars() {
        if digit != FREE_DIGIT {
            std::fs::write(dir.join(&format!(".tmp.{digit}")), b"someone else's file").unwrap();
        }
    }

    let injected_limit = path
        .with_file_name(format!(".tmp.{FREE_DIGIT}"))
        .as_os_str()
        .as_encoded_bytes()
        .len();
    let refuse_long_paths = |candidate: &Path| -> std::io::Result<std::fs::File> {
        if candidate.as_os_str().as_encoded_bytes().len() > injected_limit {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(refuse_long_paths, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a narrowed namespace holding one free name must be walked to it, not \
             reported exhausted",
        );

    assert_eq!(
        staged.path,
        dir.join(&format!(".tmp.{FREE_DIGIT}")),
        "the one name left free at the floor is the only one staging could have taken"
    );
}

const HEX_RADIX: u32 = 16;

fn two_digit_suffix_of(candidate: &Path) -> u8 {
    let component = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a staging candidate always has a UTF-8 name here");
    let hex = component
        .strip_prefix(".tmp.")
        .unwrap_or_else(|| panic!("a narrowed staging candidate is all suffix: {component}"));
    u8::from_str_radix(hex, HEX_RADIX)
        .unwrap_or_else(|err| panic!("{component} must render two hex digits: {err}"))
}

/// Each narrowed width must walk its own namespace, not stop at the floor
/// width's sixteen names. The occupied run is seeded from inside the injected
/// `open`, at the names the walk actually starts from.
#[test]
fn a_two_digit_staging_namespace_is_walked_past_its_first_sixteen_names() {
    const OCCUPIED_RUN: u8 = 16;

    let dir = TempDir::new("staging-two-digit-namespace");
    let path = dir.join("s");
    let file = SaveFile::at(&path);

    let two_digit_limit = path
        .with_file_name(".tmp.00")
        .as_os_str()
        .as_encoded_bytes()
        .len();
    let start = std::cell::Cell::new(None);
    let occupy_the_start_of_the_walk = |candidate: &Path| -> std::io::Result<std::fs::File> {
        if candidate.as_os_str().as_encoded_bytes().len() > two_digit_limit {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        if start.get().is_none() {
            let origin = two_digit_suffix_of(candidate);
            start.set(Some(origin));
            for step in 0..OCCUPIED_RUN {
                let taken = dir.join(&format!(".tmp.{:02x}", origin.wrapping_add(step)));
                std::fs::write(&taken, b"someone else's file").unwrap();
            }
        }
        create_new_exclusive(candidate)
    };

    let staged = area(&file)
        .stage_narrowing_until_accepted(occupy_the_start_of_the_walk, &vec![0u8; FLASH_IMAGE_LEN])
        .expect(
            "a two-digit namespace with 240 names free must be walked past the sixteen \
             taken ones, not reported exhausted",
        );

    let origin = start
        .get()
        .expect("the shrink chain must have reached the two-digit width");
    assert_eq!(
        staged.path,
        dir.join(&format!(".tmp.{:02x}", origin.wrapping_add(OCCUPIED_RUN))),
        "the walk must take the first free name past the occupied run"
    );
}
