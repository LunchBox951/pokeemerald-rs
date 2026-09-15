use super::{
    as_signed, centered, xcmd_payload_width, DecodeError, Event, BEND, BENDR, CLOCK_TABLE, EOT,
    FINE, GOTO, KEYSH, LFODL, LFOS, MEMACC, MEMACC_CONDITIONS, MOD, MODT, PAN, PATT, PEND, PORT,
    PRIO, REPT, RUNNING_STATUS_MIN, STATUS_BYTE_MIN, TEMPO, TIE, TUNE, VOICE, VOL, WAIT_HI,
    WAIT_LO, XCMD,
};

pub(super) struct CommandReader<'a> {
    bytes: &'a [u8],
    pub(super) cursor: usize,
    pub(super) status: Option<u8>,
    pub(super) key: u8,
    pub(super) velocity: u8,
}

impl<'a> CommandReader<'a> {
    pub(super) fn new(
        bytes: &'a [u8],
        cursor: usize,
        status: Option<u8>,
        key: u8,
        velocity: u8,
    ) -> Self {
        Self {
            bytes,
            cursor,
            status,
            key,
            velocity,
        }
    }

    pub(super) fn read(&mut self) -> Result<Event, DecodeError> {
        let offset = self.cursor;
        let byte = self.bytes[self.cursor];
        let command = if byte < STATUS_BYTE_MIN {
            self.status
                .ok_or(DecodeError::RunningStatusWithoutCommand { offset })?
        } else {
            self.cursor += 1;
            if byte >= RUNNING_STATUS_MIN {
                self.status = Some(byte);
            }
            byte
        };
        match command {
            WAIT_LO..=WAIT_HI => Ok(Event::Wait(CLOCK_TABLE[usize::from(command - WAIT_LO)])),
            FINE => Ok(Event::Fine),
            GOTO => Ok(Event::Goto(self.target()?)),
            PATT => Ok(Event::Pattern(self.target()?)),
            PEND => Ok(Event::PatternEnd),
            REPT => Ok(Event::Repeat {
                count: self.byte()?,
                target: self.target()?,
            }),
            MEMACC => {
                let op = self.byte()?;
                let addr = self.byte()?;
                let value = self.byte()?;
                let target = if MEMACC_CONDITIONS.contains(&op) {
                    Some(self.target()?)
                } else {
                    None
                };
                Ok(Event::MemAcc {
                    op,
                    addr,
                    value,
                    target,
                })
            }
            PRIO => self.unary(Event::Priority),
            TEMPO => self.unary(|value| Event::Tempo(u16::from(value) * 2)),
            KEYSH => self.unary(|value| Event::KeyShift(as_signed(value))),
            VOICE => self.unary(Event::Voice),
            VOL => self.unary(Event::Volume),
            PAN => self.unary(|value| Event::Pan(centered(value))),
            BEND => self.unary(|value| Event::Bend(centered(value))),
            BENDR => self.unary(Event::BendRange),
            LFOS => self.unary(Event::LfoSpeed),
            LFODL => self.unary(Event::LfoDelay),
            MOD => self.unary(Event::Modulation),
            MODT => self.unary(Event::ModType),
            TUNE => self.unary(|value| Event::Tune(centered(value))),
            XCMD => {
                let kind = self.byte()?;
                let mut value = 0;
                for i in 0..xcmd_payload_width(kind) {
                    value |= u32::from(self.byte()?) << (8 * i);
                }
                Ok(Event::Xcmd { kind, value })
            }
            PORT => Ok(Event::Port {
                control: self.byte()?,
                value: self.byte()?,
            }),
            EOT => {
                let key = self.optional();
                if let Some(key) = key {
                    self.key = key;
                }
                Ok(Event::EndOfTie { key })
            }
            TIE..=u8::MAX => Ok(self.note(command)),
            _ => Err(DecodeError::UnknownCommand {
                offset,
                byte: command,
            }),
        }
    }

    fn note(&mut self, command: u8) -> Event {
        let mut gate = CLOCK_TABLE[usize::from(command - TIE)];
        if let Some(key) = self.optional() {
            self.key = key;
            if let Some(velocity) = self.optional() {
                self.velocity = velocity;
                if let Some(extension) = self.optional() {
                    gate = gate.wrapping_add(extension);
                }
            }
        }
        Event::Note {
            key: self.key,
            velocity: self.velocity,
            gate,
        }
    }

    fn unary(&mut self, event: fn(u8) -> Event) -> Result<Event, DecodeError> {
        Ok(event(self.byte()?))
    }

    fn byte(&mut self) -> Result<u8, DecodeError> {
        let byte = *self
            .bytes
            .get(self.cursor)
            .ok_or(DecodeError::UnexpectedEnd)?;
        self.cursor += 1;
        Ok(byte)
    }

    fn optional(&mut self) -> Option<u8> {
        let byte = self
            .bytes
            .get(self.cursor)
            .copied()
            .filter(|byte| *byte < STATUS_BYTE_MIN)?;
        self.cursor += 1;
        Some(byte)
    }

    fn target(&mut self) -> Result<usize, DecodeError> {
        let mut bytes = [0; 4];
        for byte in &mut bytes {
            *byte = self.byte()?;
        }
        Ok(u32::from_le_bytes(bytes) as usize)
    }
}
