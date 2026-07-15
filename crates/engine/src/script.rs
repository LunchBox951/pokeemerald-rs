//! Bytecode-dispatch shell of Emerald's script engine (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the generic
//! fetch/dispatch machinery in `pokeemerald/src/script.c` +
//! `include/script.h`: [`ScriptContext`]'s cursor/call-stack/scratch state,
//! the bytecode fetch/dispatch loop (`RunScriptCommand`'s
//! `SCRIPT_MODE_BYTECODE` case) and native single-shot dispatch
//! (`SCRIPT_MODE_NATIVE`), the call stack (`ScriptPush`/`ScriptPop`/
//! `ScriptJump`/`ScriptCall`/`ScriptReturn`), and the little-endian operand
//! readers (`ScriptReadHalfword`/`ScriptReadWord`).
//!
//! # Scope
//!
//! This module is deliberately just the *shell*: it owns no concrete script
//! commands, no global singleton context (`ScriptContext_Init` /
//! `ScriptContext_RunScript` / …), and no map-header script-table dispatch.
//! A command table is any slice of [`Command`] function pointers supplied by
//! the caller; an opcode is simply an index into that table. A later slice
//! populates a real table (`gScriptCmdTable`) and wires it in.
//!
//! Two upstream details are intentionally out of scope here because they
//! belong to that later, concrete-command layer, not the generic shell:
//! `gNullScriptPtr`, a sentinel address that upstream spins on forever
//! (`asm("svc 2")`) to halt the CPU, has no meaning once scripts are borrowed
//! byte slices rather than raw ROM addresses; and `RunScriptCommand`'s
//! `ctx->comparisonResult` field is carried here as inert scratch state (see
//! [`ScriptContext::comparison_result`]) since only comparison commands (out
//! of scope) give it meaning.
//!
//! # Address space
//!
//! Upstream models a script as a raw pointer into a ROM/RAM region shared by
//! every script and every jump/call target. `(oop-boundaries)` rules out
//! that kind of implicit global address space here. Instead a "script
//! pointer" is a borrowed byte slice `&'a [u8]`: jump/call/return simply swap
//! which slice the cursor reads from next, all sharing one caller-chosen
//! lifetime `'a` (e.g. a `'static` slice of embedded script bytecode, or an
//! arena's lifetime). A call can still jump into a completely different
//! script, exactly as upstream allows, as long as both slices share `'a`.

use std::fmt;

/// Fixed call-stack depth, matching upstream's `stack[20]`.
const STACK_DEPTH: usize = 20;

/// Number of `u32` scratch registers, matching upstream `data[4]`.
const DATA_REGISTERS: usize = 4;

/// A single bytecode command.
///
/// Mirrors upstream `ScrCmdFunc` (`bool8 (*)(struct ScriptContext *)`): a
/// command reads whatever operands it needs from the context's cursor (via
/// [`ScriptContext::read_u16`]/[`ScriptContext::read_u32`]), may mutate
/// context state, and reports whether the script should yield. Returning
/// `true` means "yield to the caller now" (upstream `TRUE`, e.g. a command
/// that waits on a message box); `false` means "keep running the bytecode
/// loop" (upstream `FALSE`).
pub type Command<'a> = fn(&mut ScriptContext<'a>) -> bool;

/// A native, non-bytecode step run once per [`ScriptContext::run`] call while
/// in native mode.
///
/// Mirrors upstream `nativePtr` (`u8 (*)(void)`), which — like the real
/// function — takes no context argument. Returning `true` means "the native
/// step is done, resume bytecode next call" (upstream flips the mode to
/// `SCRIPT_MODE_BYTECODE`); `false` means "stay in native mode, call me
/// again next time".
pub type NativeStep = fn() -> bool;

