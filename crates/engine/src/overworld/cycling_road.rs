//! Cycling Road grading and the saved best run (`src/field_specials.c:203-225`, `:237-261`).

use crate::event_data::{EventData, EventDataError};

/// `VAR_CYCLING_ROAD_RECORD_COLLISIONS` (`include/constants/vars.h:57`).
const VAR_CYCLING_ROAD_RECORD_COLLISIONS: u16 = 0x4027;
/// `VAR_CYCLING_ROAD_RECORD_TIME_L` (`include/constants/vars.h:58`).
const VAR_CYCLING_ROAD_RECORD_TIME_L: u16 = 0x4028;
/// `VAR_CYCLING_ROAD_RECORD_TIME_H` (`include/constants/vars.h:59`).
const VAR_CYCLING_ROAD_RECORD_TIME_H: u16 = 0x4029;

const FRAMES_PER_SECOND: u32 = 60;

/// Grade of a Cycling Road run, `DetermineCyclingRoadResults`'s `gSpecialVar_Result`.
///
/// The collision count scores 5 for none, 4 below 4, 3 below 10, 2 below 20,
/// 1 below 100 and 0 beyond. Whole seconds (`frames / 60`) then add 5 up to
/// 10, 4 up to 15, 3 up to 20, 2 up to 40, 1 below 60 and 0 beyond.
#[must_use]
pub const fn cycling_road_grade(frames: u32, collisions: u8) -> u8 {
    let collision_score = match collisions {
        0 => 5,
        1..=3 => 4,
        4..=9 => 3,
        10..=19 => 2,
        20..=99 => 1,
        _ => 0,
    };
    let seconds = frames / FRAMES_PER_SECOND;
    let time_score = match seconds {
        0..=10 => 5,
        11..=15 => 4,
        16..=20 => 3,
        21..=40 => 2,
        41..=59 => 1,
        _ => 0,
    };
    collision_score + time_score
}

/// The best Cycling Road run kept in saved variables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CyclingRoadRecord {
    /// Run length in frames; never zero for a record read from saved data.
    pub frames: u32,
    /// Collisions during the run, as the low byte of the saved word.
    pub collisions: u8,
}

