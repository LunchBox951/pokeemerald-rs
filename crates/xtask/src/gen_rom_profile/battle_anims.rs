//! Locating the battle-animation programs (`data/battle_anim_scripts.s`).
//!
//! The pack's `battle-anim/catalog` entry is the expectation: the four
//! selector tables, every program they reach, and each program's commands.
//! The ROM stores a command as its opcode and operands, a sprite template,
//! task, or branch target as a bare pointer, and the tables as pointer
//! arrays. Nothing here records a program's bytes; only addresses, lengths,
//! and identities leave this module.
//!
//! The walk:
//!
//! 1. A selector table is a 4-aligned run of pointers whose slots each start
//!    a program that decodes, with the importer's own command reader, to the
//!    commands the pack holds for that slot's selector. Operand pointers
//!    compare as wildcards that must stay consistent. Exactly one run may
//!    satisfy a table.
//! 2. Every branch the located programs make claims an address for the
//!    program it names; the claim is placed the same way until every program
//!    of the pack has one, and a program claimed at two addresses is refused.
//! 3. Each template and task name takes the one address its operands hold in
//!    every use, and two templates or two tasks never share one.
//! 4. The importer's own reader runs over the finished plan and must yield the
//!    pack's payload byte for byte.

use std::collections::{BTreeMap, HashMap};

use assets::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog, ProgramId,
};
use rom_import::{AnimPointers, GbaPtr, ImportError, RomReader};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{to_offset, u32_at};
use super::plan::{AnimProgramPlan, AnimSymbolPlan, BattleAnimationPlan, ReportLine, Resolution};
use super::Context;

const PACK_ID: &str = rom_import::BATTLE_ANIM_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "battle-anim/";
/// The symbol of each selector table, in `AnimationTable::ALL` order.
const TABLE_SYMBOLS: [&str; 4] = [
    "gBattleAnims_Moves",
    "gBattleAnims_StatusConditions",
    "gBattleAnims_General",
    "gBattleAnims_Special",
];
/// One selector-table entry is a pointer.
const POINTER_BYTES: usize = 4;
/// The most bytes a program can have: its length is a `u16` in the profile.
const MAX_PROGRAM_BYTES: usize = u16::MAX as usize;

/// Locate the battle-animation roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `battle-anim/` but the one catalog, or the ROM's tables and programs
/// differ from the pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid catalog.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattleAnimationPlan, GenRomProfileError> {
    inventory::require_battle_anims(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(mismatch(
            &format!("{PACK_PREFIX}*"),
            format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        ));
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let catalog =
        BattleAnimationCatalog::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;

    let expected = Expected::new(&catalog)?;
    let tables = locate_tables(ctx, &expected, report)?;
    let bindings = place_programs(ctx, &expected, &tables)?;
    let plan = bindings.plan(&expected, tables)?;
    check_plan(ctx, &plan, payload)?;
    Ok(plan)
}

fn mismatch(id: &str, reason: String) -> GenRomProfileError {
    GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason,
    }
}

/// The catalog, with each selector's program named by id.
struct Expected<'a> {
    catalog: &'a BattleAnimationCatalog,
    /// Each table's slots, in `AnimationTable::ALL` order.
    slots: [Vec<ProgramId>; 4],
}

