//! Concrete script commands, slice 1 (S-5, issue #69): control flow,
//! comparison, and the flags/vars commands, wired to
//! [`event_data::EventData`](crate::event_data::EventData) via [`ScriptHost`].
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the `ScrCmd_*`
//! functions in `pokeemerald/src/scrcmd.c` this slice covers, dispatched at
//! the exact opcode numbers `pokeemerald/data/script_cmd_table.inc` assigns
//! them (`(no-verbatim)` — scripts are upstream *data*, so the numbers are
//! pinned, the implementations are not).
//!
//! # Scope
//!
//! Ported (slice 1, issue #69): `nop`/`nop1`/`end`/`return`/`goto`/`call`/
//! `goto_if`/`call_if` (control flow), `compare_var_to_value`/
//! `compare_var_to_var` — the two opcodes the community and this port both
//! call `compare`/`comparevars` (comparison), `setflag`/`clearflag`/
//! `checkflag` (flags), and `setvar`/`addvar`/`subvar`/`copyvar`/
//! `setorcopyvar` (vars).
//!
//! Ported (slice 2, issue #93): `gotostd`/`callstd`/`gotostd_if`/
//! `callstd_if` against the [`StdScript`](crate::script::std_script::StdScript)
//! label table, `random`, and a `special`/`specialvar`/`waitstate` dispatch
//! scaffold against the [`SpecialId`]/[`SPECIAL_TABLE`] table — see each
//! command's doc comment, and the [Slice 2 scaffolding](self#slice-2-scaffolding)
//! section below for the shared design those three lean on.
//!
//! Deliberately excluded, each for a reason a later slice resolves:
//! * `returnram`/`endram` (0x0c–0x0d), `setmysteryeventstatus` (0x0e) — no
//!   RAM-script loading or Mystery Event subsystem yet.
//! * `loadword`/`loadbyte`/`setptr`/`loadbytefromptr`/`setptrbyte`/
//!   `copylocal`/`copybyte` (0x0f–0x15) and `compare_local_to_ptr`/
//!   `compare_ptr_to_local`/`compare_ptr_to_value`/`compare_ptr_to_ptr`
//!   (0x1d–0x20) — every one dereferences a raw ROM/RAM address
//!   (`*(const u8 *)ScriptReadWord(ctx)`); `(oop-boundaries)` and `(no-ffi)`
//!   rule out reproducing that, and unlike `goto`/`call`'s destination (see
//!   [`ScriptContext::resolve`]) there is no byte-offset substitute for
//!   "read an arbitrary byte of the running program" that means anything.
//! * `compare_local_to_local`/`compare_local_to_value` (0x1b–0x1c) — the
//!   `ctx->data[]` scratch-register comparisons; only `compare`/`comparevars`
//!   (the var-based pair actual field scripts use for `goto_if`/`call_if`)
//!   are in this slice's scope.
//! * `callnative`/`gotonative` (0x23–0x24) — native-step dispatch through an
//!   arbitrary ROM function pointer; no such address space here
//!   `(oop-boundaries)`.
//! * `delay` (0x28) — a frame-counted native step; no frame clock threaded
//!   into `ScriptHost` yet.
//! * `initclock` through `contestlinktransfer` (0x2c–0x8e) — the clock,
//!   audio, warp, bag/PC/decoration, object-event movement, trainer-battle,
//!   message/menu, mon-buffer, Pokémart, and contest commands. Every one
//!   needs a subsystem (overworld map/objects, dialog boxes, audio mixer,
//!   the bag, party Pokémon, …) this port doesn't have yet; porting the
//!   opcode without it would mean stubbing the *behavior*, not just the
//!   dispatch, which the issue driving this slice (#93) explicitly excludes
//!   ("Do NOT implement message/movement/object commands").
//!
//! Every excluded-but-numbered opcode still gets a table entry — see
//! [`unimplemented`] — so a hand-assembled script that hits one traps
//! ([`CommandTrap::Unimplemented`]) instead of silently misbehaving or
//! panicking `(behavioral-fidelity)`.
//!
//! # Slice 2 scaffolding
//!
//! `gotostd`/`callstd`/`gotostd_if`/`callstd_if` and `special`/`specialvar`
//! both index into an upstream table of *other* code this port doesn't have
//! yet — `gStdScripts`' compiled bytecode and `gSpecials`' 527 functions
//! spanning ~30 subsystems, respectively (`data/event_scripts.s`,
//! `data/specials.inc`).
//! Rather than leave all seven opcodes opcode-level [`unimplemented`] (as
//! slice 1 did), this slice gives each a typed table
//! ([`StdScript`](crate::script::std_script::StdScript),
//! [`SpecialId`]/[`SPECIAL_TABLE`]) so a script can name *which* std script
//! or special it wants, and traps with that identity attached
//! ([`CommandTrap::StdScript`], [`CommandTrap::UnimplementedSpecial`])
//! instead of just the bare opcode. `special`'s table is a genuine callback
//! hook: [`SPECIAL_TABLE`] is `[Option<SpecialFn>; SPECIAL_COUNT]`, and a
//! later slice fills in real entries one at a time, exactly like
//! [`COMMAND_TABLE`] itself grows opcode by opcode. `gotostd`'s table has no
//! such hook — a std script *label* isn't a callable Rust value, only real
//! compiled bytecode (out of scope, see the exclusion list above) would be —
//! so [`StdScript`] only ever identifies *which* one was requested.
//!
//! `waitstate` needs no such table (it's generic pause machinery, not a
//! subsystem dispatch) and is genuinely implemented, not scaffolded — see its
//! doc comment for how it reuses [`ScriptContext`]'s existing native-step
//! mode to stand in for upstream's global `CONTEXT_WAITING` status.
//!
//! # Traps
//!
//! `Command<H>`'s signature (`fn(&mut ScriptContext<'_, '_, H>, &mut H) ->
//! bool`) mirrors upstream `ScrCmdFunc` (`bool8 (*)(struct ScriptContext
//! *)`) exactly, which leaves no room for a `Result`. A command that hits a
//! condition upstream leaves unchecked or dispatches through machinery this
//! port doesn't have therefore can't return an error the way
//! [`EventData`] or [`ScriptContext`]'s own
//! methods do; instead it records why in [`ScriptHost::trap`] and calls
//! [`ScriptContext::stop`], exactly like `ScrCmd_end` halts the script —
//! the difference is only that callers can inspect *why* afterwards, rather
//! than the reason being silently lost.
//!
//! # `VAR_` dereferencing
//!
//! Upstream draws a sharp line between two ways a var-id operand gets
//! resolved:
//! * `VarGet` (what [`EventData::var_get`] mirrors): an id below `VARS_START`
//!   passes through unchanged as an immediate value — only `setorcopyvar`'s
//!   source and `subvar`'s subtrahend use this upstream, so a script can pass
//!   either a `VAR_*` id or a literal in those two operand slots.
//! * A raw, unchecked `GetVarPointer(id)` dereference (what `setvar`,
//!   `addvar`'s destination, `copyvar`'s source, and `compare`'s var operand
//!   use upstream): `id` *must* already be a valid var id, or the dereference
//!   is undefined behaviour. Note `compare_var_to_value` reads its var operand
//!   as `*GetVarPointer(...)`, the *same* raw-deref primitive as `copyvar`'s
//!   source — not `VarGet` — so its var slot is a strict var id, never a
//!   literal-or-var slot (its *second* operand is the literal). This port has
//!   no raw-pointer primitive to reproduce that with —
//!   [`EventData::var_get`]/[`EventData::var_set`] (the *checked*
//!   `VarGet`/`VarSet`) are the only var accessors it exposes — so every
//!   command below reads and writes vars through them uniformly. For every
//!   id a real script ever actually passes (always a proper `VAR_*` id in
//!   these positions) the two give identical results; the only places this
//!   is observable are `copyvar`'s source and `compare`'s var operand, which
//!   then behave exactly like `setorcopyvar`'s already-checked resolution — a
//!   strictly safer substitute for upstream's UB, not a behavioural gap
//!   `(behavioral-fidelity)`.
//!
//! `addvar`'s second operand is the one documented exception:
//! `pokeemerald/asm/macros/event.inc` notes upstream `ScrCmd_addvar` never
//! calls `VarGet` on it (only `subvar` does) — see `addvar`'s doc comment below.