impl CyclingRoadRecord {
    /// Reads the saved record (`GetRecordedCyclingRoadResults`), or `None`
    /// while the saved frame count is zero.
    ///
    /// The saved collision word is 16 bits wide but upstream hands it to a
    /// `u8` parameter, so only its low byte counts.
    ///
    /// # Errors
    ///
    /// Returns the variable error if a record variable id is unsupported.
    pub fn read(data: &EventData) -> Result<Option<Self>, EventDataError> {
        let low = u32::from(data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_L)?);
        let high = u32::from(data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_H)?);
        let frames = low | (high << 16);
        if frames == 0 {
            return Ok(None);
        }
        let collisions = (data.var_get(VAR_CYCLING_ROAD_RECORD_COLLISIONS)? & 0x00FF) as u8;
        Ok(Some(Self { frames, collisions }))
    }

    /// Grade of this record, as `GetRecordedCyclingRoadResults` computes it.
    #[must_use]
    pub const fn grade(self) -> u8 {
        cycling_road_grade(self.frames, self.collisions)
    }

    /// Stores a finished run when no record exists or it is strictly faster
    /// (`RecordCyclingRoadResults`), returning whether it was stored. The
    /// collision count is never compared.
    ///
    /// # Errors
    ///
    /// Returns the variable error if a record variable id is unsupported.
    pub fn record_if_faster(
        data: &mut EventData,
        frames: u32,
        collisions: u8,
    ) -> Result<bool, EventDataError> {
        let faster = Self::read(data)?.is_none_or(|record| record.frames > frames);
        if faster {
            data.var_set(VAR_CYCLING_ROAD_RECORD_TIME_L, (frames & 0xFFFF) as u16)?;
            data.var_set(VAR_CYCLING_ROAD_RECORD_TIME_H, (frames >> 16) as u16)?;
            data.var_set(VAR_CYCLING_ROAD_RECORD_COLLISIONS, u16::from(collisions))?;
        }
        Ok(faster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::block::SaveBlock1;

    #[test]
    fn collision_categories_change_at_literal_boundaries() {
        let cases = [
            (0, 5),
            (1, 4),
            (3, 4),
            (4, 3),
            (9, 3),
            (10, 2),
            (19, 2),
            (20, 1),
            (99, 1),
            (100, 0),
            (255, 0),
        ];
        for (collisions, score) in cases {
            assert_eq!(cycling_road_grade(3600, collisions), score, "{collisions}");
        }
    }

    #[test]
    fn timing_categories_change_at_whole_second_boundaries() {
        let cases = [
            (0, 5),
            (659, 5),
            (660, 4),
            (959, 4),
            (960, 3),
            (1259, 3),
            (1260, 2),
            (2459, 2),
            (2460, 1),
            (3599, 1),
            (3600, 0),
            (u32::MAX, 0),
        ];
        for (frames, score) in cases {
            assert_eq!(cycling_road_grade(frames, 100), score, "{frames}");
        }
    }

    #[test]
    fn grade_sums_both_scores() {
        assert_eq!(cycling_road_grade(659, 0), 10);
        assert_eq!(cycling_road_grade(1260, 4), 5);
        assert_eq!(cycling_road_grade(3599, 99), 2);
        assert_eq!(cycling_road_grade(3600, 100), 0);
    }

    #[test]
    fn zero_frames_means_no_recorded_result() {
        let mut data = EventData::new();
        data.var_set(VAR_CYCLING_ROAD_RECORD_COLLISIONS, 7).unwrap();
        assert_eq!(CyclingRoadRecord::read(&data), Ok(None));
    }

    #[test]
    fn first_run_is_recorded_across_both_words() {
        let mut data = EventData::new();
        assert_eq!(
            CyclingRoadRecord::record_if_faster(&mut data, 5000, 3),
            Ok(true)
        );
        assert_eq!(data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_L), Ok(5000));
        assert_eq!(data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_H), Ok(0));
        assert_eq!(data.var_get(VAR_CYCLING_ROAD_RECORD_COLLISIONS), Ok(3));
        assert_eq!(
            CyclingRoadRecord::read(&data),
            Ok(Some(CyclingRoadRecord {
                frames: 5000,
                collisions: 3
            }))
        );
    }

    #[test]
    fn faster_run_replaces_time_and_collisions() {
        let mut data = EventData::new();
        CyclingRoadRecord::record_if_faster(&mut data, 5000, 3).unwrap();
        assert_eq!(
            CyclingRoadRecord::record_if_faster(&mut data, 4999, 40),
            Ok(true)
        );
        assert_eq!(
            CyclingRoadRecord::read(&data),
            Ok(Some(CyclingRoadRecord {
                frames: 4999,
                collisions: 40
            }))
        );
    }

    #[test]
    fn equal_or_slower_run_leaves_every_field_alone() {
        for (frames, collisions) in [(5000, 0), (5000, 200), (5001, 0), (u32::MAX, 0)] {
            let mut data = EventData::new();
            CyclingRoadRecord::record_if_faster(&mut data, 5000, 3).unwrap();
            let before = data.clone();
            assert_eq!(
                CyclingRoadRecord::record_if_faster(&mut data, frames, collisions),
                Ok(false)
            );
            assert_eq!(data.vars_raw(), before.vars_raw());
        }
    }

    #[test]
    fn zero_frame_run_replaces_only_while_a_record_is_absent() {
        let mut data = EventData::new();
        assert_eq!(
            CyclingRoadRecord::record_if_faster(&mut data, 0, 9),
            Ok(true)
        );
        assert_eq!(CyclingRoadRecord::read(&data), Ok(None));
        assert_eq!(data.var_get(VAR_CYCLING_ROAD_RECORD_COLLISIONS), Ok(9));
    }

    #[test]
    fn collision_word_beyond_a_byte_is_truncated_for_grading() {
        let mut data = EventData::new();
        data.var_set(VAR_CYCLING_ROAD_RECORD_TIME_L, 600).unwrap();
        data.var_set(VAR_CYCLING_ROAD_RECORD_COLLISIONS, 0x0100)
            .unwrap();
        let record = CyclingRoadRecord::read(&data).unwrap().unwrap();
        assert_eq!(record.collisions, 0);
        assert_eq!(record.grade(), 10);
    }

    #[test]
    fn high_word_frame_count_round_trips_through_a_save_and_spares_other_vars() {
        let frames = 0x0002_ABCD;
        let mut block = SaveBlock1::default();
        let neighbours = [
            VAR_CYCLING_ROAD_RECORD_COLLISIONS - 1,
            VAR_CYCLING_ROAD_RECORD_TIME_H + 1,
        ];
        for (offset, id) in neighbours.into_iter().enumerate() {
            block
                .event_data
                .var_set(id, 0x1111 + u16::try_from(offset).unwrap())
                .unwrap();
        }
        let before = block.event_data.clone();

        CyclingRoadRecord::record_if_faster(&mut block.event_data, frames, 12).unwrap();
        assert_eq!(
            block.event_data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_L),
            Ok(0xABCD)
        );
        assert_eq!(
            block.event_data.var_get(VAR_CYCLING_ROAD_RECORD_TIME_H),
            Ok(2)
        );

        let bytes = block.to_bytes(0x1234_5678);
        let restored = SaveBlock1::from_bytes(&bytes, 0x1234_5678).unwrap();
        assert_eq!(
            CyclingRoadRecord::read(&restored.event_data),
            Ok(Some(CyclingRoadRecord {
                frames,
                collisions: 12
            }))
        );
        for (index, (after, original)) in restored
            .event_data
            .vars_raw()
            .iter()
            .zip(before.vars_raw())
            .enumerate()
        {
            let id = 0x4000 + u16::try_from(index).unwrap();
            let touched = [
                VAR_CYCLING_ROAD_RECORD_COLLISIONS,
                VAR_CYCLING_ROAD_RECORD_TIME_L,
                VAR_CYCLING_ROAD_RECORD_TIME_H,
            ];
            if !touched.contains(&id) {
                assert_eq!(after, original, "{id:#x}");
            }
        }
        assert_eq!(restored.event_data.flag_bytes(), before.flag_bytes());
    }
}
