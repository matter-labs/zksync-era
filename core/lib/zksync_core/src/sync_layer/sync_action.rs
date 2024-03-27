use tokio::sync::mpsc;
use zksync_types::{L1BatchNumber, MiniblockNumber};

use super::{fetcher::FetchedTransaction, metrics::QUEUE_METRICS};
use crate::state_keeper::io::{L1BatchParams, MiniblockParams};

#[derive(Debug)]
pub struct ActionQueueSender(mpsc::Sender<SyncAction>);

impl ActionQueueSender {
    /// Pushes a set of actions to the queue.
    ///
    /// Requires that the actions are in the correct order: starts with a new open batch/miniblock,
    /// followed by 0 or more transactions, have mandatory `SealMiniblock` and optional `SealBatch` at the end.
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
        // 1. Must start with either `OpenBatch` or `Miniblock`, both of which may be met only once.
        // 2. Followed by a sequence of `Tx` actions which consists of 0 or more elements.
        // 3. Must have either `SealMiniblock` or `SealBatch` at the end.

        let mut opened = false;
        let mut miniblock_sealed = false;

        for action in actions {
            match action {
                SyncAction::OpenBatch { .. } | SyncAction::Miniblock { .. } => {
                    if opened {
                        return Err(format!("Unexpected OpenBatch/Miniblock: {:?}", actions));
                    }
                    opened = true;
                }
                SyncAction::Tx(_) => {
                    if !opened || miniblock_sealed {
                        return Err(format!("Unexpected Tx: {:?}", actions));
                    }
                }
                SyncAction::SealMiniblock | SyncAction::SealBatch => {
                    if !opened || miniblock_sealed {
                        return Err(format!("Unexpected SealMiniblock/SealBatch: {:?}", actions));
                    }
                    miniblock_sealed = true;
                }
            }
        }
        if !miniblock_sealed {
            return Err(format!("Incomplete sequence: {:?}", actions));
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
        let action = self.receiver.try_recv().ok();
        if action.is_some() {
            QUEUE_METRICS.action_queue_size.dec_by(1);
        }
        action
    }

    /// Returns the first action from the queue without removing it.
    pub(super) fn peek_action(&mut self) -> Option<SyncAction> {
        if let Some(action) = &self.peeked {
            return Some(action.clone());
        }
        self.peeked = self.receiver.try_recv().ok();
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
        first_miniblock_number: MiniblockNumber,
    },
    Miniblock {
        params: MiniblockParams,
        // Additional parameters used only for sanity checks
        number: MiniblockNumber,
    },
    Tx(Box<FetchedTransaction>),
    /// We need an explicit action for the miniblock sealing, since we fetch the whole miniblocks and already know
    /// that they are sealed, but at the same time the next miniblock may not exist yet.
    /// By having a dedicated action for that we prevent a situation where the miniblock is kept open on the EN until
    /// the next one is sealed on the main node.
    SealMiniblock,
    /// Similarly to `SealMiniblock` we must be able to seal the batch even if there is no next miniblock yet.
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
                first_miniblock: MiniblockParams {
                    timestamp: 1,
                    virtual_blocks: 1,
                },
            },
            number: L1BatchNumber(1),
            first_miniblock_number: MiniblockNumber(1),
        }
    }

    fn miniblock() -> SyncAction {
        SyncAction::Miniblock {
            params: MiniblockParams {
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

    fn seal_miniblock() -> SyncAction {
        SyncAction::SealMiniblock
    }

    fn seal_batch() -> SyncAction {
        SyncAction::SealBatch
    }

    #[test]
    fn correct_sequence() {
        let test_vector = vec![
            vec![open_batch(), seal_miniblock()],
            vec![open_batch(), seal_batch()],
            vec![open_batch(), tx(), seal_miniblock()],
            vec![open_batch(), tx(), tx(), tx(), seal_miniblock()],
            vec![open_batch(), tx(), seal_batch()],
            vec![miniblock(), seal_miniblock()],
            vec![miniblock(), seal_batch()],
            vec![miniblock(), tx(), seal_miniblock()],
            vec![miniblock(), tx(), seal_batch()],
        ];
        for (idx, sequence) in test_vector.into_iter().enumerate() {
            ActionQueueSender::check_action_sequence(&sequence)
                .unwrap_or_else(|_| panic!("Valid sequence #{} failed", idx));
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
            (vec![miniblock()], "Incomplete sequence"),
            (vec![miniblock(), tx()], "Incomplete sequence"),
            // Unexpected tx
            (vec![tx()], "Unexpected Tx"),
            (vec![open_batch(), seal_miniblock(), tx()], "Unexpected Tx"),
            // Unexpected `OpenBatch/Miniblock`
            (
                vec![miniblock(), miniblock()],
                "Unexpected OpenBatch/Miniblock",
            ),
            (
                vec![miniblock(), open_batch()],
                "Unexpected OpenBatch/Miniblock",
            ),
            (
                vec![open_batch(), miniblock()],
                "Unexpected OpenBatch/Miniblock",
            ),
            // Unexpected `SealMiniblock`
            (vec![seal_miniblock()], "Unexpected SealMiniblock"),
            (
                vec![miniblock(), seal_miniblock(), seal_miniblock()],
                "Unexpected SealMiniblock",
            ),
            (
                vec![open_batch(), seal_miniblock(), seal_batch(), seal_batch()],
                "Unexpected SealMiniblock/SealBatch",
            ),
            (
                vec![miniblock(), seal_miniblock(), seal_batch(), seal_batch()],
                "Unexpected SealMiniblock/SealBatch",
            ),
            (vec![seal_batch()], "Unexpected SealMiniblock/SealBatch"),
        ];
        for (idx, (sequence, expected_err)) in test_vector.into_iter().enumerate() {
            let Err(err) = ActionQueueSender::check_action_sequence(&sequence) else {
                panic!(
                    "Invalid sequence passed the test. Sequence #{}, expected error: {}",
                    idx, expected_err
                );
            };
            assert!(
                err.starts_with(expected_err),
                "Sequence #{} failed. Expected error: {}, got: {}",
                idx,
                expected_err,
                err
            );
        }
    }
}