impl<'a> Expected<'a> {
    fn new(catalog: &'a BattleAnimationCatalog) -> Result<Self, GenRomProfileError> {
        let by_symbol: HashMap<&str, ProgramId> = catalog
            .programs()
            .iter()
            .enumerate()
            .map(|(index, program)| {
                let id = u16::try_from(index).map(ProgramId::new).map_err(|_| {
                    mismatch(PACK_ID, "more programs than a u16 id names".to_owned())
                })?;
                Ok((program.symbol().as_str(), id))
            })
            .collect::<Result<_, GenRomProfileError>>()?;
        let mut slots: [Vec<ProgramId>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for (table, ids) in AnimationTable::ALL.into_iter().zip(&mut slots) {
            for index in 0..table.len() {
                let selector = AnimationSelector::new(table, index)
                    .map_err(|err| mismatch(PACK_ID, err.to_string()))?;
                let symbol = catalog.program(selector).symbol().as_str();
                ids.push(by_symbol[symbol]);
            }
        }
        Ok(Self { catalog, slots })
    }

    fn name(&self, id: ProgramId) -> &str {
        self.catalog
            .program_by_id(id)
            .map_or("?", |program| program.symbol().as_str())
    }
}

/// An operand a command resolves through the profile, in expected form.
#[derive(Debug, Clone, Copy)]
enum Operand<'a> {
    Template(&'a AnimSymbol),
    Task(&'a AnimSymbol),
    Program(ProgramId),
}

/// The pointer operands of `command`, in the order the importer reads them.
fn operands(command: &AnimCommand) -> Vec<Operand<'_>> {
    match command {
        AnimCommand::CreateSprite { template, .. } => vec![Operand::Template(template)],
        AnimCommand::CreateVisualTask { task, .. } | AnimCommand::CreateSoundTask { task, .. } => {
            vec![Operand::Task(task)]
        }
        AnimCommand::Call(target)
        | AnimCommand::Goto(target)
        | AnimCommand::JumpIfContest(target)
        | AnimCommand::JumpIfMoveTurn { target, .. }
        | AnimCommand::JumpArgEq { target, .. } => vec![Operand::Program(*target)],
        AnimCommand::ChooseTwoTurnAnim { even, odd } => {
            vec![Operand::Program(*even), Operand::Program(*odd)]
        }
        _ => Vec::new(),
    }
}

/// `command` with its pointer operands blanked, so the rest compares.
fn blank(command: &AnimCommand) -> AnimCommand {
    use AnimCommand as C;
    let symbol = || AnimSymbol::new("Blank").expect("a plain identifier");
    let id = ProgramId::new(0);
    match command {
        C::CreateSprite {
            relative_to_target,
            subpriority,
            args,
            ..
        } => C::CreateSprite {
            template: symbol(),
            relative_to_target: *relative_to_target,
            subpriority: *subpriority,
            args: args.clone(),
        },
        C::CreateVisualTask { priority, args, .. } => C::CreateVisualTask {
            task: symbol(),
            priority: *priority,
            args: args.clone(),
        },
        C::CreateSoundTask { args, .. } => C::CreateSoundTask {
            task: symbol(),
            args: args.clone(),
        },
        C::Call(_) => C::Call(id),
        C::Goto(_) => C::Goto(id),
        C::JumpIfContest(_) => C::JumpIfContest(id),
        C::JumpIfMoveTurn { turn, .. } => C::JumpIfMoveTurn {
            turn: *turn,
            target: id,
        },
        C::JumpArgEq { arg, value, .. } => C::JumpArgEq {
            arg: *arg,
            value: *value,
            target: id,
        },
        C::ChooseTwoTurnAnim { .. } => C::ChooseTwoTurnAnim { even: id, odd: id },
        other => other.clone(),
    }
}

/// The addresses a decoded program's operands held, in read order.
#[derive(Default)]
struct Recorder {
    seen: Vec<Seen>,
}

#[derive(Debug, Clone, Copy)]
enum Seen {
    Template(u32),
    Task(u32),
    Program(u32),
}

impl AnimPointers for Recorder {
    fn template(&mut self, _at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        self.seen.push(Seen::Template(ptr.raw()));
        Ok(AnimSymbol::new("Blank").expect("a plain identifier"))
    }

    fn task(&mut self, _at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        self.seen.push(Seen::Task(ptr.raw()));
        Ok(AnimSymbol::new("Blank").expect("a plain identifier"))
    }

