//! The Battle Pyramid floor-template catalog: the sixteen floor templates and
//! the per-floor options that choose between them.
//!
//! Source: `src/battle_pyramid.c`. Three tables live there, and this module
//! keeps them as three separate things:
//!
//! 1. **The floor templates** `sPyramidFloorTemplates` (`:107-237`): sixteen
//!    records, each with an item count, a trainer count, a position policy for
//!    items and one for trainers, a run multiplier, and eight ordered layout
//!    offsets. See [`PyramidFloorTemplate`], [`floor_template`] and
//!    [`floor_templates`].
//! 2. **The template options** `sPyramidFloorTemplateOptions` (`:239-282`):
//!    34 `{cutoff, template id}` entries, grouped by floor. See
//!    [`FloorTemplateOption`] and [`floor_template_options`].
//! 3. **The floor offsets** `sFloorTemplateOffsets` (`:284-287`): the index
//!    in the option list where each of the seven floors starts. See
//!    [`floor_template_option_offset`].
//!
//! The cutoffs are cumulative, as upstream stores them, and are not converted
//! to weights. Upstream picks a template for a floor by drawing a value from
//! `0` to `99` and walking the options from that floor's offset, taking the
//! first whose cutoff is greater than the draw
//! (`GetPyramidFloorTemplateId`, `src/battle_pyramid.c:1919-1927`). Every
//! floor's last cutoff is `100`, so that walk always stops within the floor.
//!
//! Floors, template ids and option positions are all indexed from `0`, in the
//! order the upstream tables give. Every lookup refuses an index that is out
//! of range instead of clamping it.
//!
//! This is data only. The draw and the walk, the choice of the layout offset
//! for each square (`src/battle_pyramid.c:1900-1913`), the placement of items
//! and trainers by their position policy, the run multiplier's use, and the
//! wild rosters are not ported here. The item distributions are in
//! [`battle_pyramid_items`](crate::battle_pyramid_items).

use std::error::Error;
use std::fmt;

use crate::battle_pyramid_items::PYRAMID_FLOORS;

/// The number of floor templates (`sPyramidFloorTemplates`).
pub const FLOOR_TEMPLATE_COUNT: usize = 16;

/// The number of entries in the template option list, over all floors.
pub const FLOOR_TEMPLATE_OPTION_COUNT: usize = 34;

/// The number of layout offsets each template holds (`NUM_LAYOUT_OFFSETS`).
pub const LAYOUT_OFFSETS_PER_TEMPLATE: usize = 8;

/// A failed lookup in the Battle Pyramid floor-template tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidFloorError {
    /// The template id is not below [`FLOOR_TEMPLATE_COUNT`].
    InvalidTemplate(usize),
    /// The floor is not below [`PYRAMID_FLOORS`].
    InvalidFloor(usize),
}

impl fmt::Display for PyramidFloorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTemplate(id) => write!(
                f,
                "Battle Pyramid floor template `{id}` is out of range (there are \
                 {FLOOR_TEMPLATE_COUNT})"
            ),
            Self::InvalidFloor(floor) => write!(
                f,
                "Battle Pyramid floor `{floor}` is out of range (there are {PYRAMID_FLOORS})"
            ),
        }
    }
}

impl Error for PyramidFloorError {}

/// Where a template places its items or its trainers (the
/// `OBJ_POSITIONS_*` constants of `include/constants/battle_pyramid.h`).
///
/// This is only the policy name. The placement it asks for is not ported, and
/// no template uses [`NearEntrance`](Self::NearEntrance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectPositions {
    /// `OBJ_POSITIONS_UNIFORM`.
    Uniform,
    /// `OBJ_POSITIONS_IN_AND_NEAR_ENTRANCE`.
    InAndNearEntrance,
    /// `OBJ_POSITIONS_IN_AND_NEAR_EXIT`.
    InAndNearExit,
    /// `OBJ_POSITIONS_NEAR_ENTRANCE`.
    NearEntrance,
    /// `OBJ_POSITIONS_NEAR_EXIT`.
    NearExit,
}

impl ObjectPositions {
    /// Returns the value of the upstream `OBJ_POSITIONS_*` constant.
    #[must_use]
    pub const fn value(self) -> u8 {
        match self {
            Self::Uniform => 0,
            Self::InAndNearEntrance => 1,
            Self::InAndNearExit => 2,
            Self::NearEntrance => 3,
            Self::NearExit => 4,
        }
    }
}

