use super::*;

const MAP: MapId = MapId("MAP_ISLAND_CAVE");

type Prepare = fn(&mut EventData);

fn started() -> EventData {
    let mut data = EventData::new();
    data.flag_set(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap();
    data
}

fn steps(data: &EventData) -> [u16; 3] {
    [VAR_REGICE_STEPS_1, VAR_REGICE_STEPS_2, VAR_REGICE_STEPS_3].map(|id| data.var_get(id).unwrap())
}

fn set_steps(data: &mut EventData, values: [u16; 3]) {
    data.var_set(VAR_REGICE_STEPS_1, values[0]).unwrap();
    data.var_set(VAR_REGICE_STEPS_2, values[1]).unwrap();
    data.var_set(VAR_REGICE_STEPS_3, values[2]).unwrap();
}

fn snapshot(data: &EventData) -> String {
    format!("{data:?}")
}

#[test]
fn path_matches_upstream_table() {
    assert_eq!(PATH.len(), 36);
    for (index, tile) in [
        (0, (4, 21)),
        (4, (8, 21)),
        (8, (12, 21)),
        (11, (13, 23)),
        (15, (13, 27)),
        (16, (12, 27)),
        (17, (12, 28)),
        (18, (4, 29)),
        (26, (12, 29)),
        (27, (4, 28)),
        (29, (3, 27)),
        (33, (3, 23)),
        (34, (4, 23)),
        (35, (4, 22)),
    ] {
        assert_eq!(PATH[index], tile, "index {index}");
    }
    for (i, tile) in PATH.iter().enumerate() {
        assert!(!PATH[..i].contains(tile), "duplicate {tile:?}");
    }
}

#[test]
fn every_position_sets_exactly_its_bit() {
    for (index, &pos) in PATH.iter().enumerate() {
        let mut data = started();
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, pos, &mut data),
            Ok(false)
        );
        let mut expected = [0u16; 3];
        expected[index / 16] = 1 << (index % 16);
        assert_eq!(steps(&data), expected, "index {index} at {pos:?}");
        assert!(data.flag_get(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap());
        assert!(!data.flag_get(FLAG_TEMP_REGICE_PUZZLE_FAILED).unwrap());
    }
}

#[test]
fn bits_cross_the_variable_boundaries() {
    for (index, expected) in [
        (15, [0x8000, 0, 0]),
        (16, [0, 0x0001, 0]),
        (31, [0, 0x8000, 0]),
        (32, [0, 0, 0x0001]),
        (35, [0, 0, 0x0008]),
    ] {
        let mut data = started();
        should_do_braille_regice_puzzle(MAP, PATH[index], &mut data).unwrap();
        assert_eq!(steps(&data), expected, "index {index}");
    }
}

#[test]
fn bits_accumulate_and_repeat_visits_change_nothing() {
    let mut data = started();
    for pos in [PATH[0], PATH[15], PATH[16], PATH[35]] {
        should_do_braille_regice_puzzle(MAP, pos, &mut data).unwrap();
    }
    assert_eq!(steps(&data), [0x8001, 0x0001, 0x0008]);
    let before = snapshot(&data);
    for pos in [PATH[0], PATH[15], PATH[16], PATH[35]] {
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, pos, &mut data),
            Ok(false)
        );
        assert_eq!(snapshot(&data), before);
    }
}

#[test]
fn inactive_states_return_false_and_change_nothing() {
    let off_path = (0, 0);
    for pos in [PATH[4], PATH[7], off_path] {
        let cases: [(MapId, Prepare); 4] = [
            (MapId("MAP_ROUTE101"), |_| {}),
            (MAP, |d| {
                d.flag_set(FLAG_SYS_BRAILLE_REGICE_COMPLETED).unwrap();
            }),
            (MAP, |d| {
                d.flag_clear(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap();
            }),
            (MAP, |d| {
                d.flag_set(FLAG_TEMP_REGICE_PUZZLE_FAILED).unwrap();
            }),
        ];
        for (map, prepare) in cases {
            let mut data = started();
            set_steps(&mut data, [0x0003, 0x0010, 0x0001]);
            prepare(&mut data);
            let before = snapshot(&data);
            assert_eq!(
                should_do_braille_regice_puzzle(map, pos, &mut data),
                Ok(false)
            );
            assert_eq!(snapshot(&data), before, "{map:?} {pos:?}");
        }
    }
}

#[test]
fn completed_flag_blocks_even_a_full_lap_at_the_finish() {
    let mut data = started();
    set_steps(&mut data, [0xFFFF, 0xFFFF, 0x000F]);
    data.flag_set(FLAG_SYS_BRAILLE_REGICE_COMPLETED).unwrap();
    let before = snapshot(&data);
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
        Ok(false)
    );
    assert_eq!(snapshot(&data), before);
}

