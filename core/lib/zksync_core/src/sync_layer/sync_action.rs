use tokio::sync::mpsc;
use zksync_types::{L1BatchNumber, L2BlockNumber};

use super::{fetcher::FetchedTransaction, metrics::QUEUE_METRICS};
use crate::state_keeper::io::{L1BatchParams, L2BlockParams};

#[derive(Debug)]
pub struct ActionQueueSender(mpsc::Sender<SyncAction>);

impl ActionQueueSender {
    /// Pushes a set of actions to the queue.
    ///
    /// Requires that the actions are in the correct order: starts with a new open L1 batch / L2 block,
    /// followed by 0 or more transactions, have mandatory `SealL2Block` and optional `SealBatch` at the end.
    /// Would panic if the order is incorrect.
    pub(crate) async fn push_actions(&self, actions: Vec<SyncAction>) {
        Self::check_action_sequence(&actions).unwrap();
        for action in actions {
            self.0.send(action).await.expect("EN sync logic panicked");
            QUEUE_METRICS
                .action_queue_size
                .set(self.0.max_capacity() - self.0.capacity());
        }
    }

    /// Checks whether the action sequence is valid.
    /// Returned error is meant to be used as a panic message, since an invalid sequence represents an unrecoverable
    /// error. This function itself does not panic for the ease of testing.
    fn check_action_sequence(actions: &[SyncAction]) -> Result<(), String> {
        // Rules for the sequence:
        // 1. Must start with either `OpenBatch` or `L2Block`, both of which may be met only once.
        // 2. Followed by a sequence of `Tx` actions which consists of 0 or more elements.
        // 3. Must have either `SealL2Block` or `SealBatch` at the end.

        let mut opened = false;
        let mut l2_block_sealed = false;

        for action in actions {
            match action {
                SyncAction::OpenBatch { .. } | SyncAction::L2Block { .. } => {
                    if opened {
                        return Err(format!("Unexpected OpenBatch / L2Block: {actions:?}"));
                    }
                    opened = true;
                }
                SyncAction::Tx(_) => {
                    if !opened || l2_block_sealed {
                        return Err(format!("Unexpected Tx: {actions:?}"));
                    }
                }
                SyncAction::SealL2Block | SyncAction::SealBatch => {
                    if !opened || l2_block_sealed {
                        return Err(format!("Unexpected SealL2Block / SealBatch: {actions:?}"));
                    }
                    l2_block_sealed = true;
                }
            }
        }
        if !l2_block_sealed {
            return Err(format!("Incomplete sequence: {actions:?}"));
        }
        Ok(())
    }
}

/// Action queue is used to communicate between the fetcher and the rest of the external node
/// by collecting the fetched data in memory until it gets processed by the different entities.
#[derive(Debug)]
pub struct ActionQueue {
    receiver: mpsc::Receiver<SyncAction>,
    peeked: Option<SyncAction>,
}

impl ActionQueue {
    pub fn new() -> (ActionQueueSender, Self) {
        const ACTION_CAPACITY: usize = 32_768; // TODO: Make it configurable.

        let (sender, receiver) = mpsc::channel(ACTION_CAPACITY);
        let sender = ActionQueueSender(sender);
        let this = Self {
            receiver,
            peeked: None,
        };
        (sender, this)
    }

    /// Removes the first action from the queue.
    pub(super) fn pop_action(&mut self) -> Option<SyncAction> {
        if let Some(peeked) = self.peeked.take() {
            QUEUE_METRICS.action_queue_size.dec_by(1);
            return Some(peeked);
        }
        let action = self.receiver.try_recv().ok()?;
        QUEUE_METRICS.action_queue_size.dec_by(1);
        Some(action)
    }

    /// Removes the first action from the queue.
    pub(super) async fn recv_action(
        &mut self,
        max_wait: tokio::time::Duration,
    ) -> Option<SyncAction> {
        if let Some(action) = self.pop_action() {
            return Some(action);
        }
        let action = tokio::time::timeout(max_wait, self.receiver.recv())
            .await
            .ok()??;
        QUEUE_METRICS.action_queue_size.dec_by(1);
        Some(action)
    }

    /// Returns the first action from the queue without removing it.
    pub(super) fn peek_action(&mut self) -> Option<SyncAction> {
        if let Some(action) = &self.peeked {
            return Some(action.clone());
        }
        self.peeked = self.receiver.try_recv().ok();
        self.peeked.clone()
    }

    /// Returns the first action from the queue without removing it.
    pub(super) async fn peek_action_async(
        &mut self,
        max_wait: tokio::time::Duration,
    ) -> Option<SyncAction> {
        if let Some(action) = &self.peeked {
            return Some(action.clone());
        }
        self.peeked = tokio::time::timeout(max_wait, self.receiver.recv())
            .await
            .ok()?;
        self.peeked.clone()
    }
}

