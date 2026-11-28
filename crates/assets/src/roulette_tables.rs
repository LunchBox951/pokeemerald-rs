//! The Game Corner Roulette's two per-table motion settings: how fast the
//! wheel and ball move, how far the ball travels, and where the Shroomish and
//! Taillow helpers act on it.
//!
//! Source: `src/roulette.c` (`sRouletteTables` and the `RouletteTable`,
//! `Shroomish` and `Taillow` structs; the distances derive from
//! `DEGREES_PER_SLOT`, which is 30 degrees because the wheel has 12 slots).
//!
//! The tables are ordered left, right, and a table id selects its row; any id
//! other than `0` and `1` is refused instead of clamped. The minimum bet is
//! not part of a row: the game never reads that field of `sRouletteTables`
//! (it takes the bet from `sTableMinBets`). The physics, sprites, random
//! travel selection, wheel-slot data, betting and coin changes are not ported
//! here.

use std::error::Error;
use std::fmt;

/// The number of roulette tables (left and right).
pub const ROULETTE_TABLE_COUNT: usize = 2;

/// A failed lookup in the Roulette table data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouletteTableError {
    /// The table id is not below [`ROULETTE_TABLE_COUNT`].
    UnknownTable(usize),
}

impl fmt::Display for RouletteTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTable(table) => write!(
                f,
                "unknown Roulette table `{table}` (there are {ROULETTE_TABLE_COUNT})"
            ),
        }
    }
}

impl Error for RouletteTableError {}

/// The Shroomish helper settings (`struct Shroomish`).
///
/// The game compares the two angles against the ball's angle in degrees,
/// adding 180 when the ball is stuck on the left side of the wheel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShroomishSettings {
    /// The ball angle at which the Shroomish's shadow appears.
    pub start_angle: u16,
    /// The ball angle at which the Shroomish starts to fall in.
    pub drop_angle: u16,
    /// The divisor applied to the slot offset to give the ball's fall speed.
    pub fall_slowdown: u16,
}

/// The Taillow helper timing (`struct Taillow`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaillowSettings {
    /// The part of the Taillow's drop delay that is added to 45 and to a slot
    /// offset scaled by [`RouletteMotionSettings::rand_distance_high`].
    pub base_drop_delay: u16,
    /// The Taillow reacts when the ball angle is this plus 90 (ball stuck on
    /// the right side of the wheel).
    pub right_start_angle: u16,
    /// The Taillow reacts when the ball angle is this plus 270 (ball stuck on
    /// the left side of the wheel).
    pub left_start_angle: u16,
}

/// One table's motion settings (`struct RouletteTable`).
///
/// Distances and angles are in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouletteMotionSettings {
    /// The larger random-distance step (`randDistanceHigh`): one of the
    /// distances the random ball-travel choice returns, and the factor that
    /// scales a slot offset into a ball fall distance and a Taillow drop
    /// delay.
    pub rand_distance_high: u8,
    /// The smaller random-distance step (`randDistanceLow`): one of the
    /// distances the random ball-travel choice returns, whole or halved or
    /// quartered, and the offset added (less one) to a ball fall distance.
    pub rand_distance_low: u8,
    /// The degrees the wheel turns each time it moves (`wheelSpeed`), and
    /// the numerator of the ball slowdown when it reaches its landing slot.
    pub wheel_speed: u8,
    /// The number of frames between wheel turns (`wheelDelay`); the ball
    /// slowdown divides by this plus one.
    pub wheel_delay: u8,
    /// The Shroomish helper angles.
    pub shroomish: ShroomishSettings,
    /// The Taillow helper timing.
    pub taillow: TaillowSettings,
    /// The ball's angular speed at the start of a roll (`ballSpeed`).
    pub ball_speed: u16,
    /// The base distance the ball travels before random adjustment
    /// (`baseTravelDist`).
    pub base_travel_dist: u16,
    /// The ball's angular speed when it sticks to the wheel (`var1C`), the
    /// raw upstream `f32`; negative on both tables.
    pub stuck_ball_angle_speed: f32,
}

/// `sRouletteTables`, left then right.
const TABLES: [RouletteMotionSettings; ROULETTE_TABLE_COUNT] = [
    RouletteMotionSettings {
        rand_distance_high: 60,
        rand_distance_low: 30,
        wheel_speed: 1,
        wheel_delay: 1,
        shroomish: ShroomishSettings {
            start_angle: 45,
            drop_angle: 30,
            fall_slowdown: 1,
        },
        taillow: TaillowSettings {
            base_drop_delay: 75,
            right_start_angle: 27,
            left_start_angle: 24,
        },
        ball_speed: 10,
        base_travel_dist: 360,
        stuck_ball_angle_speed: -0.5,
    },
    RouletteMotionSettings {
        rand_distance_high: 30,
        rand_distance_low: 15,
        wheel_speed: 1,
        wheel_delay: 0,
        shroomish: ShroomishSettings {
            start_angle: 75,
            drop_angle: 60,
            fall_slowdown: 2,
        },
        taillow: TaillowSettings {
            base_drop_delay: 0,
            right_start_angle: 54,
            left_start_angle: 48,
        },
        ball_speed: 10,
        base_travel_dist: 270,
        stuck_ball_angle_speed: -1.0,
    },
];

