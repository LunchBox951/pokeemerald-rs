//! Caller-owned script bytecode execution.
//!
//! [`ScriptContext`] owns a script cursor, return stack, scratch registers,
//! comparison result, and dispatch mode. Commands and native steps receive a
//! mutable host alongside that state instead of accessing a global world.
//!
//! Command tables and script bytes have independent lifetimes. The
//! higher-ranked [`Command`] and [`NativeStep`] pointers let a long-lived table
//! execute short-lived script data.

use std::fmt;

pub mod commands;
pub mod specials;
pub mod std_script;

const CALL_STACK_SLOTS: usize = 20;
const MAX_CALL_DEPTH: usize = CALL_STACK_SLOTS - 1;
const DATA_REGISTER_COUNT: usize = 4;

/// A bytecode command that returns whether execution should yield.
///
/// Commands may read operands and mutate both the context and caller-owned
/// host. Elided lifetimes keep the command table independent of script data.
pub type Command<H> = fn(&mut ScriptContext<'_, '_, H>, &mut H) -> bool;

/// A native step that returns whether bytecode should resume on the next run.
pub type NativeStep<H> = fn(&mut ScriptContext<'_, '_, H>, &mut H) -> bool;

/// Failures from script stack, dispatch, target, and operand operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptError {
    /// The return stack is at its Emerald-compatible effective limit.
    StackOverflow,
    /// The return stack is empty.
    StackUnderflow,
    /// The opcode has no command-table entry.
    OpcodeOutOfRange(u8),
    /// No script is loaded, or the byte offset is outside it.
    InvalidJumpTarget(u32),
    /// The cursor does not contain a complete operand.
    UnexpectedEnd,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StackOverflow => {
                write!(f, "script call stack is full (max depth {MAX_CALL_DEPTH})")
            }
            Self::StackUnderflow => write!(f, "script call stack is empty"),
            Self::OpcodeOutOfRange(op) => {
                write!(f, "opcode {op:#04x} has no command table entry")
            }
            Self::InvalidJumpTarget(offset) => {
                write!(f, "jump-target offset {offset:#010x} is out of range")
            }
            Self::UnexpectedEnd => write!(f, "script cursor ran out of bytes"),
        }
    }
}

impl std::error::Error for ScriptError {}

#[derive(Debug)]
enum Mode<H> {
    Stopped,
    Bytecode,
    Native(NativeStep<H>),
}

/// Execution state for one script.
///
/// `'commands` and `'script` are independent. `H` is caller-owned state passed
/// to each command and native step.
#[derive(Debug)]
pub struct ScriptContext<'commands, 'script, H> {
    commands: &'commands [Command<H>],
    mode: Mode<H>,
    comparison_result: u8,
    cursor: Option<&'script [u8]>,
    script_base: Option<&'script [u8]>,
    return_stack: [&'script [u8]; CALL_STACK_SLOTS],
    return_stack_depth: usize,
    data_registers: [u32; DATA_REGISTER_COUNT],
}

impl<'commands, 'script, H> ScriptContext<'commands, 'script, H> {
    /// Creates a stopped context with no loaded script.
    #[must_use]
    pub const fn new(commands: &'commands [Command<H>]) -> Self {
        Self {
            commands,
            mode: Mode::Stopped,
            comparison_result: 0,
            cursor: None,
            script_base: None,
            return_stack: [&[]; CALL_STACK_SLOTS],
            return_stack_depth: 0,
            data_registers: [0; DATA_REGISTER_COUNT],
        }
    }

    /// Loads a script, resets its offset base, and enters bytecode mode.
    pub fn setup_bytecode(&mut self, script: &'script [u8]) {
        self.cursor = Some(script);
        self.script_base = Some(script);
        self.mode = Mode::Bytecode;
    }

    /// Enters native mode and runs `step` on the next [`run`](Self::run) call.
    pub fn setup_native(&mut self, step: NativeStep<H>) {
        self.mode = Mode::Native(step);
    }

    /// Stops execution and clears the cursor.
    pub fn stop(&mut self) {
        self.mode = Mode::Stopped;
        self.cursor = None;
    }

    /// Returns whether execution has stopped.
    #[must_use]
    pub const fn is_stopped(&self) -> bool {
        matches!(self.mode, Mode::Stopped)
    }

    /// Returns the current return-stack depth.
    #[must_use]
    pub const fn stack_depth(&self) -> usize {
        self.return_stack_depth
    }

