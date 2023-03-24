use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use chrono::{DateTime, Utc};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{L1BatchNumber, MiniblockNumber, Transaction, H256};

/// Action queue is used to communicate between the fetcher and the rest of the external node
/// by collecting the fetched data in memory until it gets processed by the different entities.
#[derive(Debug, Clone, Default)]
pub struct ActionQueue {
    inner: Arc<RwLock<ActionQueueInner>>,
}

impl ActionQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes the first action from the queue.
    pub(crate) fn pop_action(&self) -> Option<SyncAction> {
        let mut write_lock = self.inner.write().unwrap();
        write_lock.actions.pop_front()
    }

    /// Returns the first action from the queue without removing it.
    pub(crate) fn peek_action(&self) -> Option<SyncAction> {
        let read_lock = self.inner.read().unwrap();
        read_lock.actions.front().cloned()
    }

    /// Returns true if the queue has capacity for a new action.
    /// Capacity is limited to avoid memory exhaustion.
    pub(crate) fn has_action_capacity(&self) -> bool {
        const ACTION_CAPACITY: usize = 32_768;

        // Since the capacity is read before the action is pushed,
        // it is possible that the capacity will be exceeded, since the fetcher will
        // decompose received data into a sequence of actions.
        // This is not a problem, since the size of decomposed action is much smaller
        // than the configured capacity.
        let read_lock = self.inner.read().unwrap();
        read_lock.actions.len() < ACTION_CAPACITY
    }

    /// Returns true if the queue has capacity for a new status change.
    /// Capacity is limited to avoid memory exhaustion.
    pub(crate) fn has_status_change_capacity(&self) -> bool {
        const STATUS_CHANGE_CAPACITY: usize = 8192;

        // We don't really care about any particular queue size, as the only intention
        // of this check is to prevent memory exhaustion.
        let read_lock = self.inner.read().unwrap();
        read_lock.commit_status_changes.len() < STATUS_CHANGE_CAPACITY
            && read_lock.prove_status_changes.len() < STATUS_CHANGE_CAPACITY
            && read_lock.execute_status_changes.len() < STATUS_CHANGE_CAPACITY
    }

    /// Pushes a set of actions to the queue.
    ///
    /// Requires that the actions are in the correct order: starts with a new open batch/miniblock,
    /// followed by 0 or more transactions, have mandatory `SealMiniblock` and optional `SealBatch` at the end.
    /// Would panic if the order is incorrect.
    pub(crate) fn push_actions(&self, actions: Vec<SyncAction>) {
        // We need to enforce the ordering of actions to make sure that they can be processed.
        Self::check_action_sequence(&actions).expect("Invalid sequence of actions.");

        let mut write_lock = self.inner.write().unwrap();
        write_lock.actions.extend(actions);
    }

    /// Pushes a notification about certain batch being committed.
    pub(crate) fn push_commit_status_change(&self, change: BatchStatusChange) {
        let mut write_lock = self.inner.write().unwrap();
        write_lock.commit_status_changes.push_back(change);
    }

    /// Pushes a notification about certain batch being proven.
    pub(crate) fn push_prove_status_change(&self, change: BatchStatusChange) {
        let mut write_lock = self.inner.write().unwrap();
        write_lock.prove_status_changes.push_back(change);
    }

    /// Pushes a notification about certain batch being executed.
    pub(crate) fn push_execute_status_change(&self, change: BatchStatusChange) {
        let mut write_lock = self.inner.write().unwrap();
        write_lock.execute_status_changes.push_back(change);
    }

    /// Collects all status changes and returns them.
    pub(crate) fn take_status_changes(&self) -> StatusChanges {
        let mut write_lock = self.inner.write().unwrap();
        StatusChanges {
            commit: write_lock.commit_status_changes.drain(..).collect(),
            prove: write_lock.prove_status_changes.drain(..).collect(),
            execute: write_lock.execute_status_changes.drain(..).collect(),
        }
    }

    /// Checks whether the action sequence is valid.
    /// Returned error is meant to be used as a panic message, since an invalid sequence represents an unrecoverable
    /// error. This function itself does not panic for the ease of testing.
    fn check_action_sequence(actions: &[SyncAction]) -> Result<(), String> {
        // Rules for the sequence:
        // 1. Must start with either `OpenBatch` or `Miniblock`, both of which may be met only once.
        // 2. Followed by a sequence of `Tx` actions which consists of 0 or more elements.
        // 3. Must have `SealMiniblock` come after transactions.
        // 4. May or may not have `SealBatch` come after `SealMiniblock`.

        let mut opened = false;
        let mut miniblock_sealed = false;
        let mut batch_sealed = false;

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
                SyncAction::SealMiniblock => {
                    if !opened || miniblock_sealed {
                        return Err(format!("Unexpected SealMiniblock: {:?}", actions));
                    }
                    miniblock_sealed = true;
                }
                SyncAction::SealBatch => {
                    if !miniblock_sealed || batch_sealed {
                        return Err(format!("Unexpected SealBatch: {:?}", actions));
                    }
                    batch_sealed = true;
                }
            }
        }
        if !miniblock_sealed {
            return Err(format!("Incomplete sequence: {:?}", actions));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct StatusChanges {
    pub(crate) commit: Vec<BatchStatusChange>,
    pub(crate) prove: Vec<BatchStatusChange>,
    pub(crate) execute: Vec<BatchStatusChange>,
}

#[derive(Debug, Default)]
struct ActionQueueInner {
    actions: VecDeque<SyncAction>,
    commit_status_changes: VecDeque<BatchStatusChange>,
    prove_status_changes: VecDeque<BatchStatusChange>,
    execute_status_changes: VecDeque<BatchStatusChange>,
}

/// An instruction for the ExternalIO to request a certain action from the state keeper.
#[derive(Debug, Clone)]
pub(crate) enum SyncAction {
    OpenBatch {
        number: L1BatchNumber,
        timestamp: u64,
        l1_gas_price: u64,
        l2_fair_gas_price: u64,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    },
    Miniblock {
        number: MiniblockNumber,
        timestamp: u64,
    },
    Tx(Box<Transaction>),
    /// We need an explicit action for the miniblock sealing, since we fetch the whole miniblocks and already know
    /// that they are sealed, but at the same time the next miniblock may not exist yet.
    /// By having a dedicated action for that we prevent a situation where the miniblock is kept open on the EN until
    /// the next one is sealed on the main node.
    SealMiniblock,
    /// Similarly to `SealMiniblock` we must be able to seal the batch even if there is no next miniblock yet.
    SealBatch,
}

/// Represents a change in the batch status.
/// It may be a batch being committed, proven or executed.
#[derive(Debug)]
pub(crate) struct BatchStatusChange {
    pub(crate) number: L1BatchNumber,
    pub(crate) l1_tx_hash: H256,
    pub(crate) happened_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use zksync_types::l2::L2Tx;

    use super::*;

    fn open_batch() -> SyncAction {
        SyncAction::OpenBatch {
            number: 1.into(),
            timestamp: 1,
            l1_gas_price: 1,
            l2_fair_gas_price: 1,
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        }
    }

    fn miniblock() -> SyncAction {
        SyncAction::Miniblock {
            number: 1.into(),
            timestamp: 1,
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

        SyncAction::Tx(Box::new(tx.into()))
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
            vec![open_batch(), tx(), seal_miniblock()],
            vec![open_batch(), seal_miniblock(), seal_batch()],
            vec![open_batch(), tx(), seal_miniblock(), seal_batch()],
            vec![miniblock(), seal_miniblock()],
            vec![miniblock(), tx(), seal_miniblock()],
            vec![miniblock(), seal_miniblock(), seal_batch()],
            vec![miniblock(), tx(), seal_miniblock(), seal_batch()],
        ];
        for (idx, sequence) in test_vector.into_iter().enumerate() {
            ActionQueue::check_action_sequence(&sequence)
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
            // Unexpected OpenBatch/Miniblock
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
            // Unexpected SealMiniblock
            (vec![seal_miniblock()], "Unexpected SealMiniblock"),
            (
                vec![miniblock(), seal_miniblock(), seal_miniblock()],
                "Unexpected SealMiniblock",
            ),
            // Unexpected SealBatch.
            (
                vec![open_batch(), tx(), seal_batch()],
                "Unexpected SealBatch",
            ),
            (vec![open_batch(), seal_batch()], "Unexpected SealBatch"),
            (
                vec![open_batch(), seal_miniblock(), seal_batch(), seal_batch()],
                "Unexpected SealBatch",
            ),
            (vec![miniblock(), seal_batch()], "Unexpected SealBatch"),
            (
                vec![miniblock(), seal_miniblock(), seal_batch(), seal_batch()],
                "Unexpected SealBatch",
            ),
            (vec![seal_batch()], "Unexpected SealBatch"),
            (
                vec![miniblock(), tx(), seal_batch()],
                "Unexpected SealBatch",
            ),
        ];
        for (idx, (sequence, expected_err)) in test_vector.into_iter().enumerate() {
            let err =
                ActionQueue::check_action_sequence(&sequence).expect_err("Invalid sequence passed");
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