/// Returns the motion settings of Roulette table `table` (`0` left, `1`
/// right).
///
/// # Errors
///
/// Returns [`RouletteTableError::UnknownTable`] when `table` is not below
/// [`ROULETTE_TABLE_COUNT`].
pub fn roulette_motion_at(
    table: usize,
) -> Result<&'static RouletteMotionSettings, RouletteTableError> {
    TABLES
        .get(table)
        .ok_or(RouletteTableError::UnknownTable(table))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(table: usize) -> &'static RouletteMotionSettings {
        roulette_motion_at(table).expect("table exists")
    }

    #[test]
    fn the_left_table_is_pinned_field_by_field() {
        let left = at(0);
        assert_eq!(left.rand_distance_high, 60);
        assert_eq!(left.rand_distance_low, 30);
        assert_eq!(left.wheel_speed, 1);
        assert_eq!(left.wheel_delay, 1);
        assert_eq!(left.shroomish.start_angle, 45);
        assert_eq!(left.shroomish.drop_angle, 30);
        assert_eq!(left.shroomish.fall_slowdown, 1);
        assert_eq!(left.taillow.base_drop_delay, 75);
        assert_eq!(left.taillow.right_start_angle, 27);
        assert_eq!(left.taillow.left_start_angle, 24);
        assert_eq!(left.ball_speed, 10);
        assert_eq!(left.base_travel_dist, 360);
        assert_eq!(left.stuck_ball_angle_speed.to_bits(), 0xBF00_0000);
    }

    #[test]
    fn the_right_table_is_pinned_field_by_field() {
        let right = at(1);
        assert_eq!(right.rand_distance_high, 30);
        assert_eq!(right.rand_distance_low, 15);
        assert_eq!(right.wheel_speed, 1);
        assert_eq!(right.wheel_delay, 0);
        assert_eq!(right.shroomish.start_angle, 75);
        assert_eq!(right.shroomish.drop_angle, 60);
        assert_eq!(right.shroomish.fall_slowdown, 2);
        assert_eq!(right.taillow.base_drop_delay, 0);
        assert_eq!(right.taillow.right_start_angle, 54);
        assert_eq!(right.taillow.left_start_angle, 48);
        assert_eq!(right.ball_speed, 10);
        assert_eq!(right.base_travel_dist, 270);
        assert_eq!(right.stuck_ball_angle_speed.to_bits(), 0xBF80_0000);
    }

    #[test]
    fn an_unknown_table_is_refused() {
        assert_eq!(
            roulette_motion_at(ROULETTE_TABLE_COUNT),
            Err(RouletteTableError::UnknownTable(2))
        );
        assert_eq!(
            roulette_motion_at(usize::MAX),
            Err(RouletteTableError::UnknownTable(usize::MAX))
        );
        assert!(roulette_motion_at(ROULETTE_TABLE_COUNT - 1).is_ok());
    }

    #[test]
    fn errors_name_the_offending_table() {
        assert!(RouletteTableError::UnknownTable(7)
            .to_string()
            .contains('7'));
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

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let source = upstream_file("src/roulette.c");
        let from = source
            .find("static const struct RouletteTable sRouletteTables[] =")
            .expect("table found");
        let body = &source[from..];
        let body = &body[..body.find("\n};").expect("table ends")];

        // Every `.field = value` initializer, in upstream order, with the
        // slot-width expressions resolved (360 degrees over 12 slots).
        let values: Vec<f32> = body
            .lines()
            .filter_map(|line| {
                let value = line.trim().strip_prefix('.')?.split_once('=')?.1;
                let value = value.trim().trim_end_matches(',').trim();
                if value == "{" {
                    return None;
                }
                let value = value.trim_end_matches('f');
                Some(match value {
                    "DEGREES_PER_SLOT * 2" => 60.0,
                    "DEGREES_PER_SLOT" => 30.0,
                    "DEGREES_PER_SLOT / 2" => 15.0,
                    number => number.parse().unwrap_or_else(|_| panic!("`{number}`")),
                })
            })
            .collect();
        // Skip the unread minimum bet at the head of each row.
        let rows: Vec<&[f32]> = values.chunks(14).collect();
        assert_eq!(rows.len(), ROULETTE_TABLE_COUNT);
        for (table, row) in rows.iter().enumerate() {
            let settings = at(table);
            let ours = [
                f32::from(settings.rand_distance_high),
                f32::from(settings.rand_distance_low),
                f32::from(settings.wheel_speed),
                f32::from(settings.wheel_delay),
                f32::from(settings.shroomish.start_angle),
                f32::from(settings.shroomish.drop_angle),
                f32::from(settings.shroomish.fall_slowdown),
                f32::from(settings.taillow.base_drop_delay),
                f32::from(settings.taillow.right_start_angle),
                f32::from(settings.taillow.left_start_angle),
                f32::from(settings.ball_speed),
                f32::from(settings.base_travel_dist),
                settings.stuck_ball_angle_speed,
            ];
            assert_eq!(row.len(), 14, "table {table}");
            for (index, (&theirs, &ours)) in row[1..].iter().zip(&ours).enumerate() {
                assert_eq!(
                    theirs.to_bits(),
                    ours.to_bits(),
                    "table {table} field {index}"
                );
            }
        }
    }
}
