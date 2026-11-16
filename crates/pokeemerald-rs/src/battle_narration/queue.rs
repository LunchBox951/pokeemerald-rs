//! The FIFO of resolved narration batches and the gates that report them.
//!
//! A battle command that prints stays outstanding until its printer goes
//! inactive (`src/battle_controller_player.c:1339`-`:1342`), so a later stage
//! may not run on a request merely having been submitted. A [`NarrationGate`]
//! names one batch of messages; it reads [`GateStatus::Complete`] only after
//! every earlier batch and every message of its own has finished printing.

use std::collections::VecDeque;
use std::sync::Arc;

use engine::text::Token;

/// One batch of narration a stage waits on, issued by
/// [`BattleMessageLayer::reserve`](super::BattleMessageLayer::reserve).
///
/// A gate belongs to the layer that issued it: another layer reads it as a
/// reservation whose producer has not spoken, and refuses to resolve it.
#[derive(Debug, Clone)]
pub struct NarrationGate {
    owner: Arc<()>,
    batch: u64,
}

impl PartialEq for NarrationGate {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.batch == other.batch
    }
}

impl Eq for NarrationGate {}

/// How far a [`NarrationGate`]'s batch has got.
///
/// A stage proceeds on [`GateStatus::Complete`] alone: every other status,
/// including a missing producer, keeps it waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// The batch is reserved but its producer has not resolved it.
    AwaitingProducer,
    /// The batch is resolved and still queued or printing.
    Pending,
    /// Every message of the batch, and of every earlier batch, has printed.
    Complete,
    /// The batch could not be expanded; it never completes.
    Failed,
}

impl GateStatus {
    /// Whether the batch's text has finished printing.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Why a gate could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    /// The gate names no reservation in this queue.
    Unknown,
    /// The gate's batch already holds messages or a failure.
    AlreadyResolved,
}

#[derive(Debug)]
enum Batch {
    Unresolved,
    Ready(VecDeque<Vec<Token>>),
    Failed,
}

/// What sits at the head of the queue, without taking from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadKind {
    Empty,
    AwaitingProducer,
    Failed,
    Ready,
}

/// What the head of the queue offers the printer.
#[derive(Debug)]
pub(super) enum Head {
    Empty,
    AwaitingProducer,
    Failed,
    /// The head batch has no message left to start.
    Drained,
    Message(Vec<Token>),
}

/// Batches in submission order. A completed batch leaves the queue, so every
/// batch numbered below `completed` is complete.
#[derive(Debug, Default)]
pub(super) struct NarrationQueue {
    /// Names this queue in the gates it issues.
    owner: Arc<()>,
    completed: u64,
    batches: VecDeque<Batch>,
}

impl NarrationQueue {
    pub(super) fn reserve(&mut self) -> NarrationGate {
        let batch = self.completed + self.batches.len() as u64;
        self.batches.push_back(Batch::Unresolved);
        NarrationGate {
            owner: Arc::clone(&self.owner),
            batch,
        }
    }

    pub(super) fn status(&self, gate: &NarrationGate) -> GateStatus {
        if !self.issued(gate) {
            return GateStatus::AwaitingProducer;
        }
        if gate.batch < self.completed {
            return GateStatus::Complete;
        }
        match self.slot(gate).map(|index| &self.batches[index]) {
            Some(Batch::Unresolved) | None => GateStatus::AwaitingProducer,
            Some(Batch::Ready(_)) => GateStatus::Pending,
            Some(Batch::Failed) => GateStatus::Failed,
        }
    }

    /// Confirms `gate` is a reservation nobody has resolved yet.
    pub(super) fn check_unresolved(&self, gate: &NarrationGate) -> Result<(), GateError> {
        let index = self.slot(gate).ok_or(self.unknown_or_resolved(gate))?;
        match self.batches[index] {
            Batch::Unresolved => Ok(()),
            Batch::Ready(_) | Batch::Failed => Err(GateError::AlreadyResolved),
        }
    }

    pub(super) fn resolve(
        &mut self,
        gate: &NarrationGate,
        messages: Vec<Vec<Token>>,
    ) -> Result<(), GateError> {
        self.check_unresolved(gate)?;
        let index = self.slot(gate).ok_or(GateError::Unknown)?;
        self.batches[index] = Batch::Ready(messages.into());
        Ok(())
    }

    pub(super) fn fail(&mut self, gate: &NarrationGate) -> Result<(), GateError> {
        self.check_unresolved(gate)?;
        let index = self.slot(gate).ok_or(GateError::Unknown)?;
        self.batches[index] = Batch::Failed;
        Ok(())
    }

    pub(super) fn take_head(&mut self) -> Head {
        match self.batches.front_mut() {
            None => Head::Empty,
            Some(Batch::Unresolved) => Head::AwaitingProducer,
            Some(Batch::Failed) => Head::Failed,
            Some(Batch::Ready(messages)) => {
                messages.pop_front().map_or(Head::Drained, Head::Message)
            }
        }
    }

    /// Completes the head batch if it has no message left, returning its gate.
    pub(super) fn complete_drained_head(&mut self) -> Option<NarrationGate> {
        match self.batches.front() {
            Some(Batch::Ready(messages)) if messages.is_empty() => {}
            _ => return None,
        }
        self.batches.pop_front();
        let gate = NarrationGate {
            owner: Arc::clone(&self.owner),
            batch: self.completed,
        };
        self.completed += 1;
        Some(gate)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    pub(super) fn head_kind(&self) -> HeadKind {
        match self.batches.front() {
            None => HeadKind::Empty,
            Some(Batch::Unresolved) => HeadKind::AwaitingProducer,
            Some(Batch::Failed) => HeadKind::Failed,
            Some(Batch::Ready(_)) => HeadKind::Ready,
        }
    }

    fn issued(&self, gate: &NarrationGate) -> bool {
        Arc::ptr_eq(&self.owner, &gate.owner)
    }

    fn slot(&self, gate: &NarrationGate) -> Option<usize> {
        if !self.issued(gate) {
            return None;
        }
        let offset = usize::try_from(gate.batch.checked_sub(self.completed)?).ok()?;
        (offset < self.batches.len()).then_some(offset)
    }

    fn unknown_or_resolved(&self, gate: &NarrationGate) -> GateError {
        if self.issued(gate) && gate.batch < self.completed {
            GateError::AlreadyResolved
        } else {
            GateError::Unknown
        }
    }
}
