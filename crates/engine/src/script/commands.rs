//! Native handlers for Emerald's field-script bytecode.
//!
//! [`COMMAND_TABLE`] preserves the opcode assignments through `random` from
//! `data/script_cmd_table.inc`. Unsupported opcodes trap with their identity
//! instead of aliasing a handler or failing silently.
//!
//! The interpreter uses byte offsets instead of ROM pointers. Variable access
//! similarly goes through checked [`EventData`] methods; valid field scripts
//! observe the same values, while malformed identifiers trap instead of
//! dereferencing arbitrary memory. Operands passed through `VarGet` upstream
//! still accept either a literal or a variable identifier here.
//!
//! Standard-script labels and special-function identifiers are typed even when
//! their target implementation is unavailable, so traps retain the requested
//! identity. `waitstate` represents upstream's global waiting status with the
//! interpreter's native-step mode and a host-owned resume flag.

use crate::event_data::{EventData, EventDataError, SPECIAL_VARS_START};
use crate::rng::Rng;
use crate::script::specials::{SpecialId, SPECIAL_TABLE};
use crate::script::std_script::StdScript;
use crate::script::{Command, ScriptContext, ScriptError};

/// State owned by the field-script command handlers.
#[derive(Debug, Default)]
pub struct ScriptHost {
    /// Emerald's event flags and variables.
    pub event_data: EventData,
    /// Emerald's deterministic pseudorandom number generator.
    pub rng: Rng,
    /// Whether a `waitstate` command is blocking bytecode execution.
    pub waiting: bool,
    /// The last command failure. Callers clear it after handling the trap.
    pub trap: Option<CommandTrap>,
}

/// Why a field-script command halted the interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTrap {
    /// The opcode has no native command implementation.
    Unimplemented(u8),
    /// A conditional command contained an unknown comparison operator.
    InvalidCondition(u8),
    /// A flag or variable operand could not be resolved.
    EventData(EventDataError),
    /// Bytecode decoding, stack access, or jump resolution failed.
    Script(ScriptError),
    /// A known standard-script label has no compiled bytecode target.
    StdScript(StdScript),
    /// A special-function index is outside the known table.
    InvalidSpecial(u16),
    /// A known special-function identifier has no callback.
    UnimplementedSpecial(SpecialId),
    /// A `random` command resolved its modulus to zero.
    DivideByZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum CompareResult {
    Less = 0,
    Equal = 1,
    Greater = 2,
}

fn compare_u16(a: u16, b: u16) -> CompareResult {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => CompareResult::Less,
        std::cmp::Ordering::Equal => CompareResult::Equal,
        std::cmp::Ordering::Greater => CompareResult::Greater,
    }
}

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

#[derive(Clone, Copy)]
struct MatchingResults {
    less: bool,
    equal: bool,
    greater: bool,
}

impl MatchingResults {
    const fn includes(self, result: CompareResult) -> bool {
        match result {
            CompareResult::Less => self.less,
            CompareResult::Equal => self.equal,
            CompareResult::Greater => self.greater,
        }
    }
}

// `src/scrcmd.c:sScriptConditionTable` is the authority for these rows.
const CONDITION_TABLE: [(ScriptCondition, MatchingResults); 6] = [
    (
        ScriptCondition::LessThan,
        MatchingResults {
            less: true,
            equal: false,
            greater: false,
        },
    ),
    (
        ScriptCondition::Equal,
        MatchingResults {
            less: false,
            equal: true,
            greater: false,
        },
    ),
    (
        ScriptCondition::GreaterThan,
        MatchingResults {
            less: false,
            equal: false,
            greater: true,
        },
    ),
    (
        ScriptCondition::LessThanOrEqual,
        MatchingResults {
            less: true,
            equal: true,
            greater: false,
        },
    ),
    (
        ScriptCondition::GreaterThanOrEqual,
        MatchingResults {
            less: false,
            equal: true,
            greater: true,
        },
    ),
    (
        ScriptCondition::NotEqual,
        MatchingResults {
            less: true,
            equal: false,
            greater: true,
        },
    ),
];

impl ScriptCondition {
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