    /// Returns the unread script suffix, or `None` when no script is loaded.
    #[must_use]
    pub const fn cursor(&self) -> Option<&'script [u8]> {
        self.cursor
    }

    /// Returns the command scratch registers.
    #[must_use]
    pub const fn data(&self) -> [u32; DATA_REGISTER_COUNT] {
        self.data_registers
    }

    /// Returns mutable access to the command scratch registers.
    pub fn data_mut(&mut self) -> &mut [u32; DATA_REGISTER_COUNT] {
        &mut self.data_registers
    }

    /// Returns the comparison result used by conditional commands.
    #[must_use]
    pub const fn comparison_result(&self) -> u8 {
        self.comparison_result
    }

    /// Sets the comparison result used by conditional commands.
    pub fn set_comparison_result(&mut self, value: u8) {
        self.comparison_result = value;
    }

    /// Returns the command registered for `opcode`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::OpcodeOutOfRange`] when the command table has no
    /// matching entry.
    pub fn lookup_command(&self, opcode: u8) -> Result<Command<H>, ScriptError> {
        self.commands
            .get(usize::from(opcode))
            .copied()
            .ok_or(ScriptError::OpcodeOutOfRange(opcode))
    }

    /// Pushes a return address.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackOverflow`] at Emerald's effective maximum
    /// depth. `ScriptPush` in `pokeemerald/src/script.c` rejects the next push
    /// before filling `ScriptContext::stack[20]` from `include/script.h`.
    pub fn push(&mut self, return_address: &'script [u8]) -> Result<(), ScriptError> {
        if self.return_stack_depth >= MAX_CALL_DEPTH {
            return Err(ScriptError::StackOverflow);
        }
        self.return_stack[self.return_stack_depth] = return_address;
        self.return_stack_depth += 1;
        Ok(())
    }