    fn program(&mut self, _at: usize, ptr: GbaPtr) -> Result<ProgramId, ImportError> {
        self.seen.push(Seen::Program(ptr.raw()));
        Ok(ProgramId::new(0))
    }
}

/// What the located programs have bound so far.
#[derive(Default)]
struct Bindings {
    /// Each placed program's address and length.
    programs: BTreeMap<u16, (u32, u16)>,
    templates: BTreeMap<String, u32>,
    tasks: BTreeMap<String, u32>,
}

/// An address claimed for a program, still to be placed.
type Claim = (ProgramId, u32);

impl Bindings {
    /// Bind `id` to `addr`: the ROM's bytes there must be the pack's program.
    ///
    /// Returns the claims its branches make.
    fn place(
        &mut self,
        rom: &[u8],
        expected: &Expected<'_>,
        id: ProgramId,
        addr: u32,
    ) -> Result<Vec<Claim>, String> {
        if let Some((bound, _)) = self.programs.get(&id.get()) {
            return if *bound == addr {
                Ok(Vec::new())
            } else {
                Err(format!(
                    "`{}` is claimed at {bound:08X} and {addr:08X}",
                    expected.name(id)
                ))
            };
        }
        let program = expected
            .catalog
            .program_by_id(id)
            .ok_or_else(|| format!("program {} is not in the catalog", id.get()))?;
        let start = GbaPtr::new(addr).ok_or_else(|| format!("{addr:08X} is outside the ROM"))?;
        let mut recorder = Recorder::default();
        let decoded = rom_import::decode_battle_anim_program(
            &RomReader::new(rom),
            start,
            MAX_PROGRAM_BYTES,
            &mut recorder,
        )
        .map_err(|err| err.to_string())?;
        let same_shape = decoded.commands.len() == program.commands().len()
            && decoded
                .commands
                .iter()
                .zip(program.commands())
                .all(|(found, want)| blank(found) == blank(want));
        if !same_shape {
            return Err(format!(
                "the bytes at {addr:08X} are not the pack's program"
            ));
        }
        let len = u16::try_from(decoded.len).map_err(|_| "a program over 64 KiB".to_owned())?;

        let wanted: Vec<Operand<'_>> = program.commands().iter().flat_map(operands).collect();
        let mut claims = Vec::new();
        for (want, found) in wanted.into_iter().zip(&recorder.seen) {
            match (want, *found) {
                (Operand::Template(name), Seen::Template(raw)) => {
                    bind_symbol(&mut self.templates, name.as_str(), raw)?;
                }
                (Operand::Task(name), Seen::Task(raw)) => {
                    bind_symbol(&mut self.tasks, name.as_str(), raw)?;
                }
                (Operand::Program(target), Seen::Program(raw)) => claims.push((target, raw)),
                _ => return Err("the operand kinds disagree".to_owned()),
            }
        }
        self.programs.insert(id.get(), (addr, len));
        Ok(claims)
    }