    fn matches(self, result: CompareResult) -> bool {
        let (condition, matching_results) = CONDITION_TABLE[self as usize];
        debug_assert_eq!(condition, self);
        matching_results.includes(result)
    }
}

fn trap(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    why: CommandTrap,
) -> bool {
    host.trap = Some(why);
    ctx.stop();
    false
}

fn read_u16_pair(ctx: &mut ScriptContext<'_, '_, ScriptHost>) -> Result<(u16, u16), ScriptError> {
    let a = ctx.read_u16()?;
    let b = ctx.read_u16()?;
    Ok((a, b))
}

fn read_target<'script>(
    ctx: &mut ScriptContext<'_, 'script, ScriptHost>,
) -> Result<&'script [u8], ScriptError> {
    let offset = ctx.read_u32()?;
    ctx.resolve(offset)
}

fn nop(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    false
}

fn nop1(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    false
}

fn end(ctx: &mut ScriptContext<'_, '_, ScriptHost>, _host: &mut ScriptHost) -> bool {
    ctx.stop();
    false
}

fn script_return(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    match ctx.script_return() {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::Script(e)),
    }
}

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

fn goto(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    match read_target(ctx) {
        Ok(target) => {
            ctx.jump(target);
            false
        }
        Err(e) => trap(ctx, host, CommandTrap::Script(e)),
    }
}

fn goto_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    conditional(ctx, host, ConditionalAction::Jump)
}

fn call_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    conditional(ctx, host, ConditionalAction::Call)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionalAction {
    Jump,
    Call,
}

