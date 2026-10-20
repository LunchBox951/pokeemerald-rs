//! Battle-animation locator tests over a synthetic ROM and pack.
//!
//! The fixture plants the four selector tables, three program labels (two
//! sharing an address, as `Move_NONE` and `Move_POUND` do), one sprite
//! template, and one task function, then builds the catalog the extractor
//! would write for exactly those programs. Each failure test breaks one
//! fact: a missing or doubled table, a program whose bytes differ, an
//! operand that disagrees with an earlier use, an unreachable program.

use assets::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationProgram, EffectSpriteId, ProgramId,
};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{GbaPtr, ROM_BASE};

use super::battle_anims::locate;
use super::error::GenRomProfileError;
use super::plan::{BattleAnimationPlan, ReportLine, Resolution, SymbolExpectation};
use super::tests::with_context;

const PACK_ID: &str = rom_import::BATTLE_ANIM_PACK_ID;
const TABLES: [usize; 4] = [0x1000, 0x2000, 0x2100, 0x2200];
/// `Short` and `Alias` share this address.
const SHORT: usize = 0x3000;
const MAIN: usize = 0x3010;
const OTHER: usize = 0x3100;
const TEMPLATE: usize = 0x5000;
/// A task function is code, so its operand carries the Thumb bit.
const TASK: usize = 0x6000;

fn bus(offset: usize) -> u32 {
    ROM_BASE + u32::try_from(offset).expect("a fixture offset fits in u32")
}

fn ptr_bytes(offset: usize) -> [u8; 4] {
    bus(offset).to_le_bytes()
}

fn symbol(name: &str) -> AnimSymbol {
    AnimSymbol::new(name).expect("a fixture symbol")
}

/// `Main`'s bytes: a sprite sheet, a sprite, a task, a call, then a `goto`.
fn main_bytes() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x15, 0x27, 0x02];
    bytes.extend(ptr_bytes(TEMPLATE));
    bytes.extend([0x83, 0x02, 0xFF, 0xFF, 0x07, 0x00, 0x03]);
    bytes.extend(bus(TASK + 1).to_le_bytes());
    bytes.extend([0x02, 0x01, 0x05, 0x00, 0x0E]);
    bytes.extend(ptr_bytes(SHORT));
    bytes.push(0x13);
    bytes.extend(ptr_bytes(OTHER));
    bytes
}

/// `Other`: another use of the template, then `end`.
fn other_bytes() -> Vec<u8> {
    let mut bytes = vec![0x02];
    bytes.extend(ptr_bytes(TEMPLATE));
    bytes.extend([0x00, 0x00, 0x08]);
    bytes
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
        AnimCommand::Call(ProgramId::new(0)),
        AnimCommand::Goto(ProgramId::new(3)),
    ]
}

fn other_commands() -> Vec<AnimCommand> {
    vec![
        AnimCommand::CreateSprite {
            template: symbol("gTemplate"),
            relative_to_target: false,
            subpriority: 0,
            args: vec![],
        },
        AnimCommand::End,
    ]
}

/// The program each slot of `table` runs: 0 `Short`, 1 `Alias`, 2 `Main`.
///
/// The patterns differ per table so a run of pointers cannot satisfy two.
fn slot_program(table: AnimationTable, index: u16) -> u16 {
    let last = table.len() - 1;
    let main_slots: &[u16] = match table {
        AnimationTable::Moves => &[last],
        AnimationTable::StatusConditions | AnimationTable::Special => &[0, last],
        AnimationTable::General => &[0, 1, last],
    };
    if main_slots.contains(&index) {
        2
    } else {
        u16::from(table == AnimationTable::Moves && index == 0)
    }
}

fn program_offsets() -> [usize; 4] {
    [SHORT, SHORT, MAIN, OTHER]
}

fn catalog_with(extra: Vec<BattleAnimationProgram>) -> BattleAnimationCatalog {
    let mut programs = vec![
        BattleAnimationProgram::new(symbol("Short"), vec![AnimCommand::End]).unwrap(),
        BattleAnimationProgram::new(symbol("Alias"), vec![AnimCommand::End]).unwrap(),
        BattleAnimationProgram::new(symbol("Main"), main_commands()).unwrap(),
        BattleAnimationProgram::new(symbol("Other"), other_commands()).unwrap(),
    ];
    programs.extend(extra);
    let mut selectors = Vec::new();
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            selectors.push((
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(slot_program(table, index)),
            ));
        }
    }
    BattleAnimationCatalog::new(programs, selectors).unwrap()
}

fn catalog() -> BattleAnimationCatalog {
    catalog_with(Vec::new())
}