use crate::event_data::{EventData, EventDataError, SPECIAL_VARS_START};
use crate::rng::Rng;
use crate::script::specials::{SpecialId, SPECIAL_TABLE};
use crate::script::std_script::StdScript;
use crate::script::{Command, ScriptContext, ScriptError};

/// Host for this command set: owns the [`EventData`] store every flag/var/
/// comparison command reads or writes, the [`Rng`] [`random`] draws from, the
/// [`waiting`](Self::waiting) flag [`waitstate`] pauses on, and the last
/// [`CommandTrap`] a command recorded (see the [module docs, "Traps"](self#traps)
/// section). `(oop-boundaries)` — no global mutable state; the binary
/// composes a richer host still (the bag, map, audio, …) as this command set
/// grows further.
#[derive(Debug, Default)]
pub struct ScriptHost {
    /// Emerald's flags and vars — see [`EventData`].
    pub event_data: EventData,
    /// Emerald's PRNG — upstream `gRngValue`, drawn from by [`random`]
    /// (`ScrCmd_random`'s `Random()` call). See [`crate::rng::Rng`].
    pub rng: Rng,
    /// [`waitstate`]'s pause flag: `true` blocks the script (see
    /// [`waitstate`]'s doc comment for how). Any code holding `&mut
    /// ScriptHost` can clear it to resume the script, mirroring upstream's
    /// `ScriptContext_Enable()`.
    pub waiting: bool,
    /// Set by a trapping command; see the [module docs](self#traps). Cleared
    /// by nothing here — callers that resume a context after a trap should
    /// clear it themselves once they've inspected it.
    pub trap: Option<CommandTrap>,
}

/// Why a command halted the script instead of running normally — see the
/// [module docs, "Traps"](self#traps) section for why this exists instead of
/// a `Result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTrap {
    /// The opcode has a `script_cmd_table.inc` entry upstream but no command
    /// implementation in this slice (see the [module docs, "Scope"](self#scope) section). Carries the offending opcode.
    Unimplemented(u8),
    /// A `goto_if`/`call_if` condition byte was not one of the 6 upstream
    /// `enum ComparisonOperators` values. Upstream indexes
    /// `sScriptConditionTable` with it regardless (`ctx->comparisonResult`
    /// is a `u8` used as the *column*, and the condition byte is the *row*
    /// index — an out-of-range row reads past the table, undefined
    /// behaviour); this port refuses it instead. Carries the offending byte.
    InvalidCondition(u8),
    /// A flag/var access failed — see [`EventDataError`].
    EventData(EventDataError),
    /// A cursor read or a `goto`/`call`/`goto_if`/`call_if` jump-target
    /// resolution failed — see [`ScriptError`].
    Script(ScriptError),
    /// A `gotostd`/`callstd`/`gotostd_if`/`callstd_if` index resolved to a
    /// valid [`StdScript`] label, but this port has no compiled std-script
    /// content to jump/call into yet — see the
    /// [Slice 2 scaffolding](self#slice-2-scaffolding) section. An *invalid*
    /// index is not an error — see [`dispatch_std_script`]'s doc comment.
    StdScript(StdScript),
    /// A `special`/`specialvar` index operand did not resolve to a valid
    /// [`SpecialId`] (i.e. `index >= SPECIAL_COUNT`
    /// (`crate::script::specials`)). Upstream's `gSpecials[index]()` never
    /// checks this — an out-of-range call is an out-of-bounds
    /// function-pointer call, undefined behaviour; this port refuses it
    /// instead. Carries the offending raw index.
    InvalidSpecial(u16),
    /// A `special`/`specialvar` index resolved to a valid [`SpecialId`], but
    /// [`SPECIAL_TABLE`]'s slot for it is still `None` — no slice has wired
    /// that special's subsystem in yet — see the
    /// [Slice 2 scaffolding](self#slice-2-scaffolding) section.
    UnimplementedSpecial(SpecialId),
    /// `random`'s `limit` operand resolved to `0`. Upstream's `Random() %
    /// max` is a plain C `%` by zero — not something GBA hardware or
    /// upstream's software division routine defines cleanly, and no real
    /// script ever calls `random` with a zero limit. This port refuses it as
    /// a typed trap instead of the panic Rust's `%` operator would otherwise
    /// raise `(behavioral-fidelity)`.
    DivideByZero,
}

/// The result of upstream's `Compare(u16 a, u16 b)`, used as the row index
/// into [`CONDITION_TABLE`]. Not a general-purpose ordering type — it exists
/// solely to name the three cases `Compare` (and, degenerately,
/// `checkflag`'s bare `0`/`1`) ever store into `comparisonResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum CompareResult {
    /// `a < b`. Also `checkflag`'s "flag clear" result.
    Less = 0,
    /// `a == b`. Also `checkflag`'s "flag set" result.
    Equal = 1,
    /// `a > b`. Never produced by `checkflag`.
    Greater = 2,
}

/// Mirrors upstream `Compare` (`src/scrcmd.c`): `Less` if `a < b`, `Equal` if
/// `a == b`, `Greater` if `a > b`.
fn compare_u16(a: u16, b: u16) -> CompareResult {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => CompareResult::Less,
        std::cmp::Ordering::Equal => CompareResult::Equal,
        std::cmp::Ordering::Greater => CompareResult::Greater,
    }
}

/// One of the 6 upstream `enum ComparisonOperators` values
/// (`include/constants/comparison_operators.h`), read from a `goto_if`/
/// `call_if` condition byte. Explicit discriminants match the upstream enum
/// (and this type's own [`CONDITION_TABLE`] row order) so `self as usize`
/// indexes correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ScriptCondition {
    LessThan = 0,
    Equal = 1,
    GreaterThan = 2,
    LessThanOrEqual = 3,
    GreaterThanOrEqual = 4,
    NotEqual = 5,
}

/// Transcribed 1:1 from upstream's `sScriptConditionTable`
/// (`src/scrcmd.c`) — ground truth, not derived from the logic it checks —
/// row = [`ScriptCondition`], column = [`CompareResult`] (`<`, `==`, `>`).
/// `true` means "`goto_if`/`call_if` jumps".
const CONDITION_TABLE: [[bool; 3]; 6] = [
    // <      ==     >
    [true, false, false], // LessThan
    [false, true, false], // Equal
    [false, false, true], // GreaterThan
    [true, true, false],  // LessThanOrEqual
    [false, true, true],  // GreaterThanOrEqual
    [true, false, true],  // NotEqual
];

impl ScriptCondition {
    /// Classify a raw condition byte, mirroring the *value space* of
    /// upstream's `enum ComparisonOperators` (upstream itself does not
    /// validate this byte — see [`CommandTrap::InvalidCondition`]).
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::LessThan),
            1 => Some(Self::Equal),
            2 => Some(Self::GreaterThan),
            3 => Some(Self::LessThanOrEqual),
            4 => Some(Self::GreaterThanOrEqual),
            5 => Some(Self::NotEqual),
            _ => None,
        }
    }

    /// `sScriptConditionTable[self][result]` — whether `goto_if`/`call_if`
    /// should jump given the last comparison's [`CompareResult`].
    fn matches(self, result: CompareResult) -> bool {
        CONDITION_TABLE[self as usize][result as usize]
    }
}

/// Record `why` on the host, halt the script (mirroring `ScrCmd_end`'s
/// `StopScript`), and return the upstream-matching "keep the bytecode loop
/// going" `false` — the loop reads `false` as "don't yield", but the halt
/// itself makes the *next* fetch see an empty cursor and stop. See the
/// [module docs, "Traps"](self#traps) section.
fn trap(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    why: CommandTrap,
) -> bool {
    host.trap = Some(why);
    ctx.stop();
    false
}

/// A [`ScriptCondition`]-independent operand pair reader: every `setvar`/
/// `addvar`/`subvar`/`copyvar`/`setorcopyvar`/`compare_var_to_*` opcode reads
/// exactly two little-endian halfwords, in order.
fn read_u16_pair(ctx: &mut ScriptContext<'_, '_, ScriptHost>) -> Result<(u16, u16), ScriptError> {
    let a = ctx.read_u16()?;
    let b = ctx.read_u16()?;
    Ok((a, b))
}