fn conditional(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    action: ConditionalAction,
) -> bool {
    let condition_byte = match ctx.read_u8() {
        Ok(b) => b,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    // `ScrCmd_goto_if` and `ScrCmd_call_if` consume the target before testing
    // the condition, so a truncated target still fails on a false branch.
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

fn current_compare_result(ctx: &ScriptContext<'_, '_, ScriptHost>) -> CompareResult {
    match ctx.comparison_result() {
        0 => CompareResult::Less,
        1 => CompareResult::Equal,
        // Command handlers only store 0, 1, or 2. Treat corrupted host state
        // as the final table column instead of indexing out of bounds.
        _ => CompareResult::Greater,
    }
}

fn gotostd(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let index = match ctx.read_u8() {
        Ok(i) => i,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    dispatch_std_script(ctx, host, index)
}

fn callstd(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    gotostd(ctx, host)
}

fn gotostd_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    std_conditional(ctx, host)
}

fn callstd_if(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    std_conditional(ctx, host)
}

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

fn dispatch_std_script(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
    index: u8,
) -> bool {
    // All four upstream standard-script commands ignore indices at or beyond
    // `gStdScripts_End` (`src/scrcmd.c`).
    match StdScript::from_index(index) {
        Some(id) => trap(ctx, host, CommandTrap::StdScript(id)),
        None => false,
    }
}

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

fn addvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    let (dest, literal) = match read_u16_pair(ctx) {
        Ok(pair) => pair,
        Err(e) => return trap(ctx, host, CommandTrap::Script(e)),
    };
    let current = match host.event_data.var_get(dest) {
        Ok(v) => v,
        Err(e) => return trap(ctx, host, CommandTrap::EventData(e)),
    };
    // `ScrCmd_addvar` deliberately treats its second operand as a literal;
    // unlike `subvar`, it does not call `VarGet` (`src/scrcmd.c`).
    match host.event_data.var_set(dest, current.wrapping_add(literal)) {
        Ok(()) => false,
        Err(e) => trap(ctx, host, CommandTrap::EventData(e)),
    }
}

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

fn setorcopyvar(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    copyvar(ctx, host)
}

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

const VAR_RESULT: u16 = SPECIAL_VARS_START + 0xD;

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

fn waitstate(ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    host.waiting = true;
    ctx.setup_native(step_waitstate);
    true
}

fn step_waitstate(_ctx: &mut ScriptContext<'_, '_, ScriptHost>, host: &mut ScriptHost) -> bool {
    !host.waiting
}

fn unimplemented<const OP: u8>(
    ctx: &mut ScriptContext<'_, '_, ScriptHost>,
    host: &mut ScriptHost,
) -> bool {
    trap(ctx, host, CommandTrap::Unimplemented(OP))
}

macro_rules! define_command_table {
    ($($name:ident = $opcode:literal $(=> $handler:expr)?),+ $(,)?) => {
        #[cfg(test)]
        macro_rules! opcode {
            $(($name) => { $opcode };)+
        }

        /// Handlers indexed by Emerald field-script opcode.
        ///
        /// The table covers `NOP` through `RANDOM`. Entries without an explicit
        /// handler trap as [`CommandTrap::Unimplemented`].
        pub const COMMAND_TABLE: [Command<ScriptHost>; 0x90] = {
            let mut expected_opcode = 0u8;
            $(
                assert!($opcode == expected_opcode, concat!("non-sequential opcode: ", stringify!($name)));
                expected_opcode += 1;
            )+
            assert!(expected_opcode == 0x90);
            [$(define_command_table!(@handler $opcode $(=> $handler)?)),+]
        };
    };
    (@handler $opcode:literal => $handler:expr) => { $handler };
    (@handler $opcode:literal) => { unimplemented::<$opcode> };
}

define_command_table! {
    NOP = 0x00 => nop,
    NOP1 = 0x01 => nop1,
    END = 0x02 => end,
    RETURN = 0x03 => script_return,
    CALL = 0x04 => call,
    GOTO = 0x05 => goto,
    GOTO_IF = 0x06 => goto_if,
    CALL_IF = 0x07 => call_if,
    GOTO_STD = 0x08 => gotostd,
    CALL_STD = 0x09 => callstd,
    GOTO_STD_IF = 0x0a => gotostd_if,
    CALL_STD_IF = 0x0b => callstd_if,
    RETURN_RAM = 0x0c,
    END_RAM = 0x0d,
    SET_MYSTERY_EVENT_STATUS = 0x0e,
    LOAD_WORD = 0x0f,
    LOAD_BYTE = 0x10,
    SET_PTR = 0x11,
    LOAD_BYTE_FROM_PTR = 0x12,
    SET_PTR_BYTE = 0x13,
    COPY_LOCAL = 0x14,
    COPY_BYTE = 0x15,
    SET_VAR = 0x16 => setvar,
    ADD_VAR = 0x17 => addvar,
    SUB_VAR = 0x18 => subvar,
    COPY_VAR = 0x19 => copyvar,
    SET_OR_COPY_VAR = 0x1a => setorcopyvar,
    COMPARE_LOCAL_TO_LOCAL = 0x1b,
    COMPARE_LOCAL_TO_VALUE = 0x1c,
    COMPARE_LOCAL_TO_PTR = 0x1d,
    COMPARE_PTR_TO_LOCAL = 0x1e,
    COMPARE_PTR_TO_VALUE = 0x1f,
    COMPARE_PTR_TO_PTR = 0x20,
    COMPARE_VAR_TO_VALUE = 0x21 => compare,
    COMPARE_VAR_TO_VAR = 0x22 => comparevars,
    CALL_NATIVE = 0x23,
    GOTO_NATIVE = 0x24,
    SPECIAL = 0x25 => special,
    SPECIAL_VAR = 0x26 => specialvar,
    WAIT_STATE = 0x27 => waitstate,
    DELAY = 0x28,
    SET_FLAG = 0x29 => setflag,
    CLEAR_FLAG = 0x2a => clearflag,
    CHECK_FLAG = 0x2b => checkflag,
    INIT_CLOCK = 0x2c,
    DO_TIME_BASED_EVENTS = 0x2d,
    GET_TIME = 0x2e,
    PLAY_SE = 0x2f,
    WAIT_SE = 0x30,
    PLAY_FANFARE = 0x31,
    WAIT_FANFARE = 0x32,
    PLAY_BGM = 0x33,
    SAVE_BGM = 0x34,
    FADE_DEFAULT_BGM = 0x35,
    FADE_NEW_BGM = 0x36,
    FADE_OUT_BGM = 0x37,
    FADE_IN_BGM = 0x38,
    WARP = 0x39,
    WARP_SILENT = 0x3a,
    WARP_DOOR = 0x3b,
    WARP_HOLE = 0x3c,
    WARP_TELEPORT = 0x3d,
    SET_WARP = 0x3e,
    SET_DYNAMIC_WARP = 0x3f,
    SET_DIVE_WARP = 0x40,
    SET_HOLE_WARP = 0x41,
    GET_PLAYER_XY = 0x42,
    GET_PARTY_SIZE = 0x43,
    ADD_ITEM = 0x44,
    REMOVE_ITEM = 0x45,
    CHECK_ITEM_SPACE = 0x46,
    CHECK_ITEM = 0x47,
    CHECK_ITEM_TYPE = 0x48,
    ADD_PC_ITEM = 0x49,
    CHECK_PC_ITEM = 0x4a,
    ADD_DECORATION = 0x4b,
    REMOVE_DECORATION = 0x4c,
    CHECK_DECOR = 0x4d,
    CHECK_DECOR_SPACE = 0x4e,
    APPLY_MOVEMENT = 0x4f,
    APPLY_MOVEMENT_AT = 0x50,
    WAIT_MOVEMENT = 0x51,
    WAIT_MOVEMENT_AT = 0x52,
    REMOVE_OBJECT = 0x53,
    REMOVE_OBJECT_AT = 0x54,
    ADD_OBJECT = 0x55,
    ADD_OBJECT_AT = 0x56,
    SET_OBJECT_XY = 0x57,
    SHOW_OBJECT_AT = 0x58,
    HIDE_OBJECT_AT = 0x59,
    FACE_PLAYER = 0x5a,
    TURN_OBJECT = 0x5b,
    TRAINER_BATTLE = 0x5c,
    DO_TRAINER_BATTLE = 0x5d,
    GOTO_POST_BATTLE_SCRIPT = 0x5e,
    GOTO_BEATEN_SCRIPT = 0x5f,
    CHECK_TRAINER_FLAG = 0x60,
    SET_TRAINER_FLAG = 0x61,
    CLEAR_TRAINER_FLAG = 0x62,
    SET_OBJECT_XY_PERMANENT = 0x63,
    COPY_OBJECT_XY_TO_PERMANENT = 0x64,
    SET_OBJECT_MOVEMENT_TYPE = 0x65,
    WAIT_MESSAGE = 0x66,
    MESSAGE = 0x67,
    CLOSE_MESSAGE = 0x68,
    LOCK_ALL = 0x69,
    LOCK = 0x6a,
    RELEASE_ALL = 0x6b,
    RELEASE = 0x6c,
    WAIT_BUTTON_PRESS = 0x6d,
    YES_NO_BOX = 0x6e,
    MULTICHOICE = 0x6f,
    MULTICHOICE_DEFAULT = 0x70,
    MULTICHOICE_GRID = 0x71,
    DRAW_BOX = 0x72,
    ERASE_BOX = 0x73,
    DRAW_BOX_TEXT = 0x74,
    SHOW_MON_PIC = 0x75,
    HIDE_MON_PIC = 0x76,
    SHOW_CONTEST_PAINTING = 0x77,
    BRAILLE_MESSAGE = 0x78,
    GIVE_MON = 0x79,
    GIVE_EGG = 0x7a,
    SET_MON_MOVE = 0x7b,
    CHECK_PARTY_MOVE = 0x7c,
    BUFFER_SPECIES_NAME = 0x7d,
    BUFFER_LEAD_MON_SPECIES_NAME = 0x7e,
    BUFFER_PARTY_MON_NICKNAME = 0x7f,
    BUFFER_ITEM_NAME = 0x80,
    BUFFER_DECORATION_NAME = 0x81,
    BUFFER_MOVE_NAME = 0x82,
    BUFFER_NUMBER_STRING = 0x83,
    BUFFER_STD_STRING = 0x84,
    BUFFER_STRING = 0x85,
    POKEMART = 0x86,
    POKEMART_DECORATION = 0x87,
    POKEMART_DECORATION_2 = 0x88,
    PLAY_SLOT_MACHINE = 0x89,
    SET_BERRY_TREE = 0x8a,
    CHOOSE_CONTEST_MON = 0x8b,
    START_CONTEST = 0x8c,
    SHOW_CONTEST_RESULTS = 0x8d,
    CONTEST_LINK_TRANSFER = 0x8e,
    RANDOM = 0x8f => random,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_data::{SPECIAL_FLAGS_START, VARS_START};

    fn setup() -> (ScriptContext<'static, 'static, ScriptHost>, ScriptHost) {
        (ScriptContext::new(&COMMAND_TABLE), ScriptHost::default())
    }

    #[test]
    fn nop_and_nop1_do_nothing_and_advance_past_themselves() {
        let (mut ctx, mut host) = setup();
        let bytes = [opcode!(NOP), opcode!(NOP1), opcode!(END)];
        ctx.setup_bytecode(&bytes);
        assert!(!ctx.run(&mut host), "end halts the script");
        assert!(ctx.is_stopped());
        assert_eq!(host.trap, None);
    }

    #[test]
    fn end_halts_the_script() {
        let (mut ctx, mut host) = setup();
        let bytes = [opcode!(END), opcode!(NOP)];
        ctx.setup_bytecode(&bytes);
        assert!(!ctx.run(&mut host));
        assert!(ctx.is_stopped());
    }

    #[test]
    fn goto_jumps_to_the_resolved_offset_skipping_what_linear_flow_would_hit() {
        let (mut ctx, mut host) = setup();
        let skipped_flag = 999u16;
        let target_flag = 100u16;
        let target_offset = 10u32;
        let mut bytes = vec![opcode!(GOTO)];
        bytes.extend_from_slice(&target_offset.to_le_bytes());
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&skipped_flag.to_le_bytes());
        bytes.push(opcode!(END));
        bytes.push(opcode!(NOP));
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&target_flag.to_le_bytes());
        bytes.push(opcode!(END));
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(
            host.event_data.flag_get(target_flag),
            Ok(true),
            "the jump target's setflag ran"
        );
        assert_eq!(
            host.event_data.flag_get(skipped_flag),
            Ok(false),
            "the setflag linear flow would have hit was skipped over"
        );
    }

    #[test]
    fn call_pushes_a_return_address_and_return_resumes_after_it() {
        let (mut ctx, mut host) = setup();
        let after_call_flag = 200u16;
        let subroutine_flag = 100u16;
        let subroutine_offset = 9u32;
        let mut bytes = vec![opcode!(CALL)];
        bytes.extend_from_slice(&subroutine_offset.to_le_bytes());
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&after_call_flag.to_le_bytes());
        bytes.push(opcode!(END));
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&subroutine_flag.to_le_bytes());
        bytes.push(opcode!(RETURN));
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(
            host.event_data.flag_get(subroutine_flag),
            Ok(true),
            "subroutine ran"
        );
        assert_eq!(
            host.event_data.flag_get(after_call_flag),
            Ok(true),
            "resumed after the call site"
        );
        assert_eq!(ctx.stack_depth(), 0);
    }

    #[test]
    fn goto_with_out_of_range_target_traps() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(GOTO)];
        bytes.extend_from_slice(&999u32.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::Script(ScriptError::InvalidJumpTarget(999)))
        );
    }

    #[test]
    fn goto_if_truth_table_matches_upstream_condition_table() {
        #[rustfmt::skip]
        let cases = [
            (ScriptCondition::LessThan, CompareResult::Less, true),
            (ScriptCondition::LessThan, CompareResult::Equal, false),
            (ScriptCondition::LessThan, CompareResult::Greater, false),
            (ScriptCondition::Equal, CompareResult::Less, false),
            (ScriptCondition::Equal, CompareResult::Equal, true),
            (ScriptCondition::Equal, CompareResult::Greater, false),
            (ScriptCondition::GreaterThan, CompareResult::Less, false),
            (ScriptCondition::GreaterThan, CompareResult::Equal, false),
            (ScriptCondition::GreaterThan, CompareResult::Greater, true),
            (ScriptCondition::LessThanOrEqual, CompareResult::Less, true),
            (ScriptCondition::LessThanOrEqual, CompareResult::Equal, true),
            (ScriptCondition::LessThanOrEqual, CompareResult::Greater, false),
            (ScriptCondition::GreaterThanOrEqual, CompareResult::Less, false),
            (ScriptCondition::GreaterThanOrEqual, CompareResult::Equal, true),
            (ScriptCondition::GreaterThanOrEqual, CompareResult::Greater, true),
            (ScriptCondition::NotEqual, CompareResult::Less, true),
            (ScriptCondition::NotEqual, CompareResult::Equal, false),
            (ScriptCondition::NotEqual, CompareResult::Greater, true),
        ];

        for (condition, comparison_result, expect_jump) in cases {
            let (mut ctx, mut host) = setup();
            let linear_flag = 1u16;
            let target_flag = 2u16;
            let target_offset = 10u32;
            let mut bytes = vec![opcode!(GOTO_IF), condition as u8];
            bytes.extend_from_slice(&target_offset.to_le_bytes());
            bytes.push(opcode!(SET_FLAG));
            bytes.extend_from_slice(&linear_flag.to_le_bytes());
            bytes.push(opcode!(END));
            bytes.push(opcode!(SET_FLAG));
            bytes.extend_from_slice(&target_flag.to_le_bytes());
            bytes.push(opcode!(END));
            ctx.setup_bytecode(&bytes);
            ctx.set_comparison_result(comparison_result as u8);

            assert!(!ctx.run(&mut host));
            assert_eq!(
                host.trap, None,
                "condition {condition:?} result {comparison_result:?}"
            );
            assert_eq!(
                host.event_data.flag_get(target_flag),
                Ok(expect_jump),
                "condition {condition:?}, comparison result {comparison_result:?}: expected jump = {expect_jump}"
            );
            assert_eq!(host.event_data.flag_get(linear_flag), Ok(!expect_jump));
        }
    }

    #[test]
    fn call_if_jumps_via_call_so_return_comes_back() {
        let (mut ctx, mut host) = setup();
        let after_call_flag = 9u16;
        let subroutine_flag = 10u16;
        let subroutine_offset = 15u32;
        let mut bytes = vec![opcode!(COMPARE_VAR_TO_VALUE)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(opcode!(CALL_IF));
        bytes.push(ScriptCondition::Equal as u8);
        bytes.extend_from_slice(&subroutine_offset.to_le_bytes());
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&after_call_flag.to_le_bytes());
        bytes.push(opcode!(END));
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&subroutine_flag.to_le_bytes());
        bytes.push(opcode!(RETURN));
        ctx.setup_bytecode(&bytes);
        host.event_data.var_set(VARS_START, 5).unwrap();

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None);
        assert_eq!(
            host.event_data.flag_get(subroutine_flag),
            Ok(true),
            "call_if called the subroutine"
        );
        assert_eq!(
            host.event_data.flag_get(after_call_flag),
            Ok(true),
            "resumed after the call site"
        );
    }

    #[test]
    fn goto_if_with_invalid_condition_byte_traps() {
        let (mut ctx, mut host) = setup();
        let invalid_condition = 6;
        let mut bytes = vec![opcode!(GOTO_IF), invalid_condition];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::InvalidCondition(invalid_condition))
        );
    }

    #[test]
    fn setvar_writes_a_literal_value() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SET_VAR)];
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
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SET_VAR)];
        bytes.extend_from_slice(&VAR_RESULT.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, None, "setvar on a special var must not trap");
        assert_eq!(host.event_data.var_get(VAR_RESULT), Ok(7));
    }

    #[test]
    fn addvar_adds_a_literal_and_wraps() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 0xFFFF).unwrap();
        let mut bytes = vec![opcode!(ADD_VAR)];
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
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 3).unwrap();
        host.event_data.var_set(VARS_START + 1, 10).unwrap();
        let mut bytes = vec![opcode!(ADD_VAR)];
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
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
        let mut bytes = vec![opcode!(SUB_VAR)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
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
        let mut bytes = vec![opcode!(COPY_VAR)];
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
        let mut bytes = vec![opcode!(SET_OR_COPY_VAR)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.var_get(VARS_START), Ok(55));
    }

    #[test]
    fn setorcopyvar_treats_a_non_var_id_as_a_literal() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SET_OR_COPY_VAR)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&123u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.var_get(VARS_START), Ok(123));
    }

    #[test]
    fn compare_var_to_value_sets_comparison_result() {
        for (stored, literal, expected) in [
            (5u16, 10u16, CompareResult::Less),
            (5, 5, CompareResult::Equal),
            (10, 5, CompareResult::Greater),
        ] {
            let (mut ctx, mut host) = setup();
            host.event_data.var_set(VARS_START, stored).unwrap();
            let mut bytes = vec![opcode!(COMPARE_VAR_TO_VALUE)];
            bytes.extend_from_slice(&VARS_START.to_le_bytes());
            bytes.extend_from_slice(&literal.to_le_bytes());
            ctx.setup_bytecode(&bytes);

            ctx.run(&mut host);
            assert_eq!(
                ctx.comparison_result(),
                expected as u8,
                "stored={stored} literal={literal}"
            );
        }
    }

    #[test]
    fn comparevars_sets_comparison_result() {
        let (mut ctx, mut host) = setup();
        host.event_data.var_set(VARS_START, 3).unwrap();
        host.event_data.var_set(VARS_START + 1, 9).unwrap();
        let mut bytes = vec![opcode!(COMPARE_VAR_TO_VAR)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&(VARS_START + 1).to_le_bytes());
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(ctx.comparison_result(), 0, "3 < 9");
    }

    #[test]
    fn setflag_clearflag_checkflag_round_trip() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SET_FLAG)];
        bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        bytes.push(opcode!(CHECK_FLAG));
        bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        bytes.push(opcode!(END));
        ctx.setup_bytecode(&bytes);

        ctx.run(&mut host);
        assert_eq!(host.event_data.flag_get(SPECIAL_FLAGS_START), Ok(true));
        assert_eq!(
            ctx.comparison_result(),
            1,
            "checkflag stored the flag's TRUE value"
        );

        let (mut ctx2, mut host2) = setup();
        let mut clear_bytes = vec![opcode!(CLEAR_FLAG)];
        clear_bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        clear_bytes.push(opcode!(CHECK_FLAG));
        clear_bytes.extend_from_slice(&SPECIAL_FLAGS_START.to_le_bytes());
        clear_bytes.push(opcode!(END));
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
        let bytes = [opcode!(RETURN_RAM)];
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::Unimplemented(opcode!(RETURN_RAM)))
        );
        assert!(ctx.is_stopped());
    }

    #[test]
    fn event_data_out_of_range_error_traps_instead_of_propagating_silently() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SET_FLAG)];
        let bad_id = SPECIAL_FLAGS_START - 1;
        bytes.extend_from_slice(&bad_id.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::EventData(EventDataError::OutOfRange(bad_id)))
        );
    }

    #[test]
    fn gotostd_traps_naming_the_resolved_std_script() {
        let (mut ctx, mut host) = setup();
        let bytes = [opcode!(GOTO_STD), StdScript::MsgboxNpc.index()];
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
        let bytes = [opcode!(CALL_STD), StdScript::ObtainItem.index()];
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
        let mut bytes = vec![opcode!(GOTO_STD), StdScript::COUNT];
        bytes.push(opcode!(SET_FLAG));
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(opcode!(END));
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
        let mut bytes = vec![opcode!(COMPARE_VAR_TO_VALUE)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.push(opcode!(GOTO_STD_IF));
        bytes.push(ScriptCondition::NotEqual as u8);
        bytes.push(StdScript::MsgboxSign.index());
        bytes.push(opcode!(END));
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
        let bytes = [
            opcode!(GOTO_STD_IF),
            ScriptCondition::Equal as u8,
            StdScript::MsgboxSign.index(),
        ];
        ctx.setup_bytecode(&bytes);
        ctx.set_comparison_result(CompareResult::Equal as u8);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::MsgboxSign))
        );
    }

    #[test]
    fn callstd_if_behaves_like_gotostd_if() {
        let (mut ctx, mut host) = setup();
        let bytes = [
            opcode!(CALL_STD_IF),
            ScriptCondition::Equal as u8,
            StdScript::ObtainDecoration.index(),
        ];
        ctx.setup_bytecode(&bytes);
        ctx.set_comparison_result(CompareResult::Equal as u8);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::StdScript(StdScript::ObtainDecoration))
        );
    }

    #[test]
    fn gotostd_if_with_invalid_condition_byte_traps() {
        let (mut ctx, mut host) = setup();
        let invalid_condition = 6;
        let bytes = [
            opcode!(GOTO_STD_IF),
            invalid_condition,
            StdScript::ObtainItem.index(),
        ];
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(
            host.trap,
            Some(CommandTrap::InvalidCondition(invalid_condition))
        );
    }

    #[test]
    fn random_writes_random_mod_limit_to_var_result() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(RANDOM)];
        bytes.extend_from_slice(&10u16.to_le_bytes());
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
        let mut bytes = vec![opcode!(RANDOM)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
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
        let mut bytes = vec![opcode!(RANDOM)];
        bytes.extend_from_slice(&0u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::DivideByZero));
    }

    #[test]
    fn random_draws_advance_the_generator_across_calls() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(RANDOM)];
        bytes.extend_from_slice(&100u16.to_le_bytes());
        bytes.push(opcode!(RANDOM));
        bytes.extend_from_slice(&100u16.to_le_bytes());
        ctx.setup_bytecode(&bytes);
        host.rng.seed(0);

        assert!(!ctx.run(&mut host));
        let mut probe = crate::rng::Rng::new(0);
        let _first_draw = probe.next_u16();
        let expected_second = probe.next_u16() % 100;
        assert_eq!(
            host.event_data.var_get(VAR_RESULT),
            Ok(expected_second),
            "the second random draw used the advanced generator state, not a repeat of the first"
        );
    }

    #[test]
    fn special_traps_unimplemented_for_every_currently_valid_id() {
        let (mut ctx, mut host) = setup();
        let special_index = 0u16;
        let mut bytes = vec![opcode!(SPECIAL)];
        bytes.extend_from_slice(&special_index.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        let id = crate::script::specials::SpecialId::from_index(special_index).unwrap();
        assert_eq!(host.trap, Some(CommandTrap::UnimplementedSpecial(id)));
    }

    #[test]
    fn special_with_out_of_range_index_traps_invalid_special() {
        let (mut ctx, mut host) = setup();
        let mut bytes = vec![opcode!(SPECIAL)];
        let bad = u16::try_from(crate::script::specials::SPECIAL_COUNT).unwrap();
        bytes.extend_from_slice(&bad.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert_eq!(host.trap, Some(CommandTrap::InvalidSpecial(bad)));
    }

    #[test]
    fn specialvar_reads_output_var_then_index_and_traps_unimplemented() {
        let (mut ctx, mut host) = setup();
        let special_index = 1u16;
        let mut bytes = vec![opcode!(SPECIAL_VAR)];
        bytes.extend_from_slice(&VARS_START.to_le_bytes());
        bytes.extend_from_slice(&special_index.to_le_bytes());
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        let id = crate::script::specials::SpecialId::from_index(special_index).unwrap();
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
        let mut bytes = vec![opcode!(SPECIAL_VAR)];
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
        let flag_id = 5u16;
        let mut bytes = vec![opcode!(WAIT_STATE), opcode!(SET_FLAG)];
        bytes.extend_from_slice(&flag_id.to_le_bytes());
        bytes.push(opcode!(END));
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host), "waitstate yields, script stays active");
        assert!(!ctx.is_stopped());
        assert!(host.waiting, "waitstate sets the pause flag");
        assert_eq!(
            host.event_data.flag_get(flag_id),
            Ok(false),
            "bytecode after waitstate must not run while still waiting"
        );

        assert!(ctx.run(&mut host));
        assert!(!ctx.is_stopped());
        assert_eq!(host.event_data.flag_get(flag_id), Ok(false));

        host.waiting = false;
        assert!(
            ctx.run(&mut host),
            "still yields on the call that finishes the native wait"
        );
        assert!(!ctx.is_stopped());

        assert!(!ctx.run(&mut host), "bytecode resumes and runs to `end`");
        assert!(ctx.is_stopped());
        assert_eq!(
            host.event_data.flag_get(flag_id),
            Ok(true),
            "setflag after waitstate ran once unblocked"
        );
    }
}