fn full_rom() -> RomFixture {
    let offsets = program_offsets();
    let mut rom = RomFixture::new()
        .emerald_header()
        .write(SHORT, &[0x08])
        .write(MAIN, &main_bytes())
        .write(OTHER, &other_bytes());
    for (table, base) in AnimationTable::ALL.into_iter().zip(TABLES) {
        for index in 0..table.len() {
            let program = usize::from(slot_program(table, index));
            let entry = base + usize::from(index) * 4;
            rom = rom.write_ptr(entry, GbaPtr::at(bus(offsets[program])));
        }
    }
    rom
}

fn pack(catalog: &BattleAnimationCatalog) -> Vec<PackEntry> {
    vec![raw_entry(PACK_ID.to_owned(), catalog.encode())]
}

type Located = Result<(BattleAnimationPlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn refused(result: Located) -> (String, String) {
    match result.unwrap_err() {
        GenRomProfileError::StructMismatch { id, reason } => (id, reason),
        other => panic!("expected a struct mismatch, got {other}"),
    }
}

#[test]
fn a_matching_catalog_is_located() {
    let (plan, report) =
        locate_in("ba-matching", full_rom(), pack(&catalog())).expect("a consistent ROM and pack");
    assert_eq!(plan.tables, TABLES.map(bus));

    let symbols: Vec<&str> = plan.programs.iter().map(|p| p.symbol.as_str()).collect();
    assert_eq!(symbols, ["Short", "Alias", "Main", "Other"]);
    let placed: Vec<(u32, u16)> = plan.programs.iter().map(|p| (p.addr, p.len)).collect();
    let main_len = u16::try_from(main_bytes().len()).unwrap();
    let other_len = u16::try_from(other_bytes().len()).unwrap();
    assert_eq!(
        placed,
        [
            (bus(SHORT), 1),
            (bus(SHORT), 1),
            (bus(MAIN), main_len),
            (bus(OTHER), other_len)
        ]
    );
    assert_eq!(plan.templates.len(), 1);
    assert_eq!(
        (plan.templates[0].symbol.as_str(), plan.templates[0].addr),
        ("gTemplate", bus(TEMPLATE))
    );
    assert_eq!(plan.tasks.len(), 1);
    assert_eq!(
        (plan.tasks[0].symbol.as_str(), plan.tasks[0].addr),
        ("AnimTask_Fixture", bus(TASK + 1))
    );
    let expected: Vec<u16> = AnimationTable::ALL
        .into_iter()
        .flat_map(|table| (0..table.len()).map(move |index| slot_program(table, index)))
        .collect();
    assert_eq!(plan.selectors, expected);

    assert_eq!(report.len(), 4);
    for (line, symbol) in report.iter().zip([
        "gBattleAnims_Moves",
        "gBattleAnims_StatusConditions",
        "gBattleAnims_General",
        "gBattleAnims_Special",
    ]) {
        assert_eq!(line.id, PACK_ID);
        assert_eq!(line.resolution, Resolution::StructDerived);
        assert_eq!(line.symbol, SymbolExpectation::Exact(symbol.to_owned()));
    }
}

#[test]
fn the_plan_records_addresses_and_no_command_bytes() {
    let (plan, report) = locate_in("ba-no-bytes", full_rom(), pack(&catalog())).expect("located");
    let rendered = format!("{plan:?} {report:?}");
    assert!(!rendered.contains(&format!("{:?}", main_bytes())));
}

#[test]
fn a_pack_with_another_battle_anim_entry_is_refused() {
    let mut entries = pack(&catalog());
    entries.push(raw_entry("battle-anim/sheet".to_owned(), vec![0]));
    let (id, reason) = refused(locate_in("ba-surplus", full_rom(), entries));
    assert_eq!(id, "battle-anim/*");
    assert!(reason.contains("battle-anim/sheet"), "{reason}");
}

#[test]
fn a_catalog_that_does_not_decode_is_refused() {
    let err = locate_in(
        "ba-malformed",
        full_rom(),
        vec![raw_entry(PACK_ID.to_owned(), vec![1, 2, 3])],
    )
    .unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_catalog_entry_of_the_wrong_kind_is_refused() {
    let image = image_entry(PACK_ID.to_owned(), 8, 8, 4, vec![0; 64]).expect("an image entry");
    let err = locate_in("ba-wrong-kind", full_rom(), vec![image]).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::WrongPackEntryKind { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_missing_table_is_refused() {
    let offsets = program_offsets();
    let mut rom = full_rom();
    // Overwrite the status table so no run of pointers starts its programs.
    for slot in 0..9usize {
        rom = rom.write_ptr(TABLES[1] + slot * 4, GbaPtr::at(bus(offsets[3])));
    }
    let (id, reason) = refused(locate_in("ba-no-table", rom, pack(&catalog())));
    assert_eq!(id, "gBattleAnims_StatusConditions");
    assert!(reason.contains("0 runs"), "{reason}");
}

#[test]
fn a_table_the_rom_holds_twice_is_refused() {
    let offsets = program_offsets();
    let mut rom = full_rom();
    for index in 0..AnimationTable::Special.len() {
        let program = usize::from(slot_program(AnimationTable::Special, index));
        rom = rom.write_ptr(
            0x2300 + usize::from(index) * 4,
            GbaPtr::at(bus(offsets[program])),
        );
    }
    let (id, reason) = refused(locate_in("ba-two-tables", rom, pack(&catalog())));
    assert_eq!(id, "gBattleAnims_Special");
    assert!(reason.contains("2 runs"), "{reason}");
}

#[test]
fn a_program_whose_bytes_differ_is_refused() {
    // `Main` delays where the pack says it calls: the table still points at
    // a program, but not the pack's.
    let mut bytes = main_bytes();
    bytes[0] = 0x05;
    let rom = full_rom().write(MAIN, &bytes);
    let (id, _) = refused(locate_in("ba-differs", rom, pack(&catalog())));
    assert_eq!(id, "gBattleAnims_Moves");
}

#[test]
fn a_branch_to_the_wrong_program_is_refused() {
    // `Main`'s `goto` lands on `Short`, not `Other`.
    let mut bytes = main_bytes();
    let goto = bytes.len() - 4;
    bytes[goto..].copy_from_slice(&ptr_bytes(SHORT));
    let rom = full_rom().write(MAIN, &bytes);
    let (id, reason) = refused(locate_in("ba-wrong-branch", rom, pack(&catalog())));
    assert_eq!(id, "Other");
    assert!(reason.contains("not the pack's program"), "{reason}");
}

#[test]
fn a_program_claimed_at_two_addresses_is_refused() {
    // `Other` is also planted elsewhere and the table points at that copy,
    // while `Main`'s `goto` still names the first.
    let rom = full_rom()
        .write(0x3200, &other_bytes())
        .write_ptr(TABLES[0] + 4, GbaPtr::at(bus(0x3200)));
    let catalog = catalog_with_other_selected();
    let (id, reason) = refused(locate_in("ba-two-addresses", rom, pack(&catalog)));
    assert_eq!(id, "Other");
    assert!(reason.contains("is claimed at"), "{reason}");
}

/// The catalog where `Moves[1]` runs `Other` instead of `Short`.
fn catalog_with_other_selected() -> BattleAnimationCatalog {
    let programs = catalog().programs().to_vec();
    let mut selectors = Vec::new();
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            let program = if table == AnimationTable::Moves && index == 1 {
                3
            } else {
                slot_program(table, index)
            };
            selectors.push((
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(program),
            ));
        }
    }
    BattleAnimationCatalog::new(programs, selectors).unwrap()
}

#[test]
fn a_template_at_two_addresses_is_refused() {
    // `Other` names the template at another address than `Main` does.
    let mut bytes = other_bytes();
    bytes[1..5].copy_from_slice(&ptr_bytes(TEMPLATE + 0x40));
    let rom = full_rom().write(OTHER, &bytes);
    let (id, reason) = refused(locate_in("ba-template-moves", rom, pack(&catalog())));
    assert_eq!(id, "Other");
    assert!(reason.contains("`gTemplate` is"), "{reason}");
}

#[test]
fn two_templates_at_one_address_are_refused() {
    // `Other` names a second template at the first one's address.
    let mut other = other_commands();
    other[0] = AnimCommand::CreateSprite {
        template: symbol("gOtherTemplate"),
        relative_to_target: false,
        subpriority: 0,
        args: vec![],
    };
    let mut programs = catalog().programs().to_vec();
    programs[3] = BattleAnimationProgram::new(symbol("Other"), other).unwrap();
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| {
            (
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(slot_program(table, index)),
            )
        })
    });
    let catalog = BattleAnimationCatalog::new(programs, selectors).unwrap();
    let (id, reason) = refused(locate_in("ba-shared-address", full_rom(), pack(&catalog)));
    assert_eq!(id, "Other");
    assert!(reason.contains("share"), "{reason}");
}

#[test]
fn a_program_nothing_reaches_is_refused() {
    let stray = BattleAnimationProgram::new(symbol("Stray"), vec![AnimCommand::End]).unwrap();
    let (id, reason) = refused(locate_in(
        "ba-unreached",
        full_rom(),
        pack(&catalog_with(vec![stray])),
    ));
    assert_eq!(id, "Stray");
    assert!(reason.contains("reaches"), "{reason}");
}