/// Errors surfaced by [`ScriptContext`]'s stack, dispatch, and operand-read
/// primitives.
///
/// Upstream leaves the equivalent conditions either unchecked (raw-pointer
/// operand reads) or silently swallowed (`ScriptPush`'s overflow return value
/// is discarded by `ScriptCall`; `ScriptPop`'s `NULL` on underflow is written
/// straight into `scriptPtr` and only surfaces as a stall on the *next*
/// dispatch). This port never panics and never discards the condition —
/// every one of them comes back as a typed error instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptError {
    /// The call stack is full; a push did not happen.
    ///
    /// Upstream `ScriptPush` returns `TRUE` (its overflow sentinel) once
    /// `stackDepth + 1 >= 20`, i.e. once `stackDepth` reaches 19 — so only 19
    /// of the 20 `stack` slots are ever actually used. That off-by-one is
    /// preserved here bug-for-bug `(behavioral-fidelity)`: existing scripts
    /// were authored (and tested) against call depths that never exceed it.
    StackOverflow,
    /// The call stack is empty; a pop had nothing to return.
    StackUnderflow,
    /// The fetched opcode byte has no entry in the command table — upstream's
    /// `func >= cmdTableEnd` bounds check. Carries the offending opcode.
    OpcodeOutOfRange(u8),
    /// An operand reader ([`ScriptContext::read_u16`]/
    /// [`ScriptContext::read_u32`]) needed more bytes than the cursor had
    /// left (including an unset cursor). Upstream has no equivalent check —
    /// `ScriptReadHalfword`/`ScriptReadWord` dereference a raw pointer with
    /// nothing to stop it reading past the end — but running off the end of
    /// a Rust slice isn't representable, so this is the defined,
    /// non-panicking substitute.
    UnexpectedEnd,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StackOverflow => {
                write!(
                    f,
                    "script call stack is full (max depth {})",
                    STACK_DEPTH - 1
                )
            }
            Self::StackUnderflow => write!(f, "script call stack is empty"),
            Self::OpcodeOutOfRange(op) => {
                write!(f, "opcode {op:#04x} has no command table entry")
            }
            Self::UnexpectedEnd => write!(f, "script cursor ran out of bytes"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Dispatch mode, matching upstream's `SCRIPT_MODE_*` enum — except a native
/// step's function pointer lives directly on the [`Native`](Mode::Native)
/// variant instead of a separate, sometimes-stale `nativePtr` field.
///
/// No `PartialEq`/`Eq`: comparing function pointers for equality is not
/// meaningful in Rust (their addresses aren't guaranteed unique), and
/// nothing here needs to compare modes — only match on them.
#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Upstream `SCRIPT_MODE_STOPPED`.
    Stopped,
    /// Upstream `SCRIPT_MODE_BYTECODE`.
    Bytecode,
    /// Upstream `SCRIPT_MODE_NATIVE`.
    Native(NativeStep),
}

/// The generic engine driving one script's execution.
///
/// Owns everything upstream's `struct ScriptContext` holds except the
/// command-table-*independent* globals (`sLockFieldControls` and friends,
/// out of scope here): the read cursor, a 20-deep call stack, the `data[4]`
/// scratch registers commands use for cross-command temporaries, and the
/// comparison-result byte. No part of this state is global — `(oop-boundaries)`
/// — so a caller can run as many independent contexts as it likes, e.g. one
/// per running NPC script.
#[derive(Debug)]
pub struct ScriptContext<'a> {
    cmd_table: &'a [Command<'a>],
    mode: Mode,
    comparison_result: u8,
    cursor: Option<&'a [u8]>,
    stack: [&'a [u8]; STACK_DEPTH],
    stack_depth: usize,
    data: [u32; DATA_REGISTERS],
}