    /// Pops the most recently pushed return address.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackUnderflow`] when the stack is empty.
    pub fn pop(&mut self) -> Result<&'script [u8], ScriptError> {
        if self.return_stack_depth == 0 {
            return Err(ScriptError::StackUnderflow);
        }
        self.return_stack_depth -= 1;
        Ok(self.return_stack[self.return_stack_depth])
    }

    /// Moves the cursor to `target` without changing the return stack.
    pub fn jump(&mut self, target: &'script [u8]) {
        self.cursor = Some(target);
    }

    /// Pushes the current cursor and moves it to `target`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackOverflow`] without moving the cursor when
    /// the return stack is full.
    pub fn call(&mut self, target: &'script [u8]) -> Result<(), ScriptError> {
        self.push(self.cursor.unwrap_or(&[]))?;
        self.cursor = Some(target);
        Ok(())
    }

    /// Pops a return address and moves the cursor to it.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackUnderflow`] when the return stack is empty.
    pub fn script_return(&mut self) -> Result<(), ScriptError> {
        let return_address = self.pop()?;
        self.cursor = Some(return_address);
        Ok(())
    }

    /// Resolves a byte offset from the most recently loaded script's start.
    ///
    /// Offsets preserve the four-byte `ScriptReadWord` operand from
    /// `pokeemerald/src/script.c` while replacing `ScriptContext::scriptPtr`'s
    /// raw address space from `include/script.h` with a target bounded by the
    /// script slice `(oop-boundaries)`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::InvalidJumpTarget`] when no script is loaded or
    /// `offset` exceeds its length.
    pub fn resolve(&self, offset: u32) -> Result<&'script [u8], ScriptError> {
        self.script_base
            .and_then(|script| {
                usize::try_from(offset)
                    .ok()
                    .and_then(|offset| script.get(offset..))
            })
            .ok_or(ScriptError::InvalidJumpTarget(offset))
    }

    /// Reads one byte and advances the cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] when no byte remains.
    pub fn read_u8(&mut self) -> Result<u8, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let (&value, rest) = cursor.split_first().ok_or(ScriptError::UnexpectedEnd)?;
        self.cursor = Some(rest);
        Ok(value)
    }

    /// Reads a little-endian `u16` and advances the cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] when fewer than two bytes remain.
    pub fn read_u16(&mut self) -> Result<u16, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let bytes = cursor.get(..2).ok_or(ScriptError::UnexpectedEnd)?;
        let value = u16::from_le_bytes([bytes[0], bytes[1]]);
        self.cursor = Some(&cursor[2..]);
        Ok(value)
    }

    /// Reads a little-endian `u32` and advances the cursor.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] when fewer than four bytes remain.
    pub fn read_u32(&mut self) -> Result<u32, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let bytes = cursor.get(..4).ok_or(ScriptError::UnexpectedEnd)?;
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        self.cursor = Some(&cursor[4..]);
        Ok(value)
    }

    /// Runs until a command yields, one native step runs, or execution stops.
    ///
    /// Returns `true` while work remains. Native mode always yields for one
    /// call; matching `RunScriptCommand` in `pokeemerald/src/script.c`, a
    /// completed native step resumes bytecode on the next call. Exhausted
    /// bytecode and unknown opcodes stop execution and return `false`.
    pub fn run(&mut self, host: &mut H) -> bool {
        match self.mode {
            Mode::Stopped => false,
            Mode::Native(step) => {
                if step(self, host) {
                    self.mode = Mode::Bytecode;
                }
                true
            }
            Mode::Bytecode => loop {
                let Some(cursor) = self.cursor else {
                    self.mode = Mode::Stopped;
                    break false;
                };
                let Some((&opcode, rest)) = cursor.split_first() else {
                    self.mode = Mode::Stopped;
                    break false;
                };
                self.cursor = Some(rest);
                let Ok(command) = self.lookup_command(opcode) else {
                    self.mode = Mode::Stopped;
                    break false;
                };
                if command(self, host) {
                    break true;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct TestHost {
        ticks: u32,
    }

    fn command_noop(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        false
    }

    fn command_yield(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        true
    }

    fn command_increment(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        ctx.data_mut()[0] += 1;
        false
    }

    fn command_tick_host(_ctx: &mut ScriptContext<'_, '_, TestHost>, host: &mut TestHost) -> bool {
        host.ticks += 1;
        false
    }

    #[test]
    fn sequential_dispatch_runs_commands_in_order_until_yield() {
        const INCREMENT: u8 = 0;
        const YIELD: u8 = 1;
        let commands: &[Command<TestHost>] = &[command_increment, command_yield];
        let bytecode = [INCREMENT, INCREMENT, INCREMENT, YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(context.run(&mut host), "should yield, not stop");
        assert_eq!(context.data()[0], 3, "all three increment commands ran");
        assert!(!context.is_stopped());
    }

    #[test]
    fn commands_can_mutate_the_host_world() {
        const TICK_HOST: u8 = 0;
        const YIELD: u8 = 1;
        let commands: &[Command<TestHost>] = &[command_tick_host, command_yield];
        let bytecode = [TICK_HOST, TICK_HOST, YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(context.run(&mut host));
        assert_eq!(host.ticks, 2, "both ticks reached the host");
    }

    #[test]
    fn static_command_table_drives_a_locally_owned_script() {
        const INCREMENT: u8 = 0;
        const YIELD: u8 = 1;
        static COMMANDS: &[Command<TestHost>] = &[command_increment, command_yield];
        let locally_owned_bytecode = vec![INCREMENT, INCREMENT, YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(COMMANDS);
        context.setup_bytecode(&locally_owned_bytecode);

        assert!(context.run(&mut host));
        assert_eq!(context.data()[0], 2);
    }

    #[test]
    fn opcode_past_table_end_stops_the_script_without_panicking() {
        const UNKNOWN_OPCODE: u8 = 5;
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytecode = [UNKNOWN_OPCODE];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(
            !context.run(&mut host),
            "out-of-range opcode stops the script"
        );
        assert!(context.is_stopped());
    }

    #[test]
    fn lookup_command_reports_the_offending_opcode() {
        const NOOP: u8 = 0;
        const UNKNOWN_OPCODE: u8 = 9;
        let commands: &[Command<TestHost>] = &[command_noop, command_yield];
        let context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(context.lookup_command(NOOP).map(|_| ()), Ok(()));
        assert_eq!(
            context.lookup_command(UNKNOWN_OPCODE),
            Err(ScriptError::OpcodeOutOfRange(UNKNOWN_OPCODE))
        );
    }

    #[test]
    fn running_off_the_end_of_the_script_stops_cleanly() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytecode = [];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(!context.run(&mut host));
        assert!(context.is_stopped());
    }

    #[test]
    fn a_context_that_was_never_started_reports_stopped() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        assert!(context.is_stopped());
        assert!(!context.run(&mut host));
    }

    #[test]
    fn jump_redirects_the_cursor() {
        const INCREMENT: u8 = 0;
        const YIELD: u8 = 1;
        const JUMP: u8 = 2;
        static JUMP_TARGET: [u8; 2] = [INCREMENT, YIELD];

        fn command_jump_to_increment(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            _host: &mut TestHost,
        ) -> bool {
            ctx.jump(&JUMP_TARGET);
            false
        }
        let commands: &[Command<TestHost>] =
            &[command_increment, command_yield, command_jump_to_increment];
        let bytecode = [JUMP];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(context.run(&mut host));
        assert_eq!(context.data()[0], 1, "the jumped-to increment ran");
    }

    #[test]
    fn call_and_return_resume_after_the_call_site() {
        const INCREMENT: u8 = 0;
        const YIELD: u8 = 1;
        const CALL_SUBROUTINE: u8 = 2;
        const RETURN: u8 = 3;
        static SUBROUTINE: [u8; 2] = [INCREMENT, RETURN];

        fn command_call_subroutine(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            _host: &mut TestHost,
        ) -> bool {
            ctx.call(&SUBROUTINE).expect("stack has room");
            false
        }
        fn command_return(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let commands: &[Command<TestHost>] = &[
            command_increment,
            command_yield,
            command_call_subroutine,
            command_return,
        ];
        let bytecode = [CALL_SUBROUTINE, INCREMENT, YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(context.run(&mut host));
        assert_eq!(
            context.data()[0],
            2,
            "one increment inside the call, one after"
        );
        assert_eq!(context.stack_depth(), 0, "the return unwound the frame");
    }

    #[test]
    fn nested_calls_unwind_in_reverse_order() {
        const INCREMENT: u8 = 0;
        const YIELD: u8 = 1;
        const CALL_INNER: u8 = 2;
        const RETURN: u8 = 3;
        const CALL_OUTER: u8 = 4;
        static INNER: [u8; 2] = [INCREMENT, RETURN];
        static OUTER: [u8; 3] = [CALL_INNER, INCREMENT, RETURN];

        fn command_call_inner(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            _host: &mut TestHost,
        ) -> bool {
            ctx.call(&INNER).expect("stack has room");
            false
        }
        fn command_call_outer(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            _host: &mut TestHost,
        ) -> bool {
            ctx.call(&OUTER).expect("stack has room");
            false
        }
        fn command_return(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let commands: &[Command<TestHost>] = &[
            command_increment,
            command_yield,
            command_call_inner,
            command_return,
            command_call_outer,
        ];
        let bytecode = [CALL_OUTER, YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);

        assert!(context.run(&mut host));
        assert_eq!(context.data()[0], 2, "inner and outer each increment once");
        assert_eq!(context.stack_depth(), 0, "both frames unwound");
    }

    #[test]
    fn stack_overflow_is_a_typed_error_not_a_panic() {
        const EMERALD_EFFECTIVE_CALL_DEPTH: usize = 19;
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        let filler: &'static [u8] = &[];

        let mut pushed = 0;
        loop {
            match context.push(filler) {
                Ok(()) => pushed += 1,
                Err(ScriptError::StackOverflow) => break,
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert_eq!(MAX_CALL_DEPTH, EMERALD_EFFECTIVE_CALL_DEPTH);
        assert_eq!(pushed, EMERALD_EFFECTIVE_CALL_DEPTH);
        assert_eq!(context.stack_depth(), EMERALD_EFFECTIVE_CALL_DEPTH);
        assert_eq!(context.push(filler), Err(ScriptError::StackOverflow));
    }

    #[test]
    fn stack_underflow_is_a_typed_error_not_a_panic() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(context.pop(), Err(ScriptError::StackUnderflow));
        assert_eq!(context.script_return(), Err(ScriptError::StackUnderflow));
    }

    #[test]
    fn call_does_not_jump_when_the_stack_is_full() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        let bytecode: &'static [u8] = &[1, 2, 3];
        context.setup_bytecode(bytecode);
        for _ in 0..MAX_CALL_DEPTH {
            context.push(&[]).unwrap();
        }

        let target: &'static [u8] = &[9];
        assert_eq!(context.call(target), Err(ScriptError::StackOverflow));
        assert_eq!(
            context.cursor(),
            Some(bytecode),
            "the cursor is untouched when the call is refused"
        );
    }

    #[test]
    fn read_u8_advances_the_cursor() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x2A, 0xFF];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);

        assert_eq!(context.read_u8(), Ok(0x2A));
        assert_eq!(context.cursor(), Some(&bytes[1..]));
        assert_eq!(context.read_u8(), Ok(0xFF));
        assert_eq!(context.cursor(), Some(&bytes[2..]));
    }

    #[test]
    fn read_u8_past_the_end_is_a_typed_error() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x01];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);
        assert_eq!(context.read_u8(), Ok(0x01));
        assert_eq!(context.read_u8(), Err(ScriptError::UnexpectedEnd));

        let mut fresh: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(fresh.read_u8(), Err(ScriptError::UnexpectedEnd));
    }

    #[test]
    fn read_u16_is_little_endian_and_advances_the_cursor() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x34, 0x12, 0xAA];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);

        assert_eq!(context.read_u16(), Ok(0x1234));
        assert_eq!(context.cursor(), Some(&bytes[2..]));
    }

    #[test]
    fn read_u32_is_little_endian_and_advances_the_cursor() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x78, 0x56, 0x34, 0x12, 0xFF];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);

        assert_eq!(context.read_u32(), Ok(0x1234_5678));
        assert_eq!(context.cursor(), Some(&bytes[4..]));
    }

    #[test]
    fn reads_past_the_end_of_the_cursor_are_a_typed_error() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x01];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);
        assert_eq!(context.read_u16(), Err(ScriptError::UnexpectedEnd));

        let mut fresh: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(fresh.read_u32(), Err(ScriptError::UnexpectedEnd));
    }

    #[test]
    fn native_mode_always_yields_and_can_hand_off_to_bytecode() {
        fn step_not_done(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            false
        }
        fn step_done(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            true
        }
        const YIELD: u8 = 0;
        let commands: &[Command<TestHost>] = &[command_yield];
        let bytecode = [YIELD];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_bytecode(&bytecode);
        context.setup_native(step_not_done);

        assert!(
            context.run(&mut host),
            "native mode always yields this call"
        );
        assert!(!context.is_stopped());

        context.setup_native(step_done);
        assert!(
            context.run(&mut host),
            "still yields on the call that finishes the step"
        );
        assert!(context.run(&mut host), "bytecode command yields");
    }

    #[test]
    fn native_step_can_inspect_context_and_host() {
        const TICKS_UNTIL_DONE: u32 = 2;

        fn step_wait_two_ticks(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            host: &mut TestHost,
        ) -> bool {
            host.ticks += 1;
            ctx.data_mut()[0] = host.ticks;
            host.ticks >= TICKS_UNTIL_DONE
        }
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut host = TestHost::default();
        let mut context = ScriptContext::new(commands);
        context.setup_native(step_wait_two_ticks);

        assert!(context.run(&mut host), "still waiting after one tick");
        assert_eq!(host.ticks, 1);
        assert_eq!(
            context.data()[0],
            1,
            "native step wrote context scratch state"
        );

        assert!(context.run(&mut host), "yields on the finishing call too");
        assert_eq!(host.ticks, 2, "native step reached the host again");
    }

    #[test]
    fn resolve_returns_the_suffix_at_the_given_offset() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x10, 0x11, 0x12, 0x13];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);

        assert_eq!(context.resolve(0), Ok(&bytes[..]));
        assert_eq!(context.resolve(2), Ok(&bytes[2..]));
        assert_eq!(
            context.resolve(4),
            Ok(&bytes[4..]),
            "offset == len is the empty tail"
        );
    }

    #[test]
    fn resolve_past_the_end_is_a_typed_error() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let bytes = [0x10, 0x11];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        context.setup_bytecode(&bytes);

        assert_eq!(context.resolve(3), Err(ScriptError::InvalidJumpTarget(3)));
        assert_eq!(
            context.resolve(u32::MAX),
            Err(ScriptError::InvalidJumpTarget(u32::MAX))
        );
    }

    #[test]
    fn resolve_with_no_script_loaded_is_a_typed_error() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(context.resolve(0), Err(ScriptError::InvalidJumpTarget(0)));
    }

    #[test]
    fn comparison_result_and_data_registers_round_trip() {
        let commands: &[Command<TestHost>] = &[command_noop];
        let mut context: ScriptContext<'_, '_, TestHost> = ScriptContext::new(commands);
        assert_eq!(context.comparison_result(), 0);
        context.set_comparison_result(7);
        assert_eq!(context.comparison_result(), 7);

        assert_eq!(context.data(), [0; DATA_REGISTER_COUNT]);
        context.data_mut()[2] = 42;
        assert_eq!(context.data(), [0, 0, 42, 0]);
    }
}
