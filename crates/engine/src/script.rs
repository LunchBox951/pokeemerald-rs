//! Bytecode-dispatch shell of Emerald's script engine (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the generic
//! fetch/dispatch machinery in `pokeemerald/src/script.c` +
//! `include/script.h`: [`ScriptContext`]'s cursor/call-stack/scratch state,
//! the bytecode fetch/dispatch loop (`RunScriptCommand`'s
//! `SCRIPT_MODE_BYTECODE` case) and native single-shot dispatch
//! (`SCRIPT_MODE_NATIVE`), the call stack (`ScriptPush`/`ScriptPop`/
//! `ScriptJump`/`ScriptCall`/`ScriptReturn`), and the little-endian operand
//! readers (`ScriptReadByte`/`ScriptReadHalfword`/`ScriptReadWord`).
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
//! pointer" is a borrowed byte slice `&'script [u8]`: jump/call/return simply
//! swap which slice the cursor reads from next, all sharing one caller-chosen
//! lifetime `'script` (e.g. a `'static` slice of embedded script bytecode, an
//! arena's lifetime, or a `Vec` owned on the stack). A call can still jump
//! into a completely different script, exactly as upstream allows, as long as
//! both slices share `'script`.
//!
//! [`ScriptContext::resolve`] gives the concrete-command layer a way to turn
//! a *byte offset* operand into one of these slices without ever
//! reintroducing a raw address; see its docs for how that maps onto
//! upstream's `goto`/`call`/`goto_if`/`call_if`.
//!
//! # Lifetimes and host state
//!
//! The command table and the script bytes have **independent** lifetimes
//! (`'cmd` and `'script`), so a `'static` command table can drive a
//! short-lived, locally-owned (RAM/save/arena) script — the two never need to
//! coincide. Keeping them fused (as an earlier draft did, with a single `'a`
//! on both plus a `Command` type mentioning `ScriptContext<'a>`) makes `'a`
//! *invariant* through the fn-pointer argument and forces `'cmd == 'script`;
//! [`Command`] is written with elided (higher-ranked) lifetimes precisely so
//! that knot never forms.
//!
//! Concrete commands (`setvar`, `additem`, `warp`, `playse`, …) need to reach
//! save-vars, the bag, the map, and audio. `(oop-boundaries)` forbids a
//! global mutable singleton for that, so the world is threaded explicitly: a
//! [`ScriptContext`] is generic over a host type `H` chosen by the caller,
//! and every command — and every native step — receives `&mut H` alongside
//! the context. The shell places no constraints on `H`; it is an opaque
//! handle to whatever state the embedder owns.

use std::fmt;

pub mod commands;
pub mod specials;
pub mod std_script;

/// Fixed call-stack depth, matching upstream's `stack[20]`.
const STACK_DEPTH: usize = 20;

/// Number of `u32` scratch registers, matching upstream `data[4]`.
const DATA_REGISTERS: usize = 4;

/// A single bytecode command, parameterised by the caller's host type `H`.
///
/// Mirrors upstream `ScrCmdFunc` (`bool8 (*)(struct ScriptContext *)`): a
/// command reads whatever operands it needs from the context's cursor (via
/// [`ScriptContext::read_u8`]/[`ScriptContext::read_u16`]/
/// [`ScriptContext::read_u32`]), may mutate context state, and reaches the
/// wider game world through the `&mut H` host handle (save-vars, bag, map,
/// audio — none of which live in the context, `(oop-boundaries)`). Returning
/// `true` means "yield to the caller now" (upstream `TRUE`, e.g. a command
/// that waits on a message box); `false` means "keep running the bytecode
/// loop" (upstream `FALSE`).
///
/// The lifetimes are deliberately **elided**: as a fn-pointer type this makes
/// `Command<H>` higher-ranked (`for<'cmd, 'script> fn(…)`), which is what
/// keeps the command table's lifetime independent of any one script's
/// lifetime. See the [module docs](self#lifetimes-and-host-state).
pub type Command<H> = fn(&mut ScriptContext<'_, '_, H>, &mut H) -> bool;