/// One floor template (`struct PyramidFloorTemplate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyramidFloorTemplate {
    num_items: u8,
    num_trainers: u8,
    item_positions: ObjectPositions,
    trainer_positions: ObjectPositions,
    run_multiplier: u8,
    layout_offsets: [u8; LAYOUT_OFFSETS_PER_TEMPLATE],
}

impl PyramidFloorTemplate {
    /// Returns the number of items on the floor (`numItems`).
    #[must_use]
    pub const fn num_items(&self) -> u8 {
        self.num_items
    }

    /// Returns the number of trainers on the floor (`numTrainers`).
    #[must_use]
    pub const fn num_trainers(&self) -> u8 {
        self.num_trainers
    }

    /// Returns the position policy for items (`itemPositions`).
    #[must_use]
    pub const fn item_positions(&self) -> ObjectPositions {
        self.item_positions
    }

    /// Returns the position policy for trainers (`trainerPositions`).
    #[must_use]
    pub const fn trainer_positions(&self) -> ObjectPositions {
        self.trainer_positions
    }

    /// Returns the run multiplier (`runMultiplier`).
    #[must_use]
    pub const fn run_multiplier(&self) -> u8 {
        self.run_multiplier
    }

    /// Returns the eight layout offsets in upstream order (`layoutOffsets`).
    #[must_use]
    pub const fn layout_offsets(&self) -> &[u8; LAYOUT_OFFSETS_PER_TEMPLATE] {
        &self.layout_offsets
    }
}

/// One template option: a cumulative cutoff and the template it picks.
///
/// A draw `d` in `0..100` takes the first option of a floor with
/// `d < cutoff`. The cutoffs of a floor increase and end at `100`; they are
/// not weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FloorTemplateOption {
    cutoff: u8,
    template_id: u8,
}

impl FloorTemplateOption {
    /// An option; fails to compile when the template id is not below
    /// [`FLOOR_TEMPLATE_COUNT`], so a bad table row cannot build.
    const fn new(cutoff: u8, template_id: u8) -> Self {
        assert!(
            (template_id as usize) < FLOOR_TEMPLATE_COUNT,
            "template option names a template that does not exist"
        );
        Self {
            cutoff,
            template_id,
        }
    }

    /// Returns the cumulative cutoff: the exclusive upper bound of the draws
    /// this option takes.
    #[must_use]
    pub const fn cutoff(self) -> u8 {
        self.cutoff
    }

    /// Returns the id of the template this option picks.
    #[must_use]
    pub const fn template_id(self) -> usize {
        self.template_id as usize
    }
}

/// A template record.
const fn template(
    num_items: u8,
    num_trainers: u8,
    item_positions: ObjectPositions,
    trainer_positions: ObjectPositions,
    run_multiplier: u8,
    layout_offsets: [u8; LAYOUT_OFFSETS_PER_TEMPLATE],
) -> PyramidFloorTemplate {
    PyramidFloorTemplate {
        num_items,
        num_trainers,
        item_positions,
        trainer_positions,
        run_multiplier,
        layout_offsets,
    }
}

use ObjectPositions::{InAndNearEntrance, InAndNearExit, NearExit, Uniform};

/// `sPyramidFloorTemplates` (`src/battle_pyramid.c:107-237`).
#[rustfmt::skip]
const FLOOR_TEMPLATES: [PyramidFloorTemplate; FLOOR_TEMPLATE_COUNT] = [
    template(7, 3, Uniform, Uniform,           128, [0, 0, 1, 1, 2, 2, 3, 3]),
    template(6, 3, Uniform, Uniform,           128, [1, 1, 2, 2, 3, 3, 4, 4]),
    template(5, 3, Uniform, Uniform,           120, [2, 2, 3, 3, 4, 4, 5, 5]),
    template(4, 4, Uniform, Uniform,           120, [3, 3, 4, 4, 5, 5, 6, 6]),
    template(4, 4, Uniform, InAndNearEntrance, 112, [4, 4, 5, 5, 6, 6, 7, 7]),
    template(3, 5, Uniform, InAndNearExit,     112, [5, 6, 7, 8, 9, 10, 11, 12]),
    template(3, 5, Uniform, Uniform,           104, [6, 7, 8, 9, 10, 11, 12, 13]),
    template(2, 4, Uniform, InAndNearEntrance, 104, [7, 8, 9, 10, 11, 12, 13, 14]),
    template(4, 5, Uniform, InAndNearExit,      96, [8, 9, 10, 11, 12, 13, 14, 15]),
    template(3, 6, Uniform, NearExit,           96, [8, 9, 10, 11, 12, 13, 14, 15]),
    template(2, 3, Uniform, Uniform,            88, [12, 13, 14, 12, 13, 14, 12, 13]),
    template(4, 5, Uniform, Uniform,            88, [11, 11, 11, 11, 11, 11, 11, 11]),
    template(3, 7, Uniform, Uniform,            80, [12, 12, 12, 12, 12, 12, 12, 12]),
    template(2, 4, Uniform, Uniform,            80, [13, 13, 13, 13, 13, 13, 13, 13]),
    template(3, 6, Uniform, Uniform,            80, [14, 14, 14, 14, 14, 14, 14, 14]),
    template(3, 8, Uniform, Uniform,            80, [15, 15, 15, 15, 15, 15, 15, 15]),
];

