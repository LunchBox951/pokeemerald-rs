//! The Frontier Brain identities: for each of the seven Battle Frontier
//! facilities, its Brain's trainer id, overworld graphics id and sex.
//!
//! Source: `sFrontierBrainTrainerIds` and `sFrontierBrainObjEventGfx` in
//! `src/frontier_util.c` (`:719-728`, `:668-677`), indexed by the
//! `FRONTIER_FACILITY_*` values in `include/constants/battle_frontier.h`.
//! The readers are `:2443-2465` (trainer id) and `:2483-2493` (graphics id and
//! female flag).
//!
//! Data only: the Brain teams, names and speeches, battle eligibility,
//! graphics extraction and object-event wiring are not ported here. A facility
//! id outside the table is refused rather than clamped.

use std::error::Error;
use std::fmt;

use crate::frontier_brain_thresholds::FRONTIER_FACILITY_COUNT;
use crate::trainers::TrainerId;

/// A failed lookup in the Frontier Brain identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierBrainIdentityError {
    /// The facility is not below [`FRONTIER_FACILITY_COUNT`].
    InvalidFacility(usize),
}

impl fmt::Display for FrontierBrainIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFacility(facility) => write!(
                f,
                "Battle Frontier facility `{facility}` is out of range (there are \
                 {FRONTIER_FACILITY_COUNT})"
            ),
        }
    }
}

impl Error for FrontierBrainIdentityError {}

/// One facility's Brain, in upstream's column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrontierBrainIdentity {
    trainer: TrainerId,
    graphics_id: u8,
    female: bool,
}

impl FrontierBrainIdentity {
    /// Returns the Brain's entry in the trainer table.
    #[must_use]
    pub const fn trainer(self) -> TrainerId {
        self.trainer
    }

    /// Returns the Brain's overworld graphics id (`OBJ_EVENT_GFX_*`).
    #[must_use]
    pub const fn graphics_id(self) -> u8 {
        self.graphics_id
    }

    /// Returns whether the Brain is female.
    #[must_use]
    pub const fn is_female(self) -> bool {
        self.female
    }
}

/// The identities, indexed by `FRONTIER_FACILITY_*`: Tower, Dome, Palace,
/// Arena, Factory, Pike, Pyramid.
const IDENTITIES: [FrontierBrainIdentity; FRONTIER_FACILITY_COUNT] = [
    row(TrainerId::ANABEL, 70, true),
    row(TrainerId::TUCKER, 71, false),
    row(TrainerId::SPENSER, 73, false),
    row(TrainerId::GRETA, 72, true),
    row(TrainerId::NOLAND, 74, false),
    row(TrainerId::LUCY, 75, true),
    row(TrainerId::BRANDON, 234, false),
];

const fn row(trainer: TrainerId, graphics_id: u8, female: bool) -> FrontierBrainIdentity {
    FrontierBrainIdentity {
        trainer,
        graphics_id,
        female,
    }
}

