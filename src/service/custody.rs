//! Observe every admitted SDK unit before its bounded ticket history expires.

use super::*;
use sophia_shell_client::{Admission, Custody, ShellClientError, Ticket};

// The pinned SDK retains 256 tickets. Bound the span, rather than just the
// number of pending tickets: newer completed writes must not evict an older
// submission whose custody is still unknown.
const MAX_TICKET_SPAN: u64 = 128;

#[derive(Default)]
pub(super) struct CustodyWatch {
    pending: VecDeque<Ticket>,
    last: u64,
}

impl CustodyWatch {
    fn reserve(&self, maximum: usize) -> Result<(), ShellClientError> {
        if self
            .pending
            .front()
            .is_some_and(|first| self.last - first.0 + 1 + maximum as u64 > MAX_TICKET_SPAN)
        {
            return Err(ShellClientError::QueueSaturated);
        }
        Ok(())
    }

    fn record(&mut self, admission: Admission) {
        for ticket in admission.tickets() {
            self.last = ticket.0;
            self.pending.push_back(ticket);
        }
    }

    pub(super) fn observe(&mut self, connection: &ShellConnection) -> Result<(), String> {
        for _ in 0..self.pending.len() {
            let ticket = self.pending.pop_front().expect("bounded ticket count");
            match connection.custody(ticket) {
                Some(Custody::Queued | Custody::InFlight) => self.pending.push_back(ticket),
                Some(Custody::Submitted | Custody::Stored) => {}
                Some(Custody::Written) => {
                    return Err("file submission reported socket-only custody".into());
                }
                Some(Custody::Refused(errno)) => {
                    return Err(format!(
                        "shell submission {} refused: errno={errno}",
                        ticket.0
                    ));
                }
                Some(Custody::Unknown | Custody::DroppedUnsent) => {
                    return Err(format!(
                        "shell submission {} ended without custody",
                        ticket.0
                    ));
                }
                None => return Err("shell submission ticket expired before observation".into()),
            }
        }
        Ok(())
    }
}

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn enqueue_content(
        &mut self,
        transaction: TransactionId,
        record: &ShellContentRecord,
    ) -> Result<(), ShellClientError> {
        self.custody.reserve(1)?;
        let admission = self
            .connection
            .enqueue_content_tracked(transaction, record)?;
        self.custody.record(admission);
        Ok(())
    }

    pub(super) fn enqueue_candidate(
        &mut self,
        transaction: TransactionId,
        records: &[ShellContentRecord],
    ) -> Result<(), ShellClientError> {
        self.custody.reserve(records.len())?;
        let admission =
            self.connection
                .enqueue_candidate_tracked(&mut self.lifecycle, transaction, records)?;
        self.custody.record(admission);
        Ok(())
    }

    pub(super) fn enqueue_action_response(
        &mut self,
        transaction: TransactionId,
        ack: &ContentActionAck,
        activation: Option<(TransactionId, &ShellIndicatorActivation)>,
    ) -> Result<(), ShellClientError> {
        self.custody
            .reserve(1 + usize::from(activation.is_some()))?;
        let admission = self.connection.enqueue_indicator_action_response_tracked(
            transaction,
            ack,
            activation,
        )?;
        self.custody.record(admission);
        Ok(())
    }
}