/// `sPyramidFloorTemplateOptions` (`src/battle_pyramid.c:239-282`), floor
/// after floor.
#[rustfmt::skip]
const FLOOR_TEMPLATE_OPTIONS: [FloorTemplateOption; FLOOR_TEMPLATE_OPTION_COUNT] = [
    FloorTemplateOption::new(40, 0),
    FloorTemplateOption::new(70, 1),
    FloorTemplateOption::new(90, 2),
    FloorTemplateOption::new(100, 3),
    FloorTemplateOption::new(35, 1),
    FloorTemplateOption::new(55, 2),
    FloorTemplateOption::new(75, 3),
    FloorTemplateOption::new(90, 4),
    FloorTemplateOption::new(100, 10),
    FloorTemplateOption::new(35, 2),
    FloorTemplateOption::new(55, 3),
    FloorTemplateOption::new(75, 4),
    FloorTemplateOption::new(90, 5),
    FloorTemplateOption::new(100, 11),
    FloorTemplateOption::new(35, 3),
    FloorTemplateOption::new(55, 4),
    FloorTemplateOption::new(75, 5),
    FloorTemplateOption::new(90, 6),
    FloorTemplateOption::new(100, 12),
    FloorTemplateOption::new(35, 4),
    FloorTemplateOption::new(55, 5),
    FloorTemplateOption::new(75, 6),
    FloorTemplateOption::new(90, 7),
    FloorTemplateOption::new(100, 13),
    FloorTemplateOption::new(35, 5),
    FloorTemplateOption::new(55, 6),
    FloorTemplateOption::new(75, 7),
    FloorTemplateOption::new(90, 8),
    FloorTemplateOption::new(100, 14),
    FloorTemplateOption::new(35, 6),
    FloorTemplateOption::new(55, 7),
    FloorTemplateOption::new(75, 8),
    FloorTemplateOption::new(90, 9),
    FloorTemplateOption::new(100, 15),
];

/// `sFloorTemplateOffsets` (`src/battle_pyramid.c:284-287`): the first option
/// of each floor in [`FLOOR_TEMPLATE_OPTIONS`].
const FLOOR_OPTION_OFFSETS: [usize; PYRAMID_FLOORS] = [0, 4, 9, 14, 19, 24, 29];

/// Whether each floor's options are the run starting at its offset: the runs
/// tile the option list, their cutoffs increase, and each ends at `100`.
const fn tables_are_well_formed() -> bool {
    if FLOOR_OPTION_OFFSETS[0] != 0 {
        return false;
    }
    let mut floor = 0;
    while floor < PYRAMID_FLOORS {
        let start = FLOOR_OPTION_OFFSETS[floor];
        let end = if floor + 1 < PYRAMID_FLOORS {
            FLOOR_OPTION_OFFSETS[floor + 1]
        } else {
            FLOOR_TEMPLATE_OPTION_COUNT
        };
        if end <= start {
            return false;
        }
        let mut index = start + 1;
        while index < end {
            if FLOOR_TEMPLATE_OPTIONS[index].cutoff <= FLOOR_TEMPLATE_OPTIONS[index - 1].cutoff {
                return false;
            }
            index += 1;
        }
        if FLOOR_TEMPLATE_OPTIONS[end - 1].cutoff != 100 {
            return false;
        }
        floor += 1;
    }
    true
}

const _: () = assert!(
    tables_are_well_formed(),
    "Battle Pyramid floor template options are malformed"
);