impl<'a> ScriptContext<'a> {
    /// Create a stopped context bound to `cmd_table`.
    ///
    /// Mirrors `InitScriptContext`: the stack, `data[4]`, and comparison
    /// result all start zeroed, and no script is loaded.
    #[must_use]
    pub const fn new(cmd_table: &'a [Command<'a>]) -> Self {
        Self {
            cmd_table,
            mode: Mode::Stopped,
            comparison_result: 0,
            cursor: None,
            stack: [&[]; STACK_DEPTH],
            stack_depth: 0,
            data: [0; DATA_REGISTERS],
        }
    }

    /// Load a bytecode script and switch to bytecode mode. Mirrors
    /// `SetupBytecodeScript`.
    pub fn setup_bytecode(&mut self, script: &'a [u8]) {
        self.cursor = Some(script);
        self.mode = Mode::Bytecode;
    }

    /// Switch to native mode, to be driven by `step` on the next
    /// [`run`](Self::run) call. Mirrors `SetupNativeScript`.
    pub fn setup_native(&mut self, step: NativeStep) {
        self.mode = Mode::Native(step);
    }

    /// Halt the script. Mirrors `StopScript`.
    pub fn stop(&mut self) {
        self.mode = Mode::Stopped;
        self.cursor = None;
    }

    /// Whether the context is halted (never started, ran off the end, or hit
    /// an opcode with no command-table entry).
    #[must_use]
    pub const fn is_stopped(&self) -> bool {
        matches!(self.mode, Mode::Stopped)
    }

    /// The current call-stack depth (0 at the outermost frame).
    #[must_use]
    pub const fn stack_depth(&self) -> usize {
        self.stack_depth
    }

    /// The remaining bytes of the script the cursor will read from next, or
    /// `None` if no script is loaded.
    #[must_use]
    pub const fn cursor(&self) -> Option<&'a [u8]> {
        self.cursor
    }

    /// The `data[4]` scratch registers upstream commands use for
    /// cross-command temporaries.
    #[must_use]
    pub const fn data(&self) -> [u32; DATA_REGISTERS] {
        self.data
    }

    /// Mutable access to the `data[4]` scratch registers.
    pub fn data_mut(&mut self) -> &mut [u32; DATA_REGISTERS] {
        &mut self.data
    }

    /// `ctx->comparisonResult`, an inert scratch byte here — only comparison
    /// commands (out of scope for this module) give it meaning.
    #[must_use]
    pub const fn comparison_result(&self) -> u8 {
        self.comparison_result
    }

    /// Set `ctx->comparisonResult`.
    pub fn set_comparison_result(&mut self, value: u8) {
        self.comparison_result = value;
    }

    /// Look up the command for a fetched opcode byte.
    ///
    /// Mirrors upstream's `cmdTable[cmdCode]` load plus the
    /// `func >= cmdTableEnd` bounds check `(behavioral-fidelity)`, expressed
    /// as a slice index instead of pointer arithmetic.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::OpcodeOutOfRange`] if `opcode` has no entry in
    /// the command table this context was constructed with.
    pub fn lookup_command(&self, opcode: u8) -> Result<Command<'a>, ScriptError> {
        self.cmd_table
            .get(usize::from(opcode))
            .copied()
            .ok_or(ScriptError::OpcodeOutOfRange(opcode))
    }

    /// Push a return address onto the call stack.
    ///
    /// Mirrors `ScriptPush`, including its off-by-one bound — see
    /// [`ScriptError::StackOverflow`].
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackOverflow`] if the stack is already at its
    /// upstream-matching capacity.
    pub fn push(&mut self, ptr: &'a [u8]) -> Result<(), ScriptError> {
        if self.stack_depth + 1 >= STACK_DEPTH {
            return Err(ScriptError::StackOverflow);
        }
        self.stack[self.stack_depth] = ptr;
        self.stack_depth += 1;
        Ok(())
    }

    /// Pop the most recently pushed return address.
    ///
    /// Mirrors `ScriptPop`, except an empty stack is a typed error rather
    /// than upstream's silent `NULL`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackUnderflow`] if the stack is empty.
    pub fn pop(&mut self) -> Result<&'a [u8], ScriptError> {
        if self.stack_depth == 0 {
            return Err(ScriptError::StackUnderflow);
        }
        self.stack_depth -= 1;
        Ok(self.stack[self.stack_depth])
    }

    /// Jump the cursor to `ptr` without touching the call stack. Mirrors
    /// `ScriptJump`.
    pub fn jump(&mut self, ptr: &'a [u8]) {
        self.cursor = Some(ptr);
    }

    /// Push the current cursor as a return address, then jump to `ptr`.
    /// Mirrors `ScriptCall`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackOverflow`] if the call stack is full; the
    /// jump does not happen in that case (upstream jumps anyway and silently
    /// loses the return address — this port refuses instead).
    pub fn call(&mut self, ptr: &'a [u8]) -> Result<(), ScriptError> {
        self.push(self.cursor.unwrap_or(&[]))?;
        self.cursor = Some(ptr);
        Ok(())
    }

    /// Pop a return address and jump the cursor to it. Mirrors
    /// `ScriptReturn`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::StackUnderflow`] if the call stack is empty.
    pub fn script_return(&mut self) -> Result<(), ScriptError> {
        let ptr = self.pop()?;
        self.cursor = Some(ptr);
        Ok(())
    }

    /// Read a little-endian `u16` operand and advance the cursor past it.
    /// Mirrors `ScriptReadHalfword`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] if fewer than 2 bytes remain.
    pub fn read_u16(&mut self) -> Result<u16, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let bytes = cursor.get(..2).ok_or(ScriptError::UnexpectedEnd)?;
        let value = u16::from_le_bytes([bytes[0], bytes[1]]);
        self.cursor = Some(&cursor[2..]);
        Ok(value)
    }

    /// Read a little-endian `u32` operand and advance the cursor past it.
    /// Mirrors `ScriptReadWord`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] if fewer than 4 bytes remain.
    pub fn read_u32(&mut self) -> Result<u32, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let bytes = cursor.get(..4).ok_or(ScriptError::UnexpectedEnd)?;
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        self.cursor = Some(&cursor[4..]);
        Ok(value)
    }

    /// Run one dispatch step: either a native step, or the bytecode
    /// fetch/dispatch loop until a command yields or the script stops.
    ///
    /// Mirrors `RunScriptCommand`. Returns `true` if the script is still
    /// active (a command yielded, or a native step is still running) and
    /// should be called again later; `false` once the script has stopped —
    /// out of bytes, an opcode with no command-table entry, or a context
    /// that was never started.
    pub fn run(&mut self) -> bool {
        match self.mode {
            Mode::Stopped => false,
            Mode::Native(step) => {
                if step() {
                    self.mode = Mode::Bytecode;
                }
                // Upstream always yields on the call that drives the native
                // step, whether or not this was the step that finished it.
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
                let Ok(cmd) = self.lookup_command(opcode) else {
                    self.mode = Mode::Stopped;
                    break false;
                };
                if cmd(self) {
                    break true;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny script builder so tests can spell out bytecode as
    /// opcode/operand bytes.
    fn script(opcodes: &[u8]) -> Vec<u8> {
        opcodes.to_vec()
    }

    fn cmd_nop(_ctx: &mut ScriptContext<'_>) -> bool {
        false
    }

    fn cmd_yield(_ctx: &mut ScriptContext<'_>) -> bool {
        true
    }

    fn cmd_incr(ctx: &mut ScriptContext<'_>) -> bool {
        ctx.data_mut()[0] += 1;
        false
    }

    #[test]
    fn sequential_dispatch_runs_commands_in_order_until_yield() {
        let table: &[Command<'_>] = &[cmd_incr as Command<'_>, cmd_yield as Command<'_>];
        let bytes = script(&[0, 0, 0, 1]); // incr, incr, incr, yield
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(), "should yield, not stop");
        assert_eq!(ctx.data()[0], 3, "all three incr commands ran");
        assert!(!ctx.is_stopped());
    }

    #[test]
    fn opcode_past_table_end_stops_the_script_without_panicking() {
        let table: &[Command<'_>] = &[cmd_nop];
        let bytes = script(&[5]); // opcode 5, table only has index 0
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(), "out-of-range opcode stops the script");
        assert!(ctx.is_stopped());
    }

    #[test]
    fn lookup_command_reports_the_offending_opcode() {
        let table: &[Command<'_>] = &[cmd_nop as Command<'_>, cmd_yield as Command<'_>];
        let ctx = ScriptContext::new(table);
        assert_eq!(ctx.lookup_command(0).map(|_| ()), Ok(()));
        assert_eq!(ctx.lookup_command(9), Err(ScriptError::OpcodeOutOfRange(9)));
    }

    #[test]
    fn running_off_the_end_of_the_script_stops_cleanly() {
        let table: &[Command<'_>] = &[cmd_nop];
        let bytes = script(&[]); // no opcode bytes at all
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run());
        assert!(ctx.is_stopped());
    }

    #[test]
    fn a_context_that_was_never_started_reports_stopped() {
        let table: &[Command<'_>] = &[cmd_nop];
        let mut ctx = ScriptContext::new(table);
        assert!(ctx.is_stopped());
        assert!(!ctx.run());
    }

    #[test]
    fn jump_redirects_the_cursor() {
        fn cmd_jump_to_incr(ctx: &mut ScriptContext<'_>) -> bool {
            // The target lives inside the same backing buffer as the calling
            // script in real use; a leaked 'static slice stands in for that
            // here since the test only needs *a* valid `&'a [u8]` target.
            let target: &'static [u8] = &[0, 1]; // incr, yield
            ctx.jump(target);
            false
        }
        let table: &[Command<'_>] = &[
            cmd_incr as Command<'_>,
            cmd_yield as Command<'_>,
            cmd_jump_to_incr as Command<'_>,
        ];
        let bytes = script(&[2]); // jump away immediately
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run());
        assert_eq!(ctx.data()[0], 1, "the jumped-to incr ran");
    }

    #[test]
    fn call_and_return_resume_after_the_call_site() {
        static SUB: [u8; 2] = [0, 3]; // incr, then `return`
        fn cmd_call_sub(ctx: &mut ScriptContext<'_>) -> bool {
            ctx.call(&SUB).expect("stack has room");
            false
        }
        fn cmd_ret(ctx: &mut ScriptContext<'_>) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let table: &[Command<'_>] = &[
            cmd_incr as Command<'_>,
            cmd_yield as Command<'_>,
            cmd_call_sub as Command<'_>,
            cmd_ret as Command<'_>,
        ];
        // call sub (which incr+returns), then incr once more at the call
        // site, then yield.
        let bytes = script(&[2, 0, 1]);
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run());
        assert_eq!(ctx.data()[0], 2, "one incr inside the call, one after");
        assert_eq!(ctx.stack_depth(), 0, "the return unwound the frame");
    }

    #[test]
    fn nested_calls_unwind_in_reverse_order() {
        static INNER: [u8; 2] = [0, 3]; // incr, return
        static OUTER: [u8; 3] = [2, 0, 3]; // call INNER, incr, return
        fn cmd_call_inner(ctx: &mut ScriptContext<'_>) -> bool {
            ctx.call(&INNER).expect("stack has room");
            false
        }
        fn cmd_call_outer(ctx: &mut ScriptContext<'_>) -> bool {
            ctx.call(&OUTER).expect("stack has room");
            false
        }
        fn cmd_ret(ctx: &mut ScriptContext<'_>) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let table: &[Command<'_>] = &[
            cmd_incr as Command<'_>,
            cmd_yield as Command<'_>,
            cmd_call_inner as Command<'_>,
            cmd_ret as Command<'_>,
            cmd_call_outer as Command<'_>,
        ];
        let bytes = script(&[4, 1]); // call OUTER (two nested frames), yield
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run());
        assert_eq!(ctx.data()[0], 2, "inner and outer each incr once");
        assert_eq!(ctx.stack_depth(), 0, "both frames unwound");
    }

    #[test]
    fn stack_overflow_is_a_typed_error_not_a_panic() {
        let table: &[Command<'_>] = &[cmd_nop];
        let mut ctx = ScriptContext::new(table);
        let filler: &'static [u8] = &[];

        let mut pushed = 0;
        loop {
            match ctx.push(filler) {
                Ok(()) => pushed += 1,
                Err(ScriptError::StackOverflow) => break,
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        // Upstream's off-by-one: only 19 of the 20 stack slots are ever used.
        assert_eq!(pushed, 19);
        assert_eq!(ctx.stack_depth(), 19);
        assert_eq!(ctx.push(filler), Err(ScriptError::StackOverflow));
    }

    #[test]
    fn stack_underflow_is_a_typed_error_not_a_panic() {
        let table: &[Command<'_>] = &[cmd_nop];
        let mut ctx = ScriptContext::new(table);
        assert_eq!(ctx.pop(), Err(ScriptError::StackUnderflow));
        assert_eq!(ctx.script_return(), Err(ScriptError::StackUnderflow));
    }

    #[test]
    fn call_does_not_jump_when_the_stack_is_full() {
        let table: &[Command<'_>] = &[cmd_nop];
        let mut ctx = ScriptContext::new(table);
        let bytes: &'static [u8] = &[1, 2, 3];
        ctx.setup_bytecode(bytes);
        for _ in 0..19 {
            ctx.push(&[]).unwrap();
        }

        let target: &'static [u8] = &[9];
        assert_eq!(ctx.call(target), Err(ScriptError::StackOverflow));
        assert_eq!(
            ctx.cursor(),
            Some(bytes),
            "the cursor is untouched when the call is refused"
        );
    }

    #[test]
    fn read_u16_is_little_endian_and_advances_the_cursor() {
        let table: &[Command<'_>] = &[cmd_nop];
        let bytes = [0x34, 0x12, 0xAA];
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.read_u16(), Ok(0x1234));
        assert_eq!(ctx.cursor(), Some(&bytes[2..]));
    }

    #[test]
    fn read_u32_is_little_endian_and_advances_the_cursor() {
        let table: &[Command<'_>] = &[cmd_nop];
        let bytes = [0x78, 0x56, 0x34, 0x12, 0xFF];
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.read_u32(), Ok(0x1234_5678));
        assert_eq!(ctx.cursor(), Some(&bytes[4..]));
    }

    #[test]
    fn reads_past_the_end_of_the_cursor_are_a_typed_error() {
        let table: &[Command<'_>] = &[cmd_nop];
        let bytes = [0x01];
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);
        assert_eq!(ctx.read_u16(), Err(ScriptError::UnexpectedEnd));

        let mut fresh = ScriptContext::new(table);
        assert_eq!(fresh.read_u32(), Err(ScriptError::UnexpectedEnd));
    }

    #[test]
    fn native_mode_always_yields_and_can_hand_off_to_bytecode() {
        fn step_not_done() -> bool {
            false
        }
        fn step_done() -> bool {
            true
        }
        let table: &[Command<'_>] = &[cmd_yield];
        let bytes = script(&[0]);
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);
        ctx.setup_native(step_not_done);

        assert!(ctx.run(), "native mode always yields this call");
        assert!(!ctx.is_stopped());

        ctx.setup_native(step_done);
        assert!(ctx.run(), "still yields on the call that finishes the step");
        // The next call resumes bytecode where it was set up.
        assert!(ctx.run(), "bytecode command yields");
    }

    #[test]
    fn comparison_result_and_data_registers_round_trip() {
        let table: &[Command<'_>] = &[cmd_nop];
        let mut ctx = ScriptContext::new(table);
        assert_eq!(ctx.comparison_result(), 0);
        ctx.set_comparison_result(7);
        assert_eq!(ctx.comparison_result(), 7);

        assert_eq!(ctx.data(), [0; 4]);
        ctx.data_mut()[2] = 42;
        assert_eq!(ctx.data(), [0, 0, 42, 0]);
    }
}
