//! Battle-animation reader tests over a synthetic ROM.
//!
//! The fixture plants the four selector tables, three program labels (two of
//! them sharing one address, as `Move_NONE` and `Move_POUND` do), one sprite
//! template, and one task function. The programs exercise every operand kind
//! the roots resolve. The failure tests break one root or one stream at a
//! time.

use assets::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationError, BattleAnimationProgram, EffectSpriteId, PanSweep, ProgramId,
};
use pack_format::{parse_directory, PackWriter};

use super::command::{decode_program, AnimPointers};
use super::{decode_battle_anim_payload, write, PACK_ID};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, RomReader, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{AnimProgramRoot, AnimSymbolRoot, BattleAnimationRoots, Roots};

/// Where the fixture plants each table.
const TABLES: [usize; 4] = [0x1000, 0x2000, 0x2100, 0x2200];
/// The label shared by `Alias` and `Short`.
const SHORT: usize = 0x3000;
/// The program that uses every operand kind.
const MAIN: usize = 0x3010;
const TEMPLATE: usize = 0x5000;
/// A task function: code, so its operand carries the Thumb bit.
const TASK: usize = 0x6000;
const SLOT_COUNT: usize = 355 + 9 + 23 + 7;

fn small(off: usize) -> u32 {
    u32::try_from(off).expect("a fixture offset fits in u32")
}

fn at(off: usize) -> GbaPtr {
    GbaPtr::at(ROM_BASE + small(off))
}

fn ptr_bytes(off: usize) -> [u8; 4] {
    (ROM_BASE + small(off)).to_le_bytes()
}

/// `Short`: a bare `end`.
fn short_bytes() -> Vec<u8> {
    vec![0x08]
}

/// `Main`: a sprite sheet, a sprite, a task, a blend, a call, then a `goto`.
fn main_bytes() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x15, 0x27];
    bytes.push(0x02);
    bytes.extend(ptr_bytes(TEMPLATE));
    bytes.extend([0x83, 0x02, 0xFF, 0xFF, 0x07, 0x00]);
    bytes.push(0x03);
    bytes.extend((ROM_BASE + small(TASK) + 1).to_le_bytes());
    bytes.extend([0x02, 0x01, 0x05, 0x00]);
    bytes.extend([0x0C, 0x0C, 0x08]);
    bytes.push(0x0E);
    bytes.extend(ptr_bytes(SHORT));
    bytes.push(0x13);
    bytes.extend(ptr_bytes(SHORT));
    bytes
}

fn symbol(name: &str) -> AnimSymbol {
    AnimSymbol::new(name).expect("a fixture symbol")
}

fn main_commands() -> Vec<AnimCommand> {
    vec![
        AnimCommand::LoadSpriteGfx(EffectSpriteId::new(5).unwrap()),
        AnimCommand::CreateSprite {
            template: symbol("gTemplate"),
            relative_to_target: true,
            subpriority: 3,
            args: vec![-1, 7],
        },
        AnimCommand::CreateVisualTask {
            task: symbol("AnimTask_Fixture"),
            priority: 2,
            args: vec![5],
        },
        AnimCommand::SetAlpha { eva: 12, evb: 8 },
        AnimCommand::Call(ProgramId::new(0)),
        AnimCommand::Goto(ProgramId::new(0)),
    ]
}

/// The catalog the fixture's roots describe: programs `Short`, `Alias`,
/// `Main`; every slot runs `Short` except the last of each table, which runs
/// `Main`, and the first move slot, which runs `Alias`.
fn selector_programs() -> Vec<u16> {
    let mut slots = Vec::new();
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            slots.push(if index + 1 == table.len() {
                2
            } else if table == AnimationTable::Moves && index == 0 {
                1
            } else {
                0
            });
        }
    }
    slots
}