/// Returns the sixteen floor templates in upstream order.
#[must_use]
pub const fn floor_templates() -> &'static [PyramidFloorTemplate; FLOOR_TEMPLATE_COUNT] {
    &FLOOR_TEMPLATES
}

/// Returns the floor template with the given id.
///
/// # Errors
///
/// Returns [`PyramidFloorError::InvalidTemplate`] when `id` is not below
/// [`FLOOR_TEMPLATE_COUNT`].
pub fn floor_template(id: usize) -> Result<&'static PyramidFloorTemplate, PyramidFloorError> {
    FLOOR_TEMPLATES
        .get(id)
        .ok_or(PyramidFloorError::InvalidTemplate(id))
}

/// Returns the index in the 34-entry option list where `floor`'s options
/// start (`sFloorTemplateOffsets`).
///
/// # Errors
///
/// Returns [`PyramidFloorError::InvalidFloor`] when `floor` is not below
/// [`PYRAMID_FLOORS`].
pub fn floor_template_option_offset(floor: usize) -> Result<usize, PyramidFloorError> {
    FLOOR_OPTION_OFFSETS
        .get(floor)
        .copied()
        .ok_or(PyramidFloorError::InvalidFloor(floor))
}

/// Returns the options of one floor, in order, with their cumulative
/// cutoffs. The slice ends where the next floor's options start.
///
/// # Errors
///
/// Returns [`PyramidFloorError::InvalidFloor`] when `floor` is not below
/// [`PYRAMID_FLOORS`].
pub fn floor_template_options(
    floor: usize,
) -> Result<&'static [FloorTemplateOption], PyramidFloorError> {
    let start = floor_template_option_offset(floor)?;
    let end = FLOOR_OPTION_OFFSETS
        .get(floor + 1)
        .copied()
        .unwrap_or(FLOOR_TEMPLATE_OPTION_COUNT);
    Ok(&FLOOR_TEMPLATE_OPTIONS[start..end])
}