/// Read a `goto`/`call`/`goto_if`/`call_if` destination operand (a 4-byte
/// offset) and resolve it via [`ScriptContext::resolve`] — see that method's
/// docs for what the offset means.
fn read_target<'script>(
    ctx: &mut ScriptContext<'_, 'script, ScriptHost>,
) -> Result<&'script [u8], ScriptError> {
    let offset = ctx.read_u32()?;
    ctx.resolve(offset)
}

/// `SCR_OP_NOP` (`0x00`) / [`nop1`] (`0x01`). Mirrors `ScrCmd_nop`: does
/// nothing.
fn nop(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    false
}

/// `SCR_OP_NOP1` (`0x01`). Mirrors `ScrCmd_nop1`: does nothing — a distinct
/// upstream function from [`nop`], kept distinct here too even though the
/// behaviour is identical, so the opcode↔command mapping stays 1:1.
fn nop1(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    false
}

/// `SCR_OP_END` (`0x02`). Mirrors `ScrCmd_end`: halts the script.
fn end(ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    ctx.stop();
    false
}

/// `SCR_OP_RETURN` (`0x03`). Mirrors `ScrCmd_return`: pops the call stack
/// and jumps to the popped return address.
fn script_return(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    match ctx.script_return() {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::Script(e)),
    }
}

/// `SCR_OP_CALL` (`0x04`). Mirrors `ScrCmd_call`: pushes the current cursor
/// as a return address, then jumps to the operand's target — see
/// [`read_target`].
fn call(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let target = match read_target(ctx) {
        Ok(target) => target,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    match ctx.call(target) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::Script(e)),
    }
}

/// `SCR_OP_GOTO` (`0x05`). Mirrors `ScrCmd_goto`: jumps to the operand's
/// target — see [`read_target`].
fn goto(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    match read_target(ctx) {
        Ok(target) => {
            ctx.jump(target);
            false
        }
        Err(e) => trap(ctx, host, CommandTrap::Script(e)),
    }
}

/// `SCR_OP_GOTO_IF` (`0x06`). Mirrors `ScrCmd_goto_if`: reads a condition
/// byte and a destination operand (in that order, both unconditionally, like
/// upstream), then jumps only if the condition
/// [`matches`](ScriptCondition::matches) [`ScriptContext::comparison_result`].
fn goto_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    conditional(ctx, host, ConditionalAction::Jump)
}

/// `SCR_OP_CALL_IF` (`0x07`). Mirrors `ScrCmd_call_if`: reads a condition
/// byte and a destination operand (in that order, both unconditionally, like
/// upstream), then calls only if the condition
/// [`matches`](ScriptCondition::matches) [`ScriptContext::comparison_result`].
fn call_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    conditional(ctx, host, ConditionalAction::Call)
}

/// The one difference between [`goto_if`] and [`call_if`]: whether a
/// matching condition jumps ([`ScriptContext::jump`]) or calls
/// ([`ScriptContext::call`]).
///
/// A plain enum rather than passing `ScriptContext::jump`/`ScriptContext::call`
/// as a function pointer: `jump` is infallible and `call`'s `&'script [u8]`
/// parameter is tied to the exact same `'script` as `ctx`'s — a bare `fn(...)`
/// *type* can't express that link (each elided lifetime in it is
/// independently higher-ranked, per [`Command`]'s own doc comment), so
/// [`conditional`] just branches on this flag instead of taking `act` as a
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionalAction {
    Jump,
    Call,
}