fn expected_payload() -> Vec<u8> {
    let programs = vec![
        BattleAnimationProgram::new(symbol("Short"), vec![AnimCommand::End]).unwrap(),
        BattleAnimationProgram::new(symbol("Alias"), vec![AnimCommand::End]).unwrap(),
        BattleAnimationProgram::new(symbol("Main"), main_commands()).unwrap(),
    ];
    let mut slots = selector_programs().into_iter();
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| AnimationSelector::new(table, index).unwrap())
    });
    let selectors: Vec<_> = selectors
        .map(|selector| (selector, ProgramId::new(slots.next().unwrap())))
        .collect();
    BattleAnimationCatalog::new(programs, selectors)
        .unwrap()
        .encode()
}

fn program_roots() -> Vec<AnimProgramRoot> {
    vec![
        AnimProgramRoot {
            symbol: "Short",
            addr: at(SHORT),
            len: 1,
        },
        AnimProgramRoot {
            symbol: "Alias",
            addr: at(SHORT),
            len: 1,
        },
        AnimProgramRoot {
            symbol: "Main",
            addr: at(MAIN),
            len: small_len(&main_bytes()),
        },
    ]
}

fn small_len(bytes: &[u8]) -> u16 {
    u16::try_from(bytes.len()).expect("a fixture program fits in u16")
}

fn leak<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

fn family() -> BattleAnimationRoots {
    BattleAnimationRoots {
        tables: TABLES.map(at),
        programs: leak(program_roots()),
        templates: leak(vec![AnimSymbolRoot {
            symbol: "gTemplate",
            addr: at(TEMPLATE),
        }]),
        tasks: leak(vec![AnimSymbolRoot {
            symbol: "AnimTask_Fixture",
            addr: GbaPtr::at(ROM_BASE + small(TASK) + 1),
        }]),
        selectors: leak(selector_programs()),
    }
}

/// The fixture with `main` planted as the `Main` program.
fn fixture_with(main: &[u8]) -> RomFixture {
    let programs = program_roots();
    let mut rom = RomFixture::new()
        .emerald_header()
        .write(SHORT, &short_bytes())
        .write(MAIN, main);
    let mut slots = selector_programs().into_iter();
    for (table, base) in AnimationTable::ALL.into_iter().zip(TABLES) {
        for index in 0..usize::from(table.len()) {
            let program = usize::from(slots.next().expect("a slot per selector"));
            rom = rom.write_ptr(base + index * 4, programs[program].addr);
        }
    }
    rom
}

fn fixture() -> RomFixture {
    fixture_with(&main_bytes())
}

fn rom_of(rom: RomFixture) -> Rom {
    Rom::from_bytes(rom.finish()).expect("the fixture header is valid")
}

fn payload(rom: &Rom, family: &BattleAnimationRoots) -> Result<Vec<u8>, ImportError> {
    decode_battle_anim_payload(&rom.reader(), family)
}

#[test]
fn matching_roots_yield_the_canonical_catalog() {
    let rom = rom_of(fixture());
    let bytes = payload(&rom, &family()).expect("a consistent catalog");
    assert_eq!(bytes, expected_payload());

    let catalog = BattleAnimationCatalog::decode(&bytes).expect("canonical bytes");
    let pound = AnimationSelector::new(AnimationTable::Moves, 0).unwrap();
    assert_eq!(catalog.program(pound).symbol().as_str(), "Alias");
    let last = AnimationSelector::new(AnimationTable::Special, 6).unwrap();
    assert_eq!(catalog.program(last).commands(), main_commands());
}

#[test]
fn a_branch_to_a_shared_address_names_the_first_label() {
    let rom = rom_of(fixture());
    let bytes = payload(&rom, &family()).expect("a consistent catalog");
    let catalog = BattleAnimationCatalog::decode(&bytes).unwrap();
    let main = &catalog.programs()[2];
    assert_eq!(main.commands()[4], AnimCommand::Call(ProgramId::new(0)));
}