/// Returns all 34 template options, floor after floor.
#[must_use]
pub const fn all_floor_template_options(
) -> &'static [FloorTemplateOption; FLOOR_TEMPLATE_OPTION_COUNT] {
    &FLOOR_TEMPLATE_OPTIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One template laid out as upstream's field order, with the position
    /// policies as their `OBJ_POSITIONS_*` numbers.
    /// `(items, trainers, item policy, trainer policy, run, offsets)`.
    type Row = (u8, u8, u8, u8, u8, [u8; 8]);

    const EXPECTED_TEMPLATES: [Row; 16] = [
        (7, 3, 0, 0, 128, [0, 0, 1, 1, 2, 2, 3, 3]),
        (6, 3, 0, 0, 128, [1, 1, 2, 2, 3, 3, 4, 4]),
        (5, 3, 0, 0, 120, [2, 2, 3, 3, 4, 4, 5, 5]),
        (4, 4, 0, 0, 120, [3, 3, 4, 4, 5, 5, 6, 6]),
        (4, 4, 0, 1, 112, [4, 4, 5, 5, 6, 6, 7, 7]),
        (3, 5, 0, 2, 112, [5, 6, 7, 8, 9, 10, 11, 12]),
        (3, 5, 0, 0, 104, [6, 7, 8, 9, 10, 11, 12, 13]),
        (2, 4, 0, 1, 104, [7, 8, 9, 10, 11, 12, 13, 14]),
        (4, 5, 0, 2, 96, [8, 9, 10, 11, 12, 13, 14, 15]),
        (3, 6, 0, 4, 96, [8, 9, 10, 11, 12, 13, 14, 15]),
        (2, 3, 0, 0, 88, [12, 13, 14, 12, 13, 14, 12, 13]),
        (4, 5, 0, 0, 88, [11, 11, 11, 11, 11, 11, 11, 11]),
        (3, 7, 0, 0, 80, [12, 12, 12, 12, 12, 12, 12, 12]),
        (2, 4, 0, 0, 80, [13, 13, 13, 13, 13, 13, 13, 13]),
        (3, 6, 0, 0, 80, [14, 14, 14, 14, 14, 14, 14, 14]),
        (3, 8, 0, 0, 80, [15, 15, 15, 15, 15, 15, 15, 15]),
    ];

    /// The options grouped per floor, as `(cutoff, template id)`.
    const EXPECTED_OPTIONS: [&[(u8, u8)]; 7] = [
        &[(40, 0), (70, 1), (90, 2), (100, 3)],
        &[(35, 1), (55, 2), (75, 3), (90, 4), (100, 10)],
        &[(35, 2), (55, 3), (75, 4), (90, 5), (100, 11)],
        &[(35, 3), (55, 4), (75, 5), (90, 6), (100, 12)],
        &[(35, 4), (55, 5), (75, 6), (90, 7), (100, 13)],
        &[(35, 5), (55, 6), (75, 7), (90, 8), (100, 14)],
        &[(35, 6), (55, 7), (75, 8), (90, 9), (100, 15)],
    ];

    /// The index of each floor's first option, written out.
    const EXPECTED_OFFSETS: [usize; 7] = [0, 4, 9, 14, 19, 24, 29];

    #[test]
    fn every_template_matches_the_expected_record() {
        assert_eq!(floor_templates().len(), EXPECTED_TEMPLATES.len());
        for (id, expected) in EXPECTED_TEMPLATES.iter().enumerate() {
            let t = floor_template(id).expect("template exists");
            let actual: Row = (
                t.num_items(),
                t.num_trainers(),
                t.item_positions().value(),
                t.trainer_positions().value(),
                t.run_multiplier(),
                *t.layout_offsets(),
            );
            assert_eq!(&actual, expected, "template {id}");
            assert_eq!(t, &floor_templates()[id], "template {id} by slice");
        }
    }

    #[test]
    fn position_policy_values_follow_upstream() {
        let all = [
            (ObjectPositions::Uniform, 0),
            (ObjectPositions::InAndNearEntrance, 1),
            (ObjectPositions::InAndNearExit, 2),
            (ObjectPositions::NearEntrance, 3),
            (ObjectPositions::NearExit, 4),
        ];
        for (policy, value) in all {
            assert_eq!(policy.value(), value, "{policy:?}");
        }
    }

    #[test]
    fn every_floor_matches_the_expected_options() {
        assert_eq!(EXPECTED_OPTIONS.len(), PYRAMID_FLOORS);
        for (floor, expected) in EXPECTED_OPTIONS.iter().enumerate() {
            let actual: Vec<(u8, u8)> = floor_template_options(floor)
                .expect("floor exists")
                .iter()
                .map(|option| (option.cutoff(), u8::try_from(option.template_id()).unwrap()))
                .collect();
            assert_eq!(actual.as_slice(), *expected, "floor {floor}");
        }
    }

    #[test]
    fn the_whole_option_list_has_34_entries_floor_after_floor() {
        let expected: Vec<(u8, u8)> = EXPECTED_OPTIONS
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        assert_eq!(expected.len(), FLOOR_TEMPLATE_OPTION_COUNT);
        let actual: Vec<(u8, u8)> = all_floor_template_options()
            .iter()
            .map(|option| (option.cutoff(), u8::try_from(option.template_id()).unwrap()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn the_seven_floor_boundaries_match() {
        for (floor, expected) in EXPECTED_OFFSETS.iter().enumerate() {
            assert_eq!(
                floor_template_option_offset(floor),
                Ok(*expected),
                "floor {floor}"
            );
        }
        // Each offset is the running total of the earlier floors' options.
        let mut total = 0;
        for (floor, options) in EXPECTED_OPTIONS.iter().enumerate() {
            assert_eq!(floor_template_option_offset(floor), Ok(total));
            total += options.len();
        }
    }

    #[test]
    fn every_option_names_a_valid_template() {
        for option in all_floor_template_options() {
            assert!(floor_template(option.template_id()).is_ok());
        }
    }

    #[test]
    fn each_floors_cutoffs_increase_and_end_at_100() {
        for floor in 0..PYRAMID_FLOORS {
            let options = floor_template_options(floor).unwrap();
            assert!(options
                .windows(2)
                .all(|pair| pair[0].cutoff() < pair[1].cutoff()));
            assert_eq!(options.last().map(|option| option.cutoff()), Some(100));
        }
    }

    #[test]
    fn a_draw_walk_from_the_floor_offset_stops_within_the_floor() {
        // The upstream walk runs to the end of the whole list, not the floor;
        // a final cutoff of 100 means no draw below 100 goes past the floor.
        for floor in 0..PYRAMID_FLOORS {
            let start = floor_template_option_offset(floor).unwrap();
            let own = floor_template_options(floor).unwrap();
            for draw in 0..100u8 {
                let walked = all_floor_template_options()[start..]
                    .iter()
                    .find(|option| draw < option.cutoff())
                    .unwrap();
                let within = own.iter().find(|option| draw < option.cutoff()).unwrap();
                assert_eq!(walked, within, "floor {floor} draw {draw}");
            }
        }
    }

    #[test]
    fn invalid_indices_are_refused() {
        assert_eq!(
            floor_template(FLOOR_TEMPLATE_COUNT),
            Err(PyramidFloorError::InvalidTemplate(FLOOR_TEMPLATE_COUNT))
        );
        assert_eq!(
            floor_template(usize::MAX),
            Err(PyramidFloorError::InvalidTemplate(usize::MAX))
        );
        assert_eq!(
            floor_template_option_offset(PYRAMID_FLOORS),
            Err(PyramidFloorError::InvalidFloor(PYRAMID_FLOORS))
        );
        assert_eq!(
            floor_template_options(PYRAMID_FLOORS),
            Err(PyramidFloorError::InvalidFloor(PYRAMID_FLOORS))
        );
        assert_eq!(
            floor_template_options(usize::MAX),
            Err(PyramidFloorError::InvalidFloor(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_index() {
        assert_eq!(
            PyramidFloorError::InvalidTemplate(16).to_string(),
            "Battle Pyramid floor template `16` is out of range (there are 16)"
        );
        assert_eq!(
            PyramidFloorError::InvalidFloor(7).to_string(),
            "Battle Pyramid floor `7` is out of range (there are 7)"
        );
    }

    /// Reads a file of the pinned upstream checkout.
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

    /// The text of the initializer that follows `declaration`, up to its `};`.
    fn initializer<'a>(source: &'a str, declaration: &str) -> &'a str {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        body[open..close].trim()
    }

    /// The text after `.field = ` on a line, without the trailing comma.
    fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
        line.trim()
            .strip_prefix(name)?
            .trim_start()
            .strip_prefix('=')
            .map(|rest| rest.trim().trim_end_matches(','))
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let source = upstream_file("src/battle_pyramid.c");
        let header = upstream_file("include/constants/battle_pyramid.h");
        let policies: std::collections::HashMap<&str, u8> = header
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "#define").then_some(())?;
                let name = words.next()?;
                name.starts_with("OBJ_POSITIONS_")
                    .then(|| words.next()?.parse().ok().map(|value| (name, value)))?
            })
            .collect();

        // The numeric fields of each template, in the order they appear.
        let mut parsed: Vec<Row> = Vec::new();
        let mut current = (0, 0, 0, 0, 0, [0u8; 8]);
        for line in initializer(&source, "sPyramidFloorTemplates[]").lines() {
            if let Some(value) = field(line, ".numItems") {
                current.0 = value.parse().unwrap();
            } else if let Some(value) = field(line, ".numTrainers") {
                current.1 = value.parse().unwrap();
            } else if let Some(value) = field(line, ".itemPositions") {
                current.2 = policies[value];
            } else if let Some(value) = field(line, ".trainerPositions") {
                current.3 = policies[value];
            } else if let Some(value) = field(line, ".runMultiplier") {
                current.4 = value.parse().unwrap();
            } else if let Some(value) = field(line, ".layoutOffsets") {
                let list = value.trim_start_matches('{').trim_end_matches('}');
                let offsets: Vec<u8> = list.split(',').map(|n| n.trim().parse().unwrap()).collect();
                current.5 = offsets.try_into().expect("eight layout offsets");
                parsed.push(current);
            }
        }
        assert_eq!(parsed, EXPECTED_TEMPLATES);

        let options: Vec<(u8, u8)> = initializer(&source, "sPyramidFloorTemplateOptions[][2]")
            .lines()
            .filter(|line| !line.trim().starts_with("//"))
            .map(|line| {
                let inner = line
                    .trim()
                    .trim_end_matches(',')
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
                let (cutoff, id) = inner.split_once(',').expect("two fields");
                (cutoff.trim().parse().unwrap(), id.trim().parse().unwrap())
            })
            .collect();
        let expected: Vec<(u8, u8)> = EXPECTED_OPTIONS
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        assert_eq!(options, expected);

        let offsets: Vec<usize> = initializer(
            &source,
            "sFloorTemplateOffsets[FRONTIER_STAGES_PER_CHALLENGE]",
        )
        .split(',')
        .map(|value| value.trim().parse().unwrap())
        .collect();
        assert_eq!(offsets, EXPECTED_OFFSETS);
    }
}