/// A native, non-bytecode step run once per [`ScriptContext::run`] call while
/// in native mode.
///
/// Mirrors upstream `nativePtr` (`u8 (*)(void)`). Upstream native steps take
/// no argument and poll global state — message-box progress, object-event
/// movement completion, frame-delay counters. `(oop-boundaries)` replaces
/// that global state with the caller-owned host, so the port hands the step
/// both its owning context and `&mut H`: exactly the inputs those ports will
/// poll. Returning `true` means "the native step is done, resume bytecode
/// next call" (upstream flips the mode to `SCRIPT_MODE_BYTECODE`); `false`
/// means "stay in native mode, call me again next time".
///
/// Lifetimes are elided (higher-ranked) for the same reason as [`Command`].
pub type NativeStep<H> = fn(&mut ScriptContext<'_, '_, H>, &mut H) -> bool;

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
    /// [`ScriptContext::resolve`] could not turn a jump-target byte offset
    /// into a `&'script [u8]` target — either no script is loaded, or the
    /// offset runs past the end of it. Carries the offending offset.
    ///
    /// Concrete `goto`/`call`/`goto_if`/`call_if` commands (a later,
    /// concrete-command layer, not this shell) read this offset from a
    /// 4-byte operand that upstream instead treats as a raw ROM address —
    /// see [`ScriptContext::resolve`] for why this port uses an offset
    /// instead. Upstream never bounds-checks that address at all (an
    /// out-of-range one reads whatever ROM/RAM happens to sit there); this
    /// port refuses it instead.
    InvalidJumpTarget(u32),
    /// An operand reader ([`ScriptContext::read_u8`]/
    /// [`ScriptContext::read_u16`]/[`ScriptContext::read_u32`]) needed more
    /// bytes than the cursor had left (including an unset cursor). Upstream
    /// has no equivalent check — `ScriptReadByte`/`ScriptReadHalfword`/
    /// `ScriptReadWord` dereference a raw pointer with nothing to stop it
    /// reading past the end — but running off the end of a Rust slice isn't
    /// representable, so this is the defined, non-panicking substitute.
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
            Self::InvalidJumpTarget(offset) => {
                write!(f, "jump-target offset {offset:#010x} is out of range")
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
/// nothing here needs to compare modes — only match on them. Not `Copy`
/// either: that would drag an unwanted `H: Copy` bound in, and the dispatch
/// loop only ever copies the (always-`Copy`) fn pointer out of `Native`.
#[derive(Debug)]
enum Mode<H> {
    /// Upstream `SCRIPT_MODE_STOPPED`.
    Stopped,
    /// Upstream `SCRIPT_MODE_BYTECODE`.
    Bytecode,
    /// Upstream `SCRIPT_MODE_NATIVE`.
    Native(NativeStep<H>),
}

/// The generic engine driving one script's execution.
///
/// Owns everything upstream's `struct ScriptContext` holds except the
/// command-table-*independent* globals (`sLockFieldControls` and friends,
/// out of scope here): the read cursor, a 20-deep call stack, the `data[4]`
/// scratch registers commands use for cross-command temporaries, and the
/// comparison-result byte. No part of this state is global — `(oop-boundaries)`
/// — so a caller can run as many independent contexts as it likes, e.g. one
/// per running NPC script, each fed the same host on [`run`](Self::run).
///
/// Three type parameters: `'cmd` is the command table's lifetime, `'script`
/// is the script bytes' lifetime (independent, so a `'static` table drives a
/// short-lived script), and `H` is the caller's host/world type threaded into
/// every command. See the [module docs](self#lifetimes-and-host-state).
#[derive(Debug)]
pub struct ScriptContext<'cmd, 'script, H> {
    cmd_table: &'cmd [Command<H>],
    mode: Mode<H>,
    comparison_result: u8,
    cursor: Option<&'script [u8]>,
    base: Option<&'script [u8]>,
    stack: [&'script [u8]; STACK_DEPTH],
    stack_depth: usize,
    data: [u32; DATA_REGISTERS],
}

impl<'cmd, 'script, H> ScriptContext<'cmd, 'script, H> {
    /// Create a stopped context bound to `cmd_table`.
    ///
    /// Mirrors `InitScriptContext`: the stack, `data[4]`, and comparison
    /// result all start zeroed, and no script is loaded.
    #[must_use]
    pub const fn new(cmd_table: &'cmd [Command<H>]) -> Self {
        Self {
            cmd_table,
            mode: Mode::Stopped,
            comparison_result: 0,
            cursor: None,
            base: None,
            stack: [&[]; STACK_DEPTH],
            stack_depth: 0,
            data: [0; DATA_REGISTERS],
        }
    }

    /// Load a bytecode script and switch to bytecode mode. Mirrors
    /// `SetupBytecodeScript`.
    ///
    /// `script` also becomes the base [`resolve`](Self::resolve) offsets are
    /// taken against, i.e. offset `0` means "the first byte of `script`".
    pub fn setup_bytecode(&mut self, script: &'script [u8]) {
        self.cursor = Some(script);
        self.base = Some(script);
        self.mode = Mode::Bytecode;
    }

    /// Switch to native mode, to be driven by `step` on the next
    /// [`run`](Self::run) call. Mirrors `SetupNativeScript`.
    pub fn setup_native(&mut self, step: NativeStep<H>) {
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
    pub const fn cursor(&self) -> Option<&'script [u8]> {
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
    pub fn lookup_command(&self, opcode: u8) -> Result<Command<H>, ScriptError> {
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
    pub fn push(&mut self, ptr: &'script [u8]) -> Result<(), ScriptError> {
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
    pub fn pop(&mut self) -> Result<&'script [u8], ScriptError> {
        if self.stack_depth == 0 {
            return Err(ScriptError::StackUnderflow);
        }
        self.stack_depth -= 1;
        Ok(self.stack[self.stack_depth])
    }

    /// Jump the cursor to `ptr` without touching the call stack. Mirrors
    /// `ScriptJump`.
    pub fn jump(&mut self, ptr: &'script [u8]) {
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
    pub fn call(&mut self, ptr: &'script [u8]) -> Result<(), ScriptError> {
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

    /// Resolve a byte offset into a `&'script [u8]` jump target — the
    /// concrete-command layer's equivalent of upstream's 4-byte "address"
    /// operand (`goto`/`call`/`goto_if`/`call_if` each read one via
    /// `ScriptReadWord`, then cast it straight to a `const u8 *`).
    ///
    /// Upstream scripts, their subroutines, and every jump/call target share
    /// one flat ROM address space; `(oop-boundaries)` rules that out here
    /// (see the module's [Address space](self#address-space) section), so
    /// concrete commands can't reproduce "cast the operand to a pointer"
    /// verbatim. This port's substitute keeps the same 4-byte operand width
    /// and upstream opcode numbers but gives it a meaning a hand-assembled
    /// (or, later, compiled) script can express purely in its own bytes: an
    /// offset from the start of the buffer most recently passed to
    /// [`setup_bytecode`](Self::setup_bytecode), i.e. the top-level script
    /// currently running. That covers intra-script jumps and calls to
    /// subroutines compiled into the same buffer — everything this slice's
    /// `goto`/`call`/`goto_if`/`call_if` commands are in scope for; jumping
    /// into a *different* compiled script (`gotostd` and friends) needs the
    /// std-script table machinery a later slice adds.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::InvalidJumpTarget`] if no script is loaded, or
    /// if `offset` is past the end of it.
    pub fn resolve(&self, offset: u32) -> Result<&'script [u8], ScriptError> {
        self.base
            .and_then(|base| usize::try_from(offset).ok().and_then(|off| base.get(off..)))
            .ok_or(ScriptError::InvalidJumpTarget(offset))
    }

    /// Read a single byte operand and advance the cursor past it. Mirrors
    /// `ScriptReadByte` (`*(ctx->scriptPtr++)`), which upstream uses ~173
    /// times in `src/scrcmd.c` for byte-sized operands — `goto_if`/`call_if`
    /// condition bytes, data-register indexes, warp map IDs, and so on.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::UnexpectedEnd`] if no bytes remain (including an
    /// unset cursor).
    pub fn read_u8(&mut self) -> Result<u8, ScriptError> {
        let cursor = self.cursor.ok_or(ScriptError::UnexpectedEnd)?;
        let (&value, rest) = cursor.split_first().ok_or(ScriptError::UnexpectedEnd)?;
        self.cursor = Some(rest);
        Ok(value)
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
    /// fetch/dispatch loop until a command yields or the script stops. Both
    /// commands and native steps receive `host` (`&mut H`) so they can reach
    /// the wider game world without any global mutable state
    /// `(oop-boundaries)`.
    ///
    /// Mirrors `RunScriptCommand`. Returns `true` if the script is still
    /// active (a command yielded, or a native step is still running) and
    /// should be called again later; `false` once the script has stopped —
    /// out of bytes, an opcode with no command-table entry, or a context
    /// that was never started.
    pub fn run(&mut self, host: &mut H) -> bool {
        match self.mode {
            Mode::Stopped => false,
            // `step` is a fn pointer (always `Copy`), so binding it here only
            // copies it out — it does not move `self.mode`, leaving `self`
            // free to be passed on into the step.
            Mode::Native(step) => {
                if step(self, host) {
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
                if cmd(self, host) {
                    break true;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests' stand-in host/world. Concrete commands would reach save-vars,
    /// the bag, the map, and audio through a type like this; here it just
    /// records a couple of observable effects so tests can prove the host was
    /// threaded through. `(oop-boundaries)` — no global mutable state.
    #[derive(Debug, Default)]
    struct TestHost {
        /// Bumped by [`cmd_host_tick`] to prove commands can mutate the host.
        ticks: u32,
    }

    /// A tiny script builder so tests can spell out bytecode as
    /// opcode/operand bytes.
    fn script(opcodes: &[u8]) -> Vec<u8> {
        opcodes.to_vec()
    }

    fn cmd_nop(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        false
    }

    fn cmd_yield(_ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        true
    }

    fn cmd_incr(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
        ctx.data_mut()[0] += 1;
        false
    }

    fn cmd_host_tick(_ctx: &mut ScriptContext<'_, '_, TestHost>, host: &mut TestHost) -> bool {
        host.ticks += 1;
        false
    }

    #[test]
    fn sequential_dispatch_runs_commands_in_order_until_yield() {
        let table: &[Command<TestHost>] = &[cmd_incr, cmd_yield];
        let bytes = script(&[0, 0, 0, 1]); // incr, incr, incr, yield
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host), "should yield, not stop");
        assert_eq!(ctx.data()[0], 3, "all three incr commands ran");
        assert!(!ctx.is_stopped());
    }

    #[test]
    fn commands_can_mutate_the_host_world() {
        let table: &[Command<TestHost>] = &[cmd_host_tick, cmd_yield];
        let bytes = script(&[0, 0, 1]); // tick, tick, yield
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host));
        assert_eq!(host.ticks, 2, "both ticks reached the host");
    }

    /// The design blocker this PR fixes: a `'static` command table must be
    /// able to drive a script whose bytes are owned locally (a `Vec`), i.e.
    /// `'cmd` (here `'static`) and `'script` (a short stack borrow) must be
    /// free to differ. When the two were fused on one invariant lifetime this
    /// did not compile.
    #[test]
    fn static_command_table_drives_a_locally_owned_script() {
        static TABLE: &[Command<TestHost>] = &[cmd_incr, cmd_yield];
        // Owned here, so its borrow is strictly shorter than `'static`.
        let bytes: Vec<u8> = vec![0, 0, 1]; // incr, incr, yield
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(TABLE);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host));
        assert_eq!(ctx.data()[0], 2);
    }

    #[test]
    fn opcode_past_table_end_stops_the_script_without_panicking() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = script(&[5]); // opcode 5, table only has index 0
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host), "out-of-range opcode stops the script");
        assert!(ctx.is_stopped());
    }

    #[test]
    fn lookup_command_reports_the_offending_opcode() {
        let table: &[Command<TestHost>] = &[cmd_nop, cmd_yield];
        let ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        assert_eq!(ctx.lookup_command(0).map(|_| ()), Ok(()));
        assert_eq!(ctx.lookup_command(9), Err(ScriptError::OpcodeOutOfRange(9)));
    }

    #[test]
    fn running_off_the_end_of_the_script_stops_cleanly() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = script(&[]); // no opcode bytes at all
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(!ctx.run(&mut host));
        assert!(ctx.is_stopped());
    }

    #[test]
    fn a_context_that_was_never_started_reports_stopped() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        assert!(ctx.is_stopped());
        assert!(!ctx.run(&mut host));
    }

    #[test]
    fn jump_redirects_the_cursor() {
        fn cmd_jump_to_incr(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            _host: &mut TestHost,
        ) -> bool {
            // The target lives inside the same backing buffer as the calling
            // script in real use; a leaked 'static slice stands in for that
            // here since the test only needs *a* valid `&'script [u8]` target.
            let target: &'static [u8] = &[0, 1]; // incr, yield
            ctx.jump(target);
            false
        }
        let table: &[Command<TestHost>] = &[cmd_incr, cmd_yield, cmd_jump_to_incr];
        let bytes = script(&[2]); // jump away immediately
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host));
        assert_eq!(ctx.data()[0], 1, "the jumped-to incr ran");
    }

    #[test]
    fn call_and_return_resume_after_the_call_site() {
        static SUB: [u8; 2] = [0, 3]; // incr, then `return`
        fn cmd_call_sub(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.call(&SUB).expect("stack has room");
            false
        }
        fn cmd_ret(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let table: &[Command<TestHost>] = &[cmd_incr, cmd_yield, cmd_call_sub, cmd_ret];
        // call sub (which incr+returns), then incr once more at the call
        // site, then yield.
        let bytes = script(&[2, 0, 1]);
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host));
        assert_eq!(ctx.data()[0], 2, "one incr inside the call, one after");
        assert_eq!(ctx.stack_depth(), 0, "the return unwound the frame");
    }

    #[test]
    fn nested_calls_unwind_in_reverse_order() {
        static INNER: [u8; 2] = [0, 3]; // incr, return
        static OUTER: [u8; 3] = [2, 0, 3]; // call INNER, incr, return
        fn cmd_call_inner(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.call(&INNER).expect("stack has room");
            false
        }
        fn cmd_call_outer(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.call(&OUTER).expect("stack has room");
            false
        }
        fn cmd_ret(ctx: &mut ScriptContext<'_, '_, TestHost>, _host: &mut TestHost) -> bool {
            ctx.script_return().expect("stack is non-empty");
            false
        }
        let table: &[Command<TestHost>] =
            &[cmd_incr, cmd_yield, cmd_call_inner, cmd_ret, cmd_call_outer];
        let bytes = script(&[4, 1]); // call OUTER (two nested frames), yield
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert!(ctx.run(&mut host));
        assert_eq!(ctx.data()[0], 2, "inner and outer each incr once");
        assert_eq!(ctx.stack_depth(), 0, "both frames unwound");
    }

    #[test]
    fn stack_overflow_is_a_typed_error_not_a_panic() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
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
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        assert_eq!(ctx.pop(), Err(ScriptError::StackUnderflow));
        assert_eq!(ctx.script_return(), Err(ScriptError::StackUnderflow));
    }

    #[test]
    fn call_does_not_jump_when_the_stack_is_full() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
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
    fn read_u8_advances_the_cursor() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x2A, 0xFF];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.read_u8(), Ok(0x2A));
        assert_eq!(ctx.cursor(), Some(&bytes[1..]));
        assert_eq!(ctx.read_u8(), Ok(0xFF));
        assert_eq!(ctx.cursor(), Some(&bytes[2..]));
    }

    #[test]
    fn read_u8_past_the_end_is_a_typed_error() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x01];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);
        assert_eq!(ctx.read_u8(), Ok(0x01));
        // Cursor now empty: the next read has nothing left.
        assert_eq!(ctx.read_u8(), Err(ScriptError::UnexpectedEnd));

        // An unset cursor (never started) is likewise the defined error.
        let mut fresh: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        assert_eq!(fresh.read_u8(), Err(ScriptError::UnexpectedEnd));
    }

    #[test]
    fn read_u16_is_little_endian_and_advances_the_cursor() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x34, 0x12, 0xAA];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.read_u16(), Ok(0x1234));
        assert_eq!(ctx.cursor(), Some(&bytes[2..]));
    }

    #[test]
    fn read_u32_is_little_endian_and_advances_the_cursor() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x78, 0x56, 0x34, 0x12, 0xFF];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.read_u32(), Ok(0x1234_5678));
        assert_eq!(ctx.cursor(), Some(&bytes[4..]));
    }

    #[test]
    fn reads_past_the_end_of_the_cursor_are_a_typed_error() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x01];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);
        assert_eq!(ctx.read_u16(), Err(ScriptError::UnexpectedEnd));

        let mut fresh: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
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
        let table: &[Command<TestHost>] = &[cmd_yield];
        let bytes = script(&[0]);
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);
        ctx.setup_native(step_not_done);

        assert!(ctx.run(&mut host), "native mode always yields this call");
        assert!(!ctx.is_stopped());

        ctx.setup_native(step_done);
        assert!(
            ctx.run(&mut host),
            "still yields on the call that finishes the step"
        );
        // The next call resumes bytecode where it was set up.
        assert!(ctx.run(&mut host), "bytecode command yields");
    }

    #[test]
    fn native_step_can_inspect_context_and_host() {
        // A native wait that polls the host (like a message-box or movement
        // poll would) and reads context scratch state before handing back.
        fn step_wait_two_ticks(
            ctx: &mut ScriptContext<'_, '_, TestHost>,
            host: &mut TestHost,
        ) -> bool {
            host.ticks += 1;
            ctx.data_mut()[0] = host.ticks;
            host.ticks >= 2 // done once the host has ticked twice
        }
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut host = TestHost::default();
        let mut ctx = ScriptContext::new(table);
        ctx.setup_native(step_wait_two_ticks);

        assert!(ctx.run(&mut host), "still waiting after one tick");
        assert_eq!(host.ticks, 1);
        assert_eq!(ctx.data()[0], 1, "native step wrote context scratch state");

        assert!(ctx.run(&mut host), "yields on the finishing call too");
        assert_eq!(host.ticks, 2, "native step reached the host again");
    }

    #[test]
    fn resolve_returns_the_suffix_at_the_given_offset() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x10, 0x11, 0x12, 0x13];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.resolve(0), Ok(&bytes[..]));
        assert_eq!(ctx.resolve(2), Ok(&bytes[2..]));
        assert_eq!(
            ctx.resolve(4),
            Ok(&bytes[4..]),
            "offset == len is the empty tail"
        );
    }

    #[test]
    fn resolve_past_the_end_is_a_typed_error() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let bytes = [0x10, 0x11];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        ctx.setup_bytecode(&bytes);

        assert_eq!(ctx.resolve(3), Err(ScriptError::InvalidJumpTarget(3)));
        assert_eq!(
            ctx.resolve(u32::MAX),
            Err(ScriptError::InvalidJumpTarget(u32::MAX))
        );
    }

    #[test]
    fn resolve_with_no_script_loaded_is_a_typed_error() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        assert_eq!(ctx.resolve(0), Err(ScriptError::InvalidJumpTarget(0)));
    }

    #[test]
    fn comparison_result_and_data_registers_round_trip() {
        let table: &[Command<TestHost>] = &[cmd_nop];
        let mut ctx: ScriptContext<'_, '_, TestHost> = ScriptContext::new(table);
        assert_eq!(ctx.comparison_result(), 0);
        ctx.set_comparison_result(7);
        assert_eq!(ctx.comparison_result(), 7);

        assert_eq!(ctx.data(), [0; 4]);
        ctx.data_mut()[2] = 42;
        assert_eq!(ctx.data(), [0, 0, 42, 0]);
    }
}