#[test]
fn the_domain_writes_one_raw_entry() {
    let rom = rom_of(fixture());
    let roots = Roots {
        battle_anims: family(),
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    write(&rom, &roots, &mut writer).expect("a consistent catalog");
    let bytes = writer.finish().expect("the queued entry is valid");
    let directory = parse_directory(&bytes).expect("the pack parses");
    let [entry] = directory.as_slice() else {
        panic!("one entry, got {}", directory.len())
    };
    assert_eq!(entry.id, PACK_ID);
    assert_eq!(
        bytes[entry.offset..entry.offset + entry.length],
        expected_payload()
    );
}

#[test]
fn a_profile_without_battle_animations_writes_nothing() {
    let rom = rom_of(fixture());
    let mut writer = PackWriter::new();
    write(&rom, &Roots::NONE, &mut writer).expect("no roots is no work");
    let bytes = writer.finish().expect("an empty pack is valid");
    assert!(parse_directory(&bytes).expect("the pack parses").is_empty());
}

#[test]
fn a_miscounted_selector_list_is_refused() {
    let rom = rom_of(fixture());
    let mut slots = selector_programs();
    slots.pop();
    let err = payload(
        &rom,
        &BattleAnimationRoots {
            selectors: leak(slots),
            ..family()
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: PACK_ID,
                field: "selector count"
            }
        ),
        "{err}"
    );
}