#[test]
fn off_path_step_fails_the_puzzle_and_changes_nothing_else() {
    for pos in [
        (0, 0),
        (8, 22),
        (5, 22),
        (14, 21),
        (3, 21),
        (-1, -1),
        (8, 20),
    ] {
        let mut data = started();
        set_steps(&mut data, [0x1234, 0xFFFF, 0x0005]);
        let mut expected = data.clone();
        expected.flag_set(FLAG_TEMP_REGICE_PUZZLE_FAILED).unwrap();
        expected
            .flag_clear(FLAG_TEMP_REGICE_PUZZLE_STARTED)
            .unwrap();
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, pos, &mut data),
            Ok(false)
        );
        assert_eq!(snapshot(&data), snapshot(&expected), "{pos:?}");
        assert!(data.flag_get(FLAG_TEMP_REGICE_PUZZLE_FAILED).unwrap());
        assert!(!data.flag_get(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap());
    }
}

#[test]
fn a_failed_puzzle_stays_failed_on_the_path() {
    let mut data = started();
    should_do_braille_regice_puzzle(MAP, (0, 0), &mut data).unwrap();
    let before = snapshot(&data);
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, PATH[1], &mut data),
        Ok(false)
    );
    assert_eq!(snapshot(&data), before);
}

#[test]
fn completes_only_with_every_bit_set_at_the_finish() {
    // Everything but the finish tile's own bit (index 4) is set.
    let missing_finish = [0xFFEF, 0xFFFF, 0x000F];
    let mut data = started();
    set_steps(&mut data, missing_finish);
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
        Ok(true)
    );
    assert_eq!(steps(&data), [0xFFFF, 0xFFFF, 0x000F]);
    assert!(data.flag_get(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap());
    assert!(!data.flag_get(FLAG_TEMP_REGICE_PUZZLE_FAILED).unwrap());

    // Stepping on the finish again stays complete.
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
        Ok(true)
    );

    // Full but standing on another path tile: not complete.
    for index in [0, 5, 17, 35] {
        let mut data = started();
        set_steps(&mut data, [0xFFFF, 0xFFFF, 0x000F]);
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, PATH[index], &mut data),
            Ok(false)
        );
    }

    // One bit short in each variable, standing at the finish: not complete.
    for short in [
        [0xFFEF, 0xFFFE, 0x000F],
        [0xFFEF, 0xFFFF, 0x0007],
        [0xFFEF, 0x7FFF, 0x000F],
        [0x7FEF, 0xFFFF, 0x000F],
    ] {
        let mut data = started();
        set_steps(&mut data, short);
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
            Ok(false)
        );
        assert!(data.flag_get(FLAG_TEMP_REGICE_PUZZLE_STARTED).unwrap());
    }

    // Upstream compares the third variable exactly, so a stray high bit blocks completion.
    let mut data = started();
    set_steps(&mut data, [0xFFEF, 0xFFFF, 0x001F]);
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
        Ok(false)
    );
}

#[test]
fn a_full_lap_from_the_finish_completes_on_the_return() {
    let mut data = started();
    let lap = (4..36).chain(0..4).map(|i| PATH[i]);
    for pos in lap {
        assert_eq!(
            should_do_braille_regice_puzzle(MAP, pos, &mut data),
            Ok(false),
            "{pos:?}"
        );
    }
    assert_eq!(steps(&data), [0xFFFF, 0xFFFF, 0x000F]);
    assert_eq!(
        should_do_braille_regice_puzzle(MAP, FINISH, &mut data),
        Ok(true)
    );
}