    /// The plan, once every program is placed.
    fn plan(
        &self,
        expected: &Expected<'_>,
        tables: [u32; 4],
    ) -> Result<BattleAnimationPlan, GenRomProfileError> {
        let mut programs = Vec::new();
        for (position, program) in expected.catalog.programs().iter().enumerate() {
            let symbol = program.symbol().as_str();
            let index = u16::try_from(position)
                .map_err(|_| mismatch(symbol, "more programs than a u16 id names".to_owned()))?;
            let Some((addr, len)) = self.programs.get(&index) else {
                return Err(mismatch(
                    symbol,
                    "no table, branch, or call reaches the program in the ROM".to_owned(),
                ));
            };
            programs.push(AnimProgramPlan {
                symbol: symbol.to_owned(),
                addr: *addr,
                len: *len,
            });
        }
        let symbols = |bound: &BTreeMap<String, u32>| {
            bound
                .iter()
                .map(|(symbol, addr)| AnimSymbolPlan {
                    symbol: symbol.clone(),
                    addr: *addr,
                })
                .collect::<Vec<_>>()
        };
        Ok(BattleAnimationPlan {
            tables,
            programs,
            templates: symbols(&self.templates),
            tasks: symbols(&self.tasks),
            selectors: expected.slots.iter().flatten().map(|id| id.get()).collect(),
        })
    }
}

/// Bind `name` to `raw`, refusing a name at two addresses or an address
/// under two names.
fn bind_symbol(bound: &mut BTreeMap<String, u32>, name: &str, raw: u32) -> Result<(), String> {
    match bound.get(name) {
        Some(known) if *known != raw => {
            return Err(format!("`{name}` is {known:08X} and {raw:08X}"));
        }
        Some(_) => return Ok(()),
        None => {}
    }
    if let Some((other, _)) = bound.iter().find(|(_, addr)| **addr == raw) {
        return Err(format!("`{name}` and `{other}` share {raw:08X}"));
    }
    bound.insert(name.to_owned(), raw);
    Ok(())
}

/// Find each selector table by the programs its slots start.
fn locate_tables(
    ctx: &Context<'_>,
    expected: &Expected<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<[u32; 4], GenRomProfileError> {
    let mut found = [0u32; 4];
    for (index, table) in AnimationTable::ALL.into_iter().enumerate() {
        let symbol = TABLE_SYMBOLS[index];
        let slots = &expected.slots[index];
        let candidates: Vec<usize> =
            (0..=ctx.rom.len().saturating_sub(slots.len() * POINTER_BYTES))
                .step_by(POINTER_BYTES)
                .filter(|&offset| table_at(ctx.rom, expected, slots, offset))
                .collect();
        let [offset] = candidates.as_slice() else {
            return Err(mismatch(
                symbol,
                format!(
                    "{} runs of {} pointers start the {table} programs, expected exactly 1",
                    candidates.len(),
                    slots.len()
                ),
            ));
        };
        let offset = u32::try_from(*offset).expect("a ROM offset fits in u32");
        found[index] = super::locate::to_addr(offset);
        let bytes = u32::try_from(slots.len() * POINTER_BYTES).expect("a table fits in u32");
        report.push(
            ReportLine::unique(PACK_ID, found[index], bytes)
                .with(Resolution::StructDerived)
                .symbol(symbol)
                .note(format!("{} {table} selectors", slots.len())),
        );
    }
    Ok(found)
}

/// Whether the pointers at ROM `offset` start the programs `slots` names.
fn table_at(rom: &[u8], expected: &Expected<'_>, slots: &[ProgramId], offset: usize) -> bool {
    let mut bindings = Bindings::default();
    slots.iter().enumerate().all(|(slot, id)| {
        u32_at(rom, offset + slot * POINTER_BYTES)
            .filter(|addr| to_offset(*addr).is_some())
            .is_some_and(|addr| bindings.place(rom, expected, *id, addr).is_ok())
    })
}

/// Place every program the tables and branches reach.
fn place_programs(
    ctx: &Context<'_>,
    expected: &Expected<'_>,
    tables: &[u32; 4],
) -> Result<Bindings, GenRomProfileError> {
    let mut bindings = Bindings::default();
    let mut queue: Vec<Claim> = Vec::new();
    for ((slots, base), symbol) in expected.slots.iter().zip(tables).zip(TABLE_SYMBOLS) {
        for (slot, id) in slots.iter().enumerate() {
            let entry = base + u32::try_from(slot * POINTER_BYTES).expect("a slot fits in u32");
            let addr = super::locate::u32_at_addr(ctx.rom, entry)
                .ok_or_else(|| mismatch(symbol, format!("slot {slot} is past the ROM")))?;
            queue.push((*id, addr));
        }
    }
    while let Some((id, addr)) = queue.pop() {
        let claims = bindings
            .place(ctx.rom, expected, id, addr)
            .map_err(|reason| mismatch(expected.name(id), reason))?;
        queue.extend(claims);
    }
    Ok(bindings)
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &BattleAnimationPlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let ptr = |what: &str, addr: u32| {
        GbaPtr::new(addr).ok_or_else(|| mismatch(what, format!("{addr:08X} is outside the ROM")))
    };
    let leak = |text: &str| -> &'static str { Box::leak(text.to_owned().into_boxed_str()) };
    let symbols = |roots: &[AnimSymbolPlan]| -> Result<&'static [rom_import::AnimSymbolRoot], GenRomProfileError> {
        let roots = roots
            .iter()
            .map(|root| {
                Ok(rom_import::AnimSymbolRoot {
                    symbol: leak(&root.symbol),
                    addr: ptr(&root.symbol, root.addr)?,
                })
            })
            .collect::<Result<Vec<_>, GenRomProfileError>>()?;
        Ok(Box::leak(roots.into_boxed_slice()))
    };
    let programs = plan
        .programs
        .iter()
        .map(|program| {
            Ok(rom_import::AnimProgramRoot {
                symbol: leak(&program.symbol),
                addr: ptr(&program.symbol, program.addr)?,
                len: program.len,
            })
        })
        .collect::<Result<Vec<_>, GenRomProfileError>>()?;
    let mut tables = [GbaPtr::AT_BASE; 4];
    for ((slot, addr), symbol) in tables.iter_mut().zip(plan.tables).zip(TABLE_SYMBOLS) {
        *slot = ptr(symbol, addr)?;
    }
    let roots = rom_import::BattleAnimationRoots {
        tables,
        programs: Box::leak(programs.into_boxed_slice()),
        templates: symbols(&plan.templates)?,
        tasks: symbols(&plan.tasks)?,
        selectors: Box::leak(plan.selectors.clone().into_boxed_slice()),
    };
    let reader = RomReader::new(ctx.rom);
    let decoded = rom_import::decode_battle_anim_payload(&reader, &roots)
        .map_err(|err| mismatch(PACK_ID, format!("the ROM's catalog does not decode: {err}")))?;
    if decoded != payload {
        return Err(mismatch(
            PACK_ID,
            "the ROM's decoded catalog differs from the pack entry".to_owned(),
        ));
    }
    Ok(())
}