/// An instruction for the ExternalIO to request a certain action from the state keeper.
#[derive(Debug, Clone)]
pub(crate) enum SyncAction {
    OpenBatch {
        params: L1BatchParams,
        // Additional parameters used only for sanity checks
        number: L1BatchNumber,
        first_l2_block_number: L2BlockNumber,
    },
    L2Block {
        params: L2BlockParams,
        // Additional parameters used only for sanity checks
        number: L2BlockNumber,
    },
    Tx(Box<FetchedTransaction>),
    /// We need an explicit action for the L2 block sealing, since we fetch the whole L2 blocks and already know
    /// that they are sealed, but at the same time the next L2 block may not exist yet.
    /// By having a dedicated action for that we prevent a situation where the L2 block is kept open on the EN until
    /// the next one is sealed on the main node.
    SealL2Block,
    /// Similarly to `SealL2Block` we must be able to seal the batch even if there is no next L2 block yet.
    SealBatch,
}

impl From<FetchedTransaction> for SyncAction {
    fn from(tx: FetchedTransaction) -> Self {
        Self::Tx(Box::new(tx))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{fee_model::BatchFeeInput, l2::L2Tx, Address, ProtocolVersionId, H256};

    use super::*;

    fn open_batch() -> SyncAction {
        SyncAction::OpenBatch {
            params: L1BatchParams {
                protocol_version: ProtocolVersionId::latest(),
                validation_computational_gas_limit: u32::MAX,
                operator_address: Address::default(),
                fee_input: BatchFeeInput::default(),
                first_l2_block: L2BlockParams {
                    timestamp: 1,
                    virtual_blocks: 1,
                },
            },
            number: L1BatchNumber(1),
            first_l2_block_number: L2BlockNumber(1),
        }
    }

    fn l2_block() -> SyncAction {
        SyncAction::L2Block {
            params: L2BlockParams {
                timestamp: 1,
                virtual_blocks: 1,
            },
            number: 1.into(),
        }
    }

    fn tx() -> SyncAction {
        let mut tx = L2Tx::new(
            Default::default(),
            Default::default(),
            0.into(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        tx.set_input(H256::default().0.to_vec(), H256::default());

        FetchedTransaction::new(tx.into()).into()
    }

    fn seal_l2_block() -> SyncAction {
        SyncAction::SealL2Block
    }

    fn seal_batch() -> SyncAction {
        SyncAction::SealBatch
    }

    #[test]
    fn correct_sequence() {
        let test_vector = vec![
            vec![open_batch(), seal_l2_block()],
            vec![open_batch(), seal_batch()],
            vec![open_batch(), tx(), seal_l2_block()],
            vec![open_batch(), tx(), tx(), tx(), seal_l2_block()],
            vec![open_batch(), tx(), seal_batch()],
            vec![l2_block(), seal_l2_block()],
            vec![l2_block(), seal_batch()],
            vec![l2_block(), tx(), seal_l2_block()],
            vec![l2_block(), tx(), seal_batch()],
        ];
        for (idx, sequence) in test_vector.into_iter().enumerate() {
            ActionQueueSender::check_action_sequence(&sequence)
                .unwrap_or_else(|_| panic!("Valid sequence #{idx} failed"));
        }
    }

    #[test]
    fn incorrect_sequence() {
        // Note: it is very important to check the exact error that occurs to prevent the test to pass if sequence is
        // considered invalid e.g. because it's incomplete.
        let test_vector = vec![
            // Incomplete sequences.
            (vec![open_batch()], "Incomplete sequence"),
            (vec![open_batch(), tx()], "Incomplete sequence"),
            (vec![l2_block()], "Incomplete sequence"),
            (vec![l2_block(), tx()], "Incomplete sequence"),
            // Unexpected tx
            (vec![tx()], "Unexpected Tx"),
            (vec![open_batch(), seal_l2_block(), tx()], "Unexpected Tx"),
            // Unexpected `OpenBatch / L2Block`
            (
                vec![l2_block(), l2_block()],
                "Unexpected OpenBatch / L2Block",
            ),
            (
                vec![l2_block(), open_batch()],
                "Unexpected OpenBatch / L2Block",
            ),
            (
                vec![open_batch(), l2_block()],
                "Unexpected OpenBatch / L2Block",
            ),
            // Unexpected `SealL2Block`
            (vec![seal_l2_block()], "Unexpected SealL2Block"),
            (
                vec![l2_block(), seal_l2_block(), seal_l2_block()],
                "Unexpected SealL2Block",
            ),
            (
                vec![open_batch(), seal_l2_block(), seal_batch(), seal_batch()],
                "Unexpected SealL2Block / SealBatch",
            ),
            (
                vec![l2_block(), seal_l2_block(), seal_batch(), seal_batch()],
                "Unexpected SealL2Block / SealBatch",
            ),
            (vec![seal_batch()], "Unexpected SealL2Block / SealBatch"),
        ];
        for (idx, (sequence, expected_err)) in test_vector.into_iter().enumerate() {
            let Err(err) = ActionQueueSender::check_action_sequence(&sequence) else {
                panic!("Invalid sequence passed the test. Sequence #{idx}, expected error: {expected_err}");
            };
            assert!(
                err.starts_with(expected_err),
                "Sequence #{idx} failed. Expected error: {expected_err}, got: {err}"
            );
        }
    }
}
