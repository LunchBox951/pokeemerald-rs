use std::collections::HashMap;

use super::{reader::CommandReader, DecodeError, Event};

const MAX_CONTEXTS: usize = 1 << 20;
const MAX_PATTERN_DEPTH: usize = 3;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct Context {
    offset: usize,
    status: Option<u8>,
    key: u8,
    velocity: u8,
    returns: Vec<usize>,
    repeat: u8,
}

struct Branch {
    context: Context,
    event: usize,
}

pub(super) fn decode(bytes: &[u8]) -> Result<Vec<Event>, DecodeError> {
    Compiler::new(bytes, MAX_CONTEXTS).run()
}

struct Compiler<'a> {
    bytes: &'a [u8],
    limit: usize,
    events: Vec<Event>,
    entries: HashMap<Context, usize>,
    branches: Vec<Branch>,
}

impl<'a> Compiler<'a> {
    fn new(bytes: &'a [u8], limit: usize) -> Self {
        Self {
            bytes,
            limit,
            events: Vec::new(),
            entries: HashMap::new(),
            branches: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Event>, DecodeError> {
        self.block(Context::default(), None)?;
        while let Some(branch) = self.branches.pop() {
            self.block(branch.context, Some(branch.event))?;
        }
        Ok(self.events)
    }

    fn block(
        &mut self,
        mut context: Context,
        mut branch: Option<usize>,
    ) -> Result<(), DecodeError> {
        let is_branch = branch.is_some();
        loop {
            if context.offset == self.bytes.len() {
                if is_branch || !self.branches.is_empty() {
                    self.events.push(Event::Fine);
                }
                return Ok(());
            }
            let offset = context.offset;
            let mut reader = CommandReader::new(
                self.bytes,
                offset,
                context.status,
                context.key,
                context.velocity,
            );
            let event = reader.read()?;
            context.status = reader.status;
            context.key = reader.key;
            context.velocity = reader.velocity;
            // Inputs overwritten by this command cannot distinguish its output or continuation.
            if let Some(&target) = self.entries.get(&context) {
                if let Some(branch) = branch {
                    self.resolve_branch(branch, target);
                } else {
                    self.events.push(Event::Goto(target));
                }
                return Ok(());
            }
            if self.entries.len() >= self.limit {
                return Err(DecodeError::ExpansionLimit { limit: self.limit });
            }
            if let Some(branch) = branch.take() {
                self.resolve_branch(branch, self.events.len());
            }
            self.entries.insert(context.clone(), self.events.len());
            context.offset = reader.cursor;
            match event {
                Event::Fine => {
                    self.events.push(Event::Fine);
                    return Ok(());
                }
                Event::Goto(target) => {
                    self.validate_target(offset, target)?;
                    context.offset = target;
                }
                Event::Pattern(target) => {
                    self.validate_target(offset, target)?;
                    if context.returns.len() == MAX_PATTERN_DEPTH {
                        self.events.push(Event::Fine);
                        return Ok(());
                    }
                    context.returns.push(context.offset);
                    context.offset = target;
                }
                Event::PatternEnd => {
                    if let Some(target) = context.returns.pop() {
                        context.offset = target;
                    }
                }
                Event::Repeat { count, target } => {
                    self.validate_target(offset, target)?;
                    if count == 0 {
                        context.offset = target;
                    } else {
                        context.repeat = context.repeat.wrapping_add(1);
                        if context.repeat < count {
                            context.offset = target;
                        } else {
                            context.repeat = 0;
                        }
                    }
                }
                Event::MemAcc {
                    op,
                    addr,
                    value,
                    target: Some(target),
                } => {
                    self.validate_target(offset, target)?;
                    let mut taken = context.clone();
                    taken.offset = target;
                    self.branches.push(Branch {
                        context: taken,
                        event: self.events.len(),
                    });
                    self.events.push(Event::MemAcc {
                        op,
                        addr,
                        value,
                        target: Some(usize::MAX),
                    });
                }
                event => self.events.push(event),
            }
        }
    }

    fn resolve_branch(&mut self, branch: usize, target: usize) {
        let Event::MemAcc { target: slot, .. } = &mut self.events[branch] else {
            unreachable!("only memory conditions queue branches")
        };
        *slot = Some(target);
    }

    fn validate_target(&self, offset: usize, target: usize) -> Result<(), DecodeError> {
        if target >= self.bytes.len() {
            return Err(DecodeError::UnresolvedJump {
                offset,
                target: u32::try_from(target).expect("jump operands are 32-bit offsets"),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_expansion_has_an_explicit_limit() {
        assert_eq!(
            Compiler::new(&[0x81, 0x81, 0x81, 0xb1], 2).run(),
            Err(DecodeError::ExpansionLimit { limit: 2 })
        );
    }

    #[test]
    fn repeated_contexts_do_not_consume_more_expansion_budget() {
        assert_eq!(
            Compiler::new(&[0x81, 0xb2, 0, 0, 0, 0], 2).run().unwrap(),
            [Event::Wait(1), Event::Goto(0)]
        );
    }
}