/// Shared body of [`goto_if`]/[`call_if`]: both read the same operand pair
/// in the same order and differ only in whether a matching condition jumps
/// or calls — see [`ConditionalAction`].
fn conditional(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    action: ConditionalAction,
) -> bool {
    let condition_byte = match ctx.read_u8() {
        Ok(b) => b,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    // Read unconditionally, matching upstream's unconditional
    // `ScriptReadWord` — the operand is only ever *used* (resolved) below,
    // once we know the condition actually matches.
    let offset = match ctx.read_u32() {
        Ok(o) => o,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let Some(condition) = ScriptCondition::from_byte(condition_byte) else {
        return trap(ctx, host, CommandTrap::InvalidCondition(condition_byte));
    };
    if condition.matches(current_compare_result(ctx)) {
        let target = match ctx.resolve(offset) {
            Ok(target) => target,
            Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
        };
        match action {
            ConditionalAction::Jump => ctx.jump(target),
            ConditionalAction::Call => {
                if let Err(e) = ctx.call(target) {
                    return trap(ctx, host, CommandTrap::Script(e));
                }
            }
        }
    }
    false
}

/// `ctx->comparisonResult` reinterpreted as the [`CompareResult`] it was last
/// set to. Shared by [`conditional`] (`goto_if`/`call_if`) and
/// [`std_conditional`] (`gotostd_if`/`callstd_if`), which both read it
/// identically.
fn current_compare_result(ctx: &ScriptContext<'_, '_, ScriptHost>) -> CompareResult {
    match ctx.comparison_result() {
        0 => CompareResult::Less,
        1 => CompareResult::Equal,
        // Only ever 0/1/2 here: nothing but `compare`/`comparevars`/
        // `checkflag` below ever writes `comparison_result`, and none of
        // them write anything else. Any other value is unreachable through
        // this command set, so it's treated as "never matches" rather than
        // panicking.
        _ => CompareResult::Greater,
    }
}

/// `SCR_OP_GOTO_STD` (`0x08`). Mirrors `ScrCmd_gotostd`: reads a std-script
/// index and dispatches through [`dispatch_std_script`].
fn gotostd(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let index = match ctx.read_u8() {
        Ok(i) => i,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    dispatch_std_script(ctx, host, index)
}

/// `SCR_OP_CALL_STD` (`0x09`). Mirrors `ScrCmd_callstd`. Upstream's `callstd`
/// differs from [`gotostd`] only in calling (pushing a return address) rather
/// than jumping to the resolved std script — a difference this port can't yet
/// observe, since neither can actually resolve one; see
/// [`dispatch_std_script`]'s doc comment.
fn callstd(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    gotostd(ctx, host)
}

/// `SCR_OP_GOTO_STD_IF` (`0x0a`). Mirrors `ScrCmd_gotostd_if`: reads a
/// condition byte and a std-script index (in that order, both
/// unconditionally, like upstream), then dispatches through
/// [`dispatch_std_script`] only if the condition
/// [`matches`](ScriptCondition::matches) [`ScriptContext::comparison_result`].
fn gotostd_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    std_conditional(ctx, host)
}

/// `SCR_OP_CALL_STD_IF` (`0x0b`). Mirrors `ScrCmd_callstd_if`; see
/// [`callstd`] for why this and [`gotostd_if`] currently behave identically.
fn callstd_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    std_conditional(ctx, host)
}

/// Shared body of [`gotostd_if`]/[`callstd_if`]: both read the same
/// condition-byte + index-byte operand pair unconditionally (matching
/// upstream's unconditional pair of `ScriptReadByte` calls), then dispatch
/// through [`dispatch_std_script`] only if the condition matches.
fn std_conditional(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let condition_byte = match ctx.read_u8() {
        Ok(b) => b,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let index = match ctx.read_u8() {
        Ok(i) => i,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let Some(condition) = ScriptCondition::from_byte(condition_byte) else {
        return trap(ctx, host, CommandTrap::InvalidCondition(condition_byte));
    };
    if condition.matches(current_compare_result(ctx)) {
        dispatch_std_script(ctx, host, index)
    } else {
        false
    }
}

/// Shared "resolve `index` into a [`StdScript`] label and react" body of
/// [`gotostd`]/[`callstd`]/[`std_conditional`], once each has read its own
/// operand(s). Mirrors upstream's shared bounds check (`ptr <
/// gStdScripts_End`, `src/scrcmd.c`'s `ScrCmd_gotostd`/`ScrCmd_callstd`/
/// `ScrCmd_gotostd_if`/`ScrCmd_callstd_if`): an out-of-range `index` is
/// upstream's silent no-op — it just continues at the next opcode without
/// jumping — reproduced here bug-for-bug `(behavioral-fidelity)`. An
/// in-range `index` names which [`StdScript`] label it targets, but traps
/// regardless: this port has no compiled std-script bytecode to jump/call
/// into yet — see the [Slice 2 scaffolding](self#slice-2-scaffolding)
/// section — the trap identifies *which* label so a later slice's fix is
/// easy to verify against.
fn dispatch_std_script(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    index: u8,
) -> bool {
    match StdScript::from_index(index) {
        Some(id) => trap(ctx, host, CommandTrap::StdScript(id)),
        None => false,
    }
}

/// `SCR_OP_SETVAR` (`0x16`). Mirrors `ScrCmd_setvar`: writes a literal
/// value to a var.
fn setvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (dest, value) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    match host.event_data.var_set(dest, value) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_ADDVAR` (`0x17`). Mirrors `ScrCmd_addvar`: adds a **literal**
/// value to a var (`0xFFFF + 1` wraps to `0x0000`, matching upstream's
/// unchecked `u16` overflow).
///
/// Upstream's own comment on `ScrCmd_addvar` (`src/scrcmd.c`) is explicit
/// that this is deliberate and must stay this way: "addvar doesn't support
/// adding from a variable in vanilla" — unlike [`subvar`], the second
/// operand is never resolved through `VarGet`, even though it's read from
/// the same 2-byte operand slot a `VAR_*` id would occupy.
fn addvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (dest, literal) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let current = match host.event_data.var_get(dest) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    match host.event_data.var_set(dest, current.wrapping_add(literal)) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_SUBVAR` (`0x18`). Mirrors `ScrCmd_subvar`: subtracts
/// [`EventData::var_get`]-resolved `source` (a `VAR_*` id **or** a literal —
/// see the [module docs' `VAR_` dereferencing](self#var_-dereferencing)
/// section) from `dest` (`0x0000 - 1` wraps to `0xFFFF`, matching upstream's
/// unchecked `u16` overflow). Unlike `addvar`, upstream does resolve this
/// operand through `VarGet`.
fn subvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (dest, source) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let subtrahend = match host.event_data.var_get(source) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    let current = match host.event_data.var_get(dest) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    match host
        .event_data
        .var_set(dest, current.wrapping_sub(subtrahend))
    {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_COPYVAR` (`0x19`). Mirrors `ScrCmd_copyvar`: copies `source`'s
/// value into `dest`. See the [module docs' `VAR_`
/// dereferencing](self#var_-dereferencing) section for why this reads
/// `source` through [`EventData::var_get`] (the checked `VarGet`) rather
/// than upstream's raw, unchecked pointer dereference — for every id a real
/// script passes here (always a proper `VAR_*` id) the two agree.
fn copyvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (dest, source) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let value = match host.event_data.var_get(source) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    match host.event_data.var_set(dest, value) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_SETORCOPYVAR` (`0x1a`). Mirrors `ScrCmd_setorcopyvar`: writes
/// `source` into `dest`, where `source` may be either a `VAR_*` id or a
/// literal — `EventData::var_get` resolves either case (see the [module
/// docs' `VAR_` dereferencing](self#var_-dereferencing) section), exactly
/// mirroring upstream's `VarGet` call here.
fn setorcopyvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    copyvar(ctx, host)
}

/// `SCR_OP_COMPARE_VAR_TO_VALUE` (`0x21`), commonly called `compare`.
/// Mirrors `ScrCmd_compare_var_to_value`: compares a var's value to a
/// literal and stores the result in [`ScriptContext::comparison_result`] for
/// a following `goto_if`/`call_if`.
fn compare(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (var, value) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let a = match host.event_data.var_get(var) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    ctx.set_comparison_result(compare_u16(a, value) as u8);
    false
}

/// `SCR_OP_COMPARE_VAR_TO_VAR` (`0x22`), commonly called `comparevars`.
/// Mirrors `ScrCmd_compare_var_to_var`: compares two vars' values and stores
/// the result in [`ScriptContext::comparison_result`] for a following
/// `goto_if`/`call_if`.
fn comparevars(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (var1, var2) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let a = match host.event_data.var_get(var1) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    let b = match host.event_data.var_get(var2) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    ctx.set_comparison_result(compare_u16(a, b) as u8);
    false
}

/// `SCR_OP_SETFLAG` (`0x29`). Mirrors `ScrCmd_setflag`.
fn setflag(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let id = match ctx.read_u16() {
        Ok(id) => id,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    match host.event_data.flag_set(id) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_CLEARFLAG` (`0x2a`). Mirrors `ScrCmd_clearflag`.
fn clearflag(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let id = match ctx.read_u16() {
        Ok(id) => id,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    match host.event_data.flag_clear(id) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_CHECKFLAG` (`0x2b`). Mirrors `ScrCmd_checkflag`: stores the
/// flag's value (`0` or `1`) in [`ScriptContext::comparison_result`] — a
/// following `goto_if`/`call_if` typically uses `EQUAL`/`LESS_THAN` (i.e.
/// `TRUE`/`FALSE`) against it, both of which [`CONDITION_TABLE`] already
/// covers via its `<`/`==` columns.
fn checkflag(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let id = match ctx.read_u16() {
        Ok(id) => id,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    match host.event_data.flag_get(id) {
        Ok(set) => {
            ctx.set_comparison_result(u8::from(set));
            false
        }
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `VAR_RESULT` (`include/constants/vars.h:296`, `SPECIAL_VARS_START +
/// 0xD`) — upstream `gSpecialVar_Result`, [`random`]'s destination.
const VAR_RESULT: u16 = SPECIAL_VARS_START + 0xD;

/// `SCR_OP_RANDOM` (`0x8f`, `data/script_cmd_table.inc:161`). Mirrors
/// `ScrCmd_random`: draws `Random() % limit` and writes it to `VAR_RESULT`.
/// `limit` is read through [`EventData::var_get`] (upstream `VarGet`), so it
/// may be a literal or a `VAR_*` id — see the [module docs' `VAR_`
/// dereferencing](self#var_-dereferencing) section.
fn random(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let operand = match ctx.read_u16() {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let limit = match host.event_data.var_get(operand) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    if limit == 0 {
        return trap(ctx, host, CommandTrap::DivideByZero);
    }
    let value = host.rng.next_u16() % limit;
    match host.event_data.var_set(VAR_RESULT, value) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

/// `SCR_OP_SPECIAL` (`0x25`). Mirrors `ScrCmd_special`: reads a special
/// index and calls it, discarding any result — see the
/// [Slice 2 scaffolding](self#slice-2-scaffolding) section for how
/// [`SPECIAL_TABLE`] dispatch works and why every slot traps in this slice.
/// Upstream never bounds-checks `index` against `gSpecials`'s length before
/// calling through it (an out-of-range one is an out-of-bounds
/// function-pointer call — undefined behaviour); this port refuses it
/// instead, see [`CommandTrap::InvalidSpecial`].
fn special(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let raw = match ctx.read_u16() {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let Some(id) = SpecialId::from_index(raw) else {
        return trap(ctx, host, CommandTrap::InvalidSpecial(raw));
    };
    match SPECIAL_TABLE[usize::from(id.index())] {
        Some(f) => {
            f(host);
            false
        }
        None => trap(ctx, host, CommandTrap::UnimplementedSpecial(id)),
    }
}

/// `SCR_OP_SPECIALVAR` (`0x26`). Mirrors `ScrCmd_specialvar`: reads the
/// output var id, then a special index (in that order, matching upstream's
/// operand layout — the output var is resolved *before* the special even
/// runs), calls the special, and writes its `u16` result to the output var.
/// See [`special`] for the shared dispatch/bounds-check behaviour.
fn specialvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let dest = match ctx.read_u16() {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let raw = match ctx.read_u16() {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let Some(id) = SpecialId::from_index(raw) else {
        return trap(ctx, host, CommandTrap::InvalidSpecial(raw));
    };
    match SPECIAL_TABLE[usize::from(id.index())] {
        Some(f) => {
            let value = f(host);
            match host.event_data.var_set(dest, value) {
                Ok(()) => false,
                Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
            }
        }
        None => trap(ctx, host, CommandTrap::UnimplementedSpecial(id)),
    }
}

/// `SCR_OP_WAITSTATE` (`0x27`). Mirrors `ScrCmd_waitstate`: blocks script
/// execution until something else unblocks it. Upstream does this by setting
/// the *global* script context's status to `CONTEXT_WAITING`
/// (`ScriptContext_Stop`, `src/script.c`) — `RunScriptCommand` then simply
/// isn't called again until some other code (a special's async completion, a
/// message-box handler, …) calls `ScriptContext_Enable()`. This module owns
/// no such global context to mirror that with (`(oop-boundaries)`, see the
/// [`crate::script`] module docs' "Address space" section), but
/// [`ScriptContext`]'s existing native-step mode is already exactly this
/// shape of pause — "run once per [`ScriptContext::run`] call... until some
/// external condition says stop" (see [`crate::script`]'s docs on
/// `NativeStep`) — so `waitstate` sets [`ScriptHost::waiting`] and switches
/// into a native step that polls it; any code holding `&mut ScriptHost` can
/// clear the flag to resume the script, mirroring `ScriptContext_Enable`.
fn waitstate(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    host.waiting = true;
    ctx.setup_native(step_waitstate);
    true
}

/// The native step [`waitstate`] installs: done (hands back to bytecode) once
/// [`ScriptHost::waiting`] has been cleared by external code.
fn step_waitstate(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    !host.waiting
}

/// Table entry for an opcode `script_cmd_table.inc` assigns upstream but
/// this slice doesn't implement (see the [module docs, "Scope"](self#scope) section) — traps with [`CommandTrap::Unimplemented`]
/// instead of leaving a gap or aliasing another command. `OP` is baked in
/// per table slot via a const generic, so one function body serves every
/// unimplemented opcode while each slot still reports its own opcode.
fn unimplemented<const OP: u8>(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
) -> bool {
    trap(ctx, host, CommandTrap::Unimplemented(OP))
}

/// This command table, indexed by upstream opcode (`data/script_cmd_table.inc`)
/// from `SCR_OP_NOP` (`0x00`) through `SCR_OP_RANDOM` (`0x8f`) — the highest
/// opcode this slice implements. Every index in between has an entry: either
/// a ported command, or [`unimplemented`] carrying that slot's own opcode
/// number. See the [module docs](self) for what's ported and why the rest
/// isn't yet.
pub const COMMAND_TABLE: [Command<ScriptHost>; 0x90] = [
    nop,                   // 0x00 SCR_OP_NOP
    nop1,                  // 0x01 SCR_OP_NOP1
    end,                   // 0x02 SCR_OP_END
    script_return,         // 0x03 SCR_OP_RETURN
    call,                  // 0x04 SCR_OP_CALL
    goto,                  // 0x05 SCR_OP_GOTO
    goto_if,               // 0x06 SCR_OP_GOTO_IF
    call_if,               // 0x07 SCR_OP_CALL_IF
    gotostd,               // 0x08 SCR_OP_GOTO_STD
    callstd,               // 0x09 SCR_OP_CALL_STD
    gotostd_if,            // 0x0a SCR_OP_GOTO_STD_IF
    callstd_if,            // 0x0b SCR_OP_CALL_STD_IF
    unimplemented::<0x0c>, // SCR_OP_RETURNRAM
    unimplemented::<0x0d>, // SCR_OP_ENDRAM
    unimplemented::<0x0e>, // SCR_OP_SETMYSTERYEVENTSTATUS
    unimplemented::<0x0f>, // SCR_OP_LOAD_WORD
    unimplemented::<0x10>, // SCR_OP_LOAD_BYTE
    unimplemented::<0x11>, // SCR_OP_SETPTR
    unimplemented::<0x12>, // SCR_OP_LOADBYTEFROMPTR
    unimplemented::<0x13>, // SCR_OP_SETPTRBYTE
    unimplemented::<0x14>, // SCR_OP_COPYLOCAL
    unimplemented::<0x15>, // SCR_OP_COPYBYTE
    setvar,                // 0x16 SCR_OP_SETVAR
    addvar,                // 0x17 SCR_OP_ADDVAR
    subvar,                // 0x18 SCR_OP_SUBVAR
    copyvar,               // 0x19 SCR_OP_COPYVAR
    setorcopyvar,          // 0x1a SCR_OP_SETORCOPYVAR
    unimplemented::<0x1b>, // SCR_OP_COMPARE_LOCAL_TO_LOCAL
    unimplemented::<0x1c>, // SCR_OP_COMPARE_LOCAL_TO_VALUE
    unimplemented::<0x1d>, // SCR_OP_COMPARE_LOCAL_TO_PTR
    unimplemented::<0x1e>, // SCR_OP_COMPARE_PTR_TO_LOCAL
    unimplemented::<0x1f>, // SCR_OP_COMPARE_PTR_TO_VALUE
    unimplemented::<0x20>, // SCR_OP_COMPARE_PTR_TO_PTR
    compare,               // 0x21 SCR_OP_COMPARE_VAR_TO_VALUE
    comparevars,           // 0x22 SCR_OP_COMPARE_VAR_TO_VAR
    unimplemented::<0x23>, // SCR_OP_CALLNATIVE
    unimplemented::<0x24>, // SCR_OP_GOTONATIVE
    special,               // 0x25 SCR_OP_SPECIAL
    specialvar,            // 0x26 SCR_OP_SPECIALVAR
    waitstate,             // 0x27 SCR_OP_WAITSTATE
    unimplemented::<0x28>, // SCR_OP_DELAY
    setflag,               // 0x29 SCR_OP_SETFLAG
    clearflag,             // 0x2a SCR_OP_CLEARFLAG
    checkflag,             // 0x2b SCR_OP_CHECKFLAG
    unimplemented::<0x2c>, // SCR_OP_INITCLOCK
    unimplemented::<0x2d>, // SCR_OP_DOTIMEBASEDEVENTS
    unimplemented::<0x2e>, // SCR_OP_GETTIME
    unimplemented::<0x2f>, // SCR_OP_PLAYSE
    unimplemented::<0x30>, // SCR_OP_WAITSE
    unimplemented::<0x31>, // SCR_OP_PLAYFANFARE
    unimplemented::<0x32>, // SCR_OP_WAITFANFARE
    unimplemented::<0x33>, // SCR_OP_PLAYBGM
    unimplemented::<0x34>, // SCR_OP_SAVEBGM
    unimplemented::<0x35>, // SCR_OP_FADEDEFAULTBGM
    unimplemented::<0x36>, // SCR_OP_FADENEWBGM
    unimplemented::<0x37>, // SCR_OP_FADEOUTBGM
    unimplemented::<0x38>, // SCR_OP_FADEINBGM
    unimplemented::<0x39>, // SCR_OP_WARP
    unimplemented::<0x3a>, // SCR_OP_WARPSILENT
    unimplemented::<0x3b>, // SCR_OP_WARPDOOR
    unimplemented::<0x3c>, // SCR_OP_WARPHOLE
    unimplemented::<0x3d>, // SCR_OP_WARPTELEPORT
    unimplemented::<0x3e>, // SCR_OP_SETWARP
    unimplemented::<0x3f>, // SCR_OP_SETDYNAMICWARP
    unimplemented::<0x40>, // SCR_OP_SETDIVEWARP
    unimplemented::<0x41>, // SCR_OP_SETHOLEWARP
    unimplemented::<0x42>, // SCR_OP_GETPLAYERXY
    unimplemented::<0x43>, // SCR_OP_GETPARTYSIZE
    unimplemented::<0x44>, // SCR_OP_ADDITEM
    unimplemented::<0x45>, // SCR_OP_REMOVEITEM
    unimplemented::<0x46>, // SCR_OP_CHECKITEMSPACE
    unimplemented::<0x47>, // SCR_OP_CHECKITEM
    unimplemented::<0x48>, // SCR_OP_CHECKITEMTYPE
    unimplemented::<0x49>, // SCR_OP_ADDPCITEM
    unimplemented::<0x4a>, // SCR_OP_CHECKPCITEM
    unimplemented::<0x4b>, // SCR_OP_ADDDECORATION
    unimplemented::<0x4c>, // SCR_OP_REMOVEDECORATION
    unimplemented::<0x4d>, // SCR_OP_CHECKDECOR
    unimplemented::<0x4e>, // SCR_OP_CHECKDECORSPACE
    unimplemented::<0x4f>, // SCR_OP_APPLYMOVEMENT
    unimplemented::<0x50>, // SCR_OP_APPLYMOVEMENTAT
    unimplemented::<0x51>, // SCR_OP_WAITMOVEMENT
    unimplemented::<0x52>, // SCR_OP_WAITMOVEMENTAT
    unimplemented::<0x53>, // SCR_OP_REMOVEOBJECT
    unimplemented::<0x54>, // SCR_OP_REMOVEOBJECTAT
    unimplemented::<0x55>, // SCR_OP_ADDOBJECT
    unimplemented::<0x56>, // SCR_OP_ADDOBJECTAT
    unimplemented::<0x57>, // SCR_OP_SETOBJECTXY
    unimplemented::<0x58>, // SCR_OP_SHOWOBJECTAT
    unimplemented::<0x59>, // SCR_OP_HIDEOBJECTAT
    unimplemented::<0x5a>, // SCR_OP_FACEPLAYER
    unimplemented::<0x5b>, // SCR_OP_TURNOBJECT
    unimplemented::<0x5c>, // SCR_OP_TRAINERBATTLE
    unimplemented::<0x5d>, // SCR_OP_DOTRAINERBATTLE
    unimplemented::<0x5e>, // SCR_OP_GOTOPOSTBATTLESCRIPT
    unimplemented::<0x5f>, // SCR_OP_GOTOBEATENSCRIPT
    unimplemented::<0x60>, // SCR_OP_CHECKTRAINERFLAG
    unimplemented::<0x61>, // SCR_OP_SETTRAINERFLAG
    unimplemented::<0x62>, // SCR_OP_CLEARTRAINERFLAG
    unimplemented::<0x63>, // SCR_OP_SETOBJECTXYPERM
    unimplemented::<0x64>, // SCR_OP_COPYOBJECTXYTOPERM
    unimplemented::<0x65>, // SCR_OP_SETOBJECTMOVEMENTTYPE
    unimplemented::<0x66>, // SCR_OP_WAITMESSAGE
    unimplemented::<0x67>, // SCR_OP_MESSAGE
    unimplemented::<0x68>, // SCR_OP_CLOSEMESSAGE
    unimplemented::<0x69>, // SCR_OP_LOCKALL
    unimplemented::<0x6a>, // SCR_OP_LOCK
    unimplemented::<0x6b>, // SCR_OP_RELEASEALL
    unimplemented::<0x6c>, // SCR_OP_RELEASE
    unimplemented::<0x6d>, // SCR_OP_WAITBUTTONPRESS
    unimplemented::<0x6e>, // SCR_OP_YESNOBOX
    unimplemented::<0x6f>, // SCR_OP_MULTICHOICE
    unimplemented::<0x70>, // SCR_OP_MULTICHOICEDEFAULT
    unimplemented::<0x71>, // SCR_OP_MULTICHOICEGRID
    unimplemented::<0x72>, // SCR_OP_DRAWBOX
    unimplemented::<0x73>, // SCR_OP_ERASEBOX
    unimplemented::<0x74>, // SCR_OP_DRAWBOXTEXT
    unimplemented::<0x75>, // SCR_OP_SHOWMONPIC
    unimplemented::<0x76>, // SCR_OP_HIDEMONPIC
    unimplemented::<0x77>, // SCR_OP_SHOWCONTESTPAINTING
    unimplemented::<0x78>, // SCR_OP_BRAILLEMESSAGE
    unimplemented::<0x79>, // SCR_OP_GIVEMON
    unimplemented::<0x7a>, // SCR_OP_GIVEEGG
    unimplemented::<0x7b>, // SCR_OP_SETMONMOVE
    unimplemented::<0x7c>, // SCR_OP_CHECKPARTYMOVE
    unimplemented::<0x7d>, // SCR_OP_BUFFERSPECIESNAME
    unimplemented::<0x7e>, // SCR_OP_BUFFERLEADMONSPECIESNAME
    unimplemented::<0x7f>, // SCR_OP_BUFFERPARTYMONNICK
    unimplemented::<0x80>, // SCR_OP_BUFFERITEMNAME
    unimplemented::<0x81>, // SCR_OP_BUFFERDECORATIONNAME
    unimplemented::<0x82>, // SCR_OP_BUFFERMOVENAME
    unimplemented::<0x83>, // SCR_OP_BUFFERNUMBERSTRING
    unimplemented::<0x84>, // SCR_OP_BUFFERSTDSTRING
    unimplemented::<0x85>, // SCR_OP_BUFFERSTRING
    unimplemented::<0x86>, // SCR_OP_POKEMART
    unimplemented::<0x87>, // SCR_OP_POKEMARTDECORATION
    unimplemented::<0x88>, // SCR_OP_POKEMARTDECORATION2
    unimplemented::<0x89>, // SCR_OP_PLAYSLOTMACHINE
    unimplemented::<0x8a>, // SCR_OP_SETBERRYTREE
    unimplemented::<0x8b>, // SCR_OP_CHOOSECONTESTMON
    unimplemented::<0x8c>, // SCR_OP_STARTCONTEST
    unimplemented::<0x8d>, // SCR_OP_SHOWCONTESTRESULTS
    unimplemented::<0x8e>, // SCR_OP_CONTESTLINKTRANSFER
    random,                // 0x8f SCR_OP_RANDOM
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_data::{SPECIAL_FLAGS_START, SPECIAL_VARS_START, VARS_START};

    /// Builds a fresh context wired to [`COMMAND_TABLE`] and a fresh host.
    fn setup() -> (ScriptContext<'static, 'static, ScriptHost>, ScriptHost) {
        (ScriptContext::new(&COMMAND_TABLE), ScriptHost::default())
    }

    #[test]
    fn nop_and_nop1_do_nothing_and_advance_past_themselves() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x00, 0x01, 0x02]; // nop, nop1, end
        ctx.setup_bytecode(&bytes);
        assert!(!ctx.run(&mut host), "end halts the script");
        assert!(ctx.is_stopped());
        assert_eq!(host.trap, None);
    }

    #[test]
    fn end_halts_the_script() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x02, 0x00]; // end, (nop never reached)
        ctx.setup_bytecode(&bytes);
        assert!(!ctx.run(&mut host));
        assert!(ctx.is_stopped());
    }

    #[test]
    fn goto_jumps_to_the_resolved_offset_skipping_what_linear_flow_would_hit() {
        let (mut ctx, mut host) = setup();
        // 0: goto 10          -> 05 <offset:4>
        // 5: setflag 999      -> 29 <id:2>  (would run without the jump; must NOT)
        // 8: end              -> 02          (would halt without the jump; must NOT)
        // 9: nop              -> 00          (padding, unreached either way)
        // 10: setflag 100     -> 29 <id:2>  (only reached via the jump)
        // 13: end             -> 02
        let mut bytes = vec![0x05];
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0x29);
        bytes.extend_from_slice(&999u16.to_le_bytes());
        bytes.push(0x02);
        bytes.push(0x00);
        bytes.push(0x29);
        bytes.extend_from_slice(&100u16.to_le_bytes());
        bytes.push(0x02);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(
            host.event_data.flag_get(100),
            Ok(true),
            "the jump target's setflag ran"
        );
        assert_eq!(
            host.event_data.flag_get(999),
            Ok(false),
            "the setflag linear flow would have hit was skipped over"
        );
    }

    #[test]
    fn call_pushes_a_return_address_and_return_resumes_after_it() {
        let (mut ctx, mut host) = setup();
        // 0: call 9          -> 04 <offset:4>
        // 5: setflag 200     -> 29 <id:2>  (runs after the call returns)
        // 8: end             -> 02
        // 9: setflag 100     -> 29 <id:2>  (the subroutine)
        // 12: return         -> 03
        let mut bytes = vec![0x04];
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.push(0x29);
        bytes.extend_from_slice(&200u16.to_le_bytes());
        bytes.push(0x02);
        bytes.push(0x29);
        bytes.extend_from_slice(&100u16.to_le_bytes());
        bytes.push(0x03);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(host.event_data.flag_get(100), Ok(true), "subroutine ran");
        assert_eq!(
            host.event_data.flag_get(200),
            Ok(true),
            "resumed after the call site"
        );
        assert_eq!(ctx.stack_depth(), 0);
    }

    #[test]
    fn goto_with_out_of_range_target_traps() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x05];
        bytes.extend_from_slice(&999u32.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::Script(ScriptError::InvalidJumpTarget(999)))
        );
    }

    /// Exhaustive over all 6 [`ScriptCondition`] values × all 3
    /// [`CompareResult`] outcomes: exactly the cells `sScriptConditionTable`
    /// marks `1` should jump; every other cell should not.
    #[test]
    fn goto_if_truth_table_matches_upstream_condition_table() {
        // (condition byte, comparisonResult) -> expect jump?
        #[rustfmt::skip]
        let cases: &[(u8, u8, bool)] = &[
            // LESS_THAN
            (0, 0, true), (0, 1, false), (0, 2, false),
            // EQUAL
            (1, 0, false), (1, 1, true), (1, 2, false),
            // GREATER_THAN
            (2, 0, false), (2, 1, false), (2, 2, true),
            // LESS_THAN_OR_EQUAL
            (3, 0, true), (3, 1, true), (3, 2, false),
            // GREATER_THAN_OR_EQUAL
            (4, 0, false), (4, 1, true), (4, 2, true),
            // NOT_EQUAL
            (5, 0, true), (5, 1, false), (5, 2, true),
        ];

        for &(condition, comparison_result, expect_jump) in cases {
            let (mut ctx, mut host) = setup();
            // 0: goto_if <condition>, 10   -> 06 <cond:1> <offset:4>
            // 6: setflag 1                 -> 29 <id:2>  (only if it doesn't jump)
            // 9: end                       -> 02
            // 10: setflag 2                -> 29 <id:2>  (only if it jumps)
            // 13: end                      -> 02
            let mut bytes = vec![0x06, condition];
            bytes.extend_from_slice(&10u32.to_le_bytes());
            bytes.push(0x29);
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.push(0x02);
            bytes.push(0x29);
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.push(0x02);
            ctx.setup_bytecode(&bytes);
            ctx.set_comparison_result(comparison_result);

            assert!(!ctx.run(&mut host));
            assert_eq!(
                host.trap, None,
                "condition {condition} result {comparison_result}"
            );
            assert_eq!(
                host.event_data.flag_get(2),
                Ok(expect_jump),
                "condition {condition}, comparisonResult {comparison_result}: expected jump = {expect_jump}"
            );
            assert_eq!(host.event_data.flag_get(1), Ok(!expect_jump));
        }
    }

    #[test]
    fn call_if_jumps_via_call_so_return_comes_back() {
        let (mut ctx, mut host) = setup();
        // 0: compare_var_to_value VARS_START, 5   -> 21 <var:2> <value:2>
        // 5: call_if EQUAL(1), 15                 -> 07 <cond:1> <offset:4>
        // 11: setflag 9                            -> 29 <id:2>
        // 14: end                                  -> 02
        // 15: setflag 10                           -> 29 <id:2>
        // 18: return                               -> 03
        let mut bytes = vec![0x21];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(0x07);
        bytes.push(1);
        bytes.extend_from_slice(&15u32.to_le_bytes());
        bytes.push(0x29);
        bytes.extend_from_slice(&9u16.to_le_bytes());
        bytes.push(0x02);
        bytes.push(0x29);
        bytes.extend_from_slice(&10u16.to_le_bytes());
        bytes.push(0x03);
        ctx.setup_bytecode(&bytes);
        host.event_data.var_set(VARS_START, 5).unwrap();

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(
            host.event_data.flag_get(10),
            Ok(true),
            "call_if called the subroutine"
        );
        assert_eq!(
            host.event_data.flag_get(9),
            Ok(true),
            "resumed after the call site"
        );
    }

    #[test]
    fn goto_if_with_invalid_condition_byte_traps() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x06, 6]; // condition 6 is past NOT_EQUAL(5)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::InvalidCondition(6)));
    }

    #[test]
    fn setvar_writes_a_literal_value() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x16];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&42u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(
            !ctx.run(&mut host),
            "runs off the end after the one command"
        );
        assert_eq!(host.event_data.var_get(VARS_START), Ok(42));
    }

    #[test]
    fn setvar_writes_a_special_var() {
        // Upstream `GetVarPointer` serves 0x8000..=0x8015 through the
        // `gSpecialVars` table as plain storage, so a field script's
        // `setvar VAR_RESULT, n` must write, not trap.
        let (mut ctx, mut host) = setup();
        let var_result = SPECIAL_VARS_START + 0xD; // VAR_RESULT
        let mut bytes = vec![0x16];
        bytes.extend_from_slice(&var_result.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None, "setvar on a special var must not trap");
        assert_eq!(host.event_data.var_get(var_result), Ok(7));
    }

    #[test]
    fn addvar_adds_a_literal_and_wraps() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 0xFFFF).unwrap();
        let mut bytes = vec![0x17];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(
            host.event_data.var_get(VARS_START),
            Ok(0),
            "0xFFFF + 1 wraps to 0"
        );
    }

    #[test]
    fn addvar_does_not_resolve_its_operand_through_varget() {
        // addvar's second operand is a literal even when it looks like a
        // VAR_* id — see the doc comment on `addvar`.
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 3).unwrap(); // VARS_START holds 3
        host.event_data.var_set(VARS_START + 1, 10).unwrap();
        let mut bytes = vec![0x17];
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes()); // dest
        bytes.extend_from_slice(&VARS_START.to_le_bytes()); // "literal" == VARS_START's numeric id
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(
            host.event_data.var_get(VARS_START + 1),
            Ok(10 + VARS_START),
            "the operand was added as the literal id, not the value stored at that id"
        );
    }

    #[test]
    fn subvar_resolves_its_operand_through_varget_and_wraps() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 0).unwrap();
        host.event_data.var_set(VARS_START + 1, 1).unwrap();
        let mut bytes = vec![0x18];
        bytes.extend_from_slice(&VARS_START.to_le_bytes()); // dest, currently 0
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes()); // source var holding 1
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(
            host.event_data.var_get(VARS_START),
            Ok(0xFFFF),
            "0 - 1 wraps to 0xFFFF"
        );
    }

    #[test]
    fn copyvar_copies_the_source_vars_value() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START + 1, 77).unwrap();
        let mut bytes = vec![0x19];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.var_get(VARS_START), Ok(77));
    }

    #[test]
    fn setorcopyvar_treats_a_var_id_as_a_var() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START + 1, 55).unwrap();
        let mut bytes = vec![0x1a];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.var_get(VARS_START), Ok(55));
    }

    #[test]
    fn setorcopyvar_treats_a_non_var_id_as_a_literal() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x1a];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&123u16.to_le_bytes()); // not a VAR_* id
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.var_get(VARS_START), Ok(123));
    }

    #[test]
    fn compare_var_to_value_sets_comparison_result() {
        // (var's stored value, literal operand, expected comparisonResult)
        for (stored, literal, expected) in [(5u16, 10u16, 0u8), (5, 5, 1), (10, 5, 2)] {
            let (mut ctx, mut host) = setup();
            host.event_data.var_set(VARS_START, stored).unwrap();
            let mut bytes = vec![0x21];
            bytes.extend_from_slice(&VARS_START.to_le_bytes());
            bytes.extend_from_slice(&literal.to_le_bytes());
            ctx.setup_bytecode(&bytes);

            ctx.run(&mut host);
            assert_eq!(
                ctx.comparison_result(),
                expected,
                "stored={stored} literal={literal}"
            );
        }
    }

    #[test]
    fn comparevars_sets_comparison_result() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 3).unwrap();
        host.event_data.var_set(VARS_START + 1, 9).unwrap();
        let mut bytes = vec![0x22];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(ctx.comparison_result(), 0, "3 < 9");
    }

    #[test]
    fn setflag_clearflag_checkflag_round_trip() {
        let (mut ctx, mut host) = setup();
        // setflag SPECIAL_FLAGS_START; checkflag SPECIAL_FLAGS_START; end
        let mut bytes = vec![0x29];
        bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        bytes.push(0x2b);
        bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        bytes.push(0x02);
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.flag_get(SPECIAL_FLAGS_START), Ok(true));
        assert_eq!(
            ctx.comparison_result(),
            1,
            "checkflag stored the flag's TRUE value"
        );

        let (mut ctx2, mut host2) = setup();
        let mut clear_bytes = vec![0x2a];
        clear_bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        clear_bytes.push(0x2b);
        clear_bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        clear_bytes.push(0x02);
        ctx2.setup_bytecode(&clear_bytes);
        host2.event_data.flag_set(SPECIAL_FLAGS_START).unwrap();

        ctx2.run(&mut host2);
        assert_eq!(host2.event_data.flag_get(SPECIAL_FLAGS_START), Ok(false));
        assert_eq!(
            ctx2.comparison_result(),
            0,
            "checkflag stored the flag's FALSE value"
        );
    }

    #[test]
    fn unimplemented_opcode_traps_with_its_own_number() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x0c]; // SCR_OP_RETURNRAM, not implemented in this slice
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::Unimplemented(0x0c)));
        assert!(ctx.is_stopped());
    }

    #[test]
    fn event_data_out_of_range_error_traps_instead_of_propagating_silently() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x29]; // setflag
        let bad_id = SPECIAL_FLAGS_START - 1; // the unchecked-in-C gap
        bytes.extend_from_slice(&bad_id.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::EventData(EventDataError::OutOfRange(bad_id)))
        );
    }

    // -----------------------------------------------------------------------
    // Slice 2 (issue #93): gotostd/callstd/gotostd_if/callstd_if.
    // -----------------------------------------------------------------------

    #[test]
    fn gotostd_traps_naming_the_resolved_std_script() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x08, 2]; // gotostd MSGBOX_NPC
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::MsgboxNpc))
        );
        assert!(ctx.is_stopped());
    }

    #[test]
    fn callstd_traps_identically_to_gotostd() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x09, 0]; // callstd STD_OBTAIN_ITEM
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::ObtainItem))
        );
    }

    #[test]
    fn gotostd_with_out_of_range_index_is_a_silent_no_op() {
        let (mut ctx, mut host) = setup();
        // 0: gotostd 11        -> 08 <index:1>  (11 == StdScript::COUNT, out of range)
        // 2: setflag 5         -> 29 <id:2>  (must still run: no jump, no trap)
        // 5: end               -> 02
        let mut bytes = vec![0x08, StdScript::COUNT];
        bytes.push(0x29);
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(0x02);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None, "out-of-range index must not trap");
        assert_eq!(
            host.event_data.flag_get(5),
            Ok(true),
            "execution continues past the no-op gotostd"
        );
    }

    #[test]
    fn gotostd_if_reads_both_operands_unconditionally_and_dispatches_only_on_match() {
        let (mut ctx, mut host) = setup();
        // 0: compare_var_to_value VARS_START, 5  -> 21 <var:2> <value:2>  (sets comparisonResult = EQUAL)
        // 5: gotostd_if NOT_EQUAL(5), 3           -> 0a <cond:1> <index:1>  (condition doesn't match: no dispatch)
        // 8: end                                  -> 02
        let mut bytes = vec![0x21];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(0x0a);
        bytes.push(5); // NOT_EQUAL
        bytes.push(3); // MSGBOX_SIGN
        bytes.push(0x02);
        ctx.setup_bytecode(&bytes);
        host.event_data.var_set(VARS_START, 5).unwrap();

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap, None,
            "condition didn't match: no dispatch, no trap"
        );
    }

    #[test]
    fn gotostd_if_dispatches_when_the_condition_matches() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x0a, 1, 3]; // gotostd_if EQUAL(1), MSGBOX_SIGN(3)
        ctx.setup_bytecode(&bytes);
        ctx.set_comparison_result(1); // EQUAL

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::MsgboxSign))
        );
    }

    #[test]
    fn callstd_if_behaves_like_gotostd_if() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x0b, 1, 7]; // callstd_if EQUAL(1), STD_OBTAIN_DECORATION(7)
        ctx.setup_bytecode(&bytes);
        ctx.set_comparison_result(1);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::ObtainDecoration))
        );
    }

    #[test]
    fn gotostd_if_with_invalid_condition_byte_traps() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x0a, 6, 0]; // condition 6 is past NOT_EQUAL(5)
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::InvalidCondition(6)));
    }

    // -----------------------------------------------------------------------
    // Slice 2 (issue #93): random.
    // -----------------------------------------------------------------------

    #[test]
    fn random_writes_random_mod_limit_to_var_result() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x8f];
        bytes.extend_from_slice(&10u16.to_le_bytes()); // literal limit
        ctx.setup_bytecode(&bytes);
        host.rng.seed(0);
        let mut expected_rng = crate::rng::Rng::new(0);
        let expected = expected_rng.next_u16() % 10;

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(host.event_data.var_get(VAR_RESULT), Ok(expected));
    }

    #[test]
    fn random_resolves_its_limit_operand_through_varget() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 7).unwrap();
        let mut bytes = vec![0x8f];
        bytes.extend_from_slice(&VARS_START.to_le_bytes()); // limit is a VAR_* id
        ctx.setup_bytecode(&bytes);
        host.rng.seed(0);
        let mut expected_rng = crate::rng::Rng::new(0);
        let expected = expected_rng.next_u16() % 7;

        assert!(!ctx.run(&mut host));
        assert_eq!(host.event_data.var_get(VAR_RESULT), Ok(expected));
    }

    #[test]
    fn random_with_zero_limit_traps_instead_of_panicking() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x8f];
        bytes.extend_from_slice(&0u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::DivideByZero));
    }

    #[test]
    fn random_draws_advance_the_generator_across_calls() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x8f];
        bytes.extend_from_slice(&100u16.to_le_bytes());
        bytes.push(0x8f);
        bytes.extend_from_slice(&100u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);
        host.rng.seed(0);

        assert!(!ctx.run(&mut host));
        let mut probe = crate::rng::Rng::new(0);
        probe.next_u16(); // the first random's draw
        let expected_second = probe.next_u16() % 100;
        assert_eq!(
            host.event_data.var_get(VAR_RESULT),
            Ok(expected_second),
            "the second random draw used the advanced generator state, not a repeat of the first"
        );
    }

    // -----------------------------------------------------------------------
    // Slice 2 (issue #93): special/specialvar/waitstate.
    // -----------------------------------------------------------------------

    #[test]
    fn special_traps_unimplemented_for_every_currently_valid_id() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x25];
        bytes.extend_from_slice(&0u16.to_le_bytes()); // SPECIAL index 0 (HealPlayerParty)
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        let id = crate::script::specials::SpecialId::from_index(0).unwrap();
        assert_eq!(host.trap, Some(CommandTrap::UnimplementedSpecial(id)));
    }

    #[test]
    fn special_with_out_of_range_index_traps_invalid_special() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x25];
        let bad = u16::try_from(crate::script::specials::SPECIAL_COUNT).unwrap();
        bytes.extend_from_slice(&bad.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::InvalidSpecial(bad)));
    }

    #[test]
    fn specialvar_reads_output_var_then_index_and_traps_unimplemented() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x26];
        bytes.extend_from_slice(&VARS_START.to_le_bytes()); // output var
        bytes.extend_from_slice(&1u16.to_le_bytes()); // SPECIAL index 1
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        let id = crate::script::specials::SpecialId::from_index(1).unwrap();
        assert_eq!(host.trap, Some(CommandTrap::UnimplementedSpecial(id)));
        assert_eq!(
            host.event_data.var_get(VARS_START),
            Ok(0),
            "the output var is untouched when the special is unimplemented"
        );
    }

    #[test]
    fn specialvar_with_out_of_range_index_traps_invalid_special() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![0x26];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        let bad = u16::MAX;
        bytes.extend_from_slice(&bad.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::InvalidSpecial(bad)));
    }

    #[test]
    fn waitstate_blocks_the_script_until_externally_unblocked() {
        let (mut ctx, mut host) = setup();
        let bytes = [0x27, 0x29, 5, 0, 0x02]; // waitstate; setflag 5; end
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host), "waitstate yields, script stays active");
        assert!(!ctx.is_stopped());
        assert!(host.waiting, "waitstate sets the pause flag");
        assert_eq!(
            host.event_data.flag_get(5),
            Ok(false),
            "bytecode after waitstate must not run while still waiting"
        );

        // Still blocked: nothing has cleared `waiting` yet.
        assert!(ctx.run(&mut host));
        assert!(!ctx.is_stopped());
        assert_eq!(host.event_data.flag_get(5), Ok(false));

        // External code "unblocks" the script (mirrors `ScriptContext_Enable`).
        host.waiting = false;
        assert!(
            ctx.run(&mut host),
            "still yields on the call that finishes the native wait"
        );
        assert!(!ctx.is_stopped());

        assert!(!ctx.run(&mut host), "bytecode resumes and runs to `end`");
        assert!(ctx.is_stopped());
        assert_eq!(
            host.event_data.flag_get(5),
            Ok(true),
            "setflag after waitstate ran once unblocked"
        );
    }
}