/// Returns the Brain identity of a facility, indexed as `FRONTIER_FACILITY_*`
/// (`0` for the Battle Tower through `6` for the Battle Pyramid).
///
/// # Errors
///
/// Returns [`FrontierBrainIdentityError::InvalidFacility`] when `facility` is
/// not below [`FRONTIER_FACILITY_COUNT`].
pub fn frontier_brain_identity(
    facility: usize,
) -> Result<FrontierBrainIdentity, FrontierBrainIdentityError> {
    IDENTITIES
        .get(facility)
        .copied()
        .ok_or(FrontierBrainIdentityError::InvalidFacility(facility))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trainer table indices, in facility order (`TRAINER_ANABEL` through
    /// `TRAINER_BRANDON`).
    const TRAINERS: [u16; 7] = [805, 806, 807, 808, 809, 810, 811];
    /// Overworld graphics ids, in facility order.
    const GRAPHICS: [u8; 7] = [70, 71, 73, 72, 74, 75, 234];
    const FEMALE: [bool; 7] = [true, false, false, true, false, true, false];

    #[test]
    fn every_facility_row_is_pinned_field_by_field() {
        assert_eq!(FRONTIER_FACILITY_COUNT, 7);
        for facility in 0..7 {
            let row = frontier_brain_identity(facility).expect("facility is in range");
            assert_eq!(
                row.trainer(),
                TrainerId(TRAINERS[facility]),
                "trainer {facility}"
            );
            assert_eq!(row.graphics_id(), GRAPHICS[facility], "graphics {facility}");
            assert_eq!(row.is_female(), FEMALE[facility], "female {facility}");
        }
    }

    #[test]
    fn palace_and_arena_graphics_ids_are_out_of_numeric_order() {
        let graphics = |facility| frontier_brain_identity(facility).unwrap().graphics_id();
        // Palace is facility 2 and Arena facility 3.
        assert_eq!((graphics(2), graphics(3)), (73, 72));
    }

    #[test]
    fn pyramid_graphics_id_is_not_contiguous_with_the_rest() {
        let graphics: Vec<u8> = (0..7)
            .map(|facility| frontier_brain_identity(facility).unwrap().graphics_id())
            .collect();
        let mut first_six = graphics[..6].to_vec();
        first_six.sort_unstable();
        assert_eq!(first_six, [70, 71, 72, 73, 74, 75]);
        assert_eq!(graphics[6], 234);
    }

    #[test]
    fn a_facility_outside_the_table_is_refused() {
        for facility in [7, 8, 255, usize::MAX] {
            assert_eq!(
                frontier_brain_identity(facility),
                Err(FrontierBrainIdentityError::InvalidFacility(facility))
            );
        }
    }

    #[test]
    fn the_error_names_the_offending_facility() {
        assert_eq!(
            FrontierBrainIdentityError::InvalidFacility(9).to_string(),
            "Battle Frontier facility `9` is out of range (there are 7)"
        );
    }

    fn upstream_file(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        std::fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
            panic!("cannot read pokeemerald/{relative}: {error}; run ./init.sh")
        })
    }

    /// The numeric value of every `#define NAME <number>` in an upstream
    /// header whose name starts with `prefix`.
    fn defines(header: &str, prefix: &str) -> std::collections::HashMap<String, u16> {
        upstream_file(header)
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                name.starts_with(prefix).then_some(())?;
                Some((name.to_owned(), words.next()?.parse().ok()?))
            })
            .collect()
    }

    /// The body lines of one upstream initializer, after its declaration.
    fn initializer<'a>(source: &'a str, declaration: &str) -> Vec<&'a str> {
        let body = &source[source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"))..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        body[open..close].trim().lines().map(str::trim).collect()
    }

    /// Splits `[FRONTIER_FACILITY_X] = value,` into the facility and its value.
    fn facility_row<'a>(
        line: &'a str,
        facilities: &std::collections::HashMap<String, u16>,
    ) -> (usize, &'a str) {
        let (index, value) = line.split_once(']').expect("indexed row");
        let facility = facilities[index.trim_start_matches('[').trim()];
        let value = value
            .trim()
            .strip_prefix('=')
            .unwrap_or_else(|| panic!("`=` expected: `{line}`"))
            .trim()
            .trim_end_matches(',');
        (usize::from(facility), value)
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_tables_match_the_rows() {
        let facilities = defines("include/constants/battle_frontier.h", "FRONTIER_FACILITY_");
        let trainers = defines("include/constants/opponents.h", "TRAINER_");
        let graphics = defines("include/constants/event_objects.h", "OBJ_EVENT_GFX_");
        let source = upstream_file("src/frontier_util.c");
        assert_eq!(facilities.len(), FRONTIER_FACILITY_COUNT);

        let mut expected = vec![None; FRONTIER_FACILITY_COUNT];
        for line in initializer(&source, "sFrontierBrainTrainerIds[NUM_FRONTIER_FACILITIES]") {
            let (facility, name) = facility_row(line, &facilities);
            expected[facility] = Some((trainers[name], None));
        }
        for line in initializer(
            &source,
            "sFrontierBrainObjEventGfx[NUM_FRONTIER_FACILITIES][2]",
        ) {
            let (facility, pair) = facility_row(line, &facilities);
            let (name, female) = pair
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
                .and_then(|pair| pair.split_once(','))
                .unwrap_or_else(|| panic!("braced pair expected: `{line}`"));
            let female = match female.trim() {
                "TRUE" => true,
                "FALSE" => false,
                other => panic!("TRUE or FALSE expected: `{other}`"),
            };
            let (trainer, _) = expected[facility].expect("trainer row precedes graphics row");
            expected[facility] = Some((trainer, Some((graphics[name.trim()], female))));
        }

        for (facility, row) in expected.into_iter().enumerate() {
            let (trainer, Some((graphics_id, female))) = row.expect("every facility has a row")
            else {
                panic!("facility {facility} has no graphics row");
            };
            let identity = frontier_brain_identity(facility).expect("in range");
            assert_eq!(identity.trainer(), TrainerId(trainer), "trainer {facility}");
            assert_eq!(
                u16::from(identity.graphics_id()),
                graphics_id,
                "graphics {facility}"
            );
            assert_eq!(identity.is_female(), female, "female {facility}");
        }
    }
}