#[test]
fn an_unset_family_is_refused_rather_than_read_as_empty() {
    let rom = rom_of(fixture());
    let err = payload(&rom, &BattleAnimationRoots::NONE).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "selector count",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_selector_naming_no_program_is_refused() {
    let rom = rom_of(fixture());
    let mut slots = selector_programs();
    slots[10] = 3;
    let err = payload(
        &rom,
        &BattleAnimationRoots {
            selectors: leak(slots),
            ..family()
        },
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "selector program",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_slot_that_does_not_start_its_program_is_refused() {
    let rom = rom_of(fixture().write_ptr(TABLES[1] + 4, at(MAIN)));
    let err = payload(&rom, &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "selector table",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_slot_outside_the_cartridge_is_refused() {
    let rom = rom_of(fixture().write(TABLES[2], &0x0300_0000u32.to_le_bytes()));
    let err = payload(&rom, &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::PointerOutOfRange {
                raw: 0x0300_0000,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_table_that_runs_off_the_image_is_refused() {
    let rom = rom_of(fixture());
    let err = payload(
        &rom,
        &BattleAnimationRoots {
            tables: [
                at(ROM_SIZE_BYTES + 8),
                at(TABLES[1]),
                at(TABLES[2]),
                at(TABLES[3]),
            ],
            ..family()
        },
    )
    .unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

const ROM_SIZE_BYTES: usize = crate::rom::ROM_SIZE;

#[test]
fn a_branch_to_an_unrecorded_program_is_refused() {
    let mut main = main_bytes();
    let goto = main.len() - 4;
    main[goto..].copy_from_slice(&ptr_bytes(0x7000));
    let rom = rom_of(fixture_with(&main));
    let err = payload(&rom, &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::AnimationPointer {
                what: "a program",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_command_naming_an_unrecorded_template_or_task_is_refused() {
    let mut main = main_bytes();
    main[4..8].copy_from_slice(&ptr_bytes(0x7000));
    let err = payload(&rom_of(fixture_with(&main)), &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::AnimationPointer {
                what: "a template",
                ..
            }
        ),
        "{err}"
    );

    let mut main = main_bytes();
    main[15..19].copy_from_slice(&ptr_bytes(TASK));
    let err = payload(&rom_of(fixture_with(&main)), &family()).unwrap_err();
    assert!(
        matches!(err, ImportError::AnimationPointer { what: "a task", .. }),
        "{err}"
    );
}

#[test]
fn an_operand_outside_the_cartridge_is_refused() {
    let mut main = main_bytes();
    main[4..8].copy_from_slice(&0x0200_0000u32.to_le_bytes());
    let err = payload(&rom_of(fixture_with(&main)), &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::PointerOutOfRange {
                raw: 0x0200_0000,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn an_unknown_command_byte_is_refused() {
    let mut main = main_bytes();
    main[0] = 0x30;
    let err = payload(&rom_of(fixture_with(&main)), &family()).unwrap_err();
    assert!(
        matches!(err, ImportError::AnimationOpcode { opcode: 0x30, .. }),
        "{err}"
    );
}

#[test]
fn a_sprite_tag_below_the_first_effect_sprite_is_refused() {
    let mut main = main_bytes();
    main[1..3].copy_from_slice(&9_999u16.to_le_bytes());
    let err = payload(&rom_of(fixture_with(&main)), &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BattleAnimation {
                source: BattleAnimationError::SpriteIdOutOfRange(9_999),
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn more_arguments_than_gbattleanimargs_holds_are_refused() {
    let mut main = vec![0x02];
    main.extend(ptr_bytes(TEMPLATE));
    main.extend([0x03, 9]);
    main.extend([0u8; 18]);
    main.push(0x08);
    let roots = BattleAnimationRoots {
        programs: leak(vec![
            program_roots()[0],
            program_roots()[1],
            AnimProgramRoot {
                len: small_len(&main),
                ..program_roots()[2]
            },
        ]),
        ..family()
    };
    let err = payload(&rom_of(fixture_with(&main)), &roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BattleAnimation {
                source: BattleAnimationError::ArgCountTooLarge(9),
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_program_stops_at_its_first_terminator() {
    // `return` then `end`, the `Move_BIDE` shape: the second terminator
    // belongs to whatever label follows.
    let main = [0x04, 0x01, 0x0F, 0x08];
    let roots = BattleAnimationRoots {
        programs: leak(vec![
            program_roots()[0],
            program_roots()[1],
            AnimProgramRoot {
                len: 3,
                ..program_roots()[2]
            },
        ]),
        ..family()
    };
    let bytes =
        payload(&rom_of(fixture_with(&main)), &roots).expect("the first terminator ends it");
    let catalog = BattleAnimationCatalog::decode(&bytes).unwrap();
    assert_eq!(
        catalog.programs()[2].commands(),
        [AnimCommand::Delay(1), AnimCommand::Return]
    );

    // A root that counts the trailing `end` is wrong.
    let roots = BattleAnimationRoots {
        programs: leak(vec![
            program_roots()[0],
            program_roots()[1],
            AnimProgramRoot {
                len: 4,
                ..program_roots()[2]
            },
        ]),
        ..family()
    };
    let err = payload(&rom_of(fixture_with(&main)), &roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "program length",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_program_shorter_than_its_root_claims_is_refused() {
    let roots = BattleAnimationRoots {
        programs: leak(vec![
            AnimProgramRoot {
                len: 2,
                ..program_roots()[0]
            },
            program_roots()[1],
            program_roots()[2],
        ]),
        ..family()
    };
    let err = payload(&rom_of(fixture()), &roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "program length",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_stream_with_no_terminator_is_bounded_by_its_root() {
    // Nothing but `nop` after the label: the walk stops at the root's length
    // instead of reading to the end of the image.
    let rom = rom_of(fixture_with(&[0x06; 64]));
    let err = payload(&rom, &family()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "program length",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn two_templates_at_one_address_are_refused() {
    let roots = BattleAnimationRoots {
        templates: leak(vec![
            AnimSymbolRoot {
                symbol: "gTemplate",
                addr: at(TEMPLATE),
            },
            AnimSymbolRoot {
                symbol: "gOther",
                addr: at(TEMPLATE),
            },
        ]),
        ..family()
    };
    let err = payload(&rom_of(fixture()), &roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "template roots",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_repeated_program_label_is_refused() {
    let roots = BattleAnimationRoots {
        programs: leak(vec![
            program_roots()[0],
            AnimProgramRoot {
                symbol: "Short",
                ..program_roots()[1]
            },
            program_roots()[2],
        ]),
        ..family()
    };
    let err = payload(&rom_of(fixture()), &roots).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BattleAnimation {
                source: BattleAnimationError::DuplicateSymbol(_),
                ..
            }
        ),
        "{err}"
    );
}

/// Resolves every operand to a fixed identity, so a test reads the bytes.
struct Fixed;

impl AnimPointers for Fixed {
    fn template(&mut self, _at: usize, _ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        Ok(symbol("T"))
    }

    fn task(&mut self, _at: usize, _ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        Ok(symbol("K"))
    }

    fn program(&mut self, _at: usize, _ptr: GbaPtr) -> Result<ProgramId, ImportError> {
        Ok(ProgramId::new(1))
    }
}

fn sweep() -> PanSweep {
    PanSweep {
        se: 0x0102,
        current_pan: -64,
        target_pan: 63,
        increment_pan: -1,
        delay: 4,
    }
}

#[test]
fn every_command_reads_its_macro_layout() {
    let ptr = ptr_bytes(SHORT);
    let sweep_bytes = [0x02, 0x01, 0xC0, 0x3F, 0xFF, 0x04];
    let cases: Vec<(Vec<u8>, AnimCommand)> = vec![
        (
            vec![0x00, 0x10, 0x27],
            AnimCommand::LoadSpriteGfx(EffectSpriteId::new(0).unwrap()),
        ),
        (
            vec![0x01, 0x30, 0x28],
            AnimCommand::UnloadSpriteGfx(EffectSpriteId::new(288).unwrap()),
        ),
        (
            [&[0x02][..], &ptr, &[0x05, 0x00]].concat(),
            AnimCommand::CreateSprite {
                template: symbol("T"),
                relative_to_target: false,
                subpriority: 5,
                args: vec![],
            },
        ),
        (
            [&[0x03][..], &ptr, &[0x07, 0x02, 0xFE, 0xFF, 0x00, 0x80]].concat(),
            AnimCommand::CreateVisualTask {
                task: symbol("K"),
                priority: 7,
                args: vec![-2, i16::MIN],
            },
        ),
        (vec![0x04, 9], AnimCommand::Delay(9)),
        (vec![0x05], AnimCommand::WaitForVisualFinish),
        (vec![0x06], AnimCommand::Nop),
        (vec![0x07], AnimCommand::Nop2),
        (vec![0x08], AnimCommand::End),
        (vec![0x09, 0x34, 0x12], AnimCommand::PlaySe(0x1234)),
        (vec![0x0A, 1], AnimCommand::MonBg(1)),
        (vec![0x0B, 2], AnimCommand::ClearMonBg(2)),
        (vec![0x0C, 12, 8], AnimCommand::SetAlpha { eva: 12, evb: 8 }),
        (vec![0x0D], AnimCommand::BlendOff),
        (
            [&[0x0E][..], &ptr].concat(),
            AnimCommand::Call(ProgramId::new(1)),
        ),
        (vec![0x0F], AnimCommand::Return),
        (
            vec![0x10, 15, 0xFF, 0xFF],
            AnimCommand::SetArg { arg: 15, value: -1 },
        ),
        (
            [&[0x11][..], &ptr, &ptr].concat(),
            AnimCommand::ChooseTwoTurnAnim {
                even: ProgramId::new(1),
                odd: ProgramId::new(1),
            },
        ),
        (
            [&[0x12, 1][..], &ptr].concat(),
            AnimCommand::JumpIfMoveTurn {
                turn: 1,
                target: ProgramId::new(1),
            },
        ),
        (
            [&[0x13][..], &ptr].concat(),
            AnimCommand::Goto(ProgramId::new(1)),
        ),
        (vec![0x14, 3], AnimCommand::FadeToBg(3)),
        (vec![0x15], AnimCommand::RestoreBg),
        (vec![0x16], AnimCommand::WaitBgFadeOut),
        (vec![0x17], AnimCommand::WaitBgFadeIn),
        (vec![0x18, 4], AnimCommand::ChangeBg(4)),
        (
            vec![0x19, 0x02, 0x01, 0xC0],
            AnimCommand::PlaySeWithPan {
                se: 0x0102,
                pan: -64,
            },
        ),
        (vec![0x1A, 63], AnimCommand::SetPan(63)),
        (
            [&[0x1B][..], &sweep_bytes].concat(),
            AnimCommand::PanSe(sweep()),
        ),
        (
            vec![0x1C, 0x02, 0x01, 0x3F, 6, 7],
            AnimCommand::LoopSeWithPan {
                se: 0x0102,
                pan: 63,
                wait: 6,
                times: 7,
            },
        ),
        (
            vec![0x1D, 0x02, 0x01, 0xC0, 8],
            AnimCommand::WaitPlaySeWithPan {
                se: 0x0102,
                pan: -64,
                wait: 8,
            },
        ),
        (vec![0x1E, 0xFF, 0x3F], AnimCommand::SetBldCnt(0x3FFF)),
        (
            [&[0x1F][..], &ptr, &[0x01, 0x03, 0x00]].concat(),
            AnimCommand::CreateSoundTask {
                task: symbol("K"),
                args: vec![3],
            },
        ),
        (vec![0x20], AnimCommand::WaitSound),
        (
            [&[0x21, 15, 0x01, 0x00][..], &ptr].concat(),
            AnimCommand::JumpArgEq {
                arg: 15,
                value: 1,
                target: ProgramId::new(1),
            },
        ),
        (vec![0x22, 5], AnimCommand::MonBgStatic(5)),
        (vec![0x23, 6], AnimCommand::ClearMonBgStatic(6)),
        (
            [&[0x24][..], &ptr].concat(),
            AnimCommand::JumpIfContest(ProgramId::new(1)),
        ),
        (
            vec![0x25, 1, 2, 3],
            AnimCommand::FadeToBgFromSet {
                opponent: 1,
                player: 2,
                contest: 3,
            },
        ),
        (
            [&[0x26][..], &sweep_bytes].concat(),
            AnimCommand::PanSeAdjustNone(sweep()),
        ),
        (
            [&[0x27][..], &sweep_bytes].concat(),
            AnimCommand::PanSeAdjustAll(sweep()),
        ),
        (vec![0x28, 7], AnimCommand::SplitBgPrio(7)),
        (vec![0x29], AnimCommand::SplitBgPrioAll),
        (vec![0x2A, 8], AnimCommand::SplitBgPrioFoes(8)),
        (vec![0x2B, 9], AnimCommand::Invisible(9)),
        (vec![0x2C, 10], AnimCommand::Visible(10)),
        (vec![0x2D, 11], AnimCommand::TeamAttackMoveBack(11)),
        (vec![0x2E, 12], AnimCommand::TeamAttackMoveFwd(12)),
        (vec![0x2F], AnimCommand::StopSound),
    ];
    assert_eq!(cases.len(), 0x30, "one case per sScriptCmdTable slot");
    let mut image = fixture().finish();
    for (index, (bytes, command)) in cases.into_iter().enumerate() {
        assert_eq!(usize::from(bytes[0]), index, "{command:?}");
        let mut stream = bytes.clone();
        let mut expected = vec![command];
        if !matches!(
            expected[0],
            AnimCommand::End
                | AnimCommand::Return
                | AnimCommand::Goto(_)
                | AnimCommand::ChooseTwoTurnAnim { .. }
        ) {
            stream.push(0x08);
            expected.push(AnimCommand::End);
        }
        image[MAIN..MAIN + stream.len()].copy_from_slice(&stream);
        let decoded = decode_program(&RomReader::new(&image), at(MAIN), stream.len(), &mut Fixed)
            .unwrap_or_else(|err| panic!("{expected:?}: {err}"));
        assert_eq!(decoded.commands, expected);
        assert_eq!(decoded.len, stream.len(), "{expected:?}");
    }
}
